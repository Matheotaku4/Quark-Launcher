//! Import de la bibliothèque Steam installée (`appmanifest_*.acf`).
//!
//! Quark lit les mêmes fichiers que Steam : `libraryfolders.vdf` donne les
//! bibliothèques (disques, dossiers personnalisés), chaque `appmanifest` donne
//! l'AppID, le nom et le dossier d'installation. L'exécutable est ensuite
//! cherché dans ce dossier — Steam ne le note nulle part.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Un jeu Steam installé, prêt à être ajouté à Quark.
pub struct SteamGame {
    pub app_id: u32,
    pub name: String,
    pub dir: PathBuf,
    /// `None` : aucun exécutable trouvé, le jeu sera surligné en rouge jusqu'à
    /// ce que le chemin soit corrigé dans ses paramètres.
    pub exe: Option<PathBuf>,
}

/// Racines possibles d'une installation Steam (paquet, Flatpak, Snap).
pub fn steam_roots() -> Vec<PathBuf> {
    let home = crate::config::home_dir();
    let mut roots = vec![
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".steam/root"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        home.join("snap/steam/common/.local/share/Steam"),
    ];
    roots.retain(|root| root.is_dir());
    roots.dedup();
    roots
}

/// Dossiers `steamapps` de toutes les bibliothèques Steam.
pub fn libraries() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for root in steam_roots() {
        let steamapps = root.join("steamapps");
        if steamapps.is_dir() && !dirs.contains(&steamapps) {
            dirs.push(steamapps.clone());
        }
        let vdf = steamapps.join("libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            for path in vdf_paths(&text) {
                let candidate = PathBuf::from(path).join("steamapps");
                if candidate.is_dir() && !dirs.contains(&candidate) {
                    dirs.push(candidate);
                }
            }
        }
    }
    dirs
}

/// Chemins listés par `libraryfolders.vdf` (ancien et nouveau format).
pub fn vdf_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut key: Option<String> = None;
    for token in tokens(text) {
        match (&key, token.as_str()) {
            (None, "path") => key = Some("path".to_string()),
            (Some(_), value) => {
                paths.push(value.to_string());
                key = None;
            }
            _ => {}
        }
    }
    // Ancien format : { "1" "/mnt/jeux/SteamLibrary" "2" "..." } — la clé est
    // un numéro, la valeur qui suit est le chemin.
    if paths.is_empty() {
        let mut index = tokens(text).into_iter().peekable();
        while let Some(token) = index.next() {
            let numeric_key = token.len() <= 3 && token.chars().all(|c| c.is_ascii_digit());
            if numeric_key {
                if let Some(next) = index.peek() {
                    if next.starts_with('/') {
                        paths.push(next.clone());
                        index.next();
                    }
                }
            }
        }
    }
    paths
}

/// Jeux Steam installés, une entrée par `appmanifest_*.acf`.
pub fn installed() -> Result<Vec<SteamGame>, String> {
    let libraries = libraries();
    if libraries.is_empty() {
        return Err(
            "aucune installation Steam trouvée (~/.local/share/Steam, Flatpak, Snap)".to_string(),
        );
    }
    let mut games: Vec<SteamGame> = Vec::new();
    for library in libraries {
        let Ok(entries) = std::fs::read_dir(&library) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some((app_id, title, installdir)) = manifest(&text) else {
                continue;
            };
            if games.iter().any(|game| game.app_id == app_id) {
                continue;
            }
            let dir = library.join("common").join(&installdir);
            let exe = find_exe(&dir, &title, &installdir);
            games.push(SteamGame {
                app_id,
                name: title,
                dir,
                exe,
            });
        }
    }
    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    if games.is_empty() {
        return Err("Steam ne signale aucun jeu installé".to_string());
    }
    Ok(games)
}

/// Extrait `(appid, nom, dossier d'installation)` d'un manifeste Steam.
pub fn manifest(text: &str) -> Option<(u32, String, String)> {
    let values = key_values(text);
    let app_id = values.get("appid")?.parse::<u32>().ok()?;
    let name = values.get("name")?.clone();
    let installdir = values.get("installdir")?.clone();
    if name.is_empty() || installdir.is_empty() {
        return None;
    }
    Some((app_id, name, installdir))
}

/// Couples clé/valeur d'un fichier VDF/ACF, blocs `{ … }` ignorés.
fn key_values(text: &str) -> HashMap<String, String> {
    let tokens = all_tokens(text);
    let mut map = HashMap::new();
    let mut index = 0;
    while index < tokens.len() {
        let key = &tokens[index];
        if key == "{" || key == "}" {
            index += 1;
            continue;
        }
        match tokens.get(index + 1).map(String::as_str) {
            // Un bloc s'ouvre : la clé n'a pas de valeur.
            Some("{") => index += 2,
            Some("}") | None => index += 1,
            Some(value) => {
                map.insert(key.to_lowercase(), value.to_string());
                index += 2;
            }
        }
    }
    map
}

/// Chaînes entre guillemets et accolades, dans l'ordre du fichier.
fn all_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.char_indices();
    while let Some((start, character)) = chars.next() {
        match character {
            '{' => out.push("{".to_string()),
            '}' => out.push("}".to_string()),
            '"' => {
                let value_start = start + 1;
                let mut value_end = None;
                for (index, next) in chars.by_ref() {
                    if next == '"' {
                        value_end = Some(index);
                        break;
                    }
                }
                if let Some(end) = value_end {
                    out.push(text[value_start..end].to_string());
                }
            }
            _ => {}
        }
    }
    out
}

