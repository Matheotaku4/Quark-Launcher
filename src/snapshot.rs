//! Rendu hors écran de l'interface : produit un PNG de la fenêtre Quark sans
//! écran, sans serveur X et sans GPU (tessellation egui + rasterisation
//! logicielle).
//!
//! Sert à vérifier le rendu après chaque changement d'interface :
//!
//! ```text
//! quark --ui apercu.png --view grid --hover 150,300 --frames 20
//! ```
//!
//! Le rendu reproduit la disposition, les textes, les couleurs et les
//! animations. Les bords antialiasés et les dégradés peuvent différer très
//! légèrement du backend OpenGL utilisé à l'écran.

use crate::app::{AddStep, NewsState, QuarkApp, ScanCandidate, SearchState, Tab};
use crate::artwork::{AssetPlan, SearchHit, Source};
use crate::config::{AssetKind, Artwork, Game, Library, ThemeMode, ViewMode, now_ts};
use crate::proton::{ProtonRuntime, RuntimeKind};
use crate::news::NewsItem;
use eframe::egui::epaint::textures::TexturesDelta;
use eframe::egui::epaint::{ImageData, Primitive};
use eframe::egui::{
    self, ClippedPrimitive, Color32, Event, Modifiers, PointerButton, Pos2, Rect, Vec2,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Paramètres du rendu, remplis depuis la ligne de commande (`quark --ui`).
pub struct SnapshotOptions {
    pub out: PathBuf,
    pub size: Vec2,
    pub scale: f32,
    pub frames: u32,
    pub hover: Option<Pos2>,
    pub click: Option<Pos2>,
    pub view: Option<ViewMode>,
    pub tab: Option<Tab>,
    pub select: Option<String>,
    pub theme: Option<ThemeMode>,
    pub info: bool,
    pub running: bool,
    pub key: Option<String>,
    pub dialog: Option<String>,
    pub demo: bool,
    /// Marque quelques jeux comme avec Ctrl+clic (sélection multiple).
    pub marked: bool,
}

impl Default for SnapshotOptions {
    fn default() -> Self {
        Self {
            out: PathBuf::from("quark-ui.png"),
            size: Vec2::new(1360.0, 860.0),
            scale: 1.0,
            frames: 16,
            hover: None,
            click: None,
            view: None,
            tab: None,
            select: None,
            theme: None,
            info: false,
            running: false,
            key: None,
            dialog: None,
            demo: false,
            marked: false,
        }
    }
}

pub const HELP: &str = "\
  quark --ui <fichier.png>  Rend l'interface hors écran dans un PNG (sans écran)\n\
      [--size 1360x860] [--scale 1.5] [--frames 16]\n\
      [--view list|grid] [--tab library|store] [--theme amoled|graphite]\n\
      [--select <jeu>] [--info] [--running] [--marked]\n\
      [--dialog none|add|artwork|exe|scan|settings|settings-proton|\n\
      settings-env|settings-artworks|game|updates|customize|remove]\n\
      [--hover X,Y] [--click X,Y] [--key up|down|left|right|enter|escape] [--demo]\n\
      --demo : bibliothèque de démonstration avec artworks factices (la\n\
      configuration réelle n'est ni lue ni écrite)\n";

/// Construit l'interface, la fait vivre quelques images, puis écrit le PNG.
pub fn run(opts: &SnapshotOptions) -> Result<(), String> {
    let size = Vec2::new(opts.size.x.max(320.0), opts.size.y.max(240.0));
    let scale = opts.scale.clamp(0.5, 4.0);
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(scale);
    // Sans ces chargeurs, toute image distante ou `file://` (miniatures de la
    // recherche, propositions d'artworks) s'afficherait comme une erreur.
    egui_extras::install_image_loaders(&ctx);

    let mut app = QuarkApp::new(&ctx);
    if opts.demo {
        app.lib = demo_library()?;
        app.selected = app.lib.games.first().map(|g| g.id.clone());
        app.filter.clear();
        // Runtimes factices : la VM n'a ni Steam ni Proton.
        app.runtimes = demo_runtimes();
    }
    if let Some(view) = opts.view {
        app.lib.settings.view = view;
    }
    if let Some(theme) = opts.theme {
        app.lib.settings.theme = theme;
    }
    if let Some(tab) = opts.tab {
        app.tab = tab;
    }
    if let Some(needle) = &opts.select {
        app.selected = Some(pick_game(&app, needle)?);
    }
    if opts.marked {
        // Sélection multiple (Ctrl+clic) : deux jeux en plus du courant.
        app.marked = app
            .lib
            .games
            .iter()
            .skip(1)
            .take(2)
            .map(|game| game.id.clone())
            .collect();
    }
    app.show_info = opts.info;
    if let Some(dialog) = &opts.dialog {
        apply_dialog(&mut app, dialog)?;
    }
    seed_news(&mut app);
    if opts.running {
        mark_running(&mut app);
    }

    let width = (size.x * scale).round() as usize;
    let height = (size.y * scale).round() as usize;
    let mut framebuffer = vec![Color32::BLACK; width * height];
    let mut textures = Textures::default();

    for frame in 0..opts.frames.max(1) {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, size));
        input.time = Some(frame as f64 / 60.0);
        input.predicted_dt = 1.0 / 60.0;

        if let Some(pointer) = opts.hover {
            input.events.push(Event::PointerMoved(pointer));
        }
        if let Some(pointer) = opts.click {
            // Un vrai clic : survol, appui, relâchement. Les images suivantes
            // montrent le résultat (fenêtre ouverte, jeu sélectionné…).
            if frame == 0 {
                input.events.push(Event::PointerMoved(pointer));
            }
            if frame == 1 {
                input.events.push(Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                });
            }
            if frame == 2 {
                input.events.push(Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
            }
        }

        if frame == 1 {
            if let Some(key) = opts.key.as_deref().and_then(key_from_name) {
                input.events.push(Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
            }
        }

        let output = ctx.run(input, |ctx| app.ui(ctx));
        textures.apply(&output.textures_delta);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        rasterize(
            &mut framebuffer,
            width,
            height,
            output.pixels_per_point,
            &primitives,
            &textures,
        );
    }

    for (_, game) in app.running.iter_mut() {
        let _ = game.child.kill();
    }

    save_png(&opts.out, width, height, &framebuffer)?;
    println!(
        "Rendu {}×{} ({} image(s)) -> {}",
        width,
        height,
        opts.frames,
        opts.out.display()
    );
    println!(
        "onglet {:?} · vue {:?} · jeu « {} »",
        match app.tab {
            Tab::Store => "boutique",
            Tab::Library => "bibliothèque",
        },
        match app.lib.settings.view {
            ViewMode::List => "liste",
            ViewMode::Grid => "grille",
        },
        app.selected
            .as_ref()
            .and_then(|id| app.lib.get(id))
            .map(|game| game.name.clone())
            .unwrap_or_else(|| "aucun".to_string()),
    );
    Ok(())
}

