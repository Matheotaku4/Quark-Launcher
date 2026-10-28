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
    /// anti-triche de Steam n'en ont pas : ils ne se lancent pas, ils se
    /// déclarent à Proton par une variable d'environnement (voir
    /// [`anticheat_env_name`]).
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

    /// Dossier du runtime. `path` pointe sur le script `proton` pour un Proton
    /// lançable, sur le dossier lui-même pour un runtime anti-triche.
    pub fn dir(&self) -> PathBuf {
        if self.has_proton_script {
            self.path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.path.clone())
        } else {
            self.path.clone()
        }
    }
}

/// Variable que Proton lit pour trouver un runtime anti-triche :
/// `PROTON_BATTLEYE_RUNTIME` ou `PROTON_EAC_RUNTIME`. C'est le mécanisme
/// officiel de Proton pour ces dossiers, qui ne contiennent pas de script
/// `proton` et ne peuvent donc pas être lancés directement.
pub fn anticheat_env_name(keyword: &str) -> &'static str {
    if keyword.contains("battleye") {
        "PROTON_BATTLEYE_RUNTIME"
    } else {
        "PROTON_EAC_RUNTIME"
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

impl LaunchSpec {
    /// Ligne de commande lisible : journal de session, bloc d'informations.
    pub fn command_line(&self) -> String {
        let mut words = vec![shell_quote(&self.program.display().to_string())];
        words.extend(self.args.iter().map(|arg| shell_quote(arg)));
        words.join(" ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchKind {
    Native,
    Proton,
    /// umu-launcher (`umu-run`) : Proton sans Steam, umu gérant le préfixe et
    /// le GAMEID.
    Umu,
}

/// Outils externes trouvés sur le PATH.
///
/// Une case cochée alors que l'outil n'est pas installé ne doit pas empêcher le
/// jeu de démarrer : l'option est ignorée et l'interface le dit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tools {
    pub gamescope: bool,
    pub gamemoderun: bool,
    pub umu_run: bool,
}

impl Tools {
    pub fn detect() -> Self {
        Self {
            gamescope: on_path("gamescope"),
            gamemoderun: on_path("gamemoderun"),
            umu_run: on_path("umu-run"),
        }
    }
}

fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

#[derive(Debug)]
pub struct Resolved {
    pub kind: LaunchKind,
    pub spec: LaunchSpec,
    pub runtime: Option<ProtonRuntime>,
    pub prefix: Option<PathBuf>,
    /// Option anti-triche effectivement appliquée au lancement.
    pub anticheat: AnticheatMode,
    /// Options de lancement appliquées (« gamescope 2560×1440 », « GameMode »…).
    pub extras: Vec<String>,
    /// Options demandées mais impossibles (outil absent) : dites, pas tues.
    pub warnings: Vec<String>,
}

impl Resolved {
    pub fn label(&self) -> String {
        let base = match (&self.kind, &self.runtime) {
            (LaunchKind::Native, _) => "Linux (natif)".to_string(),
            (LaunchKind::Proton, Some(rt)) => rt.label(),
            (LaunchKind::Proton, None) => "Proton".to_string(),
            (LaunchKind::Umu, Some(rt)) => format!("umu · {}", rt.name),
            (LaunchKind::Umu, None) => "umu · Proton auto".to_string(),
        };
        let mut parts = vec![base];
        if self.anticheat != AnticheatMode::None {
            parts.push(format!("anti-triche {}", self.anticheat.label()));
        }
        parts.extend(self.extras.iter().cloned());
        parts.join(" · ")
    }
}

/// Échappement pour `sh` : un mot sans caractère spécial reste tel quel, sinon
/// il est entouré de guillemets simples (un guillemet simple interne devient
/// `'\''`).
pub fn shell_quote(word: &str) -> String {
    let safe = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/:+=@%,".contains(c));
    if safe {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

fn shell_join(words: &[String]) -> String {
    words
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Applique les options graphiques à la commande du jeu.
///
/// Ordre des enveloppes, de l'extérieur vers l'intérieur : wrapper de
/// l'utilisateur, gamescope, gamemoderun, puis le jeu — c'est l'ordre que Steam
/// obtient avec `gamescope -- gamemoderun %command%`.
fn finalize(
    program: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    opts: &crate::config::LaunchOptions,
    tools: Tools,
    extras: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> LaunchSpec {
    let mut tokens: Vec<String> = std::iter::once(program.display().to_string())
        .chain(args)
        .collect();
    // Noms des enveloppes, de la plus interne à la plus externe.
    let mut wrappers: Vec<String> = Vec::new();

    if opts.gamemode {
        if tools.gamemoderun {
            let mut wrapped = vec!["gamemoderun".to_string()];
            wrapped.extend(tokens);
            tokens = wrapped;
            wrappers.push("GameMode".to_string());
        } else {
            warnings.push("gamemoderun n'est pas installé : l'option est ignorée".to_string());
        }
    }

    if opts.gamescope {
        if tools.gamescope {
            let (width, height) = opts.gamescope_size();
            let mut wrapped = vec![
                "gamescope".to_string(),
                "-W".to_string(),
                width.to_string(),
                "-H".to_string(),
                height.to_string(),
                "-f".to_string(),
            ];
            if opts.gamescope_fsr {
                wrapped.push("-F".to_string());
                wrapped.push("fsr".to_string());
            }
            if opts.gamescope_fps > 0 {
                wrapped.push("--fps-limit".to_string());
                wrapped.push(opts.gamescope_fps.min(1000).to_string());
            }
            wrapped.push("--".to_string());
            wrapped.extend(tokens);
            tokens = wrapped;
            wrappers.push(if opts.gamescope_fsr {
                format!("gamescope {width}×{height} FSR")
            } else {
                format!("gamescope {width}×{height}")
            });
        } else {
            warnings.push("gamescope n'est pas installé : l'option est ignorée".to_string());
        }
    }

    // À l'affichage, la plus externe d'abord : gamescope, puis GameMode.
    wrappers.reverse();
    extras.splice(0..0, wrappers);

    let wrapper = opts.wrapper.trim();
    if wrapper.is_empty() {
        let mut iter = tokens.into_iter();
        let program = iter.next().unwrap_or_default();
        return LaunchSpec {
            program: PathBuf::from(program),
            args: iter.collect(),
            env,
        };
    }

    // `%command%` remplacé par la commande complète, comme dans les options de
    // lancement Steam ; sans joker, la commande est préfixée.
    let command = shell_join(&tokens);
    let line = if wrapper.contains("%command%") {
        wrapper.replace("%command%", &command)
    } else {
        format!("{wrapper} {command}")
    };
    extras.insert(0, "wrapper".to_string());
    // La ligne passe par un shell : c'est ce qui permet `env FOO=1 %command%`,
    // les pipes ou les redirections.
    LaunchSpec {
        program: PathBuf::from("sh"),
        args: vec!["-c".to_string(), line],
        env,
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

/// Variables d'environnement effectives du jeu.
///
/// Les réglages généraux ne sont plus fusionnées ici : leurs variables sont
/// recopiées dans le jeu au moment de l'ajout, ce qui permet à chaque jeu de
/// garder les siennes. Un nom saisi deux fois est dédoublonné (dernière valeur
/// gagnante) sans changer l'ordre de saisie.
pub fn launch_env(game: &Game) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for var in game.env.iter() {
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
    resolve_with(game, settings, runtimes, Tools::detect())
}

/// Même chose avec des outils imposés (les tests n'ont pas gamescope installé).
pub fn resolve_with(
    game: &Game,
    settings: &Settings,
    runtimes: &[ProtonRuntime],
    tools: Tools,
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

    let mut env = launch_env(game);
    let mut extras: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // PRIME : le rendu part sur la carte NVIDIA dédiée (portables hybrides).
    if game.launch.prime {
        for (name, value) in [
            ("__NV_PRIME_RENDER_OFFLOAD", "1"),
            ("__GLX_VENDOR_LIBRARY_NAME", "nvidia"),
            ("__VK_LAYER_NV_optimus", "NVIDIA_only"),
            ("DRI_PRIME", "1"),
        ] {
            env.push((name.to_string(), value.to_string()));
        }
        extras.push("PRIME".to_string());
    }

    // MangoHud : la variable suffit (couche Vulkan implicite) ; aucun lanceur à
    // installer en plus du paquet MangoHud.
    if game.launch.mangohud {
        env.push(("MANGOHUD".to_string(), "1".to_string()));
        extras.push("MangoHud".to_string());
    }

    if game.proton_log {
        env.push(("PROTON_LOG".to_string(), "1".to_string()));
        env.push((
            "PROTON_LOG_DIR".to_string(),
            crate::config::log_dir(&game.id).display().to_string(),
        ));
        extras.push("PROTON_LOG".to_string());
    }

    if !want_proton {
        if game.umu {
            warnings.push("umu-launcher ne sert qu'aux jeux Windows : lancement natif".to_string());
        }
        let argv = std::iter::once(exe.display().to_string())
            .chain(extra)
            .collect::<Vec<String>>();
        if argv.is_empty() {
            return Err("Aucun exécutable défini pour ce jeu.".to_string());
        }
        let spec = finalize(
            PathBuf::from(&argv[0]),
            argv[1..].to_vec(),
            env,
            &game.launch,
            tools,
            &mut extras,
            &mut warnings,
        );
        return Ok(Resolved {
            kind: LaunchKind::Native,
            spec,
            runtime: None,
            prefix: None,
            anticheat: AnticheatMode::None,
            extras,
            warnings,
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

    // Option anti-triche : Steam installe ces dossiers à part (« Proton
    // BattlEye Runtime », « Proton EasyAntiCheat Runtime ») et Proton ne les
    // lance pas — il les trouve par une variable d'environnement.
    let mut anticheat_env: Vec<(String, String)> = Vec::new();
    let mut missing: Vec<&'static str> = Vec::new();
    for keyword in game.anticheat.keywords() {
        let label = if keyword.contains("battleye") {
            "BattlEye"
        } else {
            "EasyAntiCheat"
        };
        match anticheat_runtime(runtimes, keyword) {
            Some(runtime) => anticheat_env.push((
                anticheat_env_name(keyword).to_string(),
                runtime.dir().display().to_string(),
            )),
            None => missing.push(label),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "Runtime {} introuvable sous {} (dossiers anti-triche vus : {}). Vérifie le nom du \
             dossier, ou installe « Proton {} Runtime » depuis Steam.",
            missing.join(" et "),
            runtime.steam_root.display(),
            anticheat_summary(runtimes),
            missing[0]
        ));
    }

    let prefix = game.prefix_path(settings);
    // Les variables de l'utilisateur d'abord, puis celles que Quark exige :
    // `Command::env` garde la dernière valeur.
    env.extend(anticheat_env);

    if game.umu {
        if !tools.umu_run {
            return Err(
                "umu-run introuvable : installe umu-launcher (paquet `umu-launcher` ou \
                 ~/.local/bin/umu-run), ou décoche « Lancer via umu » dans les paramètres du jeu."
                    .to_string(),
            );
        }
        env.push((
            "WINEPREFIX".to_string(),
            prefix.display().to_string(),
        ));
        let game_id = if game.umu_game_id.trim().is_empty() {
            "0".to_string()
        } else {
            game.umu_game_id.trim().to_string()
        };
        env.push(("GAMEID".to_string(), game_id));
        // Un Proton choisi explicitement est transmis à umu ; sinon umu prend
        // son propre build (UMU-Proton), comme il le fait sans Steam.
        let pinned = matches!(game.proton, ProtonMode::Runtime(_))
            || settings.default_proton.is_some();
        if pinned && runtime.kind == RuntimeKind::Proton {
            env.push((
                "PROTONPATH".to_string(),
                runtime.dir().display().to_string(),
            ));
        }
        let mut args = vec![exe.display().to_string()];
        args.extend(extra);
        let spec = finalize(
            PathBuf::from("umu-run"),
            args,
            env,
            &game.launch,
            tools,
            &mut extras,
            &mut warnings,
        );
        return Ok(Resolved {
            kind: LaunchKind::Umu,
            spec,
            runtime: Some(runtime),
            prefix: Some(prefix),
            anticheat: game.anticheat,
            extras,
            warnings,
        });
    }

    env.push((
        "STEAM_COMPAT_DATA_PATH".to_string(),
        prefix.display().to_string(),
    ));
    env.push((
        "STEAM_COMPAT_CLIENT_INSTALL_PATH".to_string(),
        runtime.steam_root.display().to_string(),
    ));

    let mut args = vec!["run".to_string(), exe.display().to_string()];
    args.extend(extra);
    let spec = finalize(
        runtime.path.clone(),
        args,
        env,
        &game.launch,
        tools,
        &mut extras,
        &mut warnings,
    );

    Ok(Resolved {
        kind: LaunchKind::Proton,
        spec,
        runtime: Some(runtime),
        prefix: Some(prefix),
        anticheat: game.anticheat,
        extras,
        warnings,
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
    fn une_variable_ecrite_deux_fois_garde_la_derniere_valeur() {
        let env = launch_env(&jeu(vec![
            EnvVar::new("WINEDLLOVERRIDES", "winmm=n"),
            EnvVar::new("WINEDLLOVERRIDES", "winmm=n,b;steam_api64"),
        ]));
        // Un seul nom, une seule variable, dernière valeur gagnante.
        assert_eq!(env.len(), 1);
        assert_eq!(
            env[0],
            (
                "WINEDLLOVERRIDES".to_string(),
                "winmm=n,b;steam_api64".to_string()
            )
        );
    }

    #[test]
    fn env_sans_nom_est_ignoree() {
        assert!(launch_env(&jeu(vec![EnvVar::new("   ", "x"), EnvVar::new("", "y")])).is_empty());
    }

    fn jeu_binaire(dir: &Path) -> Game {
        let exe = dir.join("jeu.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let mut game = Game::new("Jeu", exe.display().to_string());
        // Un exécutable Windows voudrait Proton ; ces tests portent sur la
        // ligne de commande, donc on force le lancement direct.
        game.proton = ProtonMode::Native;
        game
    }

    fn dossier(nom: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("quark-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn outils(gamescope: bool, gamemoderun: bool, umu_run: bool) -> Tools {
        Tools {
            gamescope,
            gamemoderun,
            umu_run,
        }
    }

    fn valeur(env: &[(String, String)], name: &str) -> Option<String> {
        env.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }

    #[test]
    fn les_options_graphiques_s_enchainent_comme_dans_steam() {
        let dir = dossier("lancement");
        let mut game = jeu_binaire(&dir);
        game.launch.gamescope = true;
        game.launch.gamescope_width = 2560;
        game.launch.gamescope_height = 1440;
        game.launch.gamescope_fsr = true;
        game.launch.gamescope_fps = 60;
        game.launch.gamemode = true;

        let resolu = resolve_with(
            &game,
            &Settings::default(),
            &[],
            outils(true, true, false),
        )
        .unwrap();
        // gamescope enveloppe gamemoderun, qui enveloppe le jeu.
        assert_eq!(resolu.spec.program, PathBuf::from("gamescope"));
        let args: Vec<&str> = resolu.spec.args.iter().map(String::as_str).collect();
        assert_eq!(
            &args[..],
            [
                "-W",
                "2560",
                "-H",
                "1440",
                "-f",
                "-F",
                "fsr",
                "--fps-limit",
                "60",
                "--",
                "gamemoderun",
                game.exe.as_str()
            ]
        );
        assert_eq!(resolu.extras, ["gamescope 2560×1440 FSR", "GameMode"]);
        assert!(resolu.warnings.is_empty());
        assert_eq!(
            resolu.label(),
            "Linux (natif) · gamescope 2560×1440 FSR · GameMode"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn le_wrapper_remplace_le_joker_ou_prefixe_la_commande() {
        let dir = dossier("wrapper");
        let mut game = jeu_binaire(&dir);

        game.launch.wrapper = "env FOO=1 %command%".to_string();
        let resolu = resolve_with(&game, &Settings::default(), &[], Tools::default()).unwrap();
        // Une ligne de wrapper passe par sh, comme dans Steam.
        assert_eq!(resolu.spec.program, PathBuf::from("sh"));
        assert_eq!(resolu.spec.args[0], "-c");
        assert_eq!(
            resolu.spec.args[1],
            format!("env FOO=1 {}", game.exe)
        );

        // Sans joker, la commande est préfixée par le wrapper.
        game.launch.wrapper = "prime-run".to_string();
        let resolu = resolve_with(&game, &Settings::default(), &[], Tools::default()).unwrap();
        assert_eq!(resolu.spec.args[1], format!("prime-run {}", game.exe));
        assert!(resolu.extras.contains(&"wrapper".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn une_case_cochee_sans_outil_est_ignoree_et_signalee() {
        let dir = dossier("outils-absents");
        let mut game = jeu_binaire(&dir);
        game.launch.gamescope = true;
        game.launch.gamemode = true;

        let resolu = resolve_with(
            &game,
            &Settings::default(),
            &[],
            outils(false, false, false),
        )
        .unwrap();
        // Le jeu démarre quand même : aucune option n'est appliquée.
        assert_eq!(resolu.spec.program, game.exe_path());
        assert!(resolu.extras.is_empty());
        assert_eq!(resolu.warnings.len(), 2);
        assert!(resolu.warnings.iter().any(|w| w.contains("gamescope")));
        assert!(resolu.warnings.iter().any(|w| w.contains("gamemoderun")));
        assert_eq!(resolu.label(), "Linux (natif)");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prime_mangohud_et_journal_passent_par_l_environnement() {
        let dir = dossier("env-lancement");
        let mut game = jeu_binaire(&dir);
        game.launch.prime = true;
        game.launch.mangohud = true;
        game.proton_log = true;

        let resolu = resolve_with(&game, &Settings::default(), &[], Tools::default()).unwrap();
        assert_eq!(valeur(&resolu.spec.env, "MANGOHUD").as_deref(), Some("1"));
        assert_eq!(
            valeur(&resolu.spec.env, "__NV_PRIME_RENDER_OFFLOAD").as_deref(),
            Some("1")
        );
        assert_eq!(
            valeur(&resolu.spec.env, "__GLX_VENDOR_LIBRARY_NAME").as_deref(),
            Some("nvidia")
        );
        assert_eq!(valeur(&resolu.spec.env, "PROTON_LOG").as_deref(), Some("1"));
        assert_eq!(
            valeur(&resolu.spec.env, "PROTON_LOG_DIR"),
            Some(crate::config::log_dir(&game.id).display().to_string())
        );
        assert!(resolu.extras.contains(&"PRIME".to_string()));
        assert!(resolu.extras.contains(&"MangoHud".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn umu_remplace_le_script_proton() {
        let dir = dossier("umu");
        let root = dir.join("steam");
        std::fs::create_dir_all(&root).unwrap();
        let mut game = jeu_binaire(&dir);
        game.proton = ProtonMode::Runtime("GE-Proton9-20".to_string());
        game.umu = true;
        game.umu_game_id = "367520".to_string();

        let runtime = ProtonRuntime {
            key: "GE-Proton9-20".to_string(),
            name: "GE-Proton9-20".to_string(),
            version: None,
            path: root.join("GE-Proton9-20/proton"),
            steam_root: root.clone(),
            kind: RuntimeKind::Proton,
            has_proton_script: true,
        };
        let resolu = resolve_with(
            &game,
            &Settings::default(),
            &[runtime.clone()],
            outils(false, false, true),
        )
        .unwrap();
        assert_eq!(resolu.kind, LaunchKind::Umu);
        assert_eq!(resolu.spec.program, PathBuf::from("umu-run"));
        assert_eq!(resolu.spec.args, [game.exe.clone()]);
        // umu gère le préfixe et le GAMEID ; le Proton choisi lui est transmis.
        assert_eq!(
            valeur(&resolu.spec.env, "WINEPREFIX"),
            Some(game.prefix_path(&Settings::default()).display().to_string())
        );
        assert_eq!(valeur(&resolu.spec.env, "GAMEID").as_deref(), Some("367520"));
        assert_eq!(
            valeur(&resolu.spec.env, "PROTONPATH"),
            Some(root.join("GE-Proton9-20").display().to_string())
        );
        assert_eq!(resolu.label(), "umu · GE-Proton9-20");

        // GAMEID vide : umu reçoit « 0 » plutôt qu'une variable absente.
        game.umu_game_id.clear();
        let resolu = resolve_with(
            &game,
            &Settings::default(),
            &[runtime.clone()],
            outils(false, false, true),
        )
        .unwrap();
        assert_eq!(valeur(&resolu.spec.env, "GAMEID").as_deref(), Some("0"));

        // umu-run absent : message clair plutôt qu'un jeu qui ne démarre pas.
        let err = resolve_with(
            &game,
            &Settings::default(),
            &[runtime],
            outils(false, false, false),
        )
        .unwrap_err();
        assert!(err.contains("umu"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_chemin_avec_espace_est_protege() {
        assert_eq!(shell_quote("simple"), "simple");
        assert_eq!(
            shell_quote("/jeux/mon jeu/jeu.exe"),
            "'/jeux/mon jeu/jeu.exe'"
        );
        assert_eq!(shell_quote("l'a"), "'l'\\''a'");
        assert_eq!(shell_quote(""), "''");
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
    fn pointe_proton_vers_les_runtimes_anti_triche() {
        let dir = std::env::temp_dir().join(format!("quark-anti-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("jeu.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let root = dir.join("steam");
        std::fs::create_dir_all(&root).unwrap();

        // Runtimes anti-triche tels que Steam les installe : un dossier, pas de
        // script `proton` dedans.
        let anticheat = |name: &str| ProtonRuntime {
            key: name.to_string(),
            name: name.to_string(),
            version: None,
            path: root.join("steamapps/common").join(name),
            steam_root: root.clone(),
            kind: RuntimeKind::AntiCheat,
            has_proton_script: false,
        };
        let proton = ProtonRuntime {
            key: "Proton 9.0".to_string(),
            name: "Proton 9.0".to_string(),
            version: None,
            path: root.join("Proton 9.0").join("proton"),
            steam_root: root.clone(),
            kind: RuntimeKind::Proton,
            has_proton_script: true,
        };
        let runtimes = vec![
            proton,
            anticheat("Proton BattlEye Runtime"),
            anticheat("Proton EasyAntiCheat Runtime"),
        ];

        // Les deux cases cochées : Proton reste le lanceur, et les deux
        // dossiers sont déclarés à Proton par variable d'environnement.
        let mut game = Game::new("Jeu", exe.display().to_string());
        game.anticheat = AnticheatMode::Both;
        let resolu = resolve(&game, &Settings::default(), &runtimes).unwrap();
        assert_eq!(
            resolu.runtime.as_ref().map(|r| r.name.as_str()),
            Some("Proton 9.0")
        );
        let env = &resolu.spec.env;
        let value = |name: &str| {
            env.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(
            value("PROTON_BATTLEYE_RUNTIME"),
            Some(
                root.join("steamapps/common/Proton BattlEye Runtime")
                    .display()
                    .to_string()
            )
        );
        assert_eq!(
            value("PROTON_EAC_RUNTIME"),
            Some(
                root.join("steamapps/common/Proton EasyAntiCheat Runtime")
                    .display()
                    .to_string()
            )
        );

        // Sans l'option, aucune variable anti-triche n'est ajoutée.
        game.anticheat = AnticheatMode::None;
        let resolu = resolve(&game, &Settings::default(), &runtimes).unwrap();
        assert!(resolu
            .spec
            .env
            .iter()
            .all(|(key, _)| !key.contains("ANTICHEAT") && !key.contains("_RUNTIME")));

        // Option cochée mais runtime absent : message clair, pas de lancement.
        let seul_proton: Vec<ProtonRuntime> = runtimes
            .iter()
            .filter(|r| r.kind == RuntimeKind::Proton)
            .cloned()
            .collect();
        game.anticheat = AnticheatMode::EasyAntiCheat;
        let err = resolve(&game, &Settings::default(), &seul_proton).unwrap_err();
        assert!(err.contains("EasyAntiCheat"), "{err}");
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
        let env = launch_env(&jeu(vec![
            EnvVar::new("PROTON_LOG", "1"),
            EnvVar::new("WINEDEBUG", "-all"),
        ]));
        assert_eq!(
            env.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
            ["PROTON_LOG", "WINEDEBUG"]
        );
    }
}
