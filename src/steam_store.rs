//! Catalogue Steam Store : interface publique non documentée, à traiter comme best-effort.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const STORE_ORIGIN: &str = "https://store.steampowered.com";
const WEB_API_ORIGIN: &str = "https://api.steampowered.com";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorePrice {
    pub currency: String,
    pub initial: u64,
    pub final_price: u64,
    pub discount_percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreItem {
    pub app_id: u32,
    pub name: String,
    pub price: Option<StorePrice>,
    pub artwork_url: Option<String>,
    pub store_url: String,
    pub linux_available: Option<bool>,
    /// Joueurs simultanés au pic, seulement pour la section « Populaires ».
    #[serde(default)]
    pub players_peak: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreSection {
    pub title: String,
    pub items: Vec<StoreItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoreSnapshot {
    pub fetched_at: i64,
    pub sections: Vec<StoreSection>,
}

impl Default for StoreSnapshot {
    fn default() -> Self {
        Self {
            fetched_at: 0,
            sections: Vec::new(),
        }
    }
}

impl StoreSnapshot {
    pub fn is_stale(&self, now: i64) -> bool {
        self.fetched_at <= 0 || now.saturating_sub(self.fetched_at) > 12 * 60 * 60
    }
}

/// Catégories du catalogue Steam, dans l'ordre d'affichage.
const FEATURED_SECTIONS: [(&str, &str); 4] = [
    ("specials", "Offres spéciales"),
    ("top_sellers", "Meilleures ventes"),
    ("new_releases", "Nouveautés"),
    ("coming_soon", "Prochainement"),
];

fn parse_featured_item(item: &serde_json::Value) -> Option<StoreItem> {
    let app_id = item
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
        .unwrap_or(0);
    let name = item
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if app_id == 0 || name.trim().is_empty() {
        return None;
    }
    let image = item
        .get("large_capsule_image")
        .or_else(|| item.get("header_image"))
        .and_then(serde_json::Value::as_str)
        .filter(|url| is_steam_artwork_url(url))
        .map(str::to_string);
    let currency = item
        .get("currency")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let price = (!currency.is_empty()).then(|| StorePrice {
        currency: currency.to_string(),
        initial: item
            .get("original_price")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        final_price: item
            .get("final_price")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        discount_percent: item
            .get("discount_percent")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u8::try_from(n).ok())
            .filter(|n| *n <= 100)
            .filter(|n| *n > 0),
    });
    Some(StoreItem {
        app_id,
        name: name.to_string(),
        price,
        artwork_url: image,
        store_url: store_url(app_id),
        linux_available: item
            .get("linux_available")
            .and_then(serde_json::Value::as_bool),
        players_peak: None,
    })
}

pub fn parse_featured(json: &str) -> Result<StoreSnapshot, String> {
    if json.len() > 4 * 1024 * 1024 {
        return Err("réponse des offres trop volumineuse".to_string());
    }
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "réponse des offres Steam invalide".to_string())?;
    let mut sections = Vec::new();
    for (key, title) in FEATURED_SECTIONS {
        let Some(items) = root
            .get(key)
            .and_then(|section| section.get("items"))
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        let items: Vec<StoreItem> = items
            .iter()
            .filter_map(parse_featured_item)
            .take(16)
            .collect();
        if !items.is_empty() {
            sections.push(StoreSection {
                title: title.to_string(),
                items,
            });
        }
    }
    if sections.is_empty() {
        return Err("réponse Steam sans catégorie reconnue".to_string());
    }
    Ok(StoreSnapshot {
        fetched_at: crate::config::now_ts(),
        sections,
    })
}

pub fn featured(client: &Client, language: &str, country: &str) -> Result<StoreSnapshot, String> {
    let response = client
        .get(format!("{STORE_ORIGIN}/api/featuredcategories/"))
        .query(&[("l", language), ("cc", country)])
        .send()
        .map_err(|_| "connexion aux offres Steam impossible".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Steam Store a répondu {}",
            response.status().as_u16()
        ));
    }
    let bytes = read_limited(response, 4 * 1024 * 1024)?;
    let json = std::str::from_utf8(&bytes).map_err(|_| "réponse Steam non UTF-8".to_string())?;
    parse_featured(json)
}

