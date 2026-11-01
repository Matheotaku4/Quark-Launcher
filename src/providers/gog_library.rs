//! Découverte de la bibliothèque GOG.
//!
//! gogdl n'expose aucune commande de liste : ses jetons servent ici à appeler
//! l'API publique de GOG, comme le fait Heroic. Les jetons ne sortent jamais de
//! ce module : les erreurs ne contiennent ni code de retour ni contenu HTTP.

use super::achievements::{AchievementInfo, AchievementRow};
use super::model::{
    CapabilityState, GameCapabilities, InstallState, Provider, ProviderGame, ProviderGameKey,
};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Client public de GOG Galaxy (identique à gogdl et à Heroic).
pub const CLIENT_ID: &str = "46899977096215655";
pub const CLIENT_SECRET: &str = "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";

const AUTH_URL: &str = "https://auth.gog.com";
const EMBED_URL: &str = "https://embed.gog.com";
const API_URL: &str = "https://api.gog.com";
const GAMEPLAY_URL: &str = "https://gameplay.gog.com";
const LOCALE: &str = "en-US";
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
    /// Identifiant du compte GOG, exigé par l'API des succès.
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedGame {
    pub id: String,
    pub title: String,
    /// Laissée vide : GOG ne publie pas de jaquette dans la fiche produit.
    pub cover_url: Option<String>,
}

fn client() -> reqwest::blocking::Client {
    crate::artwork::http()
}

fn http_timeout() -> Duration {
    Duration::from_secs(25)
}

/// URL de jaquette verticale à partir de l'image de boîtier (`boxArtImage`)
/// publiée par l'API v2. La jaquette `glx_vertical_cover` est un dérivé du même
/// identifiant : c'est la seule forme qui fonctionne. L'ancienne déduction
/// depuis `images.background` produisait une URL valide… pour une autre image,
/// d'où des jaquettes qui ne correspondaient pas au jeu.
pub fn cover_from_boxart(href: &str) -> Option<String> {
    let href = href.trim();
    let absolute = if href.starts_with("//") {
        format!("https:{href}")
    } else if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else {
        return None;
    };
    let base = absolute
        .rsplit_once('.')
        .filter(|(_, extension)| extension.len() <= 4)
        .map(|(base, _)| base)
        .unwrap_or(&absolute);
    Some(format!("{base}_glx_vertical_cover.webp"))
}

