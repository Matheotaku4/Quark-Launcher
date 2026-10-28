//! Actualités d'un jeu : flux public Steam (`ISteamNews`).
//!
//! Le flux est récupéré une fois par jeu (voir `QuarkApp::news`), puis affiché
//! sous la bannière, comme la fiche Steam. Aucun accès n'est nécessaire pour
//! lire les actualités : l'API Steam est publique.

use serde::Deserialize;
use std::path::PathBuf;

/// Nombre d'actualités demandées à Steam.
pub const COUNT: usize = 6;

/// Longueur maximale de l'aperçu affiché sous le titre.
const EXCERPT_CHARS: usize = 260;

/// Longueur du texte intégré : le billet se lit dans Quark, sans navigateur.
const BODY_CHARS: usize = 900;

/// Hôte des images de la communauté Steam (`{STEAM_CLAN_IMAGE}`).
const CLAN_IMAGES: &str = "https://clan.cloudflare.steamstatic.com/images";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NewsItem {
    pub title: String,
    /// Lien vers la page d'actualité Steam (redirige vers l'article externe).
    pub url: String,
    /// Source du billet (« Community Announcements », « PC Gamer »…).
    pub feed: String,
    pub author: String,
    /// Horodatage Unix (format renvoyé par Steam).
    pub date: i64,
    /// Aperçu en texte brut (balisage HTML et Steam retiré).
    pub excerpt: String,
    /// Texte plus long, affiché quand le billet est déplié dans Quark.
    pub body: String,
    /// URL de la première image du billet.
    pub image_url: Option<String>,
    /// Image téléchargée une fois dans `~/.local/share/quark/news/`.
    pub image: Option<PathBuf>,
}

/// Récupère les dernières actualités d'un appid Steam.
pub fn fetch(
    client: &reqwest::blocking::Client,
    app_id: u32,
    count: usize,
) -> Result<Vec<NewsItem>, String> {
    let url = format!(
        "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/\
         ?appid={app_id}&count={count}&maxlength=1200&format=json"
    );
    let response = client
        .get(&url)
        .send()
        .map_err(|err| format!("réseau : {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Steam a répondu {}", status.as_u16()));
    }
    let feed: Feed = response
        .json()
        .map_err(|err| format!("flux illisible : {err}"))?;

    let mut items: Vec<NewsItem> = feed
        .appnews
        .newsitems
        .into_iter()
        .filter(|raw| !raw.title.trim().is_empty() && !raw.gid.trim().is_empty())
        .map(|raw| {
            let image_url = first_image(&raw.contents);
            let image = image_url
                .as_deref()
                .and_then(|url| cache_image(client, app_id, &raw.gid, url));
            NewsItem {
                title: clean(&raw.title),
                url: item_url(app_id, &raw.gid, &raw.url),
                feed: clean(&raw.feedlabel),
                author: clean(&raw.author),
                date: raw.date,
                excerpt: excerpt(&raw.contents, EXCERPT_CHARS),
                body: excerpt(&raw.contents, BODY_CHARS),
                image_url,
                image,
            }
        })
        .collect();
    items.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(items)
}

/// Page d'actualité Steam : elle existe pour les billets de la communauté comme
/// pour les articles de presse (auquel cas Steam redirige vers l'article).
fn item_url(app_id: u32, gid: &str, raw: &str) -> String {
    if !gid.is_empty() && gid.chars().all(|c| c.is_ascii_digit()) {
        return format!("https://store.steampowered.com/news/app/{app_id}/view/{gid}");
    }
    raw.to_string()
}

/// Ouvre une URL dans le navigateur du bureau, sans bloquer Quark.
pub fn open_in_browser(url: &str) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("lien invalide".to_string());
    }
    // Plusieurs bureaux possibles : on essaie les lanceurs dans l'ordre courant
    // sur Linux et on retient le premier réellement installé.
    let candidates: [&[&str]; 3] = [
        &["xdg-open"],
        &["gio", "open"],
        &["sensible-browser"],
    ];
    for args in candidates {
        let mut cmd = std::process::Command::new(args[0]);
        cmd.args(&args[1..])
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        match cmd.spawn() {
            Ok(_) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("{} : {err}", args[0])),
        }
    }
    Err("aucun navigateur trouvé (xdg-open, gio, sensible-browser)".to_string())
}

