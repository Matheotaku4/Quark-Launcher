//! Découverte de la bibliothèque GOG.
//!
//! gogdl n'expose aucune commande de liste : ses jetons servent ici à appeler
//! l'API publique de GOG, comme le fait Heroic. Les jetons ne sortent jamais de
//! ce module : les erreurs ne contiennent ni code de retour ni contenu HTTP.

use super::model::{InstallState, Provider, ProviderGame, ProviderGameKey};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Client public de GOG Galaxy (identique à gogdl et à Heroic).
pub const CLIENT_ID: &str = "46899977096215655";
pub const CLIENT_SECRET: &str = "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";

const AUTH_URL: &str = "https://auth.gog.com";
const EMBED_URL: &str = "https://embed.gog.com";
const API_URL: &str = "https://api.gog.com";
/// L'API produits accepte une liste d'identifiants ; 40 reste sous la limite
/// pratique de longueur d'URL avec une marge confortable.
const BATCH: usize = 40;
const MAX_PAGES: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GogTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub login_time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedGame {
    pub id: String,
    pub title: String,
}

fn client() -> reqwest::blocking::Client {
    crate::artwork::http()
}

fn http_timeout() -> Duration {
    Duration::from_secs(25)
}

/// Extrait les jetons du fichier écrit par gogdl.
///
/// gogdl range les sessions par `client_id` (`{"46899977096215655": {...}}`) ;
/// les anciennes versions écrivaient l'objet à la racine. Les deux formes sont
/// acceptées, la première entrée valide gagne.
pub fn read_tokens(text: &str) -> Result<GogTokens, String> {
    let root: Value =
        serde_json::from_str(text).map_err(|_| "session GOG illisible".to_string())?;
    let candidates: Vec<&Value> = match &root {
        Value::Object(map) => {
            let mut values: Vec<&Value> = vec![&root];
            if let Some(entry) = map.get(CLIENT_ID) {
                values.push(entry);
            }
            values.extend(map.values().filter(|value| value.is_object()));
            values
        }
        _ => return Err("session GOG illisible".into()),
    };
    for entry in candidates {
        let Some(object) = entry.as_object() else {
            continue;
        };
        if object.contains_key("access_token") && object.contains_key("refresh_token") {
            let access_token = object
                .get("access_token")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let refresh_token = object
                .get("refresh_token")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if access_token.is_empty() || refresh_token.is_empty() {
                continue;
            }
            return Ok(GogTokens {
                access_token,
                refresh_token,
                expires_in: object.get("expires_in").and_then(Value::as_i64).unwrap_or(0),
                login_time: object
                    .get("loginTime")
                    .and_then(Value::as_f64)
                    .map(|value| value as i64)
                    .unwrap_or(0),
            });
        }
    }
    Err("session GOG absente : reconnecte la boutique dans Paramètres → Boutiques & outils".into())
}

/// Vrai quand gogdl considérerait le jeton comme expiré, avec une minute de
/// marge pour éviter une requête qui expire en vol.
pub fn tokens_are_expired(tokens: &GogTokens, now: i64) -> bool {
    if tokens.expires_in <= 0 {
        return false;
    }
    now >= tokens.login_time + tokens.expires_in - 60
}

