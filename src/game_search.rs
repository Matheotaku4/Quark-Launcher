//! Recherche d'un jeu de la boutique sur les sites de téléchargement.
//!
//! Chaque source est interrogée séparément dans un fil : l'interface affiche
//! les résultats au fur et à mesure, sans bloquer la boutique. Aucun fichier
//! n'est téléchargé ici ; un résultat ouvre sa page dans le navigateur.
//!
//! Trois sites publient une API JSON (AnkerGames, GameBounty, Kryoto), scène.cat
//! rend ses résultats dans le HTML, et AstralGames ne les rend qu'après
//! exécution de son JavaScript : on passe alors par le lecteur texte de Jina.

use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

/// Les cinq sources demandées, dans l'ordre d'affichage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchSite {
    AnkerGames,
    AstralGames,
    GameBounty,
    Kryoto,
    SceneCat,
}

impl SearchSite {
    pub const ALL: [SearchSite; 5] = [
        SearchSite::AnkerGames,
        SearchSite::AstralGames,
        SearchSite::GameBounty,
        SearchSite::Kryoto,
        SearchSite::SceneCat,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SearchSite::AnkerGames => "AnkerGames",
            SearchSite::AstralGames => "AstralGames",
            SearchSite::GameBounty => "GameBounty",
            SearchSite::Kryoto => "Kryoto",
            SearchSite::SceneCat => "scene.cat",
        }
    }

    pub fn host(self) -> &'static str {
        match self {
            SearchSite::AnkerGames => "ankergames.net",
            SearchSite::AstralGames => "astralgames.net",
            SearchSite::GameBounty => "gamebounty.world",
            SearchSite::Kryoto => "kryo.to",
            SearchSite::SceneCat => "scene.cat",
        }
    }

    /// Recherche du site, utilisée quand l'adaptateur n'a rien pu lire.
    pub fn search_url(self, query: &str) -> String {
        let base = match self {
            SearchSite::AnkerGames => "https://ankergames.net/search",
            SearchSite::AstralGames => "https://astralgames.net/search",
            SearchSite::GameBounty => "https://gamebounty.world/search",
            SearchSite::Kryoto => "https://kryo.to/catalog",
            SearchSite::SceneCat => "https://scene.cat/",
        };
        let mut url = reqwest::Url::parse(base).expect("adresse de recherche fixe");
        url.query_pairs_mut().append_pair("q", query);
        url.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteHit {
    pub title: String,
    pub url: String,
    /// Taille, version ou catégorie : une ligne courte, jamais du HTML.
    pub detail: String,
    pub cover: Option<String>,
}

/// Interroge une source. Une erreur ne contient jamais le corps de la réponse.
pub fn search(site: SearchSite, query: &str) -> Result<Vec<SiteHit>, String> {
    let query = query.trim();
    if query.len() < 2 {
        return Ok(Vec::new());
    }
    let hits = match site {
        SearchSite::AnkerGames => search_anker(query)?,
        SearchSite::AstralGames => search_astral(query)?,
        SearchSite::GameBounty => search_gamebounty(query)?,
        SearchSite::Kryoto => search_kryoto(query)?,
        SearchSite::SceneCat => search_scene(query)?,
    };
    Ok(filter_relevant(hits, query).into_iter().take(8).collect())
}

/// Écarte les résultats qui ne reprennent pas le titre recherché.
/// Les moteurs des sites sont parfois très larges : « Portal 2 » y renvoyait
/// aussi « POSTAL 2 » et « Portal Knights », sans rapport avec la demande.
fn filter_relevant(hits: Vec<SiteHit>, query: &str) -> Vec<SiteHit> {
    let wanted = search_key(query);
    if wanted.is_empty() {
        return hits;
    }
    hits.into_iter()
        .filter(|hit| search_key(&hit.title).contains(&wanted))
        .collect()
}

/// Réduit un titre à ses lettres et chiffres, sans mots vides. La ponctuation
/// des titres varie d'un site à l'autre (« S.T.A.L.K.E.R. » / « STALKER »,
/// « Half-Life » / « Half Life ») et ne doit pas faire perdre un résultat.
fn search_key(text: &str) -> String {
    const STOP_WORDS: [&str; 10] = [
        "the", "an", "of", "and", "le", "la", "les", "de", "des", "du",
    ];
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        // Les mots d'une seule lettre restent : « a » ou « s » font partie
        // d'acronymes comme « S.T.A.L.K.E.R. » ; les chiffres restent aussi.
        .filter(|token| !token.is_empty() && !(token.len() > 1 && STOP_WORDS.contains(token)))
        .collect()
}

fn fetch_text(url: &str, accept: &str) -> Result<String, String> {
    let response = crate::artwork::http()
        .get(url)
        .header(reqwest::header::ACCEPT, accept)
        .timeout(Duration::from_secs(22))
        .send()
        .map_err(|_| "site injoignable".to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "le site a refusé la recherche (HTTP {})",
            response.status().as_u16()
        ));
    }
    crate::net::text(response, crate::net::MAX_HTML, "réponse du site")
}

