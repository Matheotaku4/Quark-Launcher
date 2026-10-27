//! Actualités d'un jeu : flux public Steam (`ISteamNews`).
//!
//! Le flux est récupéré une fois par jeu (voir `QuarkApp::news`), puis affiché
//! sous la bannière, comme la fiche Steam. Aucun accès n'est nécessaire pour
//! lire les actualités : l'API Steam est publique.

use serde::Deserialize;

/// Nombre d'actualités demandées à Steam.
pub const COUNT: usize = 6;

/// Longueur maximale de l'aperçu affiché sous le titre.
const EXCERPT_CHARS: usize = 260;

#[derive(Debug, Clone, PartialEq)]
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
        .map(|raw| NewsItem {
            title: clean(&raw.title),
            url: item_url(app_id, &raw.gid, &raw.url),
            feed: clean(&raw.feedlabel),
            author: clean(&raw.author),
            date: raw.date,
            excerpt: excerpt(&raw.contents, EXCERPT_CHARS),
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
