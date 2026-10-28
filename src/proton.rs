//! Détection des runtimes Proton et construction de la commande de lancement.

use crate::config::{AnticheatMode, Game, ProtonMode, Settings};
use std::path::{Path, PathBuf};

/// Nature d'un runtime détecté.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    /// Un vrai Proton (compatibilité générale).
    Proton,
    /// Runtime fourni par Steam pour BattlEye / EasyAntiCheat.
    AntiCheat,
}

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
    pub kind: RuntimeKind,
    /// Vrai si le dossier contient le script `proton`. Les runtimes
    /// anti-triche installés par Steam n'en ont pas toujours : ils restent
    /// détectés (et signalés), mais ne peuvent pas servir d'outil de
    /// compatibilité tant que le script manque.
    pub has_proton_script: bool,
}

impl ProtonRuntime {
    pub fn label(&self) -> String {
        let base = match self.version.as_ref().filter(|v| !v.is_empty()) {
            Some(v) => format!("{} · {}", self.name, v),
            None => self.name.clone(),
        };
        match self.kind {
            RuntimeKind::Proton => base,
            RuntimeKind::AntiCheat => format!("{base} · anti-triche"),
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

/// Vrai pour un dossier de runtime anti-triche installé par Steam
/// (« BattlEye_Runtime », « BattlEye AntiCheat Runtime »,
/// « Proton EasyAntiCheat Runtime », « EasyAntiCheat_Runtime »…).
fn is_anticheat_name(lower: &str) -> bool {
    lower.contains("battleye") || lower.contains("easyanticheat") || lower.contains("eac_runt")
}

/// Runtime anti-triche correspondant à un motif (`battleye`, `easyanticheat`…).
pub fn anticheat_runtime<'a>(
    runtimes: &'a [ProtonRuntime],
    keyword: &str,
) -> Option<&'a ProtonRuntime> {
    runtimes
        .iter()
        .find(|runtime| runtime.kind == RuntimeKind::AntiCheat && runtime.name.to_lowercase().contains(keyword))
}

/// Résumé lisible des runtimes anti-triche détectés (chemins compris), pour
/// les messages et l'interface : « aucun » quand il n'y en a pas.
pub fn anticheat_summary(runtimes: &[ProtonRuntime]) -> String {
    let list: Vec<String> = runtimes
        .iter()
        .filter(|runtime| runtime.kind == RuntimeKind::AntiCheat)
        .map(|runtime| format!("{} ({})", runtime.name, runtime.path.display()))
        .collect();
    if list.is_empty() {
        "aucun".to_string()
    } else {
        list.join(" · ")
    }
}

/// Runtimes trouvés dans une racine Steam donnée.
///
/// Les runtimes anti-triche que Steam installe à côté des Proton
/// (« Proton BattlEye Runtime », « Proton EasyAntiCheat Runtime ») en font
/// partie : ils ne remplacent pas Proton, mais un jeu peut les demander.
pub fn detect_in(root: &Path) -> Vec<ProtonRuntime> {
    let mut out: Vec<ProtonRuntime> = Vec::new();
    for base in [root.join("compatibilitytools.d"), root.join("steamapps/common")] {
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let Some(name) = dir.file_name().map(|n| n.to_string_lossy().to_string()) else {
                continue;
            };
            let lower = name.to_lowercase();
            let anticheat = is_anticheat_name(&lower);
            // Un Proton expose un script `proton` ; un runtime anti-triche est
            // reconnu à son nom, même sans script (c'est le cas des dossiers
            // « BattlEye_Runtime » / « EasyAntiCheat_Runtime » de Steam).
            let script = dir.join("proton");
            let has_proton_script = script.is_file();
            if !has_proton_script && !anticheat {
                continue;
            }
            if !lower.contains("proton") && !anticheat {
                continue;
            }
            let version = std::fs::read_to_string(dir.join("version"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            out.push(ProtonRuntime {
                key: name.clone(),
                name,
                version,
                path: if has_proton_script { script } else { dir.clone() },
                steam_root: root.to_path_buf(),
                kind: if anticheat {
                    RuntimeKind::AntiCheat
                } else {
                    RuntimeKind::Proton
                },
                has_proton_script,
            });
        }
    }
    out
}

/// Liste les runtimes de toutes les installations Steam détectées.
pub fn detect_runtimes() -> Vec<ProtonRuntime> {
    let mut out: Vec<ProtonRuntime> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    for root in steam_root_candidates() {
        for runtime in detect_in(&root) {
            let canon =
                std::fs::canonicalize(&runtime.path).unwrap_or_else(|_| runtime.path.clone());
            if seen.contains(&canon) {
                continue;
            }
            seen.push(canon);
            out.push(runtime);
        }
    }

    // Les Proton d'abord, puis les runtimes anti-triche, chacun par ordre
    // alphabétique.
    out.sort_by_key(|r| (r.kind == RuntimeKind::AntiCheat, r.name.to_lowercase()));
    out
}

/// Runtime proposé par défaut quand l'utilisateur n'en a choisi aucun.
pub fn suggested_default(runtimes: &[ProtonRuntime]) -> Option<&ProtonRuntime> {
    // Un runtime anti-triche ne remplace jamais un Proton comme choix par
    // défaut : il ne sert qu'aux jeux qui le demandent explicitement.
    let protons: Vec<&ProtonRuntime> = runtimes
        .iter()
        .filter(|runtime| runtime.kind == RuntimeKind::Proton)
        .collect();
    protons
        .iter()
        .copied()
        .filter(|r| r.name.to_lowercase().contains("ge-proton"))
        .max_by_key(|r| numeric_weight(&r.name))
        .or_else(|| {
            protons
                .iter()
                .copied()
                .find(|r| r.name.to_lowercase().contains("experimental"))
        })
        .or_else(|| {
            protons
                .iter()
                .copied()
                .filter(|r| r.name.to_lowercase().starts_with("proton "))
                .last()
        })
        .or_else(|| protons.last().copied())
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

/// Variables d'environnement effectives : d'abord celles des réglages
/// généraux, puis celles du jeu — qui écrasent le réglage global portant le
/// même nom. L'ordre est conservé (le jeu garde donc l'ordre où il les a été
/// saisies).
pub fn launch_env(game: &Game, settings: &Settings) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for var in settings.env.iter().chain(game.env.iter()) {
        if !var.is_valid() {
            continue;
        }
        let name = var.name.trim();
        let entry = (name.to_string(), var.value.clone());
        match out
            .iter_mut()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
        {
            Some(slot) => *slot = entry,
            None => out.push(entry),
        }
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
                // Les variables d'environnement s'appliquent aussi aux jeux
                // natifs : certaines servent hors Wine (MANGOHUD, LD_…).
                env: launch_env(game, settings),
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

    // Option anti-triche : quand elle est cochée, c'est le runtime fourni par
    // Steam (BattlEye / EasyAntiCheat) qui joue le rôle de Proton. Aucune
    // variable n'est inventée : on lance exactement ce runtime.
    let runtime = match game.anticheat {
        AnticheatMode::None => runtime,
        mode => {
            let mut chosen: Option<ProtonRuntime> = None;
            let mut unusable: Vec<String> = Vec::new();
            for keyword in mode.keywords() {
                match anticheat_runtime(runtimes, keyword).cloned() {
                    Some(runtime) if runtime.has_proton_script => {
                        chosen.get_or_insert(runtime);
                    }
                    Some(runtime) => {
                        unusable.push(format!("{} ({})", runtime.name, runtime.path.display()))
                    }
                    None => {}
                }
            }
            match chosen {
                Some(runtime) => runtime,
                None if !unusable.is_empty() => {
                    return Err(format!(
                        "Runtime anti-triche trouvé mais inutilisable comme outil de \
                         compatibilité (aucun script `proton` dedans) : {}.",
                        unusable.join(" · ")
                    ));
                }
                None => {
                    return Err(format!(
                        "Runtime {} introuvable. Dossiers anti-triche détectés : {}.",
                        mode.label(),
                        anticheat_summary(runtimes)
                    ));
                }
            }
        }
    };

    let prefix = game.prefix_path(settings);
    let mut args = vec!["run".to_string(), exe.display().to_string()];
    args.extend(extra);

    // Les variables de l'utilisateur d'abord, puis celles que Proton exige :
    // `Command::env` garde la dernière valeur, donc le préfixe résolu gagne
    // toujours sur une variable homonyme saisie à la main.
    let mut env = launch_env(game, settings);
    env.push((
        "STEAM_COMPAT_DATA_PATH".to_string(),
        prefix.display().to_string(),
    ));
    env.push((
        "STEAM_COMPAT_CLIENT_INSTALL_PATH".to_string(),
        runtime.steam_root.display().to_string(),
    ));

    Ok(Resolved {
        spec: LaunchSpec {
            program: runtime.path.clone(),
            args,
            env,
        },
        kind: LaunchKind::Proton,
        runtime: Some(runtime),
        prefix: Some(prefix),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EnvVar;

    fn jeu(env: Vec<EnvVar>) -> Game {
        let mut game = Game::new("Test", "/tmp/jeu.exe");
        game.env = env;
        game
    }

    #[test]
    fn env_du_jeu_ecrase_le_reglage_global() {
        let mut settings = Settings::default();
        settings.env = vec![
            EnvVar::new("WINEDLLOVERRIDES", "winmm=n"),
            EnvVar::new("DXVK_HUD", "1"),
        ];
        let env = launch_env(
            &jeu(vec![EnvVar::new(
                "WINEDLLOVERRIDES",
                "winmm=n,b;steam_api64",
            )]),
            &settings,
        );
        // La variable du jeu remplace la globale à sa place (même ordre, pas
        // de doublon).
        assert_eq!(env.len(), 2);
        assert_eq!(
            env[0],
            (
                "WINEDLLOVERRIDES".to_string(),
                "winmm=n,b;steam_api64".to_string()
            )
        );
        assert_eq!(env[1], ("DXVK_HUD".to_string(), "1".to_string()));
    }

    #[test]
    fn env_sans_nom_est_ignoree() {
        let mut settings = Settings::default();
        settings.env = vec![EnvVar::new("   ", "x"), EnvVar::new("", "y")];
        assert!(launch_env(&jeu(Vec::new()), &settings).is_empty());
    }

    fn faux_runtime(root: &Path, name: &str) {
        let dir = root.join("steamapps/common").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("proton"), "#!/bin/sh\n").unwrap();
        std::fs::write(dir.join("version"), "9.0\n").unwrap();
    }

    #[test]
    fn detecte_les_runtimes_anti_triche() {
        let root = std::env::temp_dir().join(format!("quark-runtimes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        faux_runtime(&root, "Proton 9.0");
        faux_runtime(&root, "Proton BattlEye Runtime");
        faux_runtime(&root, "Proton EasyAntiCheat Runtime");
        // Un jeu installé (aucun script `proton`) n'est pas un runtime.
        std::fs::create_dir_all(root.join("steamapps/common/UnJeu")).unwrap();

        let runtimes = detect_in(&root);
        assert_eq!(runtimes.len(), 3);
        let anti: Vec<&ProtonRuntime> = runtimes
            .iter()
            .filter(|runtime| runtime.kind == RuntimeKind::AntiCheat)
            .collect();
        assert_eq!(anti.len(), 2, "BattlEye et EasyAntiCheat étaient ignorés");
        assert!(anti.iter().any(|r| r.name.contains("BattlEye")));
        // Le Proton proposé par défaut n'est jamais un runtime anti-triche.
        let suggested = suggested_default(&runtimes).unwrap();
        assert_eq!(suggested.kind, RuntimeKind::Proton);
        assert_eq!(suggested.name, "Proton 9.0");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn choisit_le_runtime_anti_triche_demande() {
        let dir = std::env::temp_dir().join(format!("quark-anti-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("jeu.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let root = dir.join("steam");
        std::fs::create_dir_all(&root).unwrap();

        let runtime = |name: &str, kind: RuntimeKind| ProtonRuntime {
            key: name.to_string(),
            name: name.to_string(),
            version: None,
            path: root.join(name).join("proton"),
            steam_root: root.clone(),
            kind,
            has_proton_script: true,
        };
        let runtimes = vec![
            runtime("Proton 9.0", RuntimeKind::Proton),
            runtime("Proton BattlEye Runtime", RuntimeKind::AntiCheat),
        ];

        let mut game = Game::new("Jeu", exe.display().to_string());
        game.anticheat = AnticheatMode::BattlEye;
        let resolu = resolve(&game, &Settings::default(), &runtimes).unwrap();
        assert_eq!(
            resolu.runtime.as_ref().map(|r| r.name.as_str()),
            Some("Proton BattlEye Runtime")
        );

        // Sans l'option, c'est le Proton normal qui est choisi.
        game.anticheat = AnticheatMode::None;
        let resolu = resolve(&game, &Settings::default(), &runtimes).unwrap();
        assert_eq!(
            resolu.runtime.as_ref().map(|r| r.name.as_str()),
            Some("Proton 9.0")
        );

        // Option cochée mais runtime absent : message clair, pas de lancement.
        game.anticheat = AnticheatMode::EasyAntiCheat;
        assert!(resolve(&game, &Settings::default(), &runtimes).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detecte_les_dossiers_anti_triche_sans_script_proton() {
        // Cas réel de Steam : « BattlEye_Runtime » et « EasyAntiCheat_Runtime »
        // n'ont pas forcément de script `proton` dans le dossier.
        let root = std::env::temp_dir().join(format!("quark-anti-nu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for name in ["BattlEye_Runtime", "EasyAntiCheat_Runtime"] {
            std::fs::create_dir_all(root.join("steamapps/common").join(name)).unwrap();
        }

        let runtimes = detect_in(&root);
        assert_eq!(runtimes.len(), 2, "{runtimes:?}");
        assert!(runtimes.iter().all(|r| r.kind == RuntimeKind::AntiCheat));
        assert!(runtimes.iter().all(|r| !r.has_proton_script));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn les_deux_runtimes_anti_triche_sont_independants() {
        let mut mode = AnticheatMode::None;
        mode = mode.with_battleye(true);
        assert!(mode.battleye() && !mode.eac());
        mode = mode.with_eac(true);
        assert!(mode.battleye() && mode.eac());
        assert_eq!(mode, AnticheatMode::Both);
        mode = mode.with_battleye(false);
        assert!(!mode.battleye() && mode.eac());
        mode = mode.with_eac(false);
        assert_eq!(mode, AnticheatMode::None);
    }

    #[test]
    fn l_ordre_des_variables_est_conserve() {
        let settings = Settings::default();
        let env = launch_env(
            &jeu(vec![
                EnvVar::new("PROTON_LOG", "1"),
                EnvVar::new("WINEDEBUG", "-all"),
            ]),
            &settings,
        );
        assert_eq!(
            env.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
            ["PROTON_LOG", "WINEDEBUG"]
        );
    }
}
