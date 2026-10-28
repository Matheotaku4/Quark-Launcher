//! Quark — launcher léger pour Linux, en Rust + egui.
//!
//! Utilisation normale : `cargo run --release`
//! Diagnostic (sans interface) : voir `--help`.

mod app;
mod artwork;
mod config;
mod images;
mod news;
mod proton;
mod scan;
mod snapshot;
mod steam;
mod theme;
mod update;
mod ui_dialogs;
mod ui_library;
mod ui_store;
mod widgets;

use app::QuarkApp;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli(&args) {
        std::process::exit(code);
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Quark")
            .with_app_id("quark")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([1024.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Quark",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(QuarkApp::new(&cc.egui_ctx)))
        }),
    )
}

/// Mode diagnostic en ligne de commande (utile sans écran / pour tester le réseau).
fn cli(args: &[String]) -> Option<i32> {
    let command = args.first()?.as_str();
    match command {
        "--help" | "-h" => {
            println!(
                "Quark {}\n\n\
                 USAGE:\n  \
                 quark                    Lance le launcher\n  \
                 quark --doctor           Affiche l'environnement détecté (dossiers, Proton, jeux)\n  \
                 quark --search <nom>     Recherche un jeu sur Steam + SteamGridDB\n  \
                 quark --fetch <nom>      Télécharge les artworks du premier résultat\n",
                env!("CARGO_PKG_VERSION")
            );
            println!("  quark --scan <dossier>   Liste les jeux détectés dans un dossier");
            println!("  quark --news <nom|appid> Affiche les actualités Steam d'un jeu");
            println!("  quark --update           Vérifie la dernière release GitHub");
            println!("\nVérification du rendu (sans écran, aucun GPU) :");
            print!("{}", snapshot::HELP);
            Some(0)
        }
        "--update" => {
            let client = artwork::http();
            match update::latest(&client) {
                Ok(release) => {
                    println!(
                        "Quark {} · dernière release : {} ({})",
                        update::current_version(),
                        release.tag_name,
                        release.name
                    );
                    if update::is_newer(&release.tag_name, update::current_version()) {
                        println!("mise à jour disponible : {}", release.html_url);
                        match release.archive() {
                            Some(asset) => {
                                let dir = config::data_dir().join("updates");
                                match update::download(&client, asset, &dir) {
                                    Ok(path) => {
                                        println!(
                                            "archive téléchargée : {} ({} octets)",
                                            path.display(),
                                            asset.size
                                        );
                                        println!(
                                            "décompresse-la et remplace le binaire pour installer \
                                             la nouvelle version"
                                        );
                                    }
                                    Err(err) => println!("téléchargement impossible : {err}"),
                                }
                            }
                            None => println!("cette release ne contient pas d'archive"),
                        }
                    } else {
                        println!("à jour.");
                    }
                    Some(0)
                }
                Err(err) => {
                    eprintln!("Vérification impossible : {err}");
                    Some(1)
                }
            }
        }
        "--ui" => {
            let Some(out) = args.get(1).filter(|a| !a.starts_with("--")) else {
                eprintln!("Usage : quark --ui <fichier.png> [options]");
                print!("{}", snapshot::HELP);
                return Some(2);
            };
            let mut opts = snapshot::SnapshotOptions {
                out: std::path::PathBuf::from(out),
                ..snapshot::SnapshotOptions::default()
            };
            let mut index = 2;
            while index < args.len() {
                let key = args[index].clone();
                let value = args.get(index + 1).cloned();
                // Options sans valeur : un seul argument à avancer.
                match key.as_str() {
                    "--info" => {
                        opts.info = true;
                        index += 1;
                        continue;
                    }
                    "--running" => {
                        opts.running = true;
                        index += 1;
                        continue;
                    }
                    "--marked" => {
                        opts.marked = true;
                        index += 1;
                        continue;
                    }
                    "--demo" => {
                        opts.demo = true;
                        index += 1;
                        continue;
                    }
                    _ => {}
                }
                match key.as_str() {
                    "--size" => match value.as_deref().and_then(parse_size) {
                        Some(size) => opts.size = size,
                        None => {
                            eprintln!("--size attend la forme 1360x860");
                            return Some(2);
                        }
                    },
                    "--scale" => match value.as_deref().and_then(|v| v.parse::<f32>().ok()) {
                        Some(scale) => opts.scale = scale,
                        None => {
                            eprintln!("--scale attend un nombre (ex. 1.5)");
                            return Some(2);
                        }
                    },
                    "--frames" => match value.as_deref().and_then(|v| v.parse::<u32>().ok()) {
                        Some(frames) => opts.frames = frames,
                        None => {
                            eprintln!("--frames attend un entier (ex. 20)");
                            return Some(2);
                        }
                    },
                    "--hover" => match value.as_deref().and_then(parse_point) {
                        Some(point) => opts.hover = Some(point),
                        None => {
                            eprintln!("--hover attend des coordonnées X,Y");
                            return Some(2);
                        }
                    },
                    "--click" => match value.as_deref().and_then(parse_point) {
                        Some(point) => {
                            opts.click = Some(point);
                            opts.hover = Some(point);
                        }
                        None => {
                            eprintln!("--click attend des coordonnées X,Y");
                            return Some(2);
                        }
                    },
                    "--view" => {
                        opts.view = match value.as_deref() {
                            Some("list") => Some(config::ViewMode::List),
                            Some("grid") => Some(config::ViewMode::Grid),
                            _ => {
                                eprintln!("--view attend list ou grid");
                                return Some(2);
                            }
                        }
                    }
                    "--tab" => {
                        opts.tab = match value.as_deref() {
                            Some("library") => Some(app::Tab::Library),
                            Some("store") => Some(app::Tab::Store),
                            _ => {
                                eprintln!("--tab attend library ou store");
                                return Some(2);
                            }
                        }
                    }
                    "--theme" => {
                        opts.theme = match value.as_deref() {
                            Some("amoled") => Some(config::ThemeMode::Amoled),
                            Some("graphite") => Some(config::ThemeMode::Graphite),
                            _ => {
                                eprintln!("--theme attend amoled ou graphite");
                                return Some(2);
                            }
                        }
                    }
                    "--select" => match value {
                        Some(name) => opts.select = Some(name),
                        None => {
                            eprintln!("--select attend un nom de jeu");
                            return Some(2);
                        }
                    },
                    "--key" => match value {
                        Some(key) => opts.key = Some(key),
                        None => {
                            eprintln!("--key attend une touche (up, down, left, right, enter, escape)");
                            return Some(2);
                        }
                    },
                    "--dialog" => match value {
                        Some(dialog) => opts.dialog = Some(dialog),
                        None => {
                            eprintln!("--dialog attend un nom de fenêtre");
                            return Some(2);
                        }
                    },
                    _ => {
                        eprintln!("Option inconnue : {key}");
                        print!("{}", snapshot::HELP);
                        return Some(2);
                    }
                }
                index += 2;
            }
            match snapshot::run(&opts) {
                Ok(()) => Some(0),
                Err(err) => {
                    eprintln!("Rendu impossible : {err}");
                    Some(1)
                }
            }
        }
        "--doctor" => {
            let lib = config::Library::load();
            println!("Quark {}", env!("CARGO_PKG_VERSION"));
            println!("config        : {}", config::config_path().display());
            println!("données       : {}", config::data_dir().display());
            println!("artworks      : {}", config::artwork_root().display());
            println!("préfixes      : {}", config::prefix_root().display());
            println!(
                "racine Steam  : {}",
                proton::steam_root()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "introuvable".to_string())
            );
            let runtimes = proton::detect_runtimes();
            println!("runtimes      : {}", runtimes.len());
            for runtime in &runtimes {
                println!("  - {} -> {}", runtime.label(), runtime.path.display());
            }
            if let Some(suggested) = proton::suggested_default(&runtimes) {
                println!("proposé       : {}", suggested.label());
            }
            println!("anti-triche   : {}", proton::anticheat_summary(&runtimes));
            println!("jeux          : {}", lib.games.len());
            for game in &lib.games {
                let state = match proton::resolve(game, &lib.settings, &runtimes) {
                    Ok(resolved) => format!("OK · {}", resolved.label()),
                    Err(err) => format!("ERREUR · {err}"),
                };
                println!("  - {} ({}) [{state}]", game.name, game.exe);
            }
            Some(0)
        }
        "--search" => {
            let query = args.get(1).cloned().unwrap_or_else(|| {
                eprintln!("Usage : quark --search <nom du jeu>");
                std::process::exit(2);
            });
            let lib = config::Library::load();
            let client = artwork::http();
            let outcome = artwork::search(&client, &query, &lib.settings);
            println!("{} résultat(s) pour « {query} »", outcome.hits.len());
            for hit in &outcome.hits {
                println!(
                    "  - [{}] {} | appid={:?} sgdb={:?} | {}",
                    hit.source.label(),
                    hit.title,
                    hit.steam_app_id,
                    hit.sgdb_id,
                    hit.detail
                );
            }
            for warning in &outcome.warnings {
                println!("  ! {warning}");
            }
            if let Some(hit) = outcome.hits.first() {
                let plan = artwork::build_plan(&client, hit, &lib.settings);
                println!("Plan d'artworks pour « {} » :", hit.title);
                for kind in config::AssetKind::ALL {
                    let candidates = plan.candidates(kind);
                    println!("  {:<12} {} candidat(s)", kind.label(), candidates.len());
                    for url in candidates.iter().take(2) {
                        println!("      {url}");
                    }
                }
                for warning in &plan.warnings {
                    println!("  ! {warning}");
                }
            }
            Some(0)
        }
        "--scan" => {
            let folder = args.get(1).cloned().unwrap_or_else(|| {
                eprintln!("Usage : quark --scan <dossier>");
                std::process::exit(2);
            });
            let lib = config::Library::load();
            let client = artwork::http();
            let found = scan::scan(std::path::Path::new(&folder), 4, 10);
            println!("{} candidat(s) dans {folder}", found.len());
            for item in &found {
                let outcome = artwork::search(&client, &item.title, &lib.settings);
                match artwork::best_hit(&outcome, &item.title) {
                    Some(hit) => println!(
                        "  - {:<28} {:<10} {} [{}]",
                        item.title,
                        hit.title,
                        item.exe.display(),
                        hit.source.label()
                    ),
                    None => println!(
                        "  - {:<28} {:<10} {}",
                        item.title,
                        "?",
                        item.exe.display()
                    ),
                }
            }
            Some(0)
        }
        "--news" => {
            let arg = args.get(1).cloned().unwrap_or_else(|| {
                eprintln!("Usage : quark --news <appid | nom du jeu>");
                std::process::exit(2);
            });
            let lib = config::Library::load();
            let client = artwork::http();
            let app_id = match arg.parse::<u32>() {
                Ok(id) => id,
                Err(_) => {
                    let outcome = artwork::search(&client, &arg, &lib.settings);
                    match outcome.hits.iter().find_map(|hit| hit.steam_app_id) {
                        Some(id) => id,
                        None => {
                            eprintln!("Aucun appid Steam pour « {arg} »");
                            for warning in &outcome.warnings {
                                eprintln!("  ! {warning}");
                            }
                            return Some(1);
                        }
                    }
                }
            };
            match news::fetch(&client, app_id, news::COUNT) {
                Ok(items) => {
                    println!("{} actualité(s) pour l'appid {app_id}", items.len());
                    for item in &items {
                        println!("  - [{}] {}", config::human_ago(item.date), item.title);
                        println!("      {} · {}", item.feed, item.url);
                        if !item.excerpt.is_empty() {
                            println!("      {}", item.excerpt);
                        }
                    }
                    Some(0)
                }
                Err(err) => {
                    eprintln!("Actualités indisponibles : {err}");
                    Some(1)
                }
            }
        }
        "--fetch" => {
            let query = args.get(1).cloned().unwrap_or_else(|| {
                eprintln!("Usage : quark --fetch <nom du jeu>");
                std::process::exit(2);
            });
            let lib = config::Library::load();
            let client = artwork::http();
            let outcome = artwork::search(&client, &query, &lib.settings);
            let Some(hit) = outcome.hits.first().cloned() else {
                eprintln!("Aucun résultat pour « {query} »");
                for warning in &outcome.warnings {
                    eprintln!("  ! {warning}");
                }
                return Some(1);
            };
            println!("Jeu retenu : {} ({})", hit.title, hit.source.label());
            let dir = std::env::temp_dir().join("quark-artworks").join("cli-test");
            let plan = artwork::build_plan(&client, &hit, &lib.settings);
            let (artwork, warnings) = artwork::apply_plan(&client, &dir, &plan);
            for kind in config::AssetKind::ALL {
                match artwork.get(kind) {
                    Some(path) => {
                        let size = std::fs::read(path)
                            .ok()
                            .and_then(|bytes| artwork::image_size(&bytes))
                            .map(|(w, h)| format!("{w}×{h}"))
                            .unwrap_or_else(|| "?".to_string());
                        println!("  {:<12} {:<12} {}", kind.label(), size, path.display());
                    }
                    None => println!("  {:<12} {:<12} absent", kind.label(), ""),
                }
            }
            for warning in &warnings {
                println!("  ! {warning}");
            }
            println!("Dossier : {}", dir.display());
            Some(0)
        }
        _ => None,
    }
}

/// « 1360x860 » -> taille.
fn parse_size(text: &str) -> Option<eframe::egui::Vec2> {
    let (width, height) = text.split_once(|c| c == 'x' || c == 'X')?;
    Some(eframe::egui::Vec2::new(
        width.trim().parse().ok()?,
        height.trim().parse().ok()?,
    ))
}

/// « 120,340 » -> point.
fn parse_point(text: &str) -> Option<eframe::egui::Pos2> {
    let (x, y) = text.split_once(',')?;
    Some(eframe::egui::Pos2::new(
        x.trim().parse().ok()?,
        y.trim().parse().ok()?,
    ))
}
