//! Adaptateur Steam : manifestes locaux et accès optionnel à GetOwnedGames.

use crate::config::Game;
use crate::providers::{InstallState, Provider, ProviderGame, ProviderGameKey};
use reqwest::blocking::Client;
use serde::Deserialize;

const OWNED_GAMES_URL: &str = "https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/";
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Deserialize)]
struct OwnedResponse {
    response: OwnedPayload,
}

#[derive(Deserialize)]
struct OwnedPayload {
    game_count: usize,
    #[serde(default)]
    games: Vec<OwnedEntry>,
}

#[derive(Deserialize)]
struct OwnedEntry {
    appid: u32,
    #[serde(default)]
    name: String,
}

/// Convertit les jeux découverts sur disque en enregistrements provider.
/// La fiche Quark n'est ni modifiée ni remplacée ; le lien est uniquement AppID.
pub fn installed_games(
    steam_games: &[crate::steamlib::SteamGame],
    quark_games: &[Game],
) -> Vec<ProviderGame> {
    let mut result: Vec<ProviderGame> = Vec::new();
    for steam in steam_games {
        if steam.app_id == 0
            || result
                .iter()
                .any(|game| game.key.product_id == steam.app_id.to_string())
        {
            continue;
        }
        let appid = steam.app_id.to_string();
        let linked = quark_games
            .iter()
            .find(|game| game.steam_app_id == Some(steam.app_id));
        result.push(ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Steam,
                product_id: appid,
            },
            title: steam.name.clone(),
            owned: true,
            install_path: Some(steam.dir.clone()),
            install_state: InstallState::Installed,
            quark_game_id: linked.map(|game| game.id.clone()),
            capabilities: Default::default(),
            last_synced: Some(crate::config::now_ts()),
        });
    }
    result.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
    result
}

/// Parse l'API utilisateur de Steam. `GetOwnedGames` renvoie parfois une liste
/// vide lorsque la confidentialité masque les détails ; cette réponse n'est
/// pas considérée comme une bibliothèque confirmée vide.
pub fn parse_owned(json: &str) -> Result<Vec<ProviderGame>, String> {
    if json.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("réponse de bibliothèque Steam trop volumineuse".to_string());
    }
    let payload: OwnedResponse = serde_json::from_str(json)
        .map_err(|_| "réponse de bibliothèque Steam invalide".to_string())?;
    if payload.response.game_count == 0 || payload.response.games.is_empty() {
        return Err(
            "bibliothèque Steam vide ou détails privés : résultat non vérifiable".to_string(),
        );
    }
    if payload.response.game_count != payload.response.games.len() {
        return Err("réponse de bibliothèque Steam incomplète".to_string());
    }
    let mut result = Vec::with_capacity(payload.response.games.len());
    for entry in payload.response.games {
        if entry.appid == 0 {
            return Err("AppID nul dans la bibliothèque Steam".to_string());
        }
        let title = entry.name.trim();
        if title.is_empty() {
            return Err("nom de jeu absent dans la bibliothèque Steam".to_string());
        }
        if result
            .iter()
            .any(|game: &ProviderGame| game.key.product_id == entry.appid.to_string())
        {
            continue;
        }
        result.push(ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Steam,
                product_id: entry.appid.to_string(),
            },
            title: title.to_string(),
            owned: true,
            install_path: None,
            install_state: InstallState::Unknown,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: Some(crate::config::now_ts()),
        });
    }
    Ok(result)
}

/// Fusionne les résultats Steam avec les enregistrements déjà stockés.
/// Une réponse API absente/échouée (`owned == None`) ne supprime jamais la
/// dernière bibliothèque connue. Les autres boutiques ne sont jamais modifiées.
pub fn merge_sync(
    existing: &[ProviderGame],
    local: &[ProviderGame],
    owned: Option<&[ProviderGame]>,
    quark_games: &[Game],
) -> Vec<ProviderGame> {
    let mut merged: std::collections::BTreeMap<String, ProviderGame> = existing
        .iter()
        .filter(|game| game.key.provider != Provider::Steam)
        .cloned()
        .map(|game| {
            (
                format!("{:?}:{}", game.key.provider, game.key.product_id),
                game,
            )
        })
        .collect();
    let mut steam: std::collections::BTreeMap<String, ProviderGame> =
        std::collections::BTreeMap::new();

    let sources: Vec<&ProviderGame> = match owned {
        Some(records) => records.iter().chain(local.iter()).collect(),
        None => existing
            .iter()
            .filter(|game| game.key.provider == Provider::Steam)
            .chain(local.iter())
            .collect(),
    };
    for incoming in sources {
        if incoming.key.provider != Provider::Steam {
            continue;
        }
        let appid = &incoming.key.product_id;
        let mut record = incoming.clone();
        if let Some(old) = existing
            .iter()
            .find(|old| old.key.provider == Provider::Steam && old.key.product_id == *appid)
        {
            if record.install_path.is_none() {
                record.install_path = old.install_path.clone();
            }
            if record.install_state != InstallState::Installed {
                record.install_state = old.install_state;
            }
            if record.quark_game_id.is_none() {
                record.quark_game_id = old.quark_game_id.clone();
            }
            if record.last_synced.is_none() {
                record.last_synced = old.last_synced;
            }
        }
        if let Some(game) = quark_games.iter().find(|game| {
            game.steam_app_id
                .and_then(|id| (id.to_string() == *appid).then_some(id))
                .is_some()
        }) {
            record.quark_game_id = Some(game.id.clone());
        }
        match steam.get(appid) {
            Some(previous)
                if previous.install_state == InstallState::Installed
                    && record.install_state != InstallState::Installed => {}
            _ => {
                steam.insert(appid.clone(), record);
            }
        }
    }
    merged.extend(
        steam
            .into_iter()
            .map(|(id, game)| (format!("Steam:{id}"), game)),
    );
    merged.into_values().collect()
}