/// Échange le refresh token contre un access token neuf.
///
/// Retourne le JSON complet renvoyé par GOG : il contient aussi `user_id`,
/// `session_id` et un éventuel nouveau refresh token, que gogdl réutilise.
fn refresh_request(refresh_token: &str) -> Result<Value, String> {
    let response = client()
        .get(format!("{AUTH_URL}/token"))
        .query(&[
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .timeout(http_timeout())
        .send()
        .map_err(|_| "GOG injoignable pendant le renouvellement de session".to_string())?;
    if !response.status().is_success() {
        return Err(
            "GOG a refusé la session enregistrée : reconnecte la boutique dans Paramètres → Boutiques & outils"
                .into(),
        );
    }
    let body: Value = response
        .json()
        .map_err(|_| "réponse GOG illisible pendant le renouvellement de session".to_string())?;
    if body
        .get("access_token")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(
            "GOG n'a pas renvoyé de session valide : reconnecte la boutique dans Paramètres → Boutiques & outils"
                .into(),
        );
    }
    Ok(body)
}

/// Remplace l'entrée de session dans le fichier gogdl, en conservant le reste.
pub fn merge_refreshed(text: &str, refreshed: &Value, login_time: i64) -> Result<String, String> {
    let mut root: Value =
        serde_json::from_str(text).map_err(|_| "session GOG illisible".to_string())?;
    let Value::Object(map) = &mut root else {
        return Err("session GOG illisible".into());
    };
    let key = if map.contains_key(CLIENT_ID) {
        CLIENT_ID.to_string()
    } else {
        map.keys()
            .find(|key| map.get(*key).is_some_and(Value::is_object))
            .cloned()
            .unwrap_or_else(|| CLIENT_ID.to_string())
    };
    let entry = map
        .entry(key)
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Value::Object(entry) = entry else {
        return Err("session GOG illisible".into());
    };
    for field in ["access_token", "expires_in", "scope", "token_type"] {
        if let Some(value) = refreshed.get(field) {
            entry.insert(field.to_string(), value.clone());
        }
    }
    // GOG ne renvoie pas toujours un nouveau refresh token : on garde l'ancien.
    if let Some(value) = refreshed.get("refresh_token") {
        entry.insert("refresh_token".to_string(), value.clone());
    }
    for field in ["user_id", "session_id"] {
        if let Some(value) = refreshed.get(field) {
            entry.insert(field.to_string(), value.clone());
        }
    }
    entry.insert("loginTime".to_string(), Value::from(login_time));
    serde_json::to_string(&root).map_err(|_| "session GOG non enregistrable".to_string())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let nonce = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(
        ".quark-gog-session-{}-{nonce}.tmp",
        std::process::id()
    ));
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
    };
    #[cfg(not(unix))]
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp);
    let mut file = match file {
        Ok(file) => file,
        Err(_) => return Err("impossible d'écrire la session GOG".to_string()),
    };
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err("impossible d'écrire la session GOG".to_string());
    }
    std::fs::rename(&temp, path).map_err(|_| {
        let _ = std::fs::remove_file(&temp);
        "impossible de remplacer la session GOG".to_string()
    })
}

/// Session utilisable pour interroger la bibliothèque, renouvelée et
/// enregistrée dans le fichier gogdl si nécessaire.
pub fn usable_tokens(auth_path: &Path, now: i64) -> Result<GogTokens, String> {
    let text = std::fs::read_to_string(auth_path).map_err(|_| {
        "session GOG absente : connecte la boutique dans Paramètres → Boutiques & outils".to_string()
    })?;
    let tokens = read_tokens(&text)?;
    if !tokens_are_expired(&tokens, now) {
        return Ok(tokens);
    }
    let refreshed = refresh_request(&tokens.refresh_token)?;
    let merged = merge_refreshed(&text, &refreshed, now)?;
    write_atomic(auth_path, merged.as_bytes())?;
    read_tokens(&merged)
}

