//! Découverte de jeux dans un dossier (ajout par lot).
//!
//! On cherche les exécutables plausibles, on écarte les installateurs et les
//! bibliothèques, puis on propose un titre nettoyé que la recherche Steam /
//! SteamGridDB pourra reconnaître.

use std::path::{Path, PathBuf};

pub struct Discovered {
    pub exe: PathBuf,
    pub title: String,
}

/// Fragments de noms qui trahissent un fichier utilitaire, pas un jeu.
const JUNK: &[&str] = &[
    "unins",
    "uninstall",
    "setup",
    "install",
    "vcredist",
    "vc_redist",
    "dxsetup",
    "dxwebsetup",
    "directx",
    "dotnet",
    "ndp4",
    "unitycrashhandler",
    "crashhandler",
    "crashreport",
    "prereq",
    "eula",
    "readme",
    "activation",
    "activate",
    "updater",
    "goggalaxy",
    "touchup",
    "cleanup",
    "register",
    "benchmark",
    "dedicated",
    "server",
];

/// Dossiers à ne pas parcourir (moteurs, redistribuables, outils).
const SKIP_DIRS: &[&str] = &[
    "redist",
    "redistributables",
    "_commonredist",
    "directx",
    "dotnet",
    "support",
    "__macosx",
    "tools",
    "engine",
    "logs",
    "cache",
];

/// Parcourt `root` et renvoie au plus `limit` jeux candidats, les plus
/// plausibles d'abord.
pub fn scan(root: &Path, max_depth: usize, limit: usize) -> Vec<Discovered> {
    let mut found: Vec<(i64, Discovered)> = Vec::new();
    walk(root, 0, max_depth, &mut found);

    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.exe.cmp(&b.1.exe)));
    let mut out: Vec<Discovered> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();
    for (_, item) in found {
        if seen.contains(&item.exe) {
            continue;
        }
        seen.push(item.exe.clone());
        out.push(item);
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn walk(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<(i64, Discovered)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let folder = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            let lower = name.to_lowercase();
            if SKIP_DIRS.iter().any(|d| lower == *d) {
                continue;
            }
            if depth < max_depth {
                walk(&path, depth + 1, max_depth, out);
            }
            continue;
        }

        let Some(stem) = game_stem(&path) else {
            continue;
        };
        let title = guess_title(&stem, &folder);

        let mut score = 100 - (depth as i64) * 10;
        let norm_folder = normalize(&folder);
        let norm_stem = normalize(&stem);
        if !norm_folder.is_empty() && norm_folder == norm_stem {
            score += 60;
        } else if !norm_folder.is_empty()
            && (norm_folder.contains(&norm_stem) || norm_stem.contains(&norm_folder))
        {
            score += 25;
        }
        if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
        {
            score += 10;
        }
        if stem.to_lowercase().contains("launcher") {
            score -= 20;
        }
        if let Ok(meta) = entry.metadata() {
            // Un gros binaire est plus probablement le jeu lui-même.
            score += (meta.len() as i64 / (20 * 1024 * 1024)).min(15);
        }
        out.push((
            score,
            Discovered {
                exe: path,
                title: if title.is_empty() { stem } else { title },
            },
        ));
    }
}

/// Nom de fichier sans extension si le fichier ressemble à un jeu.
fn game_stem(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let lower = stem.to_lowercase();
    if JUNK.iter().any(|j| lower.contains(j)) {
        return None;
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "exe" | "appimage" | "sh" => Some(stem),
        "" => {
            // Binaire Linux sans extension : il doit être exécutable et ne pas
            // être une bibliothèque.
            if name.ends_with(".so") || name.ends_with(".dll") {
                return None;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = path.metadata().ok()?;
                if meta.permissions().mode() & 0o111 != 0 && meta.is_file() {
                    return Some(stem);
                }
            }
            None
        }
        _ => None,
    }
}

