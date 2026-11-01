//! Succès rapportés pendant une partie : pont local, persistance et envoi GOG.
//!
//! Le jeu reçoit `QUARK_ACHIEVEMENTS_URL` et `QUARK_ACHIEVEMENTS_TOKEN` au
//! lancement. Un simple POST sur cette adresse enregistre un déblocage :
//!
//! ```text
//! curl -X POST "$QUARK_ACHIEVEMENTS_URL" \
//!      -H "Authorization: Bearer $QUARK_ACHIEVEMENTS_TOKEN" \
//!      -d '{"game":"'$QUARK_GAME_ID'","achievement":"ACH_FIRST","name":"Premier pas"}'
//! ```
//!
//! GOG accepte l'écriture (`gameplay.gog.com`) : le succès y est publié avec la
//! session gogdl. Epic n'expose aucune écriture hors de son client : le succès
//! reste visible dans Quark, marqué « session ».

use crate::app::QuarkApp;
use crate::config::AssetKind;
use crate::providers::{Provider, ProviderGameKey, SessionUnlock};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

pub fn session_unlocks_path() -> PathBuf {
    crate::config::data_dir().join("session-achievements.json")
}

pub fn provider_key_id(key: &ProviderGameKey) -> String {
    let provider = match key.provider {
        Provider::Steam => "steam",
        Provider::Epic => "epic",
        Provider::Gog => "gog",
    };
    format!("{provider}:{}", key.product_id)
}

pub fn parse_provider_key_id(text: &str) -> Option<ProviderGameKey> {
    let (provider, product_id) = text.split_once(':')?;
    let provider = match provider {
        "steam" => Provider::Steam,
        "epic" => Provider::Epic,
        "gog" => Provider::Gog,
        _ => return None,
    };
    (!product_id.trim().is_empty()).then(|| ProviderGameKey {
        provider,
        product_id: product_id.trim().to_string(),
    })
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SessionUnlockFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    games: std::collections::BTreeMap<String, Vec<SessionUnlock>>,
}

pub fn load_session_unlocks() -> HashMap<ProviderGameKey, Vec<SessionUnlock>> {
    let Ok(text) = std::fs::read_to_string(session_unlocks_path()) else {
        return HashMap::new();
    };
    let Ok(file) = serde_json::from_str::<SessionUnlockFile>(&text) else {
        return HashMap::new();
    };
    file.games
        .into_iter()
        .filter_map(|(id, unlocks)| parse_provider_key_id(&id).map(|key| (key, unlocks)))
        .collect()
}

pub fn save_session_unlocks(unlocks: &HashMap<ProviderGameKey, Vec<SessionUnlock>>) {
    let path = session_unlocks_path();
    let Some(parent) = path.parent() else {
        return;
    };
    let _ = std::fs::create_dir_all(parent);
    let file = SessionUnlockFile {
        version: 1,
        games: unlocks
            .iter()
            .map(|(key, unlocks)| (provider_key_id(key), unlocks.clone()))
            .collect(),
    };
    let Ok(text) = serde_json::to_string_pretty(&file) else {
        return;
    };
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, text).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

impl QuarkApp {
    /// Démarre le pont des succès une seule fois, à la première occasion.
    pub(crate) fn ensure_bridge(&mut self, ctx: &egui::Context) {
        if self.bridge.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        match crate::achievements_bridge::start(tx, move || repaint.request_repaint()) {
            Ok(bridge) => {
                self.bridge = Some(bridge);
                self.bridge_rx = Some(rx);
            }
            Err(error) => self.toast_error(error),
        }
    }

    /// Variables transmises au jeu lancement : adresse et jeton du pont.
    pub(crate) fn bridge_env(&mut self, ctx: &egui::Context, id: &str) -> Vec<(String, String)> {
        self.ensure_bridge(ctx);
        let Some(bridge) = self.bridge.as_ref() else {
            return Vec::new();
        };
        vec![
            (
                crate::achievements_bridge::ENV_URL.to_string(),
                bridge.url().to_string(),
            ),
            (
                crate::achievements_bridge::ENV_TOKEN.to_string(),
                bridge.token().to_string(),
            ),
            ("QUARK_GAME_ID".to_string(), id.to_string()),
        ]
    }