/// Chaînes entre guillemets, dans l'ordre du fichier.
fn tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else {
            break;
        };
        tokens.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    tokens
}

/// Exécutable le plus plausible d'un dossier de jeu.
///
/// Les noms génériques (désinstalleur, redistribuables, rapports de plantage)
/// sont écartés ; un exécutable dont le nom ressemble au jeu est préféré, sinon
/// le plus gros fichier — c'est presque toujours le jeu lui-même.
pub fn find_exe(dir: &Path, title: &str, installdir: &str) -> Option<PathBuf> {
    const NOISE: [&str; 15] = [
        "unins",
        "setup",
        "vcredist",
        "vc_redist",
        "dxsetup",
        "directx",
        "dotnet",
        "crashhandler",
        "crashreport",
        "unitycrashhandler",
        "eac_launcher",
        "battleye",
        "prereq",
        "install",
        "report",
    ];
    let wanted: Vec<String> = [title, installdir]
        .iter()
        .map(|text| crate::scan::normalize(text))
        .filter(|text| !text.is_empty())
        .collect();

    let mut found: Vec<(u64, bool, PathBuf)> = Vec::new();
    walk(dir, 0, &mut found);
    let mut candidates: Vec<(u64, bool, PathBuf)> = found
        .into_iter()
        .filter(|(_, _, path)| {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            !NOISE.iter().any(|noise| name.contains(noise))
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| b.0.cmp(&a.0))
            .then_with(|| a.2.cmp(&b.2))
    });
    let _ = &wanted;
    candidates.into_iter().next().map(|(_, _, path)| path)
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<(u64, bool, PathBuf)>) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            // Dossiers de support : jamais le jeu.
            if matches!(
                name.as_str(),
                "redist" | "_commonredist" | "support" | "installer" | "directx" | "dotnet"
            ) {
                continue;
            }
            walk(&path, depth + 1, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            let stem = path
                .file_stem()
                .map(|stem| crate::scan::normalize(&stem.to_string_lossy()))
                .unwrap_or_default();
            // Un exécutable dont le nom est contenu dans celui du dossier (ou
            // l'inverse) est un bon candidat : « Witcher3.exe » dans
            // « The Witcher 3 ».
            let matches_name = !stem.is_empty()
                && (dir_hint(&path)
                    .map(|hint| hint.contains(&stem) || stem.contains(&hint))
                    .unwrap_or(false));
            out.push((size, matches_name, path));
        }
    }
}

/// Nom du dossier de jeu au-dessus de l'exécutable, normalisé.
fn dir_hint(path: &Path) -> Option<String> {
    path.parent()
        .and_then(|parent| parent.parent())
        .and_then(|parent| parent.file_name())
        .map(|name| crate::scan::normalize(&name.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lit_un_manifeste_steam() {
        let text = r#"
"AppState"
{
	"appid"		"292030"
	"name"		"The Witcher 3: Wild Hunt"
	"installdir"		"The Witcher 3"
	"StateFlags"		"4"
}
"#;
        let (app_id, name, dir) = manifest(text).expect("manifeste");
        assert_eq!(app_id, 292_030);
        assert_eq!(name, "The Witcher 3: Wild Hunt");
        assert_eq!(dir, "The Witcher 3");
    }

    #[test]
    fn les_blocs_ne_cassent_pas_les_couples() {
        let text = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/matheo/.local/share/Steam"
		"apps" { "292030" "12345" }
	}
}
"#;
        let values = key_values(text);
        assert_eq!(
            values.get("path").map(String::as_str),
            Some("/home/matheo/.local/share/Steam")
        );
    }

    #[test]
    fn lit_les_bibliotheques_steam() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/matheo/.local/share/Steam"
		"apps" { "292030" "12345" }
	}
	"1"
	{
		"path"		"/mnt/jeux/SteamLibrary"
	}
}
"#;
        let paths = vdf_paths(vdf);
        assert_eq!(
            paths,
            ["/home/matheo/.local/share/Steam", "/mnt/jeux/SteamLibrary"]
        );
        // Ancien format : clés numériques et chemins nus.
        let old = r#"{ "libraryfolders" { "1" "/mnt/a/Steam" "2" "/mnt/b/Steam" } }"#;
        assert_eq!(vdf_paths(old), ["/mnt/a/Steam", "/mnt/b/Steam"]);
    }

    #[test]
    fn choisit_l_executable_du_jeu_et_ignore_le_desinstalleur() {
        let dir = std::env::temp_dir().join(format!("quark-steam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin/x64")).unwrap();
        std::fs::write(dir.join("unins000.exe"), vec![0u8; 4096]).unwrap();
        std::fs::write(dir.join("bin/x64/game.exe"), vec![0u8; 2048]).unwrap();
        std::fs::write(dir.join("UnityCrashHandler64.exe"), vec![0u8; 90_000]).unwrap();
        let exe = find_exe(&dir, "Mon Jeu", "MonJeu").expect("exécutable");
        assert_eq!(exe.file_name().unwrap(), "game.exe");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
