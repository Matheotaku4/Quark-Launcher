//! Poids sur le disque : préfixes Proton, cache d'artworks, journaux,
//! archives de mise à jour et jeux.
//!
//! Le calcul est fait dans un thread dédié (un préfixe Proton peut contenir
//! des dizaines de milliers de fichiers) : l'interface ne se fige jamais, la
//! page affiche un indicateur pendant la mesure.

use crate::config::{self, Library};
use std::path::{Path, PathBuf};

pub struct DiskRow {
    pub label: String,
    pub path: PathBuf,
    pub bytes: u64,
    /// Précision affichée sous le libellé (nombre de jeux, dossier absent…).
    pub detail: String,
}

pub struct DiskReport {
    pub rows: Vec<DiskRow>,
    pub total: u64,
}

/// Taille d'un dossier, sans suivre les liens symboliques et sans descendre
/// indéfiniment (un préfixe Proton contient `drive_c` et ses liens).
pub fn dir_size(path: &Path) -> u64 {
    fn walk(path: &Path, depth: usize) -> u64 {
        if depth > 24 {
            return 0;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return 0;
        };
        let mut total = 0u64;
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                // Un lien n'occupe que son inode, et le suivre ferait boucler
                // `drive_c/dosdevices` sur lui-même.
                continue;
            }
            if kind.is_dir() {
                total += walk(&entry.path(), depth + 1);
            } else if kind.is_file() {
                total += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            }
        }
        total
    }
    if path.is_dir() {
        walk(path, 0)
    } else if path.is_file() {
        std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
    } else {
        0
    }
}

/// Mesure tout ce que Quark stocke, plus la place prise par les jeux.
pub fn scan(lib: &Library) -> DiskReport {
    let mut rows: Vec<DiskRow> = Vec::new();
    let settings = &lib.settings;
    let english = crate::i18n::is_english();

    let artwork = config::artwork_root();
    let artwork_bytes = dir_size(&artwork);
    rows.push(DiskRow {
        label: crate::i18n::tr("Cache d'artworks"),
        path: artwork,
        bytes: artwork_bytes,
        detail: {
            let count = lib
                .games
                .iter()
                .filter(|game| {
                    game.artwork.cover.is_some()
                        || game.artwork.hero.is_some()
                        || game.artwork.logo.is_some()
                        || game.artwork.icon.is_some()
                })
                .count();
            if english {
                if count == 1 {
                    "1 game with images".to_string()
                } else {
                    format!("{count} games with images")
                }
            } else if count == 1 {
                "1 jeu avec des images".to_string()
            } else {
                format!("{count} jeux avec des images")
            }
        },
    });

    let logs = config::data_dir().join("logs");
    rows.push(DiskRow {
        label: crate::i18n::tr("Journaux de session"),
        path: logs.clone(),
        bytes: dir_size(&logs),
        detail: if english {
            "one session.log per game, rewritten on each launch".to_string()
        } else {
            "un fichier session.log par jeu, réécrit à chaque lancement".to_string()
        },
    });

    let updates = config::data_dir().join("updates");
    rows.push(DiskRow {
        label: crate::i18n::tr("Archives de mise à jour"),
        path: updates.clone(),
        bytes: dir_size(&updates),
        detail: if english {
            "downloaded from GitHub releases".to_string()
        } else {
            "téléchargées depuis les releases GitHub".to_string()
        },
    });

    // Un dossier de préfixe par chemin réel : plusieurs jeux peuvent partager
    // le préfixe par défaut, il ne doit compter qu'une fois.
    let mut prefixes: Vec<(PathBuf, usize)> = Vec::new();
    for game in &lib.games {
        let path = game.prefix_path(settings);
        if !path.is_dir() {
            continue;
        }
        match prefixes.iter_mut().find(|(seen, _)| *seen == path) {
            Some((_, count)) => *count += 1,
            None => prefixes.push((path, 1)),
        }
    }
    prefixes.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, count) in prefixes {
        let bytes = dir_size(&path);
        let mut detail = if english {
            "Proton prefix".to_string()
        } else {
            "préfixe Proton".to_string()
        };
        if count > 1 {
            detail = if english {
                format!("shared by {count} games")
            } else {
                format!("partagé par {count} jeux")
            };
        }
        if let Some(game) = lib
            .games
            .iter()
            .find(|game| game.prefix_path(settings) == path)
        {
            detail = format!("{} · {detail}", game.name);
        }
        rows.push(DiskRow {
            label: crate::i18n::tr("Préfixe Proton"),
            path,
            bytes,
            detail,
        });
    }

    // Dossier de chaque jeu (le poids réel de l'installation), une fois par
    // dossier : deux exécutables d'un même jeu ne comptent pas deux fois.
    let mut game_dirs: Vec<(PathBuf, String)> = Vec::new();
    for game in &lib.games {
        let Some(dir) = game.exe_path().parent().map(Path::to_path_buf) else {
            continue;
        };
        if !dir.is_dir() {
            continue;
        }
        // Ne jamais mesurer la racine du compte ou `/` par accident.
        if dir.parent().is_none() || dir == config::home_dir() {
            continue;
        }
        if game_dirs.iter().any(|(seen, _)| *seen == dir) {
            continue;
        }
        game_dirs.push((dir, game.name.clone()));
    }
    game_dirs.sort_by(|a, b| a.0.cmp(&b.0));
    for (dir, name) in game_dirs {
        rows.push(DiskRow {
            label: crate::i18n::tr("Jeu"),
            path: dir.clone(),
            bytes: dir_size(&dir),
            detail: name,
        });
    }

    // Gros postes d'abord : la page répond d'abord à « où est passé l'espace ».
    rows.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let total = rows.iter().map(|row| row.bytes).sum();
    DiskReport { rows, total }
}

