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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

/// Le fichier existait mais n'a pas pu être lu : ne jamais le remplacer par
/// une carte vide, ce qui effacerait les succès en attente.
static SESSION_UNLOCKS_UNREADABLE: AtomicBool = AtomicBool::new(false);

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

pub fn load_session_unlocks() -> (HashMap<ProviderGameKey, Vec<SessionUnlock>>, Option<String>) {
    let path = session_unlocks_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (HashMap::new(), None);
        }
        Err(error) => {
            SESSION_UNLOCKS_UNREADABLE.store(true, Ordering::SeqCst);
            return (
                HashMap::new(),
                Some(format!(
                    "Succès de session illisibles ({}) : {error}. Le fichier ne sera pas écrasé.",
                    path.display()
                )),
            );
        }
    };
    let file = match serde_json::from_str::<SessionUnlockFile>(&text) {
        Ok(file) => file,
        Err(error) => {
            let backup = unique_backup_path("session-achievements.json");
            let _ = std::fs::copy(&path, &backup);
            SESSION_UNLOCKS_UNREADABLE.store(true, Ordering::SeqCst);
            return (
                HashMap::new(),
                Some(format!(
                    "Succès de session illisibles ({}) : {error}. Copie conservée dans {} ; le fichier ne sera pas écrasé.",
                    path.display(),
                    backup.display()
                )),
            );
        }
    };
    (
        file.games
            .into_iter()
            .filter_map(|(id, unlocks)| parse_provider_key_id(&id).map(|key| (key, unlocks)))
            .collect(),
        None,
    )
}

fn unique_backup_path(base: &str) -> PathBuf {
    let dir = crate::config::data_dir();
    let stamp = crate::config::now_ts();
    for attempt in 0..100 {
        let name = if attempt == 0 {
            format!("{base}.corrompu-{stamp}")
        } else {
            format!("{base}.corrompu-{stamp}-{attempt}")
        };
        let path = dir.join(name);
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("{base}.corrompu-{stamp}-{}", std::process::id()))
}

pub fn save_session_unlocks(
    unlocks: &HashMap<ProviderGameKey, Vec<SessionUnlock>>,
) -> Result<(), String> {
    if SESSION_UNLOCKS_UNREADABLE.load(Ordering::SeqCst) {
        return Err(
            "fichier de succès de session illisible : sauvegarde refusée pour ne pas l'écraser"
                .to_string(),
        );
    }
    let path = session_unlocks_path();
    let parent = path
        .parent()
        .ok_or_else(|| "dossier des succès de session introuvable".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let file = SessionUnlockFile {
        version: 1,
        games: unlocks
            .iter()
            .map(|(key, unlocks)| (provider_key_id(key), unlocks.clone()))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&file).map_err(|error| error.to_string())?;
    let temp = path.with_extension(format!(
        "json.tmp-{}-{}",
        std::process::id(),
        crate::config::now_ts()
    ));
    let result = (|| -> Result<(), String> {
        use std::io::Write as _;
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut output = options.open(&temp).map_err(|error| error.to_string())?;
        output
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string())?;
        output.sync_all().map_err(|error| error.to_string())?;
        std::fs::rename(&temp, &path).map_err(|error| error.to_string())?;
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Confirme uniquement le succès réellement envoyé : les autres déblocages de
/// la même session restent en attente et seront envoyés ensuite.
fn mark_synced(unlocks: &mut [crate::providers::SessionUnlock], achievement: &str) {
    if let Some(unlock) = unlocks
        .iter_mut()
        .find(|unlock| unlock.key.eq_ignore_ascii_case(achievement))
    {
        unlock.synced = true;
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
        if let Err(error) = save_session_unlocks(&self.session_unlocks) {
            self.toast_error(format!("Succès de session non enregistrés : {error}"));
        }
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
        let sent_achievement = achievement.clone();
        std::thread::spawn(move || {
            let result = crate::providers::gog_library::unlock_for_session(
                &crate::providers::gog::auth_config_path(),
                &product_id,
                &achievement,
                crate::config::now_ts(),
            );
            let _ = tx.send((sent_key, sent_achievement, result));
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
            Ok((key, achievement, Ok(Some(name)))) => {
                self.gog_unlock_job = None;
                if let Some(unlocks) = self.session_unlocks.get_mut(&key) {
                    mark_synced(unlocks, &achievement);
                }
                if let Err(error) = save_session_unlocks(&self.session_unlocks) {
                    self.toast_error(format!("Succès de session non enregistrés : {error}"));
                }
                // La liste déjà chargée pour ce jeu doit refléter l'écriture.
                self.achievements.remove(&key);
                self.toast_info(format!("Succès GOG « {name} » synchronisé"));
                self.start_gog_unlock(ctx);
            }
            Ok((key, achievement, Ok(None))) => {
                self.gog_unlock_job = None;
                // `Ok(None)` signifie que GOG connaît déjà ce succès comme
                // débloqué : seule cette clé est confirmée.
                if let Some(unlocks) = self.session_unlocks.get_mut(&key) {
                    mark_synced(unlocks, &achievement);
                }
                if let Err(error) = save_session_unlocks(&self.session_unlocks) {
                    self.toast_error(format!("Succès de session non enregistrés : {error}"));
                }
                self.start_gog_unlock(ctx);
            }
            Ok((_key, _achievement, Err(error))) => {
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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, SessionUnlock};

    fn unlock(key: &str) -> SessionUnlock {
        SessionUnlock {
            key: key.to_string(),
            name: key.to_string(),
            description: String::new(),
            at: 0,
            synced: false,
        }
    }

    #[test]
    fn un_seul_succes_gog_est_marque_synchronise() {
        let mut unlocks = vec![unlock("ACH_ONE"), unlock("ACH_TWO")];
        mark_synced(&mut unlocks, "ach_one");
        assert!(unlocks[0].synced);
        assert!(!unlocks[1].synced);
        // La clé du jeu reste distincte de celle du succès.
        let key = ProviderGameKey {
            provider: Provider::Gog,
            product_id: "1207658924".into(),
        };
        assert_eq!(provider_key_id(&key), "gog:1207658924");
    }
}