fn json_root(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|_| "réponse JSON illisible".to_string())
}

fn http_url(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        .map(str::to_string)
}

fn clean_detail(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn size_from_gb(value: Option<&Value>) -> Option<String> {
    let size = match value? {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    (size > 0.0).then(|| format!("{size:.1} Go").replace('.', ","))
}

// ---------------------------------------------------------------------------
// AnkerGames
// ---------------------------------------------------------------------------

fn search_anker(query: &str) -> Result<Vec<SiteHit>, String> {
    let mut url = reqwest::Url::parse("https://ankergames.net/api/v1/games")
        .map_err(|_| "adresse AnkerGames invalide".to_string())?;
    url.query_pairs_mut()
        .append_pair("search", query)
        .append_pair("limit", "8");
    let text = fetch_text(url.as_str(), "application/json")?;
    parse_anker(&text)
}

fn parse_anker(text: &str) -> Result<Vec<SiteHit>, String> {
    let root = json_root(text)?;
    let items = root
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "AnkerGames n'a pas renvoyé de liste".to_string())?;
    Ok(items
        .iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.trim();
            let slug = item.get("slug")?.as_str()?.trim();
            if title.is_empty() || slug.is_empty() {
                return None;
            }
            let version = item
                .get("version")
                .and_then(|version| version.get("label"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let detail = clean_detail(&[
                &size_from_gb(item.get("size_gb")).unwrap_or_default(),
                version,
            ]);
            Some(SiteHit {
                title: title.to_string(),
                url: format!("https://ankergames.net/game/{slug}"),
                detail,
                cover: http_url(item.get("cover")).or_else(|| http_url(item.get("poster"))),
            })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// AstralGames (résultats rendus par le lecteur Jina)
// ---------------------------------------------------------------------------

fn search_astral(query: &str) -> Result<Vec<SiteHit>, String> {
    let mut target = reqwest::Url::parse("https://astralgames.net/search")
        .map_err(|_| "adresse AstralGames invalide".to_string())?;
    target.query_pairs_mut().append_pair("q", query);
    let url = format!("https://r.jina.ai/{target}");
    let text = fetch_text(&url, "text/plain")?;
    Ok(parse_astral_markdown(&text))
}

fn parse_astral_markdown(text: &str) -> Vec<SiteHit> {
    let marker = "](https://astralgames.net/game/";
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    let mut rest = text;
    while let Some(position) = rest.find(marker) {
        let link_start = position + 2;
        let Some(end) = rest[link_start..].find(')') else {
            break;
        };
        let url = &rest[link_start..link_start + end];
        let before = &rest[..position];
        let title = before
            .rfind("### ")
            .map(|start| before[start + 4..].trim())
            .unwrap_or_default()
            .trim_end_matches('~')
            .trim();
        if !title.is_empty() && seen.insert(url.to_string()) {
            hits.push(SiteHit {
                title: decode_entities(title),
                url: url.to_string(),
                detail: String::new(),
                cover: None,
            });
        }
        rest = &rest[link_start + end + 1..];
    }
    hits
}

// ---------------------------------------------------------------------------
// GameBounty
// ---------------------------------------------------------------------------

fn search_gamebounty(query: &str) -> Result<Vec<SiteHit>, String> {
    let mut url = reqwest::Url::parse("https://gamebounty.world/api/v1/search")
        .map_err(|_| "adresse GameBounty invalide".to_string())?;
    url.query_pairs_mut().append_pair("q", query);
    let text = fetch_text(url.as_str(), "application/json")?;
    parse_gamebounty(&text)
}

fn parse_gamebounty(text: &str) -> Result<Vec<SiteHit>, String> {
    let root = json_root(text)?;
    if root.get("success").and_then(Value::as_bool) == Some(false) {
        return Err("GameBounty a refusé la recherche".to_string());
    }
    let items = root
        .get("data")
        .and_then(|data| data.get("results"))
        .and_then(Value::as_array)
        .ok_or_else(|| "GameBounty n'a pas renvoyé de liste".to_string())?;
    Ok(items
        .iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.trim();
            let slug = item.get("slug")?.as_str()?.trim();
            if title.is_empty() || slug.is_empty() {
                return None;
            }
            let size = item
                .get("size_human")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = item.get("version").and_then(Value::as_str).unwrap_or("");
            Some(SiteHit {
                title: title.to_string(),
                url: format!("https://gamebounty.world/{slug}-free-pc-download"),
                detail: clean_detail(&[size, version]),
                cover: http_url(item.get("library_capsule"))
                    .or_else(|| http_url(item.get("banner"))),
            })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Kryoto
// ---------------------------------------------------------------------------

fn search_kryoto(query: &str) -> Result<Vec<SiteHit>, String> {
    let mut url = reqwest::Url::parse("https://kryo.to/api/games/search")
        .map_err(|_| "adresse Kryoto invalide".to_string())?;
    url.query_pairs_mut().append_pair("q", query);
    let text = fetch_text(url.as_str(), "application/json")?;
    parse_kryoto(&text)
}

fn parse_kryoto(text: &str) -> Result<Vec<SiteHit>, String> {
    let root = json_root(text)?;
    let items = root
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| "Kryoto n'a pas renvoyé de liste".to_string())?;
    Ok(items
        .iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.trim();
            let slug = item.get("slug")?.as_str()?.trim();
            if title.is_empty() || slug.is_empty() {
                return None;
            }
            let developer = item.get("developer").and_then(Value::as_str).unwrap_or("");
            let year = item
                .get("year")
                .and_then(Value::as_u64)
                .map(|year| year.to_string())
                .unwrap_or_default();
            Some(SiteHit {
                title: title.to_string(),
                url: format!("https://kryo.to/game/{slug}"),
                detail: clean_detail(&[developer, &year]),
                cover: http_url(item.get("cover_vertical"))
                    .or_else(|| http_url(item.get("cover"))),
            })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// scene.cat
// ---------------------------------------------------------------------------

fn search_scene(query: &str) -> Result<Vec<SiteHit>, String> {
    let mut url = reqwest::Url::parse("https://scene.cat/")
        .map_err(|_| "adresse scene.cat invalide".to_string())?;
    url.query_pairs_mut().append_pair("q", query);
    let text = fetch_text(url.as_str(), "text/html")?;
    parse_scene(&text)
}

fn parse_scene(text: &str) -> Result<Vec<SiteHit>, String> {
    if text.contains("Just a moment") || text.contains("cf_chl_opt") {
        return Err("scene.cat demande une vérification dans un navigateur".to_string());
    }
    let marker = "<div class=\"release-row";
    if !text.contains(marker) {
        // Une recherche vide peut légitimement n'avoir aucune ligne.
        return Ok(Vec::new());
    }
    let mut hits = Vec::new();
    for block in text.split(marker).skip(1) {
        let Some(id) = between(block, "id=\"release-", "\"").filter(|id| {
            !id.is_empty() && id.chars().all(|character| character.is_ascii_digit())
        }) else {
            continue;
        };
        let Some(raw_title) = between(block, "release-name\" title=\"", "\"") else {
            continue;
        };
        let title = decode_entities(raw_title).trim().to_string();
        if title.is_empty() {
            continue;
        }
        let category = between(block, "Category: <span class=\"text-zinc-400 font-normal\">", "</span>")
            .unwrap_or("")
            .trim();
        let size = between(block, "data-size=\"", "\"")
            .and_then(|size| size.parse::<u64>().ok())
            .filter(|size| *size > 0)
            .map(crate::disk::format_bytes)
            .unwrap_or_default();
        let cover = block
            .find("class=\"metadata-cover")
            .and_then(|position| between(&block[position..], "src=\"", "\""))
            .map(|src| {
                if src.starts_with("//") {
                    format!("https:{src}")
                } else {
                    src.to_string()
                }
            })
            .filter(|src| src.starts_with("https://") || src.starts_with("http://"));
        hits.push(SiteHit {
            title,
            url: format!("https://scene.cat/release/{id}"),
            detail: clean_detail(&[category, &size]),
            cover,
        });
        if hits.len() >= 8 {
            break;
        }
    }
    Ok(hits)
}

/// Texte entre deux bornes, sans consommer la suite du bloc.
fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = &text[text.find(start)? + start.len()..];
    let length = rest.find(end)?;
    Some(&rest[..length])
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find('&') {
        out.push_str(&rest[..position]);
        let entity = &rest[position..];
        let decoded = entity
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| decode_entity(&entity[1..end]).map(|value| (value, end)));
        if let Some((value, end)) = decoded {
            out.push_str(value);
            rest = &entity[end + 1..];
        } else {
            out.push('&');
            rest = &entity[1..];
        }
    }
    out.push_str(rest);
    out
}

fn decode_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "quot" => "\"",
        "apos" | "#39" | "#x27" | "#X27" => "'",
        "lt" => "<",
        "gt" => ">",
        "nbsp" => " ",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anker_lit_son_api_json() {
        let body = r#"{"data":[
          {"title":"Cyberpunk 2077","slug":"cyberpunk-2077","size_gb":72.5,
           "cover":"https://ankergames.net/cover.jpg",
           "version":{"label":"v2.5 Hotfix"}},
          {"title":"","slug":"ignore"}
        ]}"#;
        let hits = parse_anker(body).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Cyberpunk 2077");
        assert_eq!(hits[0].url, "https://ankergames.net/game/cyberpunk-2077");
        assert_eq!(hits[0].detail, "72,5 Go · v2.5 Hotfix");
        assert_eq!(
            hits[0].cover.as_deref(),
            Some("https://ankergames.net/cover.jpg")
        );
    }

    #[test]
    fn gamebounty_construit_le_lien_canonique() {
        let body = r#"{"success":true,"data":{"results":[
          {"title":"Cyberpunk 2077","slug":"cp2077","size_human":"116.1GB",
           "version":"Build 26161058","library_capsule":"https://cdn/cover.jpg"}
        ]}}"#;
        let hits = parse_gamebounty(body).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].url,
            "https://gamebounty.world/cp2077-free-pc-download"
        );
        assert_eq!(hits[0].detail, "116.1GB · Build 26161058");
        assert_eq!(hits[0].cover.as_deref(), Some("https://cdn/cover.jpg"));
    }

    #[test]
    fn kryoto_lit_les_resultats_et_refuse_une_reponse_vide() {
        let body = r#"{"results":[
          {"title":"Cyberpunk 2077","slug":"cyberpunk-2077","developer":"CD PROJEKT RED",
           "year":2020,"cover_vertical":"https://cdn/cover.jpg"}
        ]}"#;
        let hits = parse_kryoto(body).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://kryo.to/game/cyberpunk-2077");
        assert_eq!(hits[0].detail, "CD PROJEKT RED · 2020");
        assert!(parse_kryoto("{}").is_err());
    }

    #[test]
    fn scene_lit_une_ligne_html_et_decode_le_titre() {
        let body = r#"<div role="list">
          <div class="release-row group" id="release-6565">
            <span class="text-lg font-medium break-all release-name" title="Cyberpunk.SFX-TENOKE">Cyberpunk.SFX-TENOKE</span>
            <a href="/release/6565">lien</a>
            <span>Category: <span class="text-zinc-400 font-normal">iso</span></span>
            <span class="size-cell" data-size="879247148"></span>
            <div class="metadata-cover foo"><img src="https://cdn/cover.jpg"></div>
          </div>
        </div>"#;
        let hits = parse_scene(body).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Cyberpunk.SFX-TENOKE");
        assert_eq!(hits[0].url, "https://scene.cat/release/6565");
        assert_eq!(hits[0].detail, "iso · 838,5 Mo");
        assert_eq!(hits[0].cover.as_deref(), Some("https://cdn/cover.jpg"));
    }

    #[test]
    fn une_page_scene_sans_resultat_n_est_pas_une_erreur() {
        assert!(parse_scene("<html><body>Rien ici</body></html>")
            .unwrap()
            .is_empty());
        assert!(parse_scene("Just a moment").is_err());
    }

    #[test]
    fn astral_lit_les_liens_du_lecteur_texte() {
        let body = "\
## Search Results

4 games found for “cyberpunk”

[![Image 2: Cyberpunk 2077](https://img/cover.jpg) Action RPG ### Cyberpunk 2077 ~](https://astralgames.net/game/cyberpunk-2077)[![Image 3: VA-11 Hall-A](https://img/va.jpg) Indie ### VA-11 Hall-A: Cyberpunk Bartender Action ~](https://astralgames.net/game/va-11-hall-a)";
        let hits = parse_astral_markdown(body);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Cyberpunk 2077");
        assert_eq!(hits[0].url, "https://astralgames.net/game/cyberpunk-2077");
        assert_eq!(
            hits[1].url,
            "https://astralgames.net/game/va-11-hall-a"
        );
    }

    #[test]
    fn les_entites_html_des_titres_sont_decodees() {
        assert_eq!(
            decode_entities("Tom &amp; Jerry &#39;Deluxe&#39;"),
            "Tom & Jerry 'Deluxe'"
        );
        assert_eq!(decode_entities("Rien & rien"), "Rien & rien");
    }

    #[test]
    fn les_resultats_hors_sujet_sont_ecartes() {
        let hits = vec![
            SiteHit {
                title: "Portal 2".into(),
                url: "https://example/portal-2".into(),
                detail: String::new(),
                cover: None,
            },
            SiteHit {
                title: "POSTAL 2".into(),
                url: "https://example/postal-2".into(),
                detail: String::new(),
                cover: None,
            },
            SiteHit {
                title: "Portal".into(),
                url: "https://example/portal".into(),
                detail: String::new(),
                cover: None,
            },
            SiteHit {
                title: "Portal Knights".into(),
                url: "https://example/portal-knights".into(),
                detail: String::new(),
                cover: None,
            },
        ];
        let kept = filter_relevant(hits, "Portal 2");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].title, "Portal 2");
        // La ponctuation varie d'un site à l'autre sans perdre le résultat.
        assert_eq!(search_key("The Witcher 3: Wild Hunt"), "witcher3wildhunt");
        assert_eq!(search_key("S.T.A.L.K.E.R. 2"), search_key("STALKER 2"));
        assert_eq!(search_key("Half-Life 2"), search_key("Half Life 2"));
    }

    #[test]
    fn les_cinq_sources_ont_une_recherche_web_de_repli() {
        for site in SearchSite::ALL {
            let url = site.search_url("The Witcher 3");
            assert!(url.starts_with("https://"));
            assert!(url.contains("The+Witcher+3"));
        }
    }
}