// ---------------------------------------------------------------------------
// Images des billets
// ---------------------------------------------------------------------------

/// Première image d'un billet : balise `<img>` ou marqueur `{STEAM_CLAN_IMAGE}`.
pub fn first_image(raw: &str) -> Option<String> {
    // La recherche insensible à la casse se fait sur une copie ASCII : les
    // index restent ceux du texte d'origine.
    let lower = raw.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(offset) = lower[from..].find("<img") {
        let start = from + offset;
        let end = lower[start..]
            .find('>')
            .map(|index| start + index)
            .unwrap_or(raw.len());
        let tag = &raw[start..end.min(raw.len())];
        if let Some(url) = attribute(tag, "src").or_else(|| attribute(tag, "data-src")) {
            if let Some(url) = expand_image(&url) {
                return Some(url);
            }
        }
        from = end.max(start + 4);
        if from >= lower.len() {
            break;
        }
    }
    for marker in ["{STEAM_CLAN_IMAGE}", "{STEAM_CLAN_LOC_IMAGE}"] {
        if let Some(offset) = raw.find(marker) {
            let path: String = raw[offset + marker.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || "/-._".contains(*c))
                .collect();
            if !path.is_empty() {
                return Some(format!("{CLAN_IMAGES}{path}"));
            }
        }
    }
    None
}

/// Valeur d'un attribut dans une balise déjà isolée (`src="…"`).
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(offset) = lower[from..].find(name) {
        let start = from + offset;
        let before_ok = start == 0
            || lower.as_bytes()[start - 1].is_ascii_whitespace()
            || lower.as_bytes()[start - 1] == b'<';
        let rest = &tag[start + name.len()..];
        let after_name = rest.trim_start();
        if before_ok && after_name.starts_with('=') {
            let value = after_name[1..].trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let body = &value[1..];
                let end = body.find(quote)?;
                return Some(body[..end].to_string());
            }
            let end = value
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(value.len());
            return Some(value[..end].to_string());
        }
        from = start + name.len();
    }
    None
}

/// Complète une URL d'image : marqueurs Steam développés, adresses absolues
/// acceptées telles quelles.
fn expand_image(url: &str) -> Option<String> {
    let url = url.trim();
    for marker in ["{STEAM_CLAN_IMAGE}", "{STEAM_CLAN_LOC_IMAGE}"] {
        if let Some(path) = url.strip_prefix(marker) {
            return Some(format!("{CLAN_IMAGES}{path}"));
        }
    }
    if url.starts_with("https://") || url.starts_with("http://") {
        Some(url.to_string())
    } else {
        None
    }
}