/// Classement public des jeux les plus joués (aucune clé requise).
pub fn parse_most_played(json: &str) -> Result<Vec<(u32, u32)>, String> {
    let root: serde_json::Value = serde_json::from_str(json)
        .map_err(|_| "réponse des jeux les plus joués invalide".to_string())?;
    let ranks = root
        .pointer("/response/ranks")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "réponse Steam sans classement des jeux les plus joués".to_string())?;
    Ok(ranks
        .iter()
        .filter_map(|rank| {
            let app_id = rank
                .get("appid")
                .and_then(serde_json::Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())?;
            let peak = rank
                .get("peak_in_game")
                .and_then(serde_json::Value::as_u64)
                .and_then(|peak| u32::try_from(peak).ok())
                .unwrap_or(0);
            (app_id > 0).then_some((app_id, peak))
        })
        .take(10)
        .collect())
}

pub fn parse_app_details(json: &str) -> Option<StoreItem> {
    let root: serde_json::Value = serde_json::from_str(json).ok()?;
    let entry = root.as_object()?.values().next()?;
    if !entry
        .get("success")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let data = entry.get("data")?;
    let app_id = data
        .get("steam_appid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())?;
    let name = data.get("name").and_then(serde_json::Value::as_str)?;
    if name.trim().is_empty() {
        return None;
    }
    let artwork_url = data
        .get("header_image")
        .or_else(|| data.get("capsule_image"))
        .and_then(serde_json::Value::as_str)
        .filter(|url| is_steam_artwork_url(url))
        .map(str::to_string);
    let price = data
        .get("price_overview")
        .filter(|price| price.get("currency").is_some())
        .map(|price| StorePrice {
            currency: price
                .get("currency")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            initial: price
                .get("initial")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            final_price: price
                .get("final")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            discount_percent: price
                .get("discount_percent")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u8::try_from(n).ok())
                .filter(|n| *n > 0 && *n <= 100),
        });
    Some(StoreItem {
        app_id,
        name: name.to_string(),
        price,
        artwork_url,
        store_url: store_url(app_id),
        linux_available: data
            .get("platforms")
            .and_then(|platforms| platforms.get("linux"))
            .and_then(serde_json::Value::as_bool),
        players_peak: None,
    })
}

fn app_details(
    client: &Client,
    app_id: u32,
    language: &str,
    country: &str,
) -> Result<Option<StoreItem>, String> {
    let response = client
        .get(format!("{STORE_ORIGIN}/api/appdetails"))
        .query(&[
            ("appids", app_id.to_string()),
            ("l", language.to_string()),
            ("cc", country.to_string()),
        ])
        .send()
        .map_err(|_| "connexion aux fiches Steam impossible".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Steam Store a répondu {}",
            response.status().as_u16()
        ));
    }
    let bytes = read_limited(response, 2 * 1024 * 1024)?;
    let json = std::str::from_utf8(&bytes).map_err(|_| "réponse Steam non UTF-8".to_string())?;
    Ok(parse_app_details(json))
}

/// Jeux les plus joués, avec leur fiche (nom, visuel, prix) et le pic de joueurs.
pub fn most_played(
    client: &Client,
    language: &str,
    country: &str,
) -> Result<Vec<StoreItem>, String> {
    let response = client
        .get(format!(
            "{WEB_API_ORIGIN}/ISteamChartsService/GetMostPlayedGames/v1/"
        ))
        .send()
        .map_err(|_| "connexion au classement Steam impossible".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Steam Web API a répondu {}",
            response.status().as_u16()
        ));
    }
    let bytes = read_limited(response, 512 * 1024)?;
    let json = std::str::from_utf8(&bytes).map_err(|_| "réponse Steam non UTF-8".to_string())?;
    let ranks = parse_most_played(json)?;
    if ranks.is_empty() {
        return Err("classement Steam vide".to_string());
    }
    let mut items = Vec::new();
    for (app_id, peak) in ranks {
        if let Ok(Some(mut item)) = app_details(client, app_id, language, country) {
            item.players_peak = Some(peak);
            items.push(item);
        }
    }
    if items.is_empty() {
        return Err("aucune fiche Steam n'a pu être lue pour les jeux les plus joués".to_string());
    }
    Ok(items)
}