fn parse_cover(body: &str) -> Result<String, String> {
    let root: Value = serde_json::from_str(body).map_err(|_| "fiche GOG illisible".to_string())?;
    let links = root
        .get("_links")
        .ok_or_else(|| "fiche GOG sans images".to_string())?;
    if let Some(href) = links
        .get("boxArtImage")
        .and_then(|image| image.get("href"))
        .and_then(Value::as_str)
    {
        if let Some(url) = cover_from_boxart(href) {
            return Ok(url);
        }
    }
    // Repli : l'illustration large reste la bonne œuvre, contrairement à une
    // jaquette devinée à partir d'un autre identifiant.
    links
        .get("backgroundImage")
        .and_then(|image| image.get("href"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .map(str::to_string)
        .ok_or_else(|| "fiche GOG sans couverture".to_string())
}

fn parse_hero(body: &str) -> Result<String, String> {
    let root: Value = serde_json::from_str(body).map_err(|_| "fiche GOG illisible".to_string())?;
    root.get("_links")
        .and_then(|links| links.get("backgroundImage"))
        .and_then(|image| image.get("href"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .map(str::to_string)
        .ok_or_else(|| "fiche GOG sans illustration large".to_string())
}

fn product_body(product_id: &str) -> Result<String, String> {
    if product_id.is_empty() || !product_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("identifiant GOG invalide".to_string());
    }
    let response = client()
        .get(format!("{API_URL}/v2/games/{product_id}"))
        .query(&[("locale", LOCALE)])
        .timeout(http_timeout())
        .send()
        .map_err(|_| "GOG injoignable pour les images".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "GOG a refusé la fiche du jeu (HTTP {})",
            response.status().as_u16()
        ));
    }
    response
        .text()
        .map_err(|_| "fiche GOG illisible".to_string())
}

/// Jaquette verticale d'un jeu possédé, demandée à l'API v2 (publique, sans
/// session).
pub fn cover_url(product_id: &str) -> Result<String, String> {
    let body = product_body(product_id)?;
    parse_cover(&body)
}

/// Illustration large (bannière) d'un jeu GOG.
pub fn hero_url(product_id: &str) -> Result<String, String> {
    let body = product_body(product_id)?;
    parse_hero(&body)
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
            let user_id = object
                .get("user_id")
                .map(|value| match value {
                    Value::Number(number) => number.to_string(),
                    Value::String(text) => text.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();
            return Ok(GogTokens {
                access_token,
                refresh_token,
                expires_in: object
                    .get("expires_in")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                login_time: object
                    .get("loginTime")
                    .and_then(Value::as_f64)
                    .map(|value| value as i64)
                    .unwrap_or(0),
                user_id,
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
        "session GOG absente : connecte la boutique dans Paramètres → Boutiques & outils"
            .to_string()
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
        games.push(OwnedGame {
            id,
            title,
            cover_url: None,
        });
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
            games.push(OwnedGame {
                id,
                title,
                cover_url: None,
            });
        }
        let total_pages = root.get("totalPages").and_then(Value::as_u64).unwrap_or(1);
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

// ---------------------------------------------------------------------------
// Succès
// ---------------------------------------------------------------------------

/// Lit la liste renvoyée par `gameplay.gog.com` : les succès masqués restent
/// cachés tant qu'ils ne sont pas débloqués.
pub fn parse_achievements(body: &str) -> Result<AchievementInfo, String> {
    let root: Value =
        serde_json::from_str(body).map_err(|_| "succès GOG illisibles".to_string())?;
    let items = root
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| "succès GOG illisibles".to_string())?;
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        let key = item
            .get("achievement_key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() && key.is_empty() {
            continue;
        }
        let unlocked = item
            .get("date_unlocked")
            .map(|value| !value.is_null())
            .unwrap_or(false);
        let visible = item.get("visible").and_then(Value::as_bool).unwrap_or(true);
        if !visible && !unlocked {
            continue;
        }
        let icon_url = item
            .get(if unlocked {
                "image_url_unlocked"
            } else {
                "image_url_locked"
            })
            .or_else(|| item.get("image_url_unlocked"))
            .or_else(|| item.get("image_url_locked"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
            .map(str::to_string);
        rows.push(AchievementRow {
            key: if key.is_empty() { name.clone() } else { key },
            id: item
                .get("achievement_id")
                .map(|value| match value {
                    Value::Number(number) => number.to_string(),
                    Value::String(text) => text.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default(),
            name: if name.is_empty() {
                "Succès masqué".to_string()
            } else {
                name
            },
            description: item
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
            unlocked,
            progress: if unlocked { 1.0 } else { 0.0 },
            session: false,
            rarity: super::achievements::rarity_percent(item.get("rarity")),
            icon_url,
            icon_path: None,
        });
    }
    let mut info = AchievementInfo {
        total: root
            .get("total_count")
            .and_then(Value::as_u64)
            .map(|value| value as u32)
            .unwrap_or(rows.len() as u32),
        unlocked: 0,
        rows,
    };
    info.normalize();
    Ok(info)
}

fn gameplay_get(url: &str, token: &str) -> Result<reqwest::blocking::Response, String> {
    client()
        .get(url)
        .bearer_auth(token)
        .header("X-Gog-Lc", LOCALE)
        .timeout(http_timeout())
        .send()
        .map_err(|_| "GOG injoignable pendant la lecture des succès".to_string())
}

/// Succès d'un jeu, lus avec la session gogdl. `product_id` est l'identifiant
/// du jeu ; GOG l'utilise aussi comme identifiant de client pour ses succès.
pub fn fetch_achievements(
    product_id: &str,
    token: &str,
    user_id: &str,
) -> Result<AchievementInfo, String> {
    if user_id.trim().is_empty() {
        return Err(
            "session GOG sans identifiant de compte : reconnecte la boutique dans \
             Paramètres → Boutiques & outils"
                .to_string(),
        );
    }
    let response = gameplay_get(
        &format!("{GAMEPLAY_URL}/clients/{product_id}/users/{user_id}/achievements"),
        token,
    )?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 | 403 => "GOG a refusé la session pour les succès : reconnecte la boutique dans \
                 Paramètres → Boutiques & outils"
                .to_string(),
            status => format!("GOG a répondu {status} pour les succès"),
        });
    }
    let body = response
        .text()
        .map_err(|_| "succès GOG illisibles".to_string())?;
    parse_achievements(&body)
}

/// Nom d'utilisateur GOG du compte connecté, nécessaire à la page de profil
/// publique qui publie le temps de jeu total.
fn gog_username(tokens: &GogTokens) -> Result<String, String> {
    let response = client()
        .get(format!("https://users.gog.com/users/{}", tokens.user_id))
        .bearer_auth(&tokens.access_token)
        .timeout(http_timeout())
        .send()
        .map_err(|_| "profil GOG injoignable pour le temps de jeu".to_string())?;
    if !response.status().is_success() {
        return Err("GOG a refusé le profil du compte pour le temps de jeu".to_string());
    }
    let body = response
        .text()
        .map_err(|_| "profil GOG illisible".to_string())?;
    let root: Value =
        serde_json::from_str(&body).map_err(|_| "profil GOG illisible".to_string())?;
    root.get("username")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "profil GOG sans nom d'utilisateur".to_string())
}

fn encode_path_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Temps de jeu publié sur la page de profil publique du compte.
///
/// `gameplay.gog.com` ne documente pas de lecture agrégée des sessions ; la
/// page `/u/{pseudo}/game/{id}` publie en revanche `matchedGameStats`, avec
/// `playtime` en minutes. C'est la valeur affichée par GOG GALAXY, y compris
/// pour les parties commencées sur Windows.
fn profile_playtime(tokens: &GogTokens, product_id: &str) -> Result<u64, String> {
    let username = gog_username(tokens)?;
    let response = client()
        .get(format!(
            "https://www.gog.com/u/{}/game/{}",
            encode_path_segment(&username),
            product_id
        ))
        .header("X-Gog-Lc", LOCALE)
        .timeout(http_timeout())
        .send()
        .map_err(|_| "profil GOG injoignable pour le temps de jeu".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "GOG a répondu {} pour le temps de jeu",
            response.status().as_u16()
        ));
    }
    let body = response
        .text()
        .map_err(|_| "profil GOG illisible pour le temps de jeu".to_string())?;
    parse_profile_playtime(&body, &tokens.user_id)
}

/// Lit `window.profilesData.matchedGameStats = {"<user_id>": ...}`.
/// La valeur est un objet `{"playtime": minutes}` ou un tableau (vide si le
/// compte n'a jamais lancé le jeu).
pub fn parse_profile_playtime(html: &str, user_id: &str) -> Result<u64, String> {
    let marker = "matchedGameStats = ";
    let start = html
        .find(marker)
        .map(|index| index + marker.len())
        .ok_or_else(|| "GOG ne publie pas le temps de jeu sur ce profil".to_string())?;
    let rest = &html[start..];
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut end = None;
    for (index, character) in rest.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(index + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or_else(|| "temps de jeu GOG illisible".to_string())?;
    let root: Value =
        serde_json::from_str(&rest[..end]).map_err(|_| "temps de jeu GOG illisible".to_string())?;
    let Some(stats) = root.get(user_id) else {
        return Err("GOG ne publie pas le temps de jeu de ce compte".to_string());
    };
    let minutes = match stats {
        Value::Object(map) => map.get("playtime").and_then(Value::as_u64).unwrap_or(0),
        Value::Array(items) => items
            .iter()
            .find_map(|item| item.get("playtime").and_then(Value::as_u64))
            .unwrap_or(0),
        _ => 0,
    };
    Ok(minutes.saturating_mul(60))
}

/// Temps de jeu GOG, reconstruit à partir des sessions Galaxy du compte.
/// Chaque session publie `time` en minutes ; les formes `seconds` et
/// `duration` sont acceptées au cas où GOG change d'unité.
pub fn gog_playtime(auth_path: &Path, product_id: &str, now: i64) -> Result<u64, String> {
    if product_id.trim().is_empty() {
        return Err("identifiant GOG vide".to_string());
    }
    let tokens = usable_tokens(auth_path, now)?;
    if tokens.user_id.trim().is_empty() {
        return Err(
            "session GOG sans identifiant de compte : reconnecte la boutique dans \
             Paramètres → Boutiques & outils"
                .to_string(),
        );
    }
    let urls = [
        format!(
            "{GAMEPLAY_URL}/clients/{product_id}/users/{}/sessions",
            tokens.user_id
        ),
        format!(
            "{GAMEPLAY_URL}/games/{product_id}/users/{}/sessions",
            tokens.user_id
        ),
    ];
    let mut last_error = match profile_playtime(&tokens, product_id) {
        Ok(seconds) => return Ok(seconds),
        Err(error) => error,
    };
    for url in urls {
        let response = gameplay_get(&url, &tokens.access_token)?;
        if !response.status().is_success() {
            last_error = match response.status().as_u16() {
                401 | 403 => {
                    "GOG a refusé la session pour le temps de jeu : reconnecte la boutique \
                             dans Paramètres → Boutiques & outils"
                        .to_string()
                }
                status => format!("GOG a répondu {status} pour le temps de jeu"),
            };
            continue;
        }
        let body = response
            .text()
            .map_err(|_| "temps de jeu GOG illisible".to_string())?;
        match parse_sessions_seconds(&body) {
            Ok(seconds) => return Ok(seconds),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

/// Somme les sessions publiées par GOG. `time` est en minutes ; `seconds` et
/// `duration` restent acceptés tels quels.
pub fn parse_sessions_seconds(body: &str) -> Result<u64, String> {
    let root: Value =
        serde_json::from_str(body).map_err(|_| "temps de jeu GOG illisible".to_string())?;
    for pointer in ["/total_time", "/totalTime", "/playtime", "/total_playtime"] {
        if let Some(value) = root.pointer(pointer).and_then(Value::as_u64) {
            return Ok(value);
        }
    }
    let entries = root
        .get("items")
        .or_else(|| root.get("sessions"))
        .and_then(Value::as_array)
        .or_else(|| root.as_array())
        .ok_or_else(|| "temps de jeu GOG introuvable".to_string())?;
    let mut seconds: u64 = 0;
    for entry in entries {
        if let Some(value) = entry.get("seconds").and_then(Value::as_u64) {
            seconds = seconds.saturating_add(value);
            continue;
        }
        if let Some(value) = entry.get("duration").and_then(Value::as_u64) {
            seconds = seconds.saturating_add(value);
            continue;
        }
        if let Some(minutes) = entry
            .get("time")
            .or_else(|| entry.get("time_minutes"))
            .and_then(Value::as_u64)
        {
            seconds = seconds.saturating_add(minutes.saturating_mul(60));
        }
    }
    Ok(seconds)
}

fn iso8601(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let seconds = epoch.rem_euclid(86_400);
    let (year, month, day) = crate::session::civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// Débloque un succès GOG (`achievement_id`) au nom du compte connecté.
pub fn unlock_achievement(
    product_id: &str,
    token: &str,
    user_id: &str,
    achievement_id: &str,
    now: i64,
) -> Result<(), String> {
    if achievement_id.trim().is_empty() {
        return Err("succès GOG sans identifiant".to_string());
    }
    let response = client()
        .post(format!(
            "{GAMEPLAY_URL}/clients/{product_id}/users/{user_id}/achievements/{achievement_id}"
        ))
        .bearer_auth(token)
        .header("X-Gog-Lc", LOCALE)
        .json(&serde_json::json!({ "date_unlocked": iso8601(now) }))
        .timeout(http_timeout())
        .send()
        .map_err(|_| "GOG injoignable pendant le déblocage du succès".to_string())?;
    if response.status().is_success() {
        return Ok(());
    }
    Err(format!(
        "GOG a refusé le déblocage du succès (HTTP {})",
        response.status().as_u16()
    ))
}

/// Succès d'un jeu GOG possesseur d'une session gogdl, renouvelée si besoin.
pub fn game_achievements(
    auth_path: &Path,
    product_id: &str,
    now: i64,
) -> Result<AchievementInfo, String> {
    let tokens = usable_tokens(auth_path, now)?;
    fetch_achievements(product_id, &tokens.access_token, &tokens.user_id)
}

/// Déblocage demandé par une session de jeu : le pont local ne connaît que la
/// clé (`ACH_WIN_THE_GAME`), la liste fournit l'identifiant numérique.
pub fn unlock_for_session(
    auth_path: &Path,
    product_id: &str,
    key: &str,
    now: i64,
) -> Result<Option<String>, String> {
    let tokens = usable_tokens(auth_path, now)?;
    let info = fetch_achievements(product_id, &tokens.access_token, &tokens.user_id)?;
    let needle = key.trim();
    let found = info.rows.iter().find(|row| {
        (!needle.is_empty() && row.key.eq_ignore_ascii_case(needle))
            || (!needle.is_empty() && row.name.eq_ignore_ascii_case(needle))
    });
    let row = found.ok_or_else(|| format!("GOG ne liste pas le succès « {needle} »"))?;
    if row.unlocked {
        return Ok(None);
    }
    unlock_achievement(
        product_id,
        &tokens.access_token,
        &tokens.user_id,
        &row.id,
        now,
    )?;
    Ok(Some(row.name.clone()))
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
            cover_url: entry.cover_url.clone(),
            install_exe: None,
            install_state: InstallState::NotInstalled,
            quark_game_id: None,
            capabilities: GameCapabilities {
                achievements: CapabilityState::Supported,
                cloud_saves: CapabilityState::Unknown,
            },
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
    fn le_temps_de_jeu_gog_somme_les_sessions_et_accepte_plusieurs_formes() {
        let body = r#"{"items":[
            {"session_date":1700000000,"time":90},
            {"session_date":1700003600,"time":30},
            {"seconds":45}
        ]}"#;
        assert_eq!(
            parse_sessions_seconds(body).unwrap(),
            90 * 60 + 30 * 60 + 45
        );
        assert_eq!(
            parse_sessions_seconds(r#"{"sessions":[{"duration":120}]}"#).unwrap(),
            120
        );
        assert_eq!(
            parse_sessions_seconds(r#"{"total_time":7200}"#).unwrap(),
            7200
        );
        assert!(parse_sessions_seconds(r#"{"items":[]}"#).unwrap() == 0);
        assert!(parse_sessions_seconds(r#"{"autre":1}"#).is_err());
    }

    #[test]
    fn le_profil_gog_donne_le_temps_total_en_minutes() {
        let html = r#"<script>
            window.profilesData.matchedGameStats = {"456":{"playtime":2966,"lastSession":"2020-05-28T21:28:01+00:00"}};
        </script>"#;
        assert_eq!(parse_profile_playtime(html, "456").unwrap(), 2966 * 60);
        let vide = r#"window.profilesData.matchedGameStats = {"456":[]};"#;
        assert_eq!(parse_profile_playtime(vide, "456").unwrap(), 0);
        assert!(parse_profile_playtime(html, "999").is_err());
        assert!(parse_profile_playtime("<html></html>", "456").is_err());
    }

    #[test]
    fn session_du_fichier_gogdl_est_lue_par_client() {
        let tokens = read_tokens(SESSION).unwrap();
        assert_eq!(tokens.access_token, "access-1");
        assert_eq!(tokens.refresh_token, "refresh-1");
        assert_eq!(tokens.login_time, 1_700_000_000);
        assert_eq!(tokens.expires_in, 3600);
        assert_eq!(tokens.user_id, "456");
    }

    #[test]
    fn la_jaquette_gog_derive_de_l_image_de_boitier_et_pas_de_l_arriere_plan() {
        assert_eq!(
            cover_from_boxart(
                "https://images.gog-statics.com/32385c2c39b8715a9ccfb159ed481b55503b96526b00866f75a4af48461615a5.jpg"
            )
            .unwrap(),
            "https://images.gog-statics.com/32385c2c39b8715a9ccfb159ed481b55503b96526b00866f75a4af48461615a5_glx_vertical_cover.webp"
        );
        assert_eq!(
            cover_from_boxart("//images.gog-statics.com/abc.png").unwrap(),
            "https://images.gog-statics.com/abc_glx_vertical_cover.webp"
        );
        assert!(cover_from_boxart("pas une url").is_none());

        let body = r#"{"_links":{"boxArtImage":{"href":"https://images.gog-statics.com/hash.jpg"},"backgroundImage":{"href":"https://images.gog-statics.com/bg.jpg"}}}"#;
        assert!(
            parse_cover(body)
                .unwrap()
                .ends_with("_glx_vertical_cover.webp")
        );
        // Sans boîtier, l'illustration large reste la bonne œuvre.
        let fallback =
            r#"{"_links":{"backgroundImage":{"href":"https://images.gog-statics.com/bg.jpg"}}}"#;
        assert_eq!(
            parse_cover(fallback).unwrap(),
            "https://images.gog-statics.com/bg.jpg"
        );
        assert!(parse_cover(r#"{"_links":{}}"#).is_err());
        assert!(cover_url("pas-un-id").is_err());
        // La bannière est l'illustration large, jamais la jaquette dérivée.
        assert_eq!(
            parse_hero(body).unwrap(),
            "https://images.gog-statics.com/bg.jpg"
        );
        assert!(parse_hero(r#"{"_links":{}}"#).is_err());
        assert!(hero_url("pas-un-id").is_err());
    }

    #[test]
    fn les_succes_gog_masques_ne_sortent_qu_une_fois_debloques() {
        let body = r#"{
          "total_count": 3,
          "items": [
            {"achievement_id": "11", "achievement_key": "ACH_FIRST", "name": "Premier", "description": "Fini le niveau 1", "visible": true, "date_unlocked": "2026-01-01T00:00:00+00:00", "image_url_unlocked": "", "rarity": 42.5},
            {"achievement_id": "12", "achievement_key": "ACH_SECOND", "name": "Deuxième", "description": "Fini le niveau 2", "visible": true, "date_unlocked": null, "image_url_unlocked": "", "rarity": 7.25},
            {"achievement_id": "13", "achievement_key": "ACH_SECRET", "name": "Secret", "description": "Caché", "visible": false, "date_unlocked": null, "image_url_unlocked": ""},
            {"achievement_id": "14", "achievement_key": "ACH_SECRET_2", "name": "Secret débloqué", "description": "Caché", "visible": false, "date_unlocked": "2026-01-02T00:00:00+00:00", "image_url_unlocked": ""}
          ]
        }"#;
        let info = parse_achievements(body).unwrap();
        // Le total publié inclut les succès masqués non débloqués, mais la
        // liste ne les affiche pas : la barre suit les lignes visibles.
        assert_eq!(info.total, 3);
        assert_eq!(info.unlocked, 2);
        assert!(info.rows.iter().all(|row| row.name != "Secret"));
        assert!(
            info.rows
                .iter()
                .any(|row| row.name == "Secret débloqué" && row.id == "14")
        );
        assert_eq!(
            info.rows
                .iter()
                .find(|row| row.name == "Premier")
                .and_then(|row| row.rarity),
            Some(42.5)
        );
        assert_eq!(
            info.rows
                .iter()
                .find(|row| row.name == "Deuxième")
                .and_then(|row| row.rarity),
            Some(7.25)
        );
        assert!(parse_achievements("pas du json").is_err());
        assert!(parse_achievements("{}").is_err());
    }

    #[test]
    fn session_plate_et_session_absente_sont_distinguees() {
        let flat = r#"{"access_token":"a","refresh_token":"r","expires_in":3600,"loginTime":1}"#;
        assert_eq!(read_tokens(flat).unwrap().access_token, "a");
        assert!(read_tokens("{}").unwrap_err().contains("absente"));
        assert!(read_tokens("pas du json").is_err());
        // Un refus de gogdl (`{"error": true}`) n'est pas une session.
        assert!(
            read_tokens(r#"{"error": true}"#)
                .unwrap_err()
                .contains("absente")
        );
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
                    title: "alpha".into(),
                    cover_url: None
                },
                OwnedGame {
                    id: "1".into(),
                    title: "Zebra".into(),
                    cover_url: None
                }
            ]
        );
        // La liste paginée arrive sous la clé `products`.
        let page = r#"{"totalPages":1,"products":[{"id":"9","title":"Page Game"}]}"#;
        assert_eq!(
            parse_products(page).unwrap(),
            vec![OwnedGame {
                id: "9".into(),
                title: "Page Game".into(),
                cover_url: None
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
                cover_url: None,
            }],
            99,
        );
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].key.provider, Provider::Gog);
        assert_eq!(games[0].key.product_id, "42");
        assert_eq!(games[0].install_state, InstallState::NotInstalled);
        assert_eq!(
            games[0].capabilities.achievements,
            CapabilityState::Supported
        );
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