    /// Déblocages reçus du jeu en cours : enregistrés localement, puis écrits
    /// chez GOG quand la boutique sait le faire.
    pub(crate) fn poll_session_bridge(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.bridge_rx.as_ref() else {
            return;
        };
        let mut reports = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(report) => reports.push(report),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.bridge_rx = None;
                    break;
                }
            }
        }
        if reports.is_empty() {
            return;
        }
        for report in reports {
            let key = self
                .lib
                .provider_library
                .games
                .iter()
                .find(|game| {
                    game.quark_game_id.as_deref() == Some(report.game.as_str())
                        || provider_key_id(&game.key) == report.game
                })
                .map(|game| game.key.clone())
                .or_else(|| parse_provider_key_id(&report.game));
            let Some(key) = key else {
                self.toast_info(format!(
                    "Succès « {} » reçu, mais ce jeu n'est lié à aucune boutique",
                    report.unlock.name
                ));
                continue;
            };
            let unlocks = self.session_unlocks.entry(key.clone()).or_default();
            let existing = unlocks.iter_mut().find(|unlock| {
                unlock.key.eq_ignore_ascii_case(&report.unlock.key)
                    || (!unlock.name.is_empty()
                        && unlock.name.eq_ignore_ascii_case(&report.unlock.name))
            });
            match existing {
                Some(unlock) => {
                    if unlock.name.is_empty() {
                        unlock.name = report.unlock.name.clone();
                    }
                    if unlock.description.is_empty() {
                        unlock.description = report.unlock.description.clone();
                    }
                }
                None => unlocks.push(report.unlock.clone()),
            }
            let title = if report.unlock.name.trim().is_empty() {
                report.unlock.key.clone()
            } else {
                report.unlock.name.clone()
            };
            self.announce_achievement(&key, &title);
            if key.provider == Provider::Gog {
                self.toast_info(format!("Succès GOG « {title} » · synchronisation…"));
            } else {
                self.toast_info(format!("Succès « {title} » enregistré pour cette session"));
            }
        }
        save_session_unlocks(&self.session_unlocks);
        self.start_gog_unlock(ctx);
    }

    /// Envoie à GOG le premier succès de session non confirmé.
    fn start_gog_unlock(&mut self, ctx: &egui::Context) {
        if self.gog_unlock_job.is_some() {
            return;
        }
        let candidate = self.session_unlocks.iter().find_map(|(key, unlocks)| {
            (key.provider == Provider::Gog).then(|| {
                unlocks
                    .iter()
                    .find(|unlock| !unlock.synced)
                    .map(|unlock| (key.clone(), unlock.key.clone()))
            })
        });
        let Some(Some((key, achievement))) = candidate else {
            return;
        };
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let product_id = key.product_id.clone();
        let sent_key = key.clone();
        std::thread::spawn(move || {
            let result = crate::providers::gog_library::unlock_for_session(
                &crate::providers::gog::auth_config_path(),
                &product_id,
                &achievement,
                crate::config::now_ts(),
            );
            let _ = tx.send((sent_key, result));
            repaint.request_repaint();
        });
        self.gog_unlock_job = Some(rx);
        ctx.request_repaint_after(Duration::from_millis(150));
    }

    fn poll_gog_unlock(&mut self, ctx: &egui::Context) {
        let Some(result) = self.gog_unlock_job.as_ref().map(Receiver::try_recv) else {
            return;
        };
        match result {
            Ok((key, Ok(Some(name)))) => {
                self.gog_unlock_job = None;
                if let Some(unlocks) = self.session_unlocks.get_mut(&key) {
                    for unlock in unlocks.iter_mut().filter(|unlock| !unlock.synced) {
                        unlock.synced = true;
                    }
                }
                save_session_unlocks(&self.session_unlocks);
                // La liste déjà chargée pour ce jeu doit refléter l'écriture.
                self.achievements.remove(&key);
                self.toast_info(format!("Succès GOG « {name} » synchronisé"));
                self.start_gog_unlock(ctx);
            }
            Ok((key, Ok(None))) => {
                self.gog_unlock_job = None;
                if let Some(unlocks) = self.session_unlocks.get_mut(&key) {
                    for unlock in unlocks.iter_mut().filter(|unlock| !unlock.synced) {
                        unlock.synced = true;
                    }
                }
                save_session_unlocks(&self.session_unlocks);
                self.start_gog_unlock(ctx);
            }
            Ok((_key, Err(error))) => {
                self.gog_unlock_job = None;
                self.toast_error(format!("Succès GOG non synchronisé : {error}"));
            }
            Err(TryRecvError::Disconnected) => self.gog_unlock_job = None,
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(150)),
        }
    }

    /// Jaquette et bannière d'un jeu ajouté à la bibliothèque : téléchargées
    /// en arrière-plan, puis posées sur la fiche Quark. Les images déjà
    /// présentes (personnalisées ou importées) ne sont jamais remplacées.
    pub(crate) fn poll_adopt_cover(&mut self) {
        let Some((game_id, _key, rx)) = self.adopt_cover_job.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok((art, key)) => {
                let game_id = game_id.clone();
                let mut changed = false;
                if let Some(game) = self.lib.get_mut(&game_id) {
                    for (kind, image) in [
                        (AssetKind::Cover, art.cover.as_ref()),
                        (AssetKind::Hero, art.hero.as_ref()),
                    ] {
                        let missing = game
                            .artwork
                            .get(kind)
                            .is_none_or(|path| !path.exists());
                        if missing {
                            if let Some((path, _)) = image {
                                game.artwork.set(kind, path.clone());
                                changed = true;
                            }
                        }
                    }
                }
                if let Some(record) = self
                    .lib
                    .provider_library
                    .games
                    .iter_mut()
                    .find(|game| game.key == key)
                {
                    if let Some((_, url)) = art.cover.as_ref() {
                        if url.is_some() && record.cover_url != *url {
                            record.cover_url = url.clone();
                            changed = true;
                        }
                    }
                }
                if changed {
                    self.touch();
                }
                self.adopt_cover_job = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.adopt_cover_job = None,
        }
    }

    /// Appelé à chaque frame : pont des succès puis écriture GOG en attente.
    pub(crate) fn poll_session_sync(&mut self, ctx: &egui::Context) {
        self.poll_session_bridge(ctx);
        self.poll_gog_unlock(ctx);
        self.poll_adopt_cover();
        self.ensure_provider_artwork(ctx);
    }
}