/// Identifiants des jeux possédés (`{"owned": [...]}`).
pub fn parse_owned_ids(body: &str) -> Result<Vec<String>, String> {
    let root: Value =
        serde_json::from_str(body).map_err(|_| "réponse GOG illisible".to_string())?;
    let owned = root
        .get("owned")
        .and_then(Value::as_array)
        .ok_or_else(|| "réponse GOG illisible".to_string())?;
    let mut ids = Vec::with_capacity(owned.len());
    for entry in owned {
        let id = match entry {
            Value::Number(number) => number.to_string(),
            Value::String(text) => text.clone(),
            _ => continue,
        };
        if !id.is_empty() && id.chars().all(|character| character.is_ascii_digit()) {
            ids.push(id);
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Produits renvoyés par `api.gog.com/products?ids=…` (ou par la liste paginée).
pub fn parse_products(body: &str) -> Result<Vec<OwnedGame>, String> {
    let root: Value =
        serde_json::from_str(body).map_err(|_| "fiches GOG illisibles".to_string())?;
    let entries = match &root {
        Value::Array(entries) => entries.clone(),
        Value::Object(map) => map
            .get("products")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| "fiches GOG illisibles".to_string())?,
        _ => return Err("fiches GOG illisibles".into()),
    };
    let mut games = Vec::new();
    for entry in entries {
        // Les packs et DLC partagent la liste des produits possédés : ils ne
        // sont pas des jeux lançables et n'ont pas leur place dans la grille.
        let kind = entry
            .get("game_type")
            .and_then(Value::as_str)
            .unwrap_or("game");
        if kind == "dlc" {
            continue;
        }
        let id = match entry.get("id") {
            Some(Value::Number(number)) => number.to_string(),
            Some(Value::String(text)) => text.clone(),
            _ => continue,
        };
        let title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if title.is_empty() || id.is_empty() {
            continue;
        }
        games.push(OwnedGame { id, title });
    }
    games.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
    games.dedup_by(|left, right| left.id == right.id);
    Ok(games)
}

fn authorized_get(url: &str, token: &str) -> Result<reqwest::blocking::Response, String> {
    client()
        .get(url)
        .bearer_auth(token)
        .timeout(http_timeout())
        .send()
        .map_err(|_| "GOG injoignable pendant la synchronisation".to_string())
}

fn describe_status(status: reqwest::StatusCode) -> String {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        "GOG a refusé la session enregistrée : reconnecte la boutique dans Paramètres → Boutiques & outils"
            .to_string()
    } else {
        format!("GOG a répondu {status} pendant la synchronisation")
    }
}

fn owned_ids(token: &str) -> Result<Vec<String>, String> {
    let response = authorized_get(&format!("{EMBED_URL}/user/data/games"), token)?;
    if !response.status().is_success() {
        return Err(describe_status(response.status()));
    }
    let body = response
        .text()
        .map_err(|_| "réponse GOG illisible".to_string())?;
    parse_owned_ids(&body)
}

/// Liste paginée du compte : utilisée seulement si la liste d'identifiants est
/// indisponible, car elle demande un en-tête d'autorisation par page.
fn filtered_products(token: &str) -> Result<Vec<OwnedGame>, String> {
    let mut games = Vec::new();
    let mut page = 1;
    loop {
        let response = authorized_get(
            &format!("{EMBED_URL}/account/getFilteredProducts?mediaType=1&page={page}"),
            token,
        )?;
        if !response.status().is_success() {
            return Err(describe_status(response.status()));
        }
        let body = response
            .text()
            .map_err(|_| "réponse GOG illisible".to_string())?;
        let root: Value =
            serde_json::from_str(&body).map_err(|_| "réponse GOG illisible".to_string())?;
        let entries = root
            .get("products")
            .and_then(Value::as_array)
            .ok_or_else(|| "réponse GOG illisible".to_string())?;
        for entry in entries {
            let id = match entry.get("id") {
                Some(Value::Number(number)) => number.to_string(),
                Some(Value::String(text)) => text.clone(),
                _ => continue,
            };
            let title = entry
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let is_movie = entry
                .get("isMovie")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if title.is_empty() || id.is_empty() || is_movie {
                continue;
            }
            games.push(OwnedGame { id, title });
        }
        let total_pages = root
            .get("totalPages")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if u64::from(page) >= total_pages || page >= MAX_PAGES || entries.is_empty() {
            break;
        }
        page += 1;
    }
    games.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
    games.dedup_by(|left, right| left.id == right.id);
    Ok(games)
}

/// Titres des jeux possédés, par lots d'identifiants.
pub fn library(token: &str) -> Result<Vec<OwnedGame>, String> {
    let ids = match owned_ids(token) {
        Ok(ids) if !ids.is_empty() => ids,
        Ok(_) => {
            return Err(
                "bibliothèque GOG vide : aucune possession renvoyée par le compte".to_string(),
            );
        }
        Err(error) => {
            // Repli : la liste paginée reste disponible même quand l'endpoint
            // d'identifiants change de forme.
            let games = filtered_products(token)?;
            if games.is_empty() {
                return Err(error);
            }
            return Ok(games);
        }
    };
    let mut games = Vec::with_capacity(ids.len());
    for batch in ids.chunks(BATCH) {
        let query = batch.join(",");
        let response = client()
            .get(format!("{API_URL}/products"))
            .query(&[("ids", query.as_str()), ("locale", "en-US")])
            .timeout(http_timeout())
            .send()
            .map_err(|_| "GOG injoignable pendant la lecture des fiches".to_string())?;
        if !response.status().is_success() {
            return Err(describe_status(response.status()));
        }
        let body = response
            .text()
            .map_err(|_| "fiches GOG illisibles".to_string())?;
        games.extend(parse_products(&body)?);
    }
    games.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
    games.dedup_by(|left, right| left.id == right.id);
    if games.is_empty() {
        return Err("aucune fiche GOG exploitable pour ce compte".to_string());
    }
    Ok(games)
}

pub fn to_games(entries: &[OwnedGame], fetched_at: i64) -> Vec<ProviderGame> {
    entries
        .iter()
        .map(|entry| ProviderGame {
            key: ProviderGameKey {
                provider: Provider::Gog,
                product_id: entry.id.clone(),
            },
            title: entry.title.clone(),
            owned: true,
            install_path: None,
            custom_save_paths: Vec::new(),
            install_state: InstallState::NotInstalled,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: Some(fetched_at),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = r#"{
      "46899977096215655": {
        "access_token": "access-1",
        "expires_in": 3600,
        "token_type": "bearer",
        "scope": "",
        "session_id": "123",
        "user_id": "456",
        "refresh_token": "refresh-1",
        "loginTime": 1700000000.5
      }
    }"#;

    #[test]
    fn session_du_fichier_gogdl_est_lue_par_client() {
        let tokens = read_tokens(SESSION).unwrap();
        assert_eq!(tokens.access_token, "access-1");
        assert_eq!(tokens.refresh_token, "refresh-1");
        assert_eq!(tokens.login_time, 1_700_000_000);
        assert_eq!(tokens.expires_in, 3600);
    }

    #[test]
    fn session_plate_et_session_absente_sont_distinguees() {
        let flat = r#"{"access_token":"a","refresh_token":"r","expires_in":3600,"loginTime":1}"#;
        assert_eq!(read_tokens(flat).unwrap().access_token, "a");
        assert!(read_tokens("{}").unwrap_err().contains("absente"));
        assert!(read_tokens("pas du json").is_err());
        // Un refus de gogdl (`{"error": true}`) n'est pas une session.
        assert!(read_tokens(r#"{"error": true}"#).unwrap_err().contains("absente"));
    }

    #[test]
    fn expiration_suit_login_time_et_expires_in() {
        let tokens = read_tokens(SESSION).unwrap();
        assert!(!tokens_are_expired(&tokens, 1_700_000_100));
        assert!(!tokens_are_expired(&tokens, 1_700_003_000));
        assert!(tokens_are_expired(&tokens, 1_700_003_541));
        let mut legacy = tokens.clone();
        legacy.expires_in = 0;
        assert!(!tokens_are_expired(&legacy, 2_000_000_000));
    }

    #[test]
    fn renouvellement_conserve_le_reste_du_fichier() {
        let refreshed = serde_json::json!({
            "access_token": "access-2",
            "refresh_token": "refresh-2",
            "expires_in": 3600,
            "token_type": "bearer",
            "user_id": "456",
            "session_id": "789"
        });
        let merged = merge_refreshed(SESSION, &refreshed, 1_800_000_000).unwrap();
        let tokens = read_tokens(&merged).unwrap();
        assert_eq!(tokens.access_token, "access-2");
        assert_eq!(tokens.refresh_token, "refresh-2");
        assert_eq!(tokens.login_time, 1_800_000_000);
        let root: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(
            root[CLIENT_ID].get("user_id").and_then(Value::as_str),
            Some("456")
        );
    }

    #[test]
    fn identifiants_possedes_sont_numeriques_tries_et_uniques() {
        let body = r#"{"owned":[1207658924,"1423049311",1423049311,"x",null,7]}"#;
        assert_eq!(
            parse_owned_ids(body).unwrap(),
            vec!["1207658924", "1423049311", "7"]
        );
        assert!(parse_owned_ids(r#"{"owned":"nope"}"#).is_err());
        assert!(parse_owned_ids("[]").is_err());
    }

    #[test]
    fn fiches_produits_ignorent_les_dlc_et_trient_par_titre() {
        let body = r#"[
          {"id": 1, "title": "Zebra", "game_type": "game"},
          {"id": 2, "title": "alpha", "game_type": "game"},
          {"id": 3, "title": "DLC", "game_type": "dlc"},
          {"id": 4, "title": "   ", "game_type": "game"}
        ]"#;
        let games = parse_products(body).unwrap();
        assert_eq!(
            games,
            vec![
                OwnedGame {
                    id: "2".into(),
                    title: "alpha".into()
                },
                OwnedGame {
                    id: "1".into(),
                    title: "Zebra".into()
                }
            ]
        );
        // La liste paginée arrive sous la clé `products`.
        let page = r#"{"totalPages":1,"products":[{"id":"9","title":"Page Game"}]}"#;
        assert_eq!(
            parse_products(page).unwrap(),
            vec![OwnedGame {
                id: "9".into(),
                title: "Page Game".into()
            }]
        );
        assert!(parse_products("nope").is_err());
    }

    #[test]
    fn conversion_produit_une_entree_gog_sans_etat_d_installation() {
        let games = to_games(
            &[OwnedGame {
                id: "42".into(),
                title: "Game".into(),
            }],
            99,
        );
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].key.provider, Provider::Gog);
        assert_eq!(games[0].key.product_id, "42");
        assert_eq!(games[0].install_state, InstallState::NotInstalled);
        assert_eq!(games[0].last_synced, Some(99));
    }

    #[test]
    fn l_ecriture_de_session_remplace_le_fichier_avec_les_droits_utilisateur() {
        let root = std::env::temp_dir().join(format!(
            "quark-gog-session-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("gogdl-auth.json");
        write_atomic(&path, SESSION.as_bytes()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SESSION);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
