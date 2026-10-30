//! Catalogue Steam Store : interface publique non documentée, à traiter comme best-effort.

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;

const STORE_ORIGIN: &str = "https://store.steampowered.com";

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

pub fn parse_featured(json: &str) -> Result<StoreSnapshot, String> {
    if json.len() > 4 * 1024 * 1024 {
        return Err("réponse des offres trop volumineuse".to_string());
    }
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "réponse des offres Steam invalide".to_string())?;
    let Some(items) = root
        .get("specials")
        .and_then(|section| section.get("items"))
        .and_then(serde_json::Value::as_array)
    else {
        return Err("réponse Steam sans section « specials » reconnue".to_string());
    };
    let mut deals = Vec::new();
    for item in items {
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
            continue;
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
        let price = if currency.is_empty() {
            None
        } else {
            Some(StorePrice {
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
                    .filter(|n| *n <= 100),
            })
        };
        deals.push(StoreItem {
            app_id,
            name: name.to_string(),
            price,
            artwork_url: image,
            store_url: store_url(app_id),
            linux_available: item
                .get("linux_available")
                .and_then(serde_json::Value::as_bool),
        });
    }
    Ok(StoreSnapshot {
        fetched_at: crate::config::now_ts(),
        sections: if deals.is_empty() {
            Vec::new()
        } else {
            vec![StoreSection {
                title: "Offres spéciales".to_string(),
                items: deals,
            }]
        },
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

pub fn recommend(items: &[StoreItem], games: &[crate::config::Game]) -> Vec<StoreItem> {
    let owned: std::collections::HashSet<u32> =
        games.iter().filter_map(|game| game.steam_app_id).collect();
    let owned_names: std::collections::HashSet<String> = games
        .iter()
        .map(|game| normalized_title(&game.name))
        .filter(|title| !title.is_empty())
        .collect();
    let library_tokens: std::collections::HashSet<String> = games
        .iter()
        .flat_map(|game| normalized_tokens(&game.name))
        .filter(|token| token.len() >= 4)
        .collect();
    let mut candidates: Vec<(i32, u32, StoreItem)> = items
        .iter()
        .filter(|item| {
            !owned.contains(&item.app_id) && !owned_names.contains(&normalized_title(&item.name))
        })
        .cloned()
        .map(|item| {
            let score = normalized_tokens(&item.name)
                .into_iter()
                .filter(|token| library_tokens.contains(token) && token.len() >= 4)
                .count() as i32;
            (score, item.app_id, item)
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
    fn reponse_featured_sans_specials_est_refusee_plutot_que_cachee_vide() {
        assert!(parse_featured(r#"{"other":{"items":[]}}"#).is_err());
    }

    #[test]
    fn offres_speciales_preservent_prix_image_valide_et_region() {
        let json = r#"{"specials":{"items":[
            {"id":620,"name":"Portal 2","currency":"EUR","original_price":1999,"final_price":499,"discount_percent":75,"large_capsule_image":"https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/620/capsule.jpg","linux_available":true},
            {"id":621,"name":"No price","header_image":"https://attacker.invalid/image.jpg"}
        ]}}"#;
        let snapshot = parse_featured(json).unwrap();
        assert_eq!(snapshot.sections[0].items.len(), 2);
        let game = &snapshot.sections[0].items[0];
        assert_eq!(game.price.as_ref().unwrap().currency, "EUR");
        assert_eq!(game.price.as_ref().unwrap().discount_percent, Some(75));
        assert!(game.linux_available.unwrap());
        assert!(game.artwork_url.is_some());
        assert!(snapshot.sections[0].items[1].artwork_url.is_none());
        assert_eq!(snapshot.sections[0].items[1].price, None);
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

    #[test]
    fn recommandation_locale_exclut_les_appids_possedes_et_est_deterministe() {
        let items = vec![
            StoreItem {
                app_id: 10,
                name: "Moon RPG".into(),
                price: None,
                artwork_url: None,
                store_url: store_url(10),
                linux_available: None,
            },
            StoreItem {
                app_id: 11,
                name: "Moon Quest Deluxe".into(),
                price: None,
                artwork_url: None,
                store_url: store_url(11),
                linux_available: None,
            },
            StoreItem {
                app_id: 12,
                name: "Racing Cars".into(),
                price: None,
                artwork_url: None,
                store_url: store_url(12),
                linux_available: None,
            },
        ];
        let mut owned_game = crate::config::Game::new("Moon Quest", "");
        owned_game.steam_app_id = Some(10);
        let games = vec![owned_game];
        let first = recommend(&items, &games);
        let second = recommend(&items, &games);
        assert_eq!(
            first.iter().map(|item| item.app_id).collect::<Vec<_>>(),
            second.iter().map(|item| item.app_id).collect::<Vec<_>>()
        );
        assert!(!first.iter().any(|item| item.app_id == 10));
        assert_eq!(first.first().unwrap().app_id, 11);
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