/// Jeu possédé, tel que renvoyé par `IPlayerService/GetOwnedGames`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedSteamGame {
    pub app_id: u32,
    pub name: String,
    pub playtime_minutes: u64,
}

pub fn parse_owned_games(json: &str) -> Result<Vec<OwnedSteamGame>, String> {
    if json.len() > 8 * 1024 * 1024 {
        return Err("réponse de bibliothèque Steam trop volumineuse".to_string());
    }
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "réponse Steam Web API invalide".to_string())?;
    let response = root
        .get("response")
        .ok_or_else(|| "réponse Steam Web API sans contenu".to_string())?;
    let Some(games) = response.get("games").and_then(serde_json::Value::as_array) else {
        return Err(
            "profil Steam privé ou clé Web API refusée : aucune liste de jeux renvoyée".to_string(),
        );
    };
    Ok(games
        .iter()
        .filter_map(|game| {
            let app_id = game
                .get("appid")
                .and_then(serde_json::Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())?;
            let name = game.get("name").and_then(serde_json::Value::as_str)?;
            if app_id == 0 || name.trim().is_empty() {
                return None;
            }
            Some(OwnedSteamGame {
                app_id,
                name: name.to_string(),
                playtime_minutes: game
                    .get("playtime_forever")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            })
        })
        .collect())
}

pub fn owned_games(
    client: &Client,
    key: &str,
    steam_id: &str,
) -> Result<Vec<OwnedSteamGame>, String> {
    let response = client
        .get(format!("{WEB_API_ORIGIN}/IPlayerService/GetOwnedGames/v1/"))
        .query(&[
            ("key", key),
            ("steamid", steam_id),
            ("include_appinfo", "1"),
            ("include_played_free_games", "1"),
            ("format", "json"),
        ])
        .send()
        .map_err(|_| "connexion à la bibliothèque Steam impossible".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Steam Web API a répondu {}",
            response.status().as_u16()
        ));
    }
    let bytes = read_limited(response, 8 * 1024 * 1024)?;
    let json = std::str::from_utf8(&bytes).map_err(|_| "réponse Steam non UTF-8".to_string())?;
    parse_owned_games(json)
}

/// Signaux de bibliothèque utilisés pour classer les recommandations.
#[derive(Debug, Clone, Default)]
pub struct LibrarySignal {
    owned_apps: HashSet<u32>,
    owned_names: HashSet<String>,
    token_weights: HashMap<String, u32>,
}

impl LibrarySignal {
    pub fn from_quark_games(games: &[crate::config::Game]) -> Self {
        Self::from_entries(
            games
                .iter()
                .map(|game| (game.steam_app_id, game.name.clone(), 0)),
        )
    }

    pub fn from_steam_games(games: &[OwnedSteamGame]) -> Self {
        Self::from_entries(
            games
                .iter()
                .map(|game| (Some(game.app_id), game.name.clone(), game.playtime_minutes)),
        )
    }

    fn from_entries(entries: impl IntoIterator<Item = (Option<u32>, String, u64)>) -> Self {
        let mut signal = LibrarySignal::default();
        for (app_id, name, playtime_minutes) in entries {
            if let Some(app_id) = app_id {
                signal.owned_apps.insert(app_id);
            }
            let title = normalized_title(&name);
            if !title.is_empty() {
                signal.owned_names.insert(title);
            }
            // Les jeux les plus joués pèsent davantage, sans écraser les autres.
            let weight = 1 + u32::try_from(playtime_minutes / 600)
                .unwrap_or(u32::MAX)
                .min(4);
            for token in normalized_tokens(&name) {
                if token.len() >= 4 {
                    *signal.token_weights.entry(token).or_default() += weight;
                }
            }
        }
        signal
    }

