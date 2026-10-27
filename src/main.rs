//! Quark — launcher léger pour Linux, en Rust + egui.
//!
//! Utilisation normale : `cargo run --release`
//! Diagnostic (sans interface) : voir `--help`.

mod app;
mod artwork;
mod config;
mod images;
mod proton;
mod scan;
mod theme;
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
            Ok(Box::new(QuarkApp::new(cc)))
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
            Some(0)
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
