//! Recherche de jeux (Steam + SteamGridDB) et récupération des artworks originaux.
//!
//! Les artworks sont téléchargés une fois dans `~/.local/share/quark/artwork/<id>/`
//! puis affichés depuis le disque : aucune requête réseau à l'affichage.

use crate::config::{Artwork, AssetKind, Game, Settings, artwork_root};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn user_agent() -> String {
    format!(
        "{}/{} (Linux)",
        crate::config::APP_NAME,
        env!("CARGO_PKG_VERSION")
    )
}

pub fn http() -> Client {
    Client::builder()
        .user_agent(user_agent())
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap_or_else(|_| Client::new())
}

/// Dimensions d'une image PNG ou JPEG, lues dans l'en-tête (aucun décodage).
/// Sert à refuser un artwork qui n'a pas la forme attendue.
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() > 24 && bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }
    if bytes.len() > 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2usize;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if marker == 0xFF {
                i += 1;
                continue;
            }
            // Marqueurs sans segment (RST, EOI, TEM).
            if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
                i += 2;
                continue;
            }
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            let is_sof = matches!(
                marker,
                0xC0 | 0xC1
                    | 0xC2
                    | 0xC3
                    | 0xC5
                    | 0xC6
                    | 0xC7
                    | 0xC9
                    | 0xCA
                    | 0xCB
                    | 0xCD
                    | 0xCE
                    | 0xCF
            );
            if is_sof && i + 9 < bytes.len() {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h));
            }
            if len < 2 {
                return None;
            }
            i += 2 + len;
        }
    }
    None
}

/// Dimensions réelles d'une image : lecture de l'en-tête, puis décodage complet
/// quand le format n'y est pas reconnu.
///
/// SteamGridDB sert parfois des WebP : sans ce repli, l'en-tête est illisible et
/// l'artwork est rejeté comme « image illisible » alors qu'il est valide. Les
/// icônes (les plus concernées) étaient ainsi les seules à ne jamais arriver.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if let Some(size) = image_size(bytes) {
        return Some(size);
    }
    image::load_from_memory(bytes)
        .ok()
        .map(|image| (image.width(), image.height()))
}