    #[cfg(test)]
    pub fn owned_count(&self) -> usize {
        self.owned_apps.len().max(self.owned_names.len())
    }
}

pub fn cache_path() -> std::path::PathBuf {
    crate::config::data_dir().join("store-cache.json")
}

pub fn load_cache(path: &Path) -> Result<Option<StoreSnapshot>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("cache Steam Store illisible".to_string()),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| "cache Steam Store invalide".to_string())
}

pub fn recommend(items: &[StoreItem], library: &LibrarySignal) -> Vec<StoreItem> {
    let mut candidates: Vec<(u32, u32, StoreItem)> = items
        .iter()
        .filter(|item| {
            !library.owned_apps.contains(&item.app_id)
                && !library.owned_names.contains(&normalized_title(&item.name))
        })
        .map(|item| {
            let score: u32 = normalized_tokens(&item.name)
                .iter()
                .filter(|token| token.len() >= 4)
                .filter_map(|token| library.token_weights.get(token))
                .sum();
            (score, item.app_id, item.clone())
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    candidates
        .into_iter()
        .filter(|candidate| candidate.0 > 0)
        .map(|candidate| candidate.2)
        .take(12)
        .collect()
}

/// Source des recommandations, telle qu'affichée sous la section.
pub const SOURCE_STEAM: &str = "votre bibliothèque Steam (Web API)";
pub const SOURCE_QUARK: &str = "la bibliothèque Quark";

/// Recalcule les recommandations depuis un catalogue et la meilleure
/// bibliothèque disponible, en indiquant laquelle a servi.
pub fn recommendations_for(
    snapshot: Option<&StoreSnapshot>,
    quark_games: &[crate::config::Game],
    steam_library: Option<&[OwnedSteamGame]>,
) -> (Vec<StoreItem>, Option<String>) {
    let Some(snapshot) = snapshot else {
        return (Vec::new(), None);
    };
    let mut pool: Vec<StoreItem> = snapshot
        .sections
        .iter()
        .flat_map(|section| section.items.iter().cloned())
        .collect();
    pool.sort_by_key(|item| item.app_id);
    pool.dedup_by_key(|item| item.app_id);
    match steam_library.filter(|games| !games.is_empty()) {
        Some(games) => (
            recommend(&pool, &LibrarySignal::from_steam_games(games)),
            Some(SOURCE_STEAM.to_string()),
        ),
        None => (
            recommend(&pool, &LibrarySignal::from_quark_games(quark_games)),
            Some(SOURCE_QUARK.to_string()),
        ),
    }
}

fn normalized_tokens(title: &str) -> Vec<String> {
    title
        .to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty() && !matches!(*token, "the" | "a" | "an"))
        .map(str::to_string)
        .collect()
}

fn normalized_title(title: &str) -> String {
    normalized_tokens(title).join(" ")
}

pub fn save_cache_atomic(path: &Path, snapshot: &StoreSnapshot) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|_| "dossier du cache Steam Store inaccessible".to_string())?;
    let bytes = serde_json::to_vec(snapshot)
        .map_err(|_| "cache Steam Store non sérialisable".to_string())?;
    let temp = parent.join(format!(".store-cache-{}.tmp", std::process::id()));
    std::fs::write(&temp, bytes)
        .map_err(|_| "écriture du cache Steam Store impossible".to_string())?;
    std::fs::rename(&temp, path).map_err(|_| {
        let _ = std::fs::remove_file(&temp);
        "remplacement du cache Steam Store impossible".to_string()
    })
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    #[serde(rename = "type", default)]
    item_type: String,
    #[serde(default)]
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    price: Option<RawPrice>,
    #[serde(default)]
    tiny_image: Option<String>,
    #[serde(default)]
    platforms: Option<RawPlatforms>,
}

#[derive(Deserialize)]
struct RawPrice {
    #[serde(default)]
    currency: String,
    #[serde(default)]
    initial: u64,
    #[serde(rename = "final", default)]
    final_price: u64,
}

#[derive(Deserialize)]
struct RawPlatforms {
    #[serde(default)]
    linux: bool,
}