/// Mesure en arrière-plan (voir [`scan`]).
pub fn spawn(lib: Library) -> std::sync::mpsc::Receiver<DiskReport> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(scan(&lib));
    });
    rx
}

/// « 1,4 Go » : taille lisible, en français.
pub fn format_bytes(bytes: u64, english: bool) -> String {
    const FR: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    const EN: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let units = if english { EN } else { FR };
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < units.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", units[0])
    } else {
        let number = format!("{value:.1}");
        format!(
            "{} {}",
            if english {
                number
            } else {
                number.replace('.', ",")
            },
            units[unit]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_tailles_sont_lisibles() {
        assert_eq!(format_bytes(0, false), "0 o");
        assert_eq!(format_bytes(512, false), "512 o");
        assert_eq!(format_bytes(1024, false), "1,0 Ko");
        assert_eq!(format_bytes(1_572_864, false), "1,5 Mo");
        assert_eq!(format_bytes(12 * 1024 * 1024 * 1024, false), "12,0 Go");
        assert_eq!(format_bytes(0, true), "0 B");
        assert_eq!(format_bytes(1024, true), "1.0 KB");
        assert_eq!(format_bytes(1_572_864, true), "1.5 MB");
        assert_eq!(format_bytes(12 * 1024 * 1024 * 1024, true), "12.0 GB");
    }

    #[test]
    fn un_dossier_absent_ne_pese_rien() {
        assert_eq!(dir_size(Path::new("/tmp/quark-dossier-qui-nexiste-pas")), 0);
    }

    #[test]
    fn la_mesure_additionne_les_fichiers() {
        let dir = std::env::temp_dir().join(format!("quark-disk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sous")).unwrap();
        std::fs::write(dir.join("a.bin"), vec![0u8; 1000]).unwrap();
        std::fs::write(dir.join("sous/b.bin"), vec![0u8; 24]).unwrap();
        assert_eq!(dir_size(&dir), 1024);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