/// Vérifie qu'une image a la forme attendue pour ce type d'artwork.
fn fits(kind: AssetKind, size: Option<(u32, u32)>) -> bool {
    let Some((w, h)) = size else {
        return false;
    };
    // Une icône SteamGridDB peut être fournie en 16×16 : le seuil des autres
    // types la rejetterait.
    let minimum = if kind == AssetKind::Icon { 16 } else { 32 };
    if w < minimum || h < minimum {
        return false;
    }
    let ratio = h as f32 / w as f32;
    match kind {
        // Couverture verticale (Steam : 2:3, SteamGridDB : 600x900).
        AssetKind::Cover => ratio > 1.15,
        // Bannière large (Steam : 1920x620).
        AssetKind::Hero => ratio < 0.8,
        // Logos et icônes : pas de contrainte de forme, seulement une taille.
        AssetKind::Logo => w * h > 10_000,
        // Le seuil minimum (16×16) suffit : une icône carrée de 256 pixels
        // restait sous l'ancien produit minimal et se faisait rejeter.
        AssetKind::Icon => true,
    }
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

#[derive(Debug, Clone, Default)]
pub struct AssetPlan {
    /// Pour chaque type d'asset, les URL candidates par ordre de préférence.
    pub kinds: Vec<(AssetKind, Vec<String>)>,
    pub warnings: Vec<String>,
}

impl AssetPlan {
    pub fn add(&mut self, kind: AssetKind, url: String) {
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

    /// Même plan, mais avec l'artwork choisi à la main en premier pour chaque
    /// type : les autres propositions restent en secours si le téléchargement
    /// ou le format échoue.
    pub fn with_choices(&self, choices: &[(AssetKind, usize)]) -> AssetPlan {
        let mut out = AssetPlan {
            kinds: Vec::new(),
            warnings: self.warnings.clone(),
        };
        for (kind, urls) in &self.kinds {
            let index = choices
                .iter()
                .find(|(chosen_kind, _)| chosen_kind == kind)
                .map(|(_, index)| *index)
                .unwrap_or(0);
            if let Some(url) = urls.get(index) {
                out.add(*kind, url.clone());
            }
            for url in urls {
                out.add(*kind, url.clone());
            }
        }
        out
    }

    /// Même plan limité à un seul type : remplacer une seule image sans
    /// toucher aux trois autres.
    pub fn only(&self, kind: AssetKind) -> AssetPlan {
        AssetPlan {
            kinds: self
                .kinds
                .iter()
                .filter(|(asset, _)| *asset == kind)
                .cloned()
                .collect(),
            warnings: Vec::new(),
        }
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

    // Assets officiels Steam (CDN puis `appdetails`) : second choix, ou seule
    // source quand aucune clé SteamGridDB n'est configurée.
    if let Some(appid) = hit.steam_app_id {
        for (kind, files) in [
            (
                AssetKind::Cover,
                &["library_600x900_2x.jpg", "library_600x900.jpg"][..],
            ),
            (AssetKind::Hero, &["library_hero.jpg"][..]),
            (
                AssetKind::Logo,
                &["logo.png", "logo_2x.png", "logo.jpg"][..],
            ),
            (
                AssetKind::Icon,
                &["header.jpg", "capsule_231x87.jpg", "capsule_sm_120.jpg"][..],
            ),
        ] {
            // Les icônes gardent les assets Steam en secours même quand
            // SteamGridDB a répondu : une icône absente laisse un monogramme.
            if plan.candidates(kind).is_empty() || kind == AssetKind::Icon {
                for file in files {
                    for url in steam_cdn(appid, file) {
                        plan.add(kind, url);
                    }
                }
            }
        }

        // `appdetails` donne des URL signées qui survivent aux jeux dont les
        // fichiers `library_*` n'existent pas.
        if let Some(details) = steam_details(client, appid, s) {
            for (kind, url) in [
                (AssetKind::Cover, &details.capsule_image),
                (AssetKind::Hero, &details.background_raw),
                (AssetKind::Hero, &details.background),
                (AssetKind::Icon, &details.header_image),
            ] {
                if !url.is_empty() {
                    plan.add(kind, url.clone());
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

#[derive(Debug, Default, Deserialize)]
struct SteamDetails {
    #[serde(default)]
    header_image: String,
    #[serde(default)]
    capsule_image: String,
    #[serde(default)]
    background: String,
    #[serde(default)]
    background_raw: String,
    #[serde(default)]
    short_description: String,
    #[serde(default)]
    developers: Vec<String>,
    #[serde(default)]
    genres: Vec<SteamGenre>,
    #[serde(default)]
    release_date: SteamRelease,
}

#[derive(Debug, Default, Deserialize)]
struct SteamGenre {
    #[serde(default)]
    description: String,
}

#[derive(Debug, Default, Deserialize)]
struct SteamRelease {
    #[serde(default)]
    date: String,
}

/// Métadonnées Steam d'un jeu, affichées dans sa fiche (onglet Informations).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameMetadata {
    pub developer: String,
    pub genres: Vec<String>,
    pub released: String,
    pub description: String,
}

/// Interroge `appdetails` pour le développeur, les genres, la date de sortie et
/// la description courte. Même endpoint que les artworks : une requête de plus
/// quand un jeu est ajouté, rien de récurrent.
pub fn metadata(client: &Client, appid: u32, s: &Settings) -> Result<GameMetadata, String> {
    let details = steam_details(client, appid, s)
        .ok_or_else(|| format!("fiche Steam indisponible pour l'appid {appid}"))?;
    let genres: Vec<String> = details
        .genres
        .iter()
        .map(|genre| genre.description.trim().to_string())
        .filter(|genre| !genre.is_empty())
        .collect();
    Ok(GameMetadata {
        developer: details.developers.first().cloned().unwrap_or_default(),
        genres,
        released: details.release_date.date.trim().to_string(),
        description: details.short_description.trim().to_string(),
    })
}

/// Fiche Steam d'un jeu : sert de repli pour les artworks manquants.
fn steam_details(client: &Client, appid: u32, s: &Settings) -> Option<SteamDetails> {
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(default)]
        success: bool,
        #[serde(default)]
        data: Option<SteamDetails>,
    }

    let resp = client
        .get("https://store.steampowered.com/api/appdetails")
        .query(&[
            ("appids", appid.to_string()),
            ("l", s.language.clone()),
            ("cc", s.country.clone()),
            ("filters", "basic".to_string()),
        ])
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let mut body: std::collections::HashMap<String, Envelope> = resp.json().ok()?;
    let envelope = body.remove(&appid.to_string())?;
    if envelope.success {
        envelope.data
    } else {
        None
    }
}

/// Meilleur résultat pour un titre donné : correspondance exacte d'abord,
/// puis première fiche Steam, puis n'importe quel résultat.
pub fn best_hit(outcome: &SearchOutcome, title: &str) -> Option<SearchHit> {
    let needle = crate::scan::normalize(title);
    let mut steam: Option<&SearchHit> = None;
    let mut any: Option<&SearchHit> = None;
    for hit in &outcome.hits {
        any.get_or_insert(hit);
        if hit.source == Source::Steam {
            if !needle.is_empty() && crate::scan::normalize(&hit.title) == needle {
                return Some(hit.clone());
            }
            steam.get_or_insert(hit);
        }
    }
    steam.or(any).cloned()
}

fn steam_cdn(appid: u32, file: &str) -> Vec<String> {
    vec![
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/{file}"),
        format!(
            "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{appid}/{file}"
        ),
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
            let (bytes, ext) = match download_image(client, url) {
                Ok(value) => value,
                Err(err) => {
                    last_error = err;
                    continue;
                }
            };
            let size = dimensions(&bytes);
            if !fits(kind, size) {
                last_error = match size {
                    Some((w, h)) => format!("format inattendu ({w}×{h})"),
                    None => "image illisible".to_string(),
                };
                continue;
            }
            match save_image(dir, kind, &bytes, ext) {
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
pub fn fetch(
    client: &Client,
    game_id: &str,
    hit: &SearchHit,
    s: &Settings,
) -> (Artwork, Vec<String>) {
    let dir = artwork_root().join(game_id);
    let plan = build_plan(client, hit, s);
    apply_plan(client, &dir, &plan)
}

fn download_image(client: &Client, url: &str) -> Result<(Vec<u8>, &'static str), String> {
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
    // Une icône 16×16 pèse quelques dizaines d'octets : un seuil plus haut la
    // ferait passer pour un fichier vide.
    if bytes.len() < 64 {
        return Err("fichier vide".to_string());
    }
    Ok((bytes.to_vec(), ext))
}

/// Importe une image choisie sur le disque comme artwork du jeu : elle est
/// décodée (donc validée) puis réécrite en PNG, ce qui évite les fichiers
/// concurrents et les extensions trompeuses.
pub fn import_file(source: &Path, dir: &Path, kind: AssetKind) -> Result<PathBuf, String> {
    let bytes = std::fs::read(source).map_err(|e| format!("lecture impossible : {e}"))?;
    let decoded = image::load_from_memory(&bytes).map_err(|e| format!("image illisible : {e}"))?;
    if decoded.width() < 32 || decoded.height() < 32 {
        return Err(format!(
            "image trop petite ({}×{})",
            decoded.width(),
            decoded.height()
        ));
    }
    let mut buffer = std::io::Cursor::new(Vec::new());
    decoded
        .write_to(&mut buffer, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    save_image(dir, kind, &buffer.into_inner(), "png")
}

/// Écrit l'artwork retenu dans le dossier du jeu, en supprimant l'ancienne
/// extension pour ne pas laisser deux fichiers concurrents.
fn save_image(dir: &Path, kind: AssetKind, bytes: &[u8], ext: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for old_ext in ["jpg", "png", "webp"] {
        if old_ext != ext {
            let _ = std::fs::remove_file(dir.join(format!("{}.{old_ext}", kind.stem())));
        }
    }
    let path = dir.join(format!("{}.{ext}", kind.stem()));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    /// WebP 8×8 minimal (VP8L) : l'en-tête PNG/JPEG ne le reconnaît pas, seul le
    /// décodage donne sa taille. C'est exactement le cas qui faisait rejeter
    /// les icônes comme « image illisible ».
    const WEBP_8X8: [u8; 38] = [
        0x52, 0x49, 0x46, 0x46, 0x1E, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50, 0x38,
        0x4C, 0x11, 0x00, 0x00, 0x00, 0x2F, 0x07, 0xC0, 0x01, 0x00, 0x07, 0x50, 0x94, 0x22, 0x17,
        0xA5, 0xFF, 0x81, 0x88, 0xE8, 0x7F, 0x00, 0x00,
    ];

    #[test]
    fn une_icone_webp_est_mesuree_par_decodage() {
        assert_eq!(image_size(&WEBP_8X8), None, "en-tête non reconnu");
        assert_eq!(dimensions(&WEBP_8X8), Some((8, 8)), "décodage de secours");
    }

    #[test]
    fn les_petites_icones_sont_acceptees() {
        assert!(fits(AssetKind::Icon, Some((16, 16))));
        assert!(fits(AssetKind::Icon, Some((32, 32))));
        assert!(!fits(AssetKind::Icon, Some((8, 8))));
        // Les autres types gardent leur seuil et leur forme.
        assert!(!fits(AssetKind::Logo, Some((16, 16))));
        assert!(fits(AssetKind::Logo, Some((256, 128))));
        assert!(fits(AssetKind::Cover, Some((600, 900))));
        assert!(!fits(AssetKind::Cover, Some((900, 600))));
        assert!(fits(AssetKind::Hero, Some((1920, 620))));
    }
}