/// Nom de dossier > nom de binaire quand les deux se ressemblent.
pub fn guess_title(stem: &str, folder: &str) -> String {
    let from_folder = clean_title(folder);
    let from_stem = clean_title(stem);
    if from_folder.is_empty() {
        return from_stem;
    }
    if from_stem.is_empty() {
        return from_folder;
    }
    let f = normalize(&from_folder);
    let s = normalize(&from_stem);
    if f == s || f.contains(&s) || s.contains(&f) {
        return from_folder;
    }
    // Dossier générique : le binaire est plus parlant.
    let folder_generic = matches!(
        from_folder.to_lowercase().as_str(),
        "games"
            | "jeux"
            | "program files"
            | "gog games"
            | "steam library"
            | "downloads"
            | "home"
            | "desktop"
    );
    if folder_generic {
        from_stem
    } else {
        from_folder
    }
}

/// Nettoie un nom de fichier/dossier pour en faire un titre présentable.
pub fn clean_title(raw: &str) -> String {
    let mut kept = String::new();
    let mut depth = 0i32;
    for ch in raw.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => kept.push(ch),
            _ => {}
        }
    }

    let spaced = kept.replace('_', " ");
    let mut tokens: Vec<&str> = Vec::new();
    let mut skip_numbers = false;
    for token in spaced.split_whitespace() {
        let lower = token.to_lowercase();
        if is_version_token(&lower) {
            // « v1.4.0 » : on écarte aussi les nombres qui le suivent.
            skip_numbers = true;
            continue;
        }
        if skip_numbers && lower.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        skip_numbers = false;
        if matches!(
            lower.as_str(),
            "x64"
                | "x86"
                | "win64"
                | "win32"
                | "amd64"
                | "linux"
                | "gog"
                | "steam"
                | "repack"
                | "fitgirl"
                | "codex"
                | "plaza"
                | "skidrow"
                | "multi"
                | "setup"
                | "installer"
        ) {
            continue;
        }
        tokens.push(token);
    }

    tokens
        .join(" ")
        .trim_matches(|c: char| c == '-' || c == ':' || c.is_whitespace())
        .to_string()
}

/// « v1 », « v1.4.0.0 », « 1.2.3 » : une version, pas un mot du titre.
fn is_version_token(lower: &str) -> bool {
    let rest = lower.strip_prefix('v').unwrap_or(lower);
    if rest.is_empty() {
        return false;
    }
    let mut parts = rest.split('.');
    let first = parts.next().unwrap_or("");
    if first.is_empty() || !first.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if lower.starts_with('v') {
        return true;
    }
    // Un seul nombre isolé reste un mot du titre (« Portal 2 »).
    rest.contains('.')
}

/// Clé de comparaison insensible à la casse, à la ponctuation et aux espaces.
pub fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nettoie_un_nom_de_dossier() {
        assert_eq!(clean_title("Celeste_v1.4.0.0_(64bit)_(12345)"), "Celeste");
        assert_eq!(clean_title("Hollow_Knight_v1.5.78.11833"), "Hollow Knight");
        assert_eq!(clean_title("[FitGirl] Stardew Valley"), "Stardew Valley");
    }

    #[test]
    fn prefere_le_dossier_quand_il_ressemble_au_binaire() {
        assert_eq!(guess_title("Celeste", "Celeste"), "Celeste");
        assert_eq!(
            guess_title("stardew valley", "Stardew Valley"),
            "Stardew Valley"
        );
        assert_eq!(guess_title("game", "Hades"), "Hades");
        assert_eq!(
            guess_title("hollow_knight", "Hollow Knight"),
            "Hollow Knight"
        );
    }

    #[test]
    fn garde_les_nombres_qui_font_partie_du_titre() {
        assert_eq!(clean_title("Portal 2"), "Portal 2");
        assert_eq!(clean_title("Hades II v1.0.1"), "Hades II");
    }
}
