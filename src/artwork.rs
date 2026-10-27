//! Recherche de jeux (Steam + SteamGridDB) et récupération des artworks originaux.
//!
//! Les artworks sont téléchargés une fois dans `~/.local/share/quark/artwork/<id>/`
//! puis affichés depuis le disque : aucune requête réseau à l'affichage.

use crate::config::{AssetKind, Artwork, Game, Settings, artwork_root};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn user_agent() -> String {
    format!("{}/{} (Linux)", crate::config::APP_NAME, env!("CARGO_PKG_VERSION"))
}

pub fn http() -> Client {
    Client::builder()
        .user_agent(user_agent())
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap_or_else(|_| Client::new())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Steam,
    SteamGridDb,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Steam => "Steam",
            Source::SteamGridDb => "SteamGridDB",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub source: Source,
    pub title: String,
    pub steam_app_id: Option<u32>,
    pub sgdb_id: Option<u64>,
    /// Miniature de la liste de résultats (URL distante).
    pub thumb: Option<String>,
    /// Ligne secondaire affichée dans les résultats.
    pub detail: String,
}

impl SearchHit {
    /// Reconstruit une « recherche » à partir d'un jeu déjà dans la bibliothèque
    /// (utilisé pour retélécharger les artworks).
    pub fn from_game(game: &Game) -> Self {
        Self {
            source: if game.steam_app_id.is_some() {
                Source::Steam
            } else {
                Source::SteamGridDb
            },
            title: game.name.clone(),
            steam_app_id: game.steam_app_id,
            sgdb_id: game.sgdb_id,
            thumb: None,
            detail: String::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Recherche
// ---------------------------------------------------------------------------

pub fn search(client: &Client, query: &str, s: &Settings) -> SearchOutcome {
    let mut out = SearchOutcome::default();
    let query = query.trim();
    if query.is_empty() {
        return out;
    }

    match steam_search(client, query, s) {
        Ok(hits) => out.hits.extend(hits),
        Err(err) => out.warnings.push(format!("Steam : {err}")),
    }

    if !s.sgdb_api_key.trim().is_empty() {
        match sgdb_search(client, query, s) {
            Ok(hits) => out.hits.extend(hits),
            Err(err) => out.warnings.push(format!("SteamGridDB : {err}")),
        }
    } else {
        out.warnings.push(
            "Astuce : ajoute une clé API SteamGridDB dans les paramètres pour des artworks plus complets."
                .to_string(),
        );
    }

    if out.hits.is_empty() && out.warnings.is_empty() {
        out.warnings.push("Aucun résultat.".to_string());
    }
    out
}

#[derive(Debug, Deserialize)]
struct SteamSearch {
    #[serde(default)]
    items: Vec<SteamItem>,
}

#[derive(Debug, Deserialize)]
struct SteamItem {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    tiny_image: String,
    #[serde(default)]
    platforms: Option<SteamPlatforms>,
}

#[derive(Debug, Default, Deserialize)]
struct SteamPlatforms {
    #[serde(default)]
    windows: bool,
    #[serde(default)]
    linux: bool,
}

fn steam_search(client: &Client, query: &str, s: &Settings) -> Result<Vec<SearchHit>, String> {
    let resp = client
        .get("https://store.steampowered.com/api/storesearch/")
        .query(&[
            ("term", query),
            ("l", s.language.as_str()),
            ("cc", s.country.as_str()),
        ])
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let body: SteamSearch = resp.json().map_err(|e| e.to_string())?;

    Ok(body
        .items
        .into_iter()
        .filter(|i| i.kind == "app" && i.id > 0 && !i.name.is_empty())
        .take(12)
        .map(|i| {
            let mut tags = vec![format!("AppID {}", i.id)];
            if let Some(p) = &i.platforms {
                if p.linux {
                    tags.push("Linux".to_string());
                } else if p.windows {
                    tags.push("Windows".to_string());
                }
            }
            SearchHit {
                source: Source::Steam,
                title: i.name,
                steam_app_id: Some(i.id),
                sgdb_id: None,
                thumb: if i.tiny_image.is_empty() {
                    None
                } else {
                    Some(i.tiny_image)
                },
                detail: tags.join(" · "),
            }
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct SgdbEnvelope<T> {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct SgdbGame {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    verified: bool,
}

fn sgdb_search(client: &Client, query: &str, s: &Settings) -> Result<Vec<SearchHit>, String> {
    let mut url = reqwest::Url::parse("https://www.steamgriddb.com/api/v2/search/autocomplete/")
        .map_err(|e| e.to_string())?;
    url.path_segments_mut()
        .map_err(|_| "URL invalide".to_string())?
        .push(query);

    let resp = client
        .get(url)
        .bearer_auth(s.sgdb_api_key.trim())
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(match resp.status().as_u16() {
            401 | 403 => "clé API refusée".to_string(),
            429 => "trop de requêtes".to_string(),
            code => format!("HTTP {code}"),
        });
    }
    let body: SgdbEnvelope<Vec<SgdbGame>> = resp.json().map_err(|e| e.to_string())?;
    if !body.success {
        return Err("réponse invalide".to_string());
    }

    Ok(body
        .data
        .unwrap_or_default()
        .into_iter()
        .filter(|g| !g.name.is_empty())
        .take(12)
        .map(|g| SearchHit {
            source: Source::SteamGridDb,
            title: g.name,
            steam_app_id: None,
            sgdb_id: Some(g.id),
            thumb: None,
            detail: if g.verified {
                format!("SteamGridDB #{} · vérifié", g.id)
            } else {
                format!("SteamGridDB #{}", g.id)
            },
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Artworks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct SgdbAsset {
    #[serde(default)]
    url: String,
    #[serde(default)]
    upvotes: i64,
    #[serde(default)]
    downvotes: i64,
}

#[derive(Debug, Default)]
pub struct AssetPlan {
    /// Pour chaque type d'asset, les URL candidates par ordre de préférence.
    pub kinds: Vec<(AssetKind, Vec<String>)>,
    pub warnings: Vec<String>,
}

impl AssetPlan {
    fn add(&mut self, kind: AssetKind, url: String) {
        if url.trim().is_empty() {
            return;
        }
        if let Some(entry) = self.kinds.iter_mut().find(|(k, _)| *k == kind) {
            if !entry.1.contains(&url) {
                entry.1.push(url);
            }
        } else {
            self.kinds.push((kind, vec![url]));
        }
    }

    pub fn candidates(&self, kind: AssetKind) -> &[String] {
        self.kinds
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, v)| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Construit la liste des URL d'artworks à essayer, SteamGridDB d'abord
/// (couvertures et logos de meilleure qualité), puis les assets officiels Steam.
pub fn build_plan(client: &Client, hit: &SearchHit, s: &Settings) -> AssetPlan {
    let mut plan = AssetPlan::default();
    let key = s.sgdb_api_key.trim();

    if !key.is_empty() {
        let ids = match (hit.steam_app_id, hit.sgdb_id) {
            (Some(appid), _) => Some(("steam", appid.to_string())),
            (None, Some(id)) => Some(("game", id.to_string())),
            _ => None,
        };
        if let Some((id_kind, id)) = ids {
            for (kind, route) in [
                (AssetKind::Cover, "grids"),
                (AssetKind::Hero, "heroes"),
                (AssetKind::Logo, "logos"),
                (AssetKind::Icon, "icons"),
            ] {
                match sgdb_assets(client, key, route, id_kind, &id, kind == AssetKind::Cover) {
                    Ok(assets) if !assets.is_empty() => {
                        for url in pick_sgdb(&assets) {
                            plan.add(kind, url);
                        }
                    }
                    Ok(_) => {}
                    Err(err) => plan
                        .warnings
                        .push(format!("SteamGridDB {} : {err}", kind.label())),
                }
            }
        }
    }

    // Assets officiels Steam (CDN) : utilisés en second choix ou si pas de clé.
    if let Some(appid) = hit.steam_app_id {
        for (kind, files) in [
            (AssetKind::Cover, &["library_600x900.jpg", "library_600x900_2x.jpg"][..]),
            (AssetKind::Hero, &["library_hero.jpg"][..]),
            (AssetKind::Logo, &["logo.png", "logo.jpg"][..]),
            (AssetKind::Icon, &["header.jpg"][..]),
        ] {
            if plan.candidates(kind).is_empty() {
                for file in files {
                    for url in steam_cdn(appid, file) {
                        plan.add(kind, url);
                    }
                }
            }
        }
    }

    // Dernier recours pour la couverture : la miniature de la recherche.
    if plan.candidates(AssetKind::Cover).is_empty() {
        if let Some(thumb) = &hit.thumb {
            plan.add(AssetKind::Cover, thumb.clone());
        }
    }

    plan
}

fn steam_cdn(appid: u32, file: &str) -> Vec<String> {
    vec![
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/{file}"),
        format!("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{appid}/{file}"),
    ]
}

/// Les artworks les mieux notés d'abord (SteamGridDB trie déjà par score).
fn pick_sgdb(assets: &[SgdbAsset]) -> Vec<String> {
    let mut list: Vec<&SgdbAsset> = assets.iter().filter(|a| !a.url.is_empty()).collect();
    list.sort_by_key(|a| -(a.upvotes - a.downvotes));
    list.into_iter().map(|a| a.url.clone()).take(3).collect()
}

fn sgdb_assets(
    client: &Client,
    key: &str,
    route: &str,
    id_kind: &str,
    id: &str,
    prefer_grid_size: bool,
) -> Result<Vec<SgdbAsset>, String> {
    let base = format!("https://www.steamgriddb.com/api/v2/{route}/{id_kind}/{id}");

    let filter_sets: Vec<Vec<(&str, String)>> = if prefer_grid_size {
        vec![
            vec![
                ("dimensions", "600x900".to_string()),
                ("nsfw", "false".to_string()),
                ("humor", "false".to_string()),
            ],
            vec![],
        ]
    } else {
        vec![
            vec![
                ("types", "static".to_string()),
                ("nsfw", "false".to_string()),
                ("humor", "false".to_string()),
            ],
            vec![],
        ]
    };

    let mut last_error: Option<String> = None;
    for filters in filter_sets {
        let mut url = reqwest::Url::parse(&base).map_err(|e| e.to_string())?;
        if !filters.is_empty() {
            let mut query = url.query_pairs_mut();
            for (k, v) in &filters {
                query.append_pair(k, v);
            }
        }
        let resp = match client.get(url).bearer_auth(key).send() {
            Ok(r) => r,
            Err(e) => {
                last_error = Some(e.to_string());
                continue;
            }
        };
        if !resp.status().is_success() {
            last_error = Some(format!("HTTP {}", resp.status().as_u16()));
            continue;
        }
        let body: SgdbEnvelope<Vec<SgdbAsset>> = match resp.json() {
            Ok(b) => b,
            Err(e) => {
                last_error = Some(e.to_string());
                continue;
            }
        };
        let data = body.data.unwrap_or_default();
        if !data.is_empty() {
            return Ok(data);
        }
    }

    match last_error {
        Some(err) => Err(err),
        None => Ok(Vec::new()),
    }
}

/// Télécharge le meilleur artwork disponible pour chaque type.
/// Renvoie les artworks obtenus + les avertissements (types non trouvés).
pub fn apply_plan(client: &Client, dir: &Path, plan: &AssetPlan) -> (Artwork, Vec<String>) {
    let mut artwork = Artwork::default();
    let mut warnings = plan.warnings.clone();

    if let Err(err) = std::fs::create_dir_all(dir) {
        warnings.push(format!("Dossier d'artworks inutilisable : {err}"));
        return (artwork, warnings);
    }

    for kind in AssetKind::ALL {
        let candidates = plan.candidates(kind);
        if candidates.is_empty() {
            continue;
        }
        let mut last_error = String::new();
        let mut done = false;
        for url in candidates {
            match download_to(client, url, dir, kind) {
                Ok(path) => {
                    artwork.set(kind, path);
                    done = true;
                    break;
                }
                Err(err) => last_error = err,
            }
        }
        if !done {
            warnings.push(format!("{} introuvable ({last_error})", kind.label()));
        }
    }

    (artwork, warnings)
}

/// Recherche + téléchargement complets pour un jeu (bloquant : à lancer dans un thread).
pub fn fetch(client: &Client, game_id: &str, hit: &SearchHit, s: &Settings) -> (Artwork, Vec<String>) {
    let dir = artwork_root().join(game_id);
    let plan = build_plan(client, hit, s);
    apply_plan(client, &dir, &plan)
}

fn download_to(
    client: &Client,
    url: &str,
    dir: &Path,
    kind: AssetKind,
) -> Result<PathBuf, String> {
    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.starts_with("image/") {
        return Err(format!("type non supporté ({content_type})"));
    }
    let ext = if content_type.contains("png") {
        "png"
    } else if content_type.contains("webp") {
        "webp"
    } else if content_type.contains("svg") {
        return Err("SVG non supporté".to_string());
    } else {
        "jpg"
    };

    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    if bytes.len() < 512 {
        return Err("fichier vide".to_string());
    }

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Supprime un éventuel artwork précédent avec une autre extension.
    for old_ext in ["jpg", "png", "webp"] {
        if old_ext != ext {
            let _ = std::fs::remove_file(dir.join(format!("{}.{old_ext}", kind.stem())));
        }
    }
    let path = dir.join(format!("{}.{ext}", kind.stem()));
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}
