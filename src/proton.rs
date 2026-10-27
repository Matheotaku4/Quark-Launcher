//! Détection des runtimes Proton et construction de la commande de lancement.

use crate::config::{Game, ProtonMode, Settings};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtonRuntime {
    /// Clé stable (nom du dossier), utilisée dans la config.
    pub key: String,
    pub name: String,
    pub version: Option<String>,
    /// Script `proton` du runtime.
    pub path: PathBuf,
    /// Racine Steam à passer dans STEAM_COMPAT_CLIENT_INSTALL_PATH.
    pub steam_root: PathBuf,
}

impl ProtonRuntime {
    pub fn label(&self) -> String {
        match self.version.as_ref().filter(|v| !v.is_empty()) {
            Some(v) => format!("{} · {}", self.name, v),
            None => self.name.clone(),
        }
    }
}

pub fn steam_root_candidates() -> Vec<PathBuf> {
    let h = crate::config::home_dir();
    vec![
        h.join(".steam/steam"),
        h.join(".steam/root"),
        h.join(".local/share/Steam"),
        h.join(".var/app/com.valvesoftware.Steam/data/Steam"),
        h.join("snap/steam/common/.local/share/Steam"),
    ]
}

/// Première racine Steam plausible (contient `steamapps` ou `compatibilitytools.d`).
pub fn steam_root() -> Option<PathBuf> {
    steam_root_candidates().into_iter().find(|p| {
        p.join("steamapps").is_dir() || p.join("compatibilitytools.d").is_dir()
    })
}

/// Liste les runtimes Proton : `steamapps/common/Proton*` et `compatibilitytools.d/*`.
pub fn detect_runtimes() -> Vec<ProtonRuntime> {
    let mut out: Vec<ProtonRuntime> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    for root in steam_root_candidates() {
        for base in [root.join("compatibilitytools.d"), root.join("steamapps/common")] {
            let Ok(entries) = std::fs::read_dir(&base) else {
                continue;
            };
            for entry in entries.flatten() {
                let dir = entry.path();
                if !dir.is_dir() {
                    continue;
                }
                let script = dir.join("proton");
                if !script.is_file() {
                    continue;
                }
                let Some(name) = dir.file_name().map(|n| n.to_string_lossy().to_string()) else {
                    continue;
                };
                let lower = name.to_lowercase();
                if !lower.contains("proton") || lower.contains("easyanticheat") || lower.contains("battleye") {
                    continue;
                }
                let canon = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
                if seen.contains(&canon) {
                    continue;
                }
                seen.push(canon);
                let version = std::fs::read_to_string(dir.join("version"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                out.push(ProtonRuntime {
                    key: name.clone(),
                    name,
                    version,
                    path: script,
                    steam_root: root.clone(),
                });
            }
        }
    }

    out.sort_by_key(|r| r.name.to_lowercase());
    out
}

/// Runtime proposé par défaut quand l'utilisateur n'en a choisi aucun.
pub fn suggested_default(runtimes: &[ProtonRuntime]) -> Option<&ProtonRuntime> {
    runtimes
        .iter()
        .filter(|r| r.name.to_lowercase().contains("ge-proton"))
        .max_by_key(|r| numeric_weight(&r.name))
        .or_else(|| {
            runtimes
                .iter()
                .find(|r| r.name.to_lowercase().contains("experimental"))
        })
        .or_else(|| {
            runtimes
                .iter()
                .filter(|r| r.name.to_lowercase().starts_with("proton "))
                .last()
        })
        .or_else(|| runtimes.last())
}

fn numeric_weight(name: &str) -> u64 {
    let mut weight: u64 = 0;
    let mut current: Option<u64> = None;
    for ch in name.chars() {
        if let Some(d) = ch.to_digit(10) {
            current = Some(current.unwrap_or(0) * 10 + d as u64);
        } else if let Some(n) = current.take() {
            weight = weight * 1000 + n.min(999);
        }
    }
    if let Some(n) = current {
        weight = weight * 1000 + n.min(999);
    }
    weight
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchKind {
    Native,
    Proton,
}

pub struct Resolved {
    pub kind: LaunchKind,
    pub spec: LaunchSpec,
    pub runtime: Option<ProtonRuntime>,
    pub prefix: Option<PathBuf>,
}

impl Resolved {
    pub fn label(&self) -> String {
        match (&self.kind, &self.runtime) {
            (LaunchKind::Native, _) => "Linux (natif)".to_string(),
            (LaunchKind::Proton, Some(rt)) => rt.label(),
            (LaunchKind::Proton, None) => "Proton".to_string(),
        }
    }
}

pub fn looks_windows(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext.to_lowercase().as_str(), "exe" | "bat" | "msi"),
        None => false,
    }
}

/// Découpe une ligne d'arguments en respectant les guillemets simples/doubles.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut has_content = false;

    for ch in s.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                    has_content = true;
                }
                c if c.is_whitespace() => {
                    if has_content || !current.is_empty() {
                        out.push(std::mem::take(&mut current));
                        has_content = false;
                    }
                }
                c => current.push(c),
            },
        }
    }
    if has_content || !current.is_empty() {
        out.push(current);
    }
    out
}

/// Détermine le mode de lancement effectif et construit la commande.
pub fn resolve(
    game: &Game,
    settings: &Settings,
    runtimes: &[ProtonRuntime],
) -> Result<Resolved, String> {
    if game.exe.trim().is_empty() {
        return Err("Aucun exécutable défini pour ce jeu.".to_string());
    }
    let exe = game.exe_path();
    if !exe.exists() {
        return Err(format!("Exécutable introuvable : {}", exe.display()));
    }
    let extra = split_args(&game.args);
    let want_proton = match &game.proton {
        ProtonMode::Native => false,
        ProtonMode::Runtime(_) => true,
        ProtonMode::Default => looks_windows(&exe),
    };

    if !want_proton {
        return Ok(Resolved {
            kind: LaunchKind::Native,
            spec: LaunchSpec {
                program: exe,
                args: extra,
                env: Vec::new(),
            },
            runtime: None,
            prefix: None,
        });
    }

    let runtime = match &game.proton {
        ProtonMode::Runtime(key) => runtimes
            .iter()
            .find(|r| &r.key == key)
            .cloned()
            .ok_or_else(|| format!("Runtime Proton « {key} » introuvable. Vérifie les paramètres."))?,
        _ => {
            let chosen = settings
                .default_proton
                .as_ref()
                .and_then(|k| runtimes.iter().find(|r| &r.key == k).cloned());
            chosen.or_else(|| suggested_default(runtimes).cloned()).ok_or_else(|| {
                "Aucun runtime Proton détecté (ni Steam, ni compatibilitytools.d).".to_string()
            })?
        }
    };

    let prefix = game.prefix_path(settings);
    let mut args = vec!["run".to_string(), exe.display().to_string()];
    args.extend(extra);

    Ok(Resolved {
        spec: LaunchSpec {
            program: runtime.path.clone(),
            args,
            env: vec![
                (
                    "STEAM_COMPAT_DATA_PATH".to_string(),
                    prefix.display().to_string(),
                ),
                (
                    "STEAM_COMPAT_CLIENT_INSTALL_PATH".to_string(),
                    runtime.steam_root.display().to_string(),
                ),
            ],
        },
        kind: LaunchKind::Proton,
        runtime: Some(runtime),
        prefix: Some(prefix),
    })
}