/// Télécharge l'image d'un billet dans le cache local (une seule fois par
/// billet) : l'article s'affiche alors sans aucune requête réseau.
fn cache_image(
    client: &reqwest::blocking::Client,
    app_id: u32,
    gid: &str,
    url: &str,
) -> Option<PathBuf> {
    let dir = crate::config::news_dir().join(app_id.to_string());
    std::fs::create_dir_all(&dir).ok()?;
    let mut stem: String = gid
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(40)
        .collect();
    if stem.is_empty() {
        stem = format!("news{}", crate::config::now_ts());
    }
    for ext in ["jpg", "png", "webp"] {
        let candidate = dir.join(format!("{stem}.{ext}"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let response = client.get(url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext = if content_type.contains("png") {
        "png"
    } else if content_type.contains("webp") {
        "webp"
    } else if content_type.starts_with("image/") {
        "jpg"
    } else {
        return None;
    };
    let bytes = response.bytes().ok()?;
    if bytes.len() < 1024 {
        return None;
    }
    let path = dir.join(format!("{stem}.{ext}"));
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

// ---------------------------------------------------------------------------
// Nettoyage du texte
// ---------------------------------------------------------------------------

/// Retire le balisage HTML/BBCode, décodes les entités et normalise les espaces.
pub fn clean(raw: &str) -> String {
    let mut stripped = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => {
                let (inner, closed) = read_until(&mut chars, '>');
                if closed && is_html_tag(&inner) {
                    // Une balise sépare deux mots (« fin</b>Merci »).
                    stripped.push(' ');
                } else {
                    stripped.push('<');
                    stripped.push_str(&inner);
                    if closed {
                        stripped.push('>');
                    }
                }
            }
            '{' => {
                let (inner, closed) = read_until(&mut chars, '}');
                if closed && inner.trim() == "STEAM_CLAN_IMAGE" {
                    // Référence d'image interne de Steam : le chemin qui suit
                    // (« /28424520/913f….png ») n'a rien à faire dans un aperçu.
                    while let Some(next) = chars.peek().copied() {
                        if !(next.is_ascii_alphanumeric() || "/-._".contains(next)) {
                            break;
                        }
                        chars.next();
                    }
                } else {
                    stripped.push('{');
                    stripped.push_str(&inner);
                    if closed {
                        stripped.push('}');
                    }
                }
            }
            '[' => {
                let (inner, closed) = read_until(&mut chars, ']');
                if !closed || !is_steam_tag(&inner) {
                    // Crochet qui n'ouvre pas une balise Steam : c'est du texte
                    // (« Version [1.2] ») que l'on conserve tel quel.
                    stripped.push('[');
                    stripped.push_str(&inner);
                    if closed {
                        stripped.push(']');
                    }
                }
            }
            _ => stripped.push(c),
        }
    }
    collapse(&decode_entities(&stripped))
}

/// Lit jusqu'au caractère `end` (exclu) : renvoie son contenu et indique s'il a
/// bien été trouvé. Au-delà de 300 caractères on considère qu'il n'y en a pas.
fn read_until(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, end: char) -> (String, bool) {
    let mut inner = String::new();
    for c in chars.by_ref() {
        if c == end {
            return (inner, true);
        }
        inner.push(c);
        if inner.chars().count() > 300 {
            break;
        }
    }
    (inner, false)
}

/// `<b>`, `</p>`, `<!-- … -->`… : une vraie balise HTML commence par une lettre,
/// un `/` ou un `!` (un comparaison comme `2 < 3` est donc conservée).
fn is_html_tag(inner: &str) -> bool {
    let first = inner.trim_start().chars().next();
    matches!(first, Some(c) if c == '/' || c == '!' || c.is_ascii_alphabetic())
}

/// `[img]`, `[/b]`, `[url=…]`, `[list]`, `[*]`… : le nom est alphanumérique.
fn is_steam_tag(inner: &str) -> bool {
    let body = inner.trim();
    let body = body.strip_prefix('/').unwrap_or(body);
    if body.is_empty() {
        return false;
    }
    let name = body.split('=').next().unwrap_or("");
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '*' || c == '#')
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let rest = &text[index..];
        let end = rest.find(';').unwrap_or(0);
        let entity = if end > 1 && end <= 10 { &rest[1..end] } else { "" };
        let decoded = match entity {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" | "39" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "nbsp" | "160" => Some(' '),
            "hellip" => Some('…'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "rsquo" => Some('\''),
            "lsquo" => Some('\''),
            "rdquo" | "ldquo" => Some('"'),
            other => other.strip_prefix('#').and_then(|num| {
                if let Some(hex) = num.strip_prefix(['x', 'X']) {
                    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                } else {
                    num.parse::<u32>().ok().and_then(char::from_u32)
                }
            }),
        };
        match decoded {
            Some(decoded) => {
                out.push(decoded);
                for _ in 0..end {
                    chars.next();
                }
            }
            None => out.push('&'),
        }
    }
    out
}

/// Réduit toute suite d'espaces (retours à la ligne compris) à un espace, et
/// supprime l'espace qui précéderait une ponctuation fermante (« tous . »).
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() && !matches!(c, '.' | ',' | ')' | ']' | '}' | '…') {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    out
}

/// Aperçu tronqué proprement : coupe au dernier mot et ajoute des points.
pub fn excerpt(raw: &str, max: usize) -> String {
    let text = clean(raw);
    if text.chars().count() <= max {
        return text;
    }
    let mut out: String = text.chars().take(max).collect();
    if let Some(index) = out.rfind(' ') {
        if index > max / 2 {
            out.truncate(index);
        }
    }
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// Désérialisation du flux
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Feed {
    #[serde(default)]
    appnews: AppNews,
}

#[derive(Deserialize, Default)]
struct AppNews {
    #[serde(default)]
    newsitems: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    #[serde(default)]
    gid: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    contents: String,
    #[serde(default)]
    feedlabel: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    date: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retire_le_balisage_html_et_les_entites() {
        let raw = "<b>Mise à jour</b> : les &quot;boss&quot; arrivent&nbsp;!<br>Merci <a href=\"x\">à tous</a>.";
        assert_eq!(
            clean(raw),
            "Mise à jour : les \"boss\" arrivent ! Merci à tous."
        );
    }

    #[test]
    fn retire_le_balisage_steam() {
        assert_eq!(clean("Patch [b]1.2[/b] [img]x.png[/img]"), "Patch 1.2 x.png");
    }

    #[test]
    fn garde_le_texte_entre_crochets() {
        assert_eq!(clean("Version [1.2] disponible"), "Version [1.2] disponible");
        assert_eq!(clean("Notes [non publiées"), "Notes [non publiées");
    }

    #[test]
    fn retire_les_images_internes_de_steam() {
        let raw = "{STEAM_CLAN_IMAGE}/28424520/913f4104e25e335cbfc6f5e9cdb40891e50d512a.png Hey there!";
        assert_eq!(clean(raw), "Hey there!");
    }

    #[test]
    fn pas_d_espace_avant_la_ponctuation() {
        assert_eq!(clean("Merci à tous</a>."), "Merci à tous.");
    }

    #[test]
    fn garde_une_comparaison_en_angle() {
        assert_eq!(clean("2 < 3 et 4 > 1"), "2 < 3 et 4 > 1");
    }

    #[test]
    fn apercu_coupe_au_mot() {
        let text = excerpt("Un texte assez long pour être tronqué proprement ici", 20);
        assert_eq!(text, "Un texte assez long…");
    }

    #[test]
    fn lien_pointe_vers_la_fiche_steam() {
        assert_eq!(
            item_url(391540, "1839041357036793", "https://exemple.test/x"),
            "https://store.steampowered.com/news/app/391540/view/1839041357036793"
        );
    }

    #[test]
    fn flux_reel_est_analyse() {
        let json = r#"{"appnews":{"appid":570,"newsitems":[
            {"gid":"1","title":"Patch 7.39","url":"https://x/1","contents":"<p>Nouveau héros</p>","feedlabel":"Dota 2","date":100},
            {"gid":"2","title":"","url":"https://x/2","contents":"vide","feedlabel":"X","date":200}
        ]}}"#;
        let feed: Feed = serde_json::from_str(json).expect("flux valide");
        assert_eq!(feed.appnews.newsitems.len(), 2);
        // Le billet sans titre est écarté par `fetch`.
        let kept = feed
            .appnews
            .newsitems
            .into_iter()
            .filter(|raw| !raw.title.trim().is_empty())
            .count();
        assert_eq!(kept, 1);
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;

    #[test]
    fn trouve_l_image_d_un_billet() {
        let raw = "<p>Salut</p><img src=\"{STEAM_CLAN_IMAGE}/12345/abc.png\"><p>suite</p>";
        assert_eq!(
            first_image(raw).as_deref(),
            Some("https://clan.cloudflare.steamstatic.com/images/12345/abc.png")
        );
        // Marqueur nu, sans balise autour.
        let bare = "images {STEAM_CLAN_IMAGE}/9/photo.jpg ici";
        assert_eq!(
            first_image(bare).as_deref(),
            Some("https://clan.cloudflare.steamstatic.com/images/9/photo.jpg")
        );
        // Adresse absolue classique.
        assert_eq!(
            first_image("<img class=\"x\" src=\"https://exemple.test/a.png\">").as_deref(),
            Some("https://exemple.test/a.png")
        );
        // Rien d'exploitable : pas d'image plutôt qu'une URL inventée.
        assert_eq!(first_image("aucune image ici"), None);
        assert_eq!(first_image("<img src=\"relative.png\">"), None);
    }

    #[test]
    fn le_texte_integre_est_plus_long_que_l_apercu() {
        let body = excerpt(&"mot ".repeat(400), BODY_CHARS);
        assert!(body.chars().count() > EXCERPT_CHARS);
        assert!(body.ends_with('…'));
    }
}