fn key_from_name(name: &str) -> Option<egui::Key> {
    Some(match name {
        "up" => egui::Key::ArrowUp,
        "down" => egui::Key::ArrowDown,
        "left" => egui::Key::ArrowLeft,
        "right" => egui::Key::ArrowRight,
        "enter" => egui::Key::Enter,
        "escape" => egui::Key::Escape,
        "space" => egui::Key::Space,
        _ => return None,
    })
}

fn pick_game(app: &QuarkApp, needle: &str) -> Result<String, String> {
    if let Some(game) = app.lib.get(needle) {
        return Ok(game.id.clone());
    }
    let lower = needle.to_lowercase();
    let found = app
        .lib
        .games
        .iter()
        .find(|game| game.name.to_lowercase().contains(&lower));
    match found {
        Some(game) => Ok(game.id.clone()),
        None => Err(format!(
            "aucun jeu ne correspond à « {needle} » (bibliothèque : {})",
            app.lib
                .games
                .iter()
                .map(|game| game.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Force l'ouverture d'une fenêtre pour la photographier.
fn apply_dialog(app: &mut QuarkApp, dialog: &str) -> Result<(), String> {
    match dialog {
        "none" => {}
        "settings" => app.settings_open = true,
        "settings-proton" => {
            app.settings_open = true;
            app.settings_tab = crate::app::SettingsTab::Proton;
        }
        "settings-env" => {
            app.settings_open = true;
            app.settings_tab = crate::app::SettingsTab::Environment;
        }
        "settings-artworks" => {
            app.settings_open = true;
            app.settings_tab = crate::app::SettingsTab::Artworks;
        }
        "updates" => {
            app.settings_open = true;
            app.update = crate::app::UpdateState::Available(crate::update::Release {
                tag_name: "v0.4.0".to_string(),
                name: "Quark v0.4.0".to_string(),
                html_url: "https://github.com/Matheotaku4/Quark-Launcher/releases".to_string(),
                assets: vec![crate::update::Asset {
                    name: "quark-0.4.0-linux-x86_64.tar.gz".to_string(),
                    browser_download_url: String::new(),
                    size: 8_600_000,
                }],
            });
        }
        "customize" => {
            let id = app
                .selected
                .clone()
                .ok_or_else(|| "aucun jeu sélectionné".to_string())?;
            let mut customize = crate::app::Customize::for_game(&id);
            customize.kind = Some(AssetKind::Cover);
            // Propositions de démonstration : les couvertures des autres jeux
            // (déjà sur le disque, donc affichables sans réseau).
            let mut plan = AssetPlan::default();
            for game in &app.lib.games {
                if let Some(path) = game.artwork.get(AssetKind::Cover) {
                    // Schéma `file://` : c'est ce que le chargeur d'images egui
                    // attend pour un fichier local (les vraies propositions
                    // sont des URL https).
                    plan.add(AssetKind::Cover, format!("file://{}", path.display()));
                }
            }
            customize.plan = Some(plan);
            app.customize = Some(customize);
        }
        "game" => {
            let id = app
                .selected
                .clone()
                .ok_or_else(|| "aucun jeu sélectionné".to_string())?;
            let mut game = app
                .lib
                .get(&id)
                .cloned()
                .ok_or_else(|| "jeu sélectionné introuvable".to_string())?;
            // Les deux options anti-triche cochées : c'est l'état que le rendu
            // doit montrer (les deux dossiers trouvés et leur variable).
            game.anticheat = crate::config::AnticheatMode::Both;
            app.editing = Some(game);
        }
        "remove" => {
            // Avec --marked, la confirmation porte sur toute la sélection.
            let ids: Vec<String> = if app.marked.is_empty() {
                app.selected.clone().into_iter().collect()
            } else {
                app.marked.clone()
            };
            if ids.is_empty() {
                return Err("aucun jeu sélectionné".to_string());
            }
            app.confirm_remove = ids;
        }
        "add" => {
            app.open_add_dialog();
            app.add.name = "Hollow Knight".to_string();
            app.add.searched_for = "Hollow Knight".to_string();
            app.add.state = SearchState::Done(
                vec![
                    SearchHit {
                        source: Source::Steam,
                        title: "Hollow Knight".to_string(),
                        steam_app_id: Some(367520),
                        sgdb_id: None,
                        thumb: None,
                        detail: "Team Cherry · 2017".to_string(),
                    },
                    SearchHit {
                        source: Source::Steam,
                        title: "Hollow Knight: Silksong".to_string(),
                        steam_app_id: Some(1030300),
                        sgdb_id: None,
                        thumb: None,
                        detail: "Team Cherry · à venir".to_string(),
                    },
                    SearchHit {
                        source: Source::SteamGridDb,
                        title: "Hollow Knight (custom)".to_string(),
                        steam_app_id: None,
                        sgdb_id: Some(1234),
                        thumb: None,
                        detail: "SteamGridDB".to_string(),
                    },
                ],
                Vec::new(),
            );
        }
        "exe" => {
            app.open_add_dialog();
            app.add.step = AddStep::Exe;
            app.add.name = "Hollow Knight".to_string();
            app.add.hit = Some(SearchHit {
                source: Source::Steam,
                title: "Hollow Knight".to_string(),
                steam_app_id: Some(367520),
                sgdb_id: None,
                thumb: None,
                detail: "Team Cherry · 2017".to_string(),
            });
            app.add.exe = "/jeux/Hollow Knight/hollow_knight.exe".to_string();
        }
        "artwork" => {
            app.open_add_dialog();
            app.add.step = AddStep::Artwork;
            app.add.name = "Hollow Knight".to_string();
            app.add.hit = Some(SearchHit {
                source: Source::Steam,
                title: "Hollow Knight".to_string(),
                steam_app_id: Some(367520),
                sgdb_id: None,
                thumb: None,
                detail: "Team Cherry · 2017".to_string(),
            });
            // Propositions locales : le rendu hors écran n'attend pas le réseau.
            let dir = crate::config::data_dir().join("demo-art");
            let files = ["cover.png", "hero.png", "icon.png", "logo.png"];
            let mut kinds = Vec::new();
            for (kind, base) in [
                (AssetKind::Hero, 1usize),
                (AssetKind::Logo, 3),
                (AssetKind::Cover, 0),
                (AssetKind::Icon, 2),
            ] {
                let urls = (0..3)
                    .map(|offset| {
                        format!(
                            "file://{}",
                            dir.join(files[(base + offset) % files.len()]).display()
                        )
                    })
                    .collect();
                kinds.push((kind, urls));
            }
            app.add.artwork.plan = Some(AssetPlan {
                kinds,
                warnings: Vec::new(),
            });
        }
        "scan" => {
            app.open_add_dialog();
            app.add.step = AddStep::Scan;
            app.add.scan.folder = Some(PathBuf::from("/jeux"));
            app.add.scan.candidates = vec![
                ScanCandidate {
                    exe: PathBuf::from("/jeux/Celeste/Celeste.exe"),
                    title: "Celeste".to_string(),
                    include: true,
                    hit: Some(SearchHit {
                        source: Source::Steam,
                        title: "Celeste".to_string(),
                        steam_app_id: Some(391540),
                        sgdb_id: None,
                        thumb: None,
                        detail: "Maddy Makes Games · 2018".to_string(),
                    }),
                    detail: "Steam · Maddy Makes Games · 2018".to_string(),
                    searching: false,
                },
                ScanCandidate {
                    exe: PathBuf::from("/jeux/Hollow Knight/hollow_knight.exe"),
                    title: "Hollow Knight".to_string(),
                    include: true,
                    hit: Some(SearchHit {
                        source: Source::Steam,
                        title: "Hollow Knight".to_string(),
                        steam_app_id: Some(367520),
                        sgdb_id: None,
                        thumb: None,
                        detail: "Team Cherry · 2017".to_string(),
                    }),
                    detail: "Steam · Team Cherry · 2017".to_string(),
                    searching: false,
                },
                ScanCandidate {
                    exe: PathBuf::from("/jeux/divers/tool_helper.exe"),
                    title: "tool helper".to_string(),
                    include: false,
                    hit: None,
                    detail: "aucune correspondance".to_string(),
                    searching: false,
                },
            ];
        }
        other => return Err(format!("fenêtre inconnue : {other}")),
    }
    Ok(())
}

/// Actualités de démonstration : évite tout accès réseau pendant le rendu.
fn seed_news(app: &mut QuarkApp) {
    let Some(id) = app.selected.clone() else {
        return;
    };
    let now = now_ts();
    let items = vec![
        NewsItem {
            title: "Mise à jour 1.6 : nouvelles quêtes et corrections".to_string(),
            url: "https://store.steampowered.com/news/app/292030/view/1".to_string(),
            feed: "Community Announcements".to_string(),
            author: "Studio".to_string(),
            date: now - 3600 * 5,
            excerpt: "La mise à jour apporte trois nouvelles quêtes secondaires, un rééquilibrage \
                      complet du mode difficile et une trentaine de corrections signalées par la \
                      communauté."
                .to_string(),
        },
        NewsItem {
            title: "Concours de captures d'écran : les gagnants".to_string(),
            url: "https://store.steampowered.com/news/app/292030/view/2".to_string(),
            feed: "Community Announcements".to_string(),
            author: "Équipe communauté".to_string(),
            date: now - 3 * 86400,
            excerpt: "Plus de deux mille participations cette année : merci ! Voici la sélection \
                      finale, ainsi que les coulisses du jury."
                .to_string(),
        },
        NewsItem {
            title: "Journal des développeurs : l'audio du jeu".to_string(),
            url: "https://store.steampowered.com/news/app/292030/view/3".to_string(),
            feed: "PC Gamer".to_string(),
            author: String::new(),
            date: now - 12 * 86400,
            excerpt: "Retour sur la conception de la bande-son, des premières maquettes aux \
                      enregistrements avec l'orchestre."
                .to_string(),
        },
    ];
    app.news.insert(id, NewsState::Loaded(items));
}

/// Marque le jeu sélectionné comme « en cours » (processus réel, terminé à la
/// fin du rendu) pour photographier cet état.
fn mark_running(app: &mut QuarkApp) {
    let Some(id) = app.selected.clone() else {
        return;
    };
    if let Ok(child) = Command::new("sleep").arg("600").spawn() {
        app.running.insert(
            id,
            crate::app::RunningGame {
                child,
                started: std::time::Instant::now(),
                session: 4_740,
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Bibliothèque de démonstration (artworks factices générés localement)
// ---------------------------------------------------------------------------

struct DemoArt {
    hero: PathBuf,
    cover: PathBuf,
    logo: PathBuf,
    icon: PathBuf,
}

impl DemoArt {
    fn write(dir: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        let hero = dir.join("hero.png");
        let cover = dir.join("cover.png");
        let logo = dir.join("logo.png");
        let icon = dir.join("icon.png");

        if !hero.exists() {
            save_demo_png(&hero, 1600, 516, |x, y| {
                let t = x as f32 / 1600.0;
                let s = y as f32 / 516.0;
                let mut px = [
                    (24.0 + 70.0 * t) as u8,
                    (44.0 + 26.0 * s) as u8,
                    (74.0 + 70.0 * (1.0 - t)) as u8,
                    255,
                ];
                if ((x + y) / 30) % 2 == 0 {
                    px[0] = px[0].saturating_add(12);
                    px[1] = px[1].saturating_add(9);
                }
                let dx = x as f32 - 1600.0 * 0.78;
                let dy = y as f32 - 516.0 * 0.36;
                let radius: f32 = 516.0 * 0.24;
                if dx * dx + dy * dy < radius * radius {
                    px = [232, 238, 246, 255];
                }
                if (x < 5 || x + 5 > 1600) && (y < 5 || y + 5 > 516) {
                    px = [255, 90, 70, 255];
                }
                px
            })?;
        }
        if !cover.exists() {
            save_demo_png(&cover, 600, 900, |x, y| {
                let s = y as f32 / 900.0;
                let mut px = [
                    (40.0 + 60.0 * s) as u8,
                    (70.0 + 40.0 * (1.0 - s)) as u8,
                    (110.0 + 80.0 * s) as u8,
                    255,
                ];
                let dx = x as f32 - 300.0;
                let dy = y as f32 - 380.0;
                let d = (dx * dx + dy * dy).sqrt();
                if d < 210.0 {
                    px = [235, 208, 120, 255];
                }
                if (x < 4 || x + 4 > 600) && (y < 4 || y + 4 > 900) {
                    px = [255, 90, 70, 255];
                }
                px
            })?;
        }
        if !logo.exists() {
            // Logo transparent : vérifie aussi la gestion de l'alpha.
            save_demo_png(&logo, 588, 360, |x, y| {
                let dx = x as f32 - 294.0;
                let dy = y as f32 - 180.0;
                if (x > 60 && x < 528 && y > 210 && y < 260) || (dx * dx) / 4.0 + dy * dy < 4_800.0 {
                    [245, 245, 250, 255]
                } else {
                    [0, 0, 0, 0]
                }
            })?;
        }
        if !icon.exists() {
            save_demo_png(&icon, 460, 215, |x, y| {
                let t = x as f32 / 460.0;
                let mut px = [(150.0 + 60.0 * t) as u8, 60, 190, 255];
                if x > 150 && x < 310 && y > 60 && y < 160 {
                    px = [240, 240, 245, 255];
                }
                px
            })?;
        }

        Ok(Self {
            hero,
            cover,
            logo,
            icon,
        })
    }

    fn path(&self, kind: AssetKind) -> PathBuf {
        match kind {
            AssetKind::Hero => self.hero.clone(),
            AssetKind::Cover => self.cover.clone(),
            AssetKind::Logo => self.logo.clone(),
            AssetKind::Icon => self.icon.clone(),
        }
    }
}

fn save_demo_png(
    path: &Path,
    width: u32,
    height: u32,
    color: impl Fn(u32, u32) -> [u8; 4],
) -> Result<(), String> {
    let mut image = image::RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            image.put_pixel(x, y, image::Rgba(color(x, y)));
        }
    }
    image.save(path).map_err(|err| err.to_string())
}

/// Jeux de test : certains complets, d'autres partiels, d'autres sans aucun
/// artwork — pour vérifier tous les rendus de secours.
fn demo_library() -> Result<Library, String> {
    let art = DemoArt::write(&crate::config::data_dir().join("demo-art"))?;
    let now = now_ts();

    let mut games = Vec::new();
    let mut game = demo_game(
        "The Witcher 3: Wild Hunt",
        "/jeux/witcher3/bin/x64_dx12/witcher3.exe",
        &[AssetKind::Hero, AssetKind::Cover, AssetKind::Logo, AssetKind::Icon],
        &art,
        Some(292030),
    );
    game.playtime_secs = 82 * 3600 + 1_320;
    game.last_played = Some(now - 7_200);
    game.env = vec![crate::config::EnvVar::new(
        "WINEDLLOVERRIDES",
        "winmm=n,b;steam_api64",
    )];
    games.push(game);

    let mut game = demo_game(
        "7 Days to Die",
        "/jeux/7dtd/7DaysToDie.exe",
        &[AssetKind::Hero, AssetKind::Icon],
        &art,
        Some(251570),
    );
    game.playtime_secs = 5 * 3600;
    game.last_played = Some(now - 4 * 86_400);
    // 7 Days to Die utilise EasyAntiCheat : bon cas pour voir l'option cochée.
    game.anticheat = crate::config::AnticheatMode::EasyAntiCheat;
    games.push(game);

    let mut game = demo_game(
        "A Short Hike",
        "/jeux/a-short-hike/AShortHike.exe",
        &[AssetKind::Icon],
        &art,
        None,
    );
    game.playtime_secs = 2_100;
    games.push(game);

    let mut game = demo_game(
        "Celeste",
        "/jeux/Celeste/Celeste.exe",
        &[AssetKind::Hero, AssetKind::Cover, AssetKind::Logo],
        &art,
        Some(391540),
    );
    game.playtime_secs = 46 * 3600 + 900;
    game.last_played = Some(now - 46 * 3600);
    games.push(game);

    let game = demo_game(
        "Hollow Knight",
        "/jeux/Hollow Knight/hollow_knight.exe",
        &[AssetKind::Cover],
        &art,
        Some(367520),
    );
    games.push(game);

    games.push(demo_game(
        "Les Chevaliers de Baphomet",
        "/jeux/baphomet/baphomet.exe",
        &[],
        &art,
        None,
    ));
    games.push(demo_game("Another World", "/jeux/another/another", &[], &art, None));

    let mut settings = crate::config::Settings::default();
    settings.env = vec![
        crate::config::EnvVar::new("MANGOHUD", "1"),
        crate::config::EnvVar::new("DXVK_HUD", "fps"),
    ];

    Ok(Library { games, settings })
}

/// Runtimes Proton et anti-triche factices, pour les rendus de contrôle.
fn demo_runtimes() -> Vec<ProtonRuntime> {
    let root = PathBuf::from("/tmp/steam");
    let mut out = Vec::new();
    // Noms tels qu'on les trouve dans une installation Steam réelle.
    for (name, version, kind) in [
        ("GE-Proton9-20", "9-20", RuntimeKind::Proton),
        ("Proton 9.0", "9.0", RuntimeKind::Proton),
        ("Proton Experimental", "", RuntimeKind::Proton),
        ("BattlEye AntiCheat Runtime", "", RuntimeKind::AntiCheat),
        ("EasyAntiCheat Runtime", "", RuntimeKind::AntiCheat),
    ] {
        let has_proton_script = kind == RuntimeKind::Proton;
        out.push(ProtonRuntime {
            key: name.to_string(),
            name: name.to_string(),
            version: if version.is_empty() {
                None
            } else {
                Some(version.to_string())
            },
            path: if has_proton_script {
                root.join(name).join("proton")
            } else {
                root.join("steamapps/common").join(name)
            },
            steam_root: root.clone(),
            kind,
            has_proton_script,
        });
    }
    out
}

fn demo_game(
    name: &str,
    exe: &str,
    kinds: &[AssetKind],
    art: &DemoArt,
    steam_app_id: Option<u32>,
) -> Game {
    let mut game = Game::new(name, exe);
    game.steam_app_id = steam_app_id;
    let mut artwork = Artwork::default();
    for kind in kinds {
        artwork.set(*kind, art.path(*kind));
    }
    game.artwork = artwork;
    game
}

// ---------------------------------------------------------------------------
// Textures et rasterisation logicielle
// ---------------------------------------------------------------------------

struct Texture {
    width: usize,
    height: usize,
    pixels: Vec<Color32>,
}

impl Texture {
    fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [0, 0, 0, 0];
        }
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let at = |ix: f32, iy: f32| -> [u8; 4] {
            let ix = (ix as i32).clamp(0, self.width as i32 - 1) as usize;
            let iy = (iy as i32).clamp(0, self.height as i32 - 1) as usize;
            self.pixels[iy * self.width + ix].to_array()
        };
        let p00 = at(x0, y0);
        let p10 = at(x0 + 1.0, y0);
        let p01 = at(x0, y0 + 1.0);
        let p11 = at(x0 + 1.0, y0 + 1.0);
        let mut out = [0u8; 4];
        for channel in 0..4 {
            let top = p00[channel] as f32 + (p10[channel] as f32 - p00[channel] as f32) * fx;
            let bottom = p01[channel] as f32 + (p11[channel] as f32 - p01[channel] as f32) * fx;
            out[channel] = (top + (bottom - top) * fy).round().clamp(0.0, 255.0) as u8;
        }
        out
    }
}

#[derive(Default)]
struct Textures(HashMap<egui::TextureId, Texture>);

impl Textures {
    fn apply(&mut self, delta: &TexturesDelta) {
        for (id, update) in &delta.set {
            let image = match &update.image {
                ImageData::Color(image) => image,
            };
            match update.pos {
                None => {
                    self.0.insert(
                        *id,
                        Texture {
                            width: image.size[0],
                            height: image.size[1],
                            pixels: image.pixels.clone(),
                        },
                    );
                }
                Some([x0, y0]) => {
                    if let Some(texture) = self.0.get_mut(id) {
                        for row in 0..image.size[1] {
                            let ty = y0 + row;
                            if ty >= texture.height {
                                continue;
                            }
                            for column in 0..image.size[0] {
                                let tx = x0 + column;
                                if tx >= texture.width {
                                    continue;
                                }
                                texture.pixels[ty * texture.width + tx] =
                                    image.pixels[row * image.size[0] + column];
                            }
                        }
                    }
                }
            }
        }
        for id in &delta.free {
            self.0.remove(id);
        }
    }
}

/// Rasterise les triangles d'egui dans un tampon de pixels (couleurs
/// prémultipliées, comme le fait epaint).
fn rasterize(
    framebuffer: &mut [Color32],
    width: usize,
    height: usize,
    pixels_per_point: f32,
    primitives: &[ClippedPrimitive],
    textures: &Textures,
) {
    if framebuffer.len() != width * height {
        return;
    }
    framebuffer.fill(Color32::BLACK);

    for clipped in primitives {
        let Primitive::Mesh(mesh) = &clipped.primitive else {
            continue;
        };
        let Some(texture) = textures.0.get(&mesh.texture_id) else {
            continue;
        };
        let clip = clipped.clip_rect;
        let clip_x0 = (clip.min.x * pixels_per_point).floor().max(0.0) as usize;
        let clip_y0 = (clip.min.y * pixels_per_point).floor().max(0.0) as usize;
        let clip_x1 = ((clip.max.x * pixels_per_point).ceil().max(0.0) as usize).min(width);
        let clip_y1 = ((clip.max.y * pixels_per_point).ceil().max(0.0) as usize).min(height);
        if clip_x0 >= clip_x1 || clip_y0 >= clip_y1 {
            continue;
        }

        for triangle in mesh.indices.chunks_exact(3) {
            let vertices = [
                mesh.vertices[triangle[0] as usize],
                mesh.vertices[triangle[1] as usize],
                mesh.vertices[triangle[2] as usize],
            ];
            let points = [
                Pos2::new(vertices[0].pos.x * pixels_per_point, vertices[0].pos.y * pixels_per_point),
                Pos2::new(vertices[1].pos.x * pixels_per_point, vertices[1].pos.y * pixels_per_point),
                Pos2::new(vertices[2].pos.x * pixels_per_point, vertices[2].pos.y * pixels_per_point),
            ];
            let area = edge(points[0], points[1], points[2]);
            if area.abs() < 1e-6 {
                continue;
            }

            let min_x = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min).floor();
            let max_x = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max).ceil();
            let min_y = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min).floor();
            let max_y = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max).ceil();
            let x_start = (min_x.max(clip_x0 as f32).max(0.0)) as usize;
            let x_end = (max_x.min(clip_x1 as f32)).min(width as f32) as usize;
            let y_start = (min_y.max(clip_y0 as f32).max(0.0)) as usize;
            let y_end = (max_y.min(clip_y1 as f32)).min(height as f32) as usize;
            if x_start >= x_end || y_start >= y_end {
                continue;
            }

            let colors: [[u8; 4]; 3] = vertices.map(|v| v.color.to_array());
            let uvs = [vertices[0].uv, vertices[1].uv, vertices[2].uv];
            let clockwise = area < 0.0;

            for y in y_start..y_end {
                for x in x_start..x_end {
                    let sample = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let weights = [
                        edge(points[1], points[2], sample) / area,
                        edge(points[2], points[0], sample) / area,
                        edge(points[0], points[1], sample) / area,
                    ];
                    let mut inside = true;
                    for i in 0..3 {
                        if weights[i] > 0.0 {
                            continue;
                        }
                        if weights[i] < 0.0 || !is_top_left(points[i], points[(i + 1) % 3], clockwise)
                        {
                            inside = false;
                            break;
                        }
                    }
                    if !inside {
                        continue;
                    }

                    let uv = [
                        uvs[0].x * weights[0] + uvs[1].x * weights[1] + uvs[2].x * weights[2],
                        uvs[0].y * weights[0] + uvs[1].y * weights[1] + uvs[2].y * weights[2],
                    ];
                    let texel = texture.sample(uv[0], uv[1]);
                    let mut source = [0u8; 4];
                    for channel in 0..4 {
                        let value = colors[0][channel] as f32 * weights[0]
                            + colors[1][channel] as f32 * weights[1]
                            + colors[2][channel] as f32 * weights[2];
                        source[channel] = multiply(value.round().clamp(0.0, 255.0) as u8, texel[channel]);
                    }
                    let index = y * width + x;
                    framebuffer[index] = blend(framebuffer[index], source);
                }
            }
        }
    }
}

fn edge(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

/// Règle du coin « haut-gauche » : une arête partagée n'appartient qu'à un seul
/// triangle, ce qui évite les trous comme les doubles mélanges.
fn is_top_left(a: Pos2, b: Pos2, clockwise: bool) -> bool {
    let (a, b) = if clockwise { (b, a) } else { (a, b) };
    let edge = b - a;
    edge.y > 0.0 || (edge.y == 0.0 && edge.x < 0.0)
}

fn multiply(a: u8, b: u8) -> u8 {
    ((a as u32 * b as u32 + 127) / 255) as u8
}

/// Mélange deux couleurs prémultipliées (`src over dst`).
fn blend(dst: Color32, src: [u8; 4]) -> Color32 {
    let behind = dst.to_array();
    let inverse = 255 - src[3] as u32;
    let mut out = [0u8; 4];
    for channel in 0..4 {
        let value = src[channel] as u32 + (behind[channel] as u32 * inverse + 127) / 255;
        out[channel] = value.min(255) as u8;
    }
    Color32::from_rgba_premultiplied(out[0], out[1], out[2], out[3])
}

fn save_png(path: &Path, width: usize, height: usize, framebuffer: &[Color32]) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(width * height * 4);
    for color in framebuffer {
        let [r, g, b, a] = color.to_array();
        if a == 0 || a == 255 {
            bytes.extend_from_slice(&[r, g, b, a]);
        } else {
            let straight = |value: u8| ((value as u32 * 255) / a as u32).min(255) as u8;
            bytes.extend_from_slice(&[straight(r), straight(g), straight(b), a]);
        }
    }
    let image = image::RgbaImage::from_raw(width as u32, height as u32, bytes)
        .ok_or_else(|| "tampon d'image invalide".to_string())?;
    image.save(path).map_err(|err| err.to_string())
}