pub fn parse_search(json: &str) -> Result<Vec<StoreItem>, String> {
    if json.len() > 2 * 1024 * 1024 {
        return Err("réponse de recherche trop volumineuse".to_string());
    }
    let response: SearchResponse =
        serde_json::from_str(json).map_err(|_| "réponse Steam Store invalide".to_string())?;
    Ok(response
        .items
        .into_iter()
        .filter(|item| item.item_type == "app" && item.id > 0 && !item.name.trim().is_empty())
        .map(|item| {
            let price = item
                .price
                .filter(|price| !price.currency.is_empty())
                .map(|price| {
                    let discount = price.initial.checked_sub(price.final_price).unwrap_or(0);
                    let discount_percent = (price.initial > 0 && discount > 0)
                        .then(|| ((discount.saturating_mul(100) / price.initial).min(100)) as u8);
                    StorePrice {
                        currency: price.currency,
                        initial: price.initial,
                        final_price: price.final_price,
                        discount_percent,
                    }
                });
            StoreItem {
                app_id: item.id,
                name: item.name,
                price,
                artwork_url: item.tiny_image.filter(|url| is_steam_artwork_url(url)),
                store_url: store_url(item.id),
                linux_available: item.platforms.map(|platforms| platforms.linux),
                players_peak: None,
            }
        })
        .collect())
}

fn is_steam_artwork_url(url: &str) -> bool {
    url.starts_with("https://shared.akamai.steamstatic.com/")
        || url.starts_with("https://cdn.akamai.steamstatic.com/")
        || url.starts_with("https://cdn.cloudflare.steamstatic.com/")
}

pub fn search(
    client: &Client,
    query: &str,
    language: &str,
    country: &str,
) -> Result<Vec<StoreItem>, String> {
    let response = client
        .get(format!("{STORE_ORIGIN}/api/storesearch/"))
        .query(&[("term", query), ("l", language), ("cc", country)])
        .send()
        .map_err(|_| "connexion au catalogue Steam impossible".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Steam Store a répondu {}",
            response.status().as_u16()
        ));
    }
    let bytes = read_limited(response, 2 * 1024 * 1024)?;
    let json = std::str::from_utf8(&bytes).map_err(|_| "réponse Steam non UTF-8".to_string())?;
    parse_search(json)
}

/// Résultat complet d'un rafraîchissement de la boutique.
#[derive(Debug, Clone)]
pub struct StoreFetch {
    pub snapshot: StoreSnapshot,
    pub steam_library: Option<Vec<OwnedSteamGame>>,
    pub steam_library_error: Option<String>,
    pub popular_error: Option<String>,
}