/// Appelle uniquement le host HTTPS public, avec une clé utilisateur locale.
pub fn owned(client: &Client, steam_id: &str, api_key: &str) -> Result<Vec<ProviderGame>, String> {
    if !valid_steam_id(steam_id) {
        return Err("SteamID64 invalide".to_string());
    }
    if api_key.trim().is_empty() {
        return Err("clé Steam Web API absente".to_string());
    }
    let response = client
        .get(OWNED_GAMES_URL)
        .query(&[
            ("key", api_key),
            ("steamid", steam_id),
            ("include_appinfo", "1"),
            ("include_played_free_games", "1"),
        ])
        .send()
        .map_err(|_| "connexion à Steam Web API impossible".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Steam Web API a répondu {}",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    use std::io::Read;
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "lecture de la bibliothèque Steam impossible".to_string())?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("réponse de bibliothèque Steam trop volumineuse".to_string());
    }
    let json = std::str::from_utf8(&bytes).map_err(|_| "réponse Steam non UTF-8".to_string())?;
    parse_owned(json)
}

pub fn valid_steam_id(id: &str) -> bool {
    id.len() == 17
        && id.bytes().all(|byte| byte.is_ascii_digit())
        && id
            .parse::<u64>()
            .is_ok_and(|value| value >= 76_561_197_960_265_728)
}

pub fn install_uri(app_id: u32) -> Result<String, String> {
    if app_id == 0 {
        return Err("AppID Steam invalide".to_string());
    }
    Ok(format!("steam://install/{app_id}"))
}

/// Demande au client Steam installé de démarrer l'installation.
/// Le succès de l'ouverture ne constitue pas une preuve d'installation.
pub fn request_install(app_id: u32) -> Result<(), String> {
    let uri = install_uri(app_id)?;
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(uri)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| "impossible d'ouvrir Steam (xdg-open indisponible)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn jeux_installes_sont_lies_par_appid_sans_ecraser_la_fiche_quark() {
        let mut quark = Game::new("Renommé dans Quark", "/games/portal2/portal2.exe");
        quark.steam_app_id = Some(620);
        quark.artwork.hero = Some(PathBuf::from("/artwork/custom-hero.jpg"));
        let duplicates = vec![
            crate::steamlib::SteamGame {
                app_id: 620,
                name: "Portal 2".to_string(),
                dir: PathBuf::from("/steam/common/Portal 2"),
                exe: None,
            },
            crate::steamlib::SteamGame {
                app_id: 620,
                name: "Portal 2 duplicate".to_string(),
                dir: PathBuf::from("/other/Portal 2"),
                exe: None,
            },
        ];
        let result = installed_games(&duplicates, &[quark.clone()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key.provider, Provider::Steam);
        assert_eq!(result[0].key.product_id, "620");
        assert_eq!(result[0].title, "Portal 2");
        assert_eq!(
            result[0].install_path,
            Some(PathBuf::from("/steam/common/Portal 2"))
        );
        assert_eq!(result[0].install_state, InstallState::Installed);
        assert_eq!(result[0].quark_game_id.as_deref(), Some(quark.id.as_str()));
        assert_eq!(quark.name, "Renommé dans Quark");
        assert_eq!(
            quark.artwork.hero,
            Some(PathBuf::from("/artwork/custom-hero.jpg"))
        );
    }

    #[test]
    fn api_steam_parse_les_jeux_et_refuse_la_reponse_privee_ambigue() {
        let json = r#"{"response":{"game_count":1,"games":[{"appid":620,"name":"Portal 2","playtime_forever":42}]}}"#;
        let games = parse_owned(json).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].key.product_id, "620");
        assert_eq!(games[0].title, "Portal 2");
        assert!(games[0].owned);
        assert_eq!(games[0].install_state, InstallState::Unknown);
        let ambiguous = r#"{"response":{"game_count":0,"games":[]}}"#;
        assert!(parse_owned(ambiguous).is_err());
        assert!(parse_owned(r#"{"response":{}}"#).is_err());
    }

    #[test]
    fn echec_api_preserve_la_bibliotheque_connue_et_les_autres_fournisseurs() {
        let steam = ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Steam,
                product_id: "620".into(),
            },
            title: "Portal 2".into(),
            owned: true,
            install_path: Some(PathBuf::from("/steam/Portal 2")),
            install_state: InstallState::Installed,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: Some(1),
        };
        let epic = ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Epic,
                product_id: "epic-portal".into(),
            },
            title: "Portal".into(),
            owned: true,
            install_path: None,
            install_state: InstallState::Unknown,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: None,
        };
        let merged = merge_sync(&[steam.clone(), epic.clone()], &[], None, &[]);
        assert_eq!(merged.len(), 2);
        assert!(
            merged
                .iter()
                .any(|game| game.key == steam.key && game.install_path == steam.install_path)
        );
        assert!(merged.iter().any(|game| game.key == epic.key));
    }

    #[test]
    fn steamid_et_uri_installation_sont_valides_separement() {
        assert!(valid_steam_id("76561198000000000"));
        assert!(!valid_steam_id("123"));
        assert!(!valid_steam_id("7656119800000000x"));
        assert!(valid_steam_id("76561197960265728"));
        assert_eq!(install_uri(620).unwrap(), "steam://install/620");
        assert!(install_uri(0).is_err());
    }
}