/// Charge les catégories Steam, les jeux les plus joués et, si une clé Web API
/// est configurée, la bibliothèque Steam du compte.
///
/// Les jeux les plus joués et la bibliothèque sont facultatifs : leur échec est
/// rapporté sans priver la page des catégories déjà lues.
pub fn fetch_all(
    client: &Client,
    language: &str,
    country: &str,
    steam_web_api_key: Option<&str>,
    steam_id: Option<&str>,
) -> Result<StoreFetch, String> {
    let mut snapshot = featured(client, language, country)?;
    let (popular, popular_error) = match most_played(client, language, country) {
        Ok(items) => (items, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    if !popular.is_empty() {
        let insert_at = if snapshot.sections.is_empty() { 0 } else { 1 };
        snapshot.sections.insert(
            insert_at,
            StoreSection {
                title: "Populaires en ce moment".to_string(),
                items: popular,
            },
        );
    }
    let (steam_library, steam_library_error) = match (steam_web_api_key, steam_id) {
        (Some(key), Some(steam_id)) if !key.is_empty() && !steam_id.is_empty() => {
            match owned_games(client, key, steam_id) {
                Ok(games) => (Some(games), None),
                Err(error) => (None, Some(error)),
            }
        }
        _ => (None, None),
    };
    Ok(StoreFetch {
        snapshot,
        steam_library,
        steam_library_error,
        popular_error,
    })
}

fn read_limited(response: reqwest::blocking::Response, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err("réponse Steam Store trop volumineuse".to_string());
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "lecture de la réponse Steam impossible".to_string())?;
    if bytes.len() as u64 > limit {
        return Err("réponse Steam Store trop volumineuse".to_string());
    }
    Ok(bytes)
}

fn store_url(app_id: u32) -> String {
    format!("{STORE_ORIGIN}/app/{app_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reponse_featured_sans_categorie_reconnue_est_refusee_plutot_que_cachee_vide() {
        assert!(parse_featured(r#"{"other":{"items":[]}}"#).is_err());
    }

    #[test]
    fn les_categories_steam_deviennent_des_sections_ordonnees() {
        let json = r#"{
            "specials":{"items":[
                {"id":620,"name":"Portal 2","currency":"EUR","original_price":1999,"final_price":499,"discount_percent":75,"large_capsule_image":"https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/620/capsule.jpg","linux_available":true},
                {"id":621,"name":"No price","header_image":"https://attacker.invalid/image.jpg"}
            ]},
            "top_sellers":{"items":[
                {"id":570,"name":"Dota 2","currency":"EUR","original_price":0,"final_price":0,"discount_percent":0}
            ]},
            "new_releases":{"items":[]}
        }"#;
        let snapshot = parse_featured(json).unwrap();
        let titles: Vec<&str> = snapshot
            .sections
            .iter()
            .map(|section| section.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Offres spéciales", "Meilleures ventes"]);
        let game = &snapshot.sections[0].items[0];
        assert_eq!(game.price.as_ref().unwrap().currency, "EUR");
        assert_eq!(game.price.as_ref().unwrap().discount_percent, Some(75));
        assert!(game.linux_available.unwrap());
        assert!(game.artwork_url.is_some());
        assert!(snapshot.sections[0].items[1].artwork_url.is_none());
        assert_eq!(snapshot.sections[0].items[1].price, None);
    }

    #[test]
    fn classement_des_plus_joues_ne_garde_que_des_appids_valides() {
        let json = r#"{"response":{"ranks":[
            {"rank":1,"appid":730,"peak_in_game":1207353},
            {"rank":2,"appid":0,"peak_in_game":10},
            {"rank":3,"appid":570,"peak_in_game":708985}
        ]}}"#;
        assert_eq!(
            parse_most_played(json).unwrap(),
            vec![(730, 1207353), (570, 708985)]
        );
        assert!(parse_most_played(r#"{"response":{}}"#).is_err());
    }

    #[test]
    fn une_fiche_steam_lue_par_appdetails_garde_prix_et_visuel_officiels() {
        let json = r#"{"730":{"success":true,"data":{
            "steam_appid":730,"name":"Counter-Strike 2",
            "header_image":"https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/730/header.jpg",
            "platforms":{"linux":true},
            "price_overview":{"currency":"EUR","initial":0,"final":0,"discount_percent":0}
        }}}"#;
        let item = parse_app_details(json).unwrap();
        assert_eq!(item.app_id, 730);
        assert_eq!(item.name, "Counter-Strike 2");
        assert_eq!(item.store_url, "https://store.steampowered.com/app/730");
        assert_eq!(item.linux_available, Some(true));
        assert_eq!(item.price.as_ref().unwrap().discount_percent, None);
        assert!(parse_app_details(r#"{"1":{"success":false}}"#).is_none());
        let unsafe_art = json.replace(
            "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/730/header.jpg",
            "https://attacker.invalid/header.jpg",
        );
        assert!(
            parse_app_details(&unsafe_art)
                .unwrap()
                .artwork_url
                .is_none()
        );
    }

    #[test]
    fn bibliotheque_steam_privee_est_signalee_plutot_que_vide() {
        assert!(parse_owned_games(r#"{"response":{"game_count":0}}"#).is_err());
        let json = r#"{"response":{"game_count":2,"games":[
            {"appid":620,"name":"Portal 2","playtime_forever":600},
            {"appid":0,"name":"","playtime_forever":0}
        ]}}"#;
        assert_eq!(
            parse_owned_games(json).unwrap(),
            vec![OwnedSteamGame {
                app_id: 620,
                name: "Portal 2".to_string(),
                playtime_minutes: 600
            }]
        );
    }

    #[test]
    fn cache_stale_est_restitue_sans_ecriture_et_roundtrip() {
        let path = std::env::temp_dir().join(format!(
            "quark-store-cache-{}-{}.json",
            std::process::id(),
            crate::config::now_ts()
        ));
        let snapshot = StoreSnapshot {
            fetched_at: 100,
            sections: vec![StoreSection {
                title: "Specials".into(),
                items: vec![],
            }],
        };
        save_cache_atomic(&path, &snapshot).unwrap();
        let loaded = load_cache(&path).unwrap().unwrap();
        assert_eq!(loaded, snapshot);
        assert!(loaded.is_stale(100 + 13 * 60 * 60));
        assert!(!loaded.is_stale(100 + 60));
        let _ = std::fs::remove_file(path);
    }

    fn item(app_id: u32, name: &str) -> StoreItem {
        StoreItem {
            app_id,
            name: name.into(),
            price: None,
            artwork_url: None,
            store_url: store_url(app_id),
            linux_available: None,
            players_peak: None,
        }
    }

    #[test]
    fn recommandation_locale_exclut_les_appids_possedes_et_est_deterministe() {
        let items = vec![
            item(10, "Moon RPG"),
            item(11, "Moon Quest Deluxe"),
            item(12, "Racing Cars"),
        ];
        let mut owned_game = crate::config::Game::new("Moon Quest", "");
        owned_game.steam_app_id = Some(10);
        let library = LibrarySignal::from_quark_games(&[owned_game]);
        let first = recommend(&items, &library);
        let second = recommend(&items, &library);
        assert_eq!(
            first.iter().map(|item| item.app_id).collect::<Vec<_>>(),
            second.iter().map(|item| item.app_id).collect::<Vec<_>>()
        );
        assert!(!first.iter().any(|item| item.app_id == 10));
        assert_eq!(first.first().unwrap().app_id, 11);
    }

    #[test]
    fn la_bibliotheque_steam_pondere_les_jeux_les_plus_joues() {
        let items = vec![item(10, "Stardew Valley"), item(11, "Satisfactory Factory")];
        let library = LibrarySignal::from_steam_games(&[
            OwnedSteamGame {
                app_id: 413150,
                name: "Stardew Valley".into(),
                playtime_minutes: 0,
            },
            OwnedSteamGame {
                app_id: 526870,
                name: "Satisfactory".into(),
                playtime_minutes: 6 * 600,
            },
        ]);
        assert_eq!(library.owned_count(), 2);
        // Le jeu possédé est exclu même sans AppID Quark, et l'autre est
        // recommandé parce que son titre recoupe la bibliothèque Steam.
        let recommendations = recommend(&items, &library);
        assert_eq!(
            recommendations
                .iter()
                .map(|item| item.app_id)
                .collect::<Vec<_>>(),
            vec![11]
        );
    }

    #[test]
    fn recherche_preserve_le_prix_regional_et_ne_garde_que_les_urls_steam() {
        let json = r#"{
            "items": [{
                "type": "app", "id": 620, "name": "Portal 2",
                "price": {"currency": "EUR", "initial": 1999, "final": 499},
                "tiny_image": "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/620/capsule.jpg",
                "platforms": {"windows": true, "linux": true}
            }, {
                "type": "app", "id": 621, "name": "Unsafe art",
                "tiny_image": "https://attacker.invalid/image.png"
            }, {
                "type": "sub", "id": 622, "name": "Not a game"
            }]
        }"#;
        let items = parse_search(json).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].app_id, 620);
        assert_eq!(items[0].store_url, "https://store.steampowered.com/app/620");
        assert_eq!(items[0].price.as_ref().unwrap().currency, "EUR");
        assert_eq!(items[0].price.as_ref().unwrap().final_price, 499);
        assert!(items[0].linux_available.unwrap());
        assert!(items[1].artwork_url.is_none());
        assert_eq!(items[1].price, None);
    }
}
