//! Fenêtres modales : ajout de jeu, paramètres globaux, paramètres du jeu et
//! confirmation de suppression. Les sélecteurs de fichiers sont ceux du système
//! (XDG Desktop Portal), ouverts via `QuarkApp::pick_file` / `pick_directory`.

use crate::app::{AddStep, PickerTarget, QuarkApp, SearchState};
use crate::config::{self, ProtonMode, ThemeMode, ViewMode};
use crate::proton::{self, ProtonRuntime};
use crate::theme::Palette;
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Align2, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2};
use std::path::PathBuf;
use std::time::Instant;

impl QuarkApp {
    // -----------------------------------------------------------------------
    // Ajouter un jeu : nom -> artworks (Steam / SteamGridDB) -> exécutable
    // -----------------------------------------------------------------------

    pub(crate) fn add_dialog(&mut self, ctx: &egui::Context) {
        if !self.add.open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.add.open = false;
            return;
        }

        let pal = self.pal;
        let step = self.add.step;
        let mut close = false;
        let mut continue_to_exe = false;
        let mut pick_hit: Option<usize> = None;
        let mut submit = false;
        let mut back = false;
        let mut browse_exe = false;
        let mut browse_prefix = false;
        let mut browse_folder = false;
        let mut scan_action = ScanAction::default();

        let title = match step {
            AddStep::Name => "Ajouter un jeu",
            AddStep::Scan => "Ajouter un jeu · dossier",
            AddStep::Exe => "Ajouter un jeu · exécutable",
        };

        let modal = egui::Modal::new(egui::Id::new("quark_add_game")).show(ctx, |ui| {
                ui.set_min_width(660.0);
                ui.label(RichText::new(title).size(17.0).color(pal.text));
                ui.add_space(10.0);

                match step {
                    AddStep::Name => {
                        ui.label(RichText::new("Nom du jeu").size(12.0).color(pal.text_dim));
                        ui.add_space(4.0);
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.add.name)
                                .hint_text("Ex. Hollow Knight")
                                .font(FontId::proportional(15.0))
                                .desired_width(f32::INFINITY),
                        );
                        if response.changed() {
                            // On garde les résultats affichés : ils sont remplacés
                            // dès que la nouvelle requête part.
                            self.add.last_typed = Some(Instant::now());
                        }
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.start_search(ui.ctx());
                        }
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "Quark cherche le jeu sur Steam puis sur SteamGridDB pour récupérer \
                                 les artworks originaux (bannière, logo, couverture).",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );
                        ui.add_space(12.0);

                        let mut warnings: Vec<String> = Vec::new();
                        match &self.add.state {
                            SearchState::Idle => {
                                ui.label(
                                    RichText::new("Tape au moins 2 lettres, la recherche est automatique.")
                                        .size(12.0)
                                        .color(pal.text_faint),
                                );
                            }
                            SearchState::Loading => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new("Recherche en cours…").size(12.5).color(pal.text_dim));
                                });
                            }
                            SearchState::Done(hits, warns) => {
                                warnings = warns.clone();
                                if hits.is_empty() {
                                    ui.label(
                                        RichText::new("Aucun résultat. Tu peux continuer sans artwork.")
                                            .size(12.5)
                                            .color(pal.text_dim),
                                    );
                                }
                                let list_height = (hits.len().min(6) as f32) * 46.0;
                                egui::ScrollArea::vertical()
                                    .id_salt("quark_search_results")
                                    .max_height(list_height.max(46.0))
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        for (index, hit) in hits.iter().enumerate() {
                                            if search_result_row(ui, &pal, hit).clicked() {
                                                pick_hit = Some(index);
                                            }
                                        }
                                    });
                            }
                        }
                        for warning in warnings {
                            ui.add_space(2.0);
                            ui.label(RichText::new(warning).size(11.0).color(pal.text_faint));
                        }

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            let ready = !self.add.name.trim().is_empty();
                            if widgets::accent_button(
                                ui,
                                Vec2::new(230.0, 36.0),
                                "Choisir l'exécutable",
                                &pal,
                                ready,
                                Icon::Plus,
                            )
                            .clicked()
                                && ready
                            {
                                continue_to_exe = true;
                            }
                            ui.add_space(10.0);
                            if widgets::ghost_button(ui, "Annuler", &pal, Icon::None).clicked() {
                                close = true;
                            }
                        });
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if widgets::ghost_button(ui, "Choisir un dossier…", &pal, Icon::Folder)
                                .clicked()
                            {
                                browse_folder = true;
                            }
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(
                                    "analyse un dossier et propose les jeux qu'il contient",
                                )
                                .size(11.0)
                                .color(pal.text_faint),
                            );
                        });
                    }
                    AddStep::Scan => {
                        scan_action = self.scan_step(ui, &pal);
                    }
                    AddStep::Exe => {
                        ui.label(RichText::new("Nom affiché").size(12.0).color(pal.text_dim));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.add.name)
                                .font(FontId::proportional(15.0))
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(10.0);

                        ui.label(RichText::new("Exécutable du jeu").size(12.0).color(pal.text_dim));
                        ui.horizontal(|ui| {
                            let field = (ui.available_width() - 130.0).max(200.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.add.exe)
                                    .hint_text("/chemin/vers/jeu.exe ou jeu.sh")
                                    .desired_width(field),
                            );
                            if widgets::ghost_button(ui, "Parcourir…", &pal, Icon::Folder).clicked() {
                                browse_exe = true;
                            }
                        });
                        if let Some(hit) = self.add.hit.as_ref() {
                            let info = match (hit.steam_app_id, hit.sgdb_id) {
                                (Some(appid), _) => format!("Artworks : Steam AppID {appid}"),
                                (None, Some(id)) => format!("Artworks : SteamGridDB #{id}"),
                                _ => "Artworks : aucun".to_string(),
                            };
                            ui.label(RichText::new(info).size(11.0).color(pal.text_faint));
                        }
                        ui.add_space(12.0);

                        ui.label(RichText::new("Arguments (optionnel)").size(12.0).color(pal.text_dim));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.add.args)
                                .hint_text("--fullscreen")
                                .desired_width(f32::INFINITY),
                        );
                        ui.add_space(12.0);

                        ui.label(RichText::new("Lancement").size(12.0).color(pal.text_dim));
                        runtime_combo(
                            ui,
                            "add_runtime",
                            &mut self.add.proton,
                            &self.runtimes,
                            true,
                        );
                        ui.add_space(10.0);

                        ui.checkbox(&mut self.add.custom_prefix, "Préfixe Proton personnalisé pour ce jeu");
                        if self.add.custom_prefix {
                            ui.horizontal(|ui| {
                                let field = (ui.available_width() - 130.0).max(200.0);
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.add.prefix)
                                        .hint_text("~/Games/prefixes/mon-jeu")
                                        .desired_width(field),
                                );
                                if widgets::ghost_button(ui, "Parcourir…", &pal, Icon::Folder).clicked()
                                {
                                    browse_prefix = true;
                                }
                            });
                        }

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if widgets::ghost_button(ui, "← Retour", &pal, Icon::None).clicked() {
                                back = true;
                            }
                            ui.add_space(10.0);
                            let ready = !self.add.exe.trim().is_empty();
                            if widgets::accent_button(
                                ui,
                                Vec2::new(200.0, 36.0),
                                "Ajouter le jeu",
                                &pal,
                                ready,
                                Icon::Plus,
                            )
                            .clicked()
                                && ready
                            {
                                submit = true;
                            }
                            if !ready {
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("indique l'exécutable pour valider")
                                        .size(11.0)
                                        .color(pal.text_faint),
                                );
                            }
                        });
                    }
                }
            });
        if modal.should_close() {
            close = true;
        }

        if ctx.input(|i| i.raw.dropped_files.len() > 0) && self.add.step == AddStep::Exe {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            if let Some(path) = dropped.iter().find_map(|f| f.path.clone()) {
                self.add.exe = path.display().to_string();
            }
        }

        if pick_hit.is_some() || continue_to_exe {
            if let Some(index) = pick_hit {
                if let SearchState::Done(hits, _) = &self.add.state {
                    if let Some(hit) = hits.get(index).cloned() {
                        self.add.name = hit.title.clone();
                        self.add.hit = Some(hit);
                    }
                }
            } else {
                self.add.hit = None;
            }
            self.add.step = AddStep::Exe;
            self.add.error = None;
        }

        if back {
            self.add.step = AddStep::Name;
        }

        if browse_exe {
            let start = own_dir(&self.add.exe);
            self.pick_file(ctx, start, PickerTarget::AddExe);
        }
        if browse_prefix {
            let start = own_dir(&self.add.prefix);
            self.pick_directory(ctx, start, PickerTarget::AddPrefix);
        }
        if browse_folder {
            self.pick_directory(ctx, config::home_dir(), PickerTarget::ScanFolder);
        }

        if submit {
            let name = if self.add.name.trim().is_empty() {
                "Jeu sans nom".to_string()
            } else {
                self.add.name.trim().to_string()
            };
            let mut game = crate::config::Game::new(name, self.add.exe.trim());
            game.args = self.add.args.trim().to_string();
            game.proton = self.add.proton.clone();
            if self.add.custom_prefix && !self.add.prefix.trim().is_empty() {
                game.prefix = Some(self.add.prefix.trim().to_string());
            }
            if let Some(hit) = self.add.hit.clone() {
                game.steam_app_id = hit.steam_app_id;
                game.sgdb_id = hit.sgdb_id;
            }
            let id = game.id.clone();
            let hit = self.add.hit.clone();
            self.lib.games.push(game);
            self.selected = Some(id.clone());
            self.add.open = false;
            self.touch();
            match hit {
                Some(hit) => self.start_asset_job(ctx, &id, hit),
                None => self.toast_info("Jeu ajouté sans artwork (assets manuels)"),
            }
        }

        if scan_action.back {
            self.add.step = AddStep::Name;
            self.add.scan = crate::app::ScanJob::default();
        }
        if let Some(index) = scan_action.research {
            self.start_scan_search(ctx, index);
        }
        if scan_action.submit {
            self.add_scanned(ctx);
        }

        if close {
            self.add.open = false;
        }
    }

    // -----------------------------------------------------------------------
    // Paramètres généraux
    // -----------------------------------------------------------------------

    pub(crate) fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
            return;
        }
        let pal = self.pal;
        let mut close_requested = false;
        let mut browse_prefix = false;
        let mut redetect = false;

        let max_height = (ctx.screen_rect().height() * 0.72).max(280.0);
        let modal = egui::Modal::new(egui::Id::new("quark_settings")).show(ctx, |ui| {
                ui.set_min_width(680.0);
                ui.label(RichText::new("Paramètres").size(17.0).color(pal.text));
                ui.add_space(10.0);
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                section(ui, "APPARENCE", &pal);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Thème").size(12.5).color(pal.text_dim));
                    ui.add_space(12.0);
                    for mode in [ThemeMode::Amoled, ThemeMode::Graphite] {
                        if ui
                            .radio_value(&mut self.lib.settings.theme, mode, mode.label())
                            .changed()
                        {
                            self.touch();
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Vue par défaut").size(12.5).color(pal.text_dim));
                    ui.add_space(12.0);
                    for view in [ViewMode::List, ViewMode::Grid] {
                        if ui
                            .radio_value(&mut self.lib.settings.view, view, view.label())
                            .changed()
                        {
                            self.touch();
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("(bascule aussi depuis la colonne de gauche)")
                            .size(11.0)
                            .color(pal.text_faint),
                    );
                });

                section(ui, "PROTON", &pal);
                ui.label(
                    RichText::new("Préfixe Proton par défaut")
                        .size(12.5)
                        .color(pal.text_dim),
                );
                ui.horizontal(|ui| {
                    let mut text = self.lib.settings.default_prefix.clone().unwrap_or_default();
                    let field = (ui.available_width() - 210.0).max(220.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut text)
                            .hint_text(format!("auto : {}", config::prefix_root().display()))
                            .desired_width(field),
                    );
                    if response.changed() {
                        self.lib.settings.default_prefix = if text.trim().is_empty() {
                            None
                        } else {
                            Some(text.clone())
                        };
                        self.touch();
                    }
                    if widgets::ghost_button(ui, "Parcourir…", &pal, Icon::Folder).clicked() {
                        browse_prefix = true;
                    }
                    if widgets::ghost_button(ui, "Auto", &pal, Icon::None).clicked() {
                        self.lib.settings.default_prefix = None;
                        self.touch();
                    }
                });
                ui.label(
                    RichText::new(
                        "Utilisé par tous les jeux qui n'ont pas leur propre préfixe \
                         (un sous-dossier par jeu est créé automatiquement).",
                    )
                    .size(11.0)
                    .color(pal.text_faint),
                );

                ui.add_space(10.0);
                ui.label(RichText::new("Runtime Proton par défaut").size(12.5).color(pal.text_dim));
                let mut mode = self
                    .lib
                    .settings
                    .default_proton
                    .clone()
                    .map(ProtonMode::Runtime)
                    .unwrap_or(ProtonMode::Default);
                if runtime_combo(ui, "default_runtime", &mut mode, &self.runtimes, false) {
                    self.lib.settings.default_proton = match mode {
                        ProtonMode::Runtime(key) => Some(key),
                        _ => None,
                    };
                    self.touch();
                }
                ui.horizontal(|ui| {
                    let count = self.runtimes.len();
                    ui.label(
                        RichText::new(format!("{count} runtime(s) Proton détecté(s)"))
                            .size(11.0)
                            .color(pal.text_faint),
                    );
                    if widgets::ghost_button(ui, "Redétecter", &pal, Icon::Refresh).clicked() {
                        redetect = true;
                    }
                });
                if self.runtimes.is_empty() {
                    ui.label(
                        RichText::new(
                            "Aucun runtime trouvé. Installe Proton (Steam) ou dépose un build \
                             dans ~/.steam/steam/compatibilitytools.d/.",
                        )
                        .size(11.0)
                        .color(pal.text_faint),
                    );
                }

                section(ui, "ARTWORKS", &pal);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Clé API SteamGridDB").size(12.5).color(pal.text_dim));
                    ui.add_space(8.0);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.lib.settings.sgdb_api_key)
                            .password(true)
                            .hint_text("optionnelle")
                            .desired_width(300.0),
                    );
                    if response.changed() {
                        self.touch();
                    }
                });
                ui.label(
                    RichText::new(
                        "Sans clé, Quark se limite aux artworks officiels du CDN Steam. \
                         Clé gratuite : steamgriddb.com/profile/preferences/api",
                    )
                    .size(11.0)
                    .color(pal.text_faint),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Langue / pays (recherche Steam)").size(12.5).color(pal.text_dim));
                    ui.add_space(8.0);
                    let mut language = self.lib.settings.language.clone();
                    egui::ComboBox::from_id_salt("steam_language")
                        .selected_text(language.clone())
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for option in [
                                "french",
                                "english",
                                "german",
                                "spanish",
                                "italian",
                                "portuguese",
                                "schinese",
                                "japanese",
                            ] {
                                ui.selectable_value(&mut language, option.to_string(), option);
                            }
                        });
                    if language != self.lib.settings.language {
                        self.lib.settings.language = language;
                        self.touch();
                    }
                    let mut country = self.lib.settings.country.clone();
                    let response = ui.add_sized(Vec2::new(70.0, 24.0), egui::TextEdit::singleline(&mut country));
                    if response.changed() {
                        self.lib.settings.country = country;
                        self.touch();
                    }
                });

                section(ui, "DOSSIERS", &pal);
                info_line(ui, "Configuration", &config::config_path().display().to_string(), &pal);
                info_line(ui, "Artworks", &config::artwork_root().display().to_string(), &pal);
                info_line(ui, "Préfixes", &config::prefix_root().display().to_string(), &pal);

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if widgets::accent_button(ui, Vec2::new(140.0, 34.0), "Fermer", &pal, true, Icon::None)
                        .clicked()
                    {
                        close_requested = true;
                    }
                });
                    });
            });
        if modal.should_close() {
            close_requested = true;
        }

        if redetect {
            self.runtimes = proton::detect_runtimes();
            self.toast_info(format!("{} runtime(s) Proton détecté(s)", self.runtimes.len()));
            self.touch();
        }
        if browse_prefix {
            let start = self
                .lib
                .settings
                .default_prefix
                .clone()
                .map(|p| PathBuf::from(config::expand_tilde(&p)))
                .unwrap_or_else(config::prefix_root);
            self.pick_directory(ctx, start, PickerTarget::DefaultPrefix);
        }
        if close_requested {
            self.settings_open = false;
        }
    }

    // -----------------------------------------------------------------------
    // Paramètres du jeu (Proton, préfixe, exécutable…)
    // -----------------------------------------------------------------------

    pub(crate) fn game_settings_dialog(&mut self, ctx: &egui::Context) {
        // Le brouillon est sorti de `self` pendant la frame : sans ça, egui
        // éditerait une copie jetable et chaque frappe serait perdue.
        let Some(mut game) = self.editing.take() else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return;
        }
        let pal = self.pal;
        let mut cancel = false;
        let mut save = false;
        let mut browse_exe = false;
        let mut browse_prefix = false;
        let mut remove = false;

        let max_height = (ctx.screen_rect().height() * 0.62).max(260.0);
        let modal = egui::Modal::new(egui::Id::new("quark_game_settings")).show(ctx, |ui| {
                ui.set_min_width(660.0);
                ui.label(
                    RichText::new(format!("Paramètres · {}", game.name))
                        .size(17.0)
                        .color(pal.text),
                );
                ui.add_space(10.0);
                egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                section(ui, "JEU", &pal);
                ui.label(RichText::new("Nom affiché").size(12.5).color(pal.text_dim));
                ui.add(
                    egui::TextEdit::singleline(&mut game.name)
                        .font(FontId::proportional(15.0))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                ui.label(RichText::new("Exécutable").size(12.5).color(pal.text_dim));
                ui.horizontal(|ui| {
                    let field = (ui.available_width() - 130.0).max(200.0);
                    ui.add(egui::TextEdit::singleline(&mut game.exe).desired_width(field));
                    if widgets::ghost_button(ui, "Parcourir…", &pal, Icon::Folder).clicked() {
                        browse_exe = true;
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("Arguments (optionnel)").size(12.5).color(pal.text_dim));
                ui.add(
                    egui::TextEdit::singleline(&mut game.args)
                        .hint_text("--fullscreen")
                        .desired_width(f32::INFINITY),
                );

                section(ui, "PROTON", &pal);
                runtime_combo(ui, "game_runtime", &mut game.proton, &self.runtimes, true);
                ui.add_space(8.0);
                let default_prefix = self
                    .lib
                    .settings
                    .default_prefix
                    .clone()
                    .unwrap_or_else(|| format!("auto : {}", config::prefix_root().display()));
                ui.label(
                    RichText::new(format!("Préfixe par défaut : {default_prefix}"))
                        .size(11.0)
                        .color(pal.text_faint),
                );
                let mut custom = game.prefix.is_some();
                if ui
                    .checkbox(&mut custom, "Préfixe Proton propre à ce jeu")
                    .changed()
                {
                    game.prefix = if custom { Some(String::new()) } else { None };
                }
                if let Some(prefix) = game.prefix.as_mut() {
                    ui.horizontal(|ui| {
                        let field = (ui.available_width() - 130.0).max(200.0);
                        ui.add(
                            egui::TextEdit::singleline(prefix)
                                .hint_text("~/Games/prefixes/mon-jeu")
                                .desired_width(field),
                        );
                        if widgets::ghost_button(ui, "Parcourir…", &pal, Icon::Folder).clicked() {
                            browse_prefix = true;
                        }
                    });
                }
                ui.add_space(6.0);
                let effective = game.prefix_path(&self.lib.settings);
                ui.label(
                    RichText::new(format!("Préfixe utilisé : {}", effective.display()))
                        .size(11.0)
                        .color(pal.text_faint),
                );

                section(ui, "ARTWORKS", &pal);
                let sources = match (game.steam_app_id, game.sgdb_id) {
                    (Some(appid), Some(sgdb)) => format!("Steam AppID {appid} · SteamGridDB #{sgdb}"),
                    (Some(appid), None) => format!("Steam AppID {appid}"),
                    (None, Some(sgdb)) => format!("SteamGridDB #{sgdb}"),
                    (None, None) => "aucune source (assets manuels)".to_string(),
                };
                ui.label(RichText::new(sources).size(12.0).color(pal.text_dim));

                section(ui, "DONNÉES", &pal);
                ui.label(
                    RichText::new(format!(
                        "Temps de jeu : {} · dernière session : {}",
                        game.playtime_label(),
                        game.last_played_label()
                    ))
                    .size(12.0)
                    .color(pal.text_dim),
                );

                });
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    let ready = !game.exe.trim().is_empty();
                    if widgets::accent_button(ui, Vec2::new(160.0, 34.0), "Enregistrer", &pal, ready, Icon::None)
                        .clicked()
                        && ready
                    {
                        save = true;
                    }
                    ui.add_space(8.0);
                    if widgets::ghost_button(ui, "Annuler", &pal, Icon::None).clicked() {
                        cancel = true;
                    }
                    ui.add_space(8.0);
                    if widgets::ghost_button(ui, "Réinitialiser le temps de jeu", &pal, Icon::None)
                        .clicked()
                    {
                        game.playtime_secs = 0;
                        game.last_played = None;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::danger_button(
                            ui,
                            Vec2::new(150.0, 34.0),
                            "Supprimer",
                            &pal,
                            true,
                            Icon::Trash,
                        )
                        .clicked()
                        {
                            remove = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Les fichiers du jeu et le préfixe Proton ne sont jamais supprimés.")
                        .size(11.0)
                        .color(pal.text_faint),
                );
            });
        if modal.should_close() {
            cancel = true;
        }

        if browse_exe {
            self.pick_file(ctx, own_dir(&game.exe), PickerTarget::GameExe);
        }
        if browse_prefix {
            let start = game
                .prefix
                .clone()
                .filter(|p| !p.trim().is_empty())
                .map(|p| PathBuf::from(config::expand_tilde(&p)))
                .unwrap_or_else(config::prefix_root);
            self.pick_directory(ctx, start, PickerTarget::GamePrefix);
        }
        if remove {
            self.confirm_remove = Some(game.id.clone());
            return;
        }
        if save {
            let id = game.id.clone();
            if let Some(slot) = self.lib.get_mut(&id) {
                *slot = game;
            }
            self.touch();
            self.toast_info("Paramètres du jeu enregistrés");
            return;
        }
        // Toujours ouvert : on remet le brouillon en place pour la frame suivante.
        if !cancel {
            self.editing = Some(game);
        }
    }

    // -----------------------------------------------------------------------
    // Confirmation de suppression
    // -----------------------------------------------------------------------

    pub(crate) fn confirm_remove_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_remove.clone() else {
            return;
        };
        let pal = self.pal;
        let name = self
            .lib
            .get(&id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let mut cancel = false;
        let mut confirm = false;

        let modal = egui::Modal::new(egui::Id::new("quark_confirm_remove")).show(ctx, |ui| {
                ui.set_min_width(520.0);
                ui.label(RichText::new(format!("Retirer « {name} » de la bibliothèque ?")).size(14.0));
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Le temps de jeu et les réglages du jeu seront perdus. Les fichiers du jeu \
                         et le préfixe Proton ne sont pas touchés ; les artworks mis en cache sont supprimés.",
                    )
                    .size(11.5)
                    .color(pal.text_faint),
                );
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if widgets::danger_button(ui, Vec2::new(150.0, 34.0), "Supprimer", &pal, true, Icon::Trash)
                        .clicked()
                    {
                        confirm = true;
                    }
                    ui.add_space(8.0);
                    if widgets::ghost_button(ui, "Annuler", &pal, Icon::None).clicked() {
                        cancel = true;
                    }
                });
            });
        if modal.should_close() {
            cancel = true;
        }

        if confirm {
            self.lib.remove(&id);
            self.news.remove(&id);
            let _ = std::fs::remove_dir_all(config::artwork_root().join(&id));
            // Plus rien à télécharger pour ce jeu, et le job en cours est ignoré.
            self.job_queue.retain(|(queued, _)| queued != &id);
            if self.selected.as_deref() == Some(id.as_str()) {
                self.selected = self.lib.games.first().map(|g| g.id.clone());
            }
            self.confirm_remove = None;
            self.touch();
            self.toast_info("Jeu retiré de la bibliothèque");
        } else if cancel {
            self.confirm_remove = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Aides
// ---------------------------------------------------------------------------

fn section(ui: &mut egui::Ui, title: &str, pal: &Palette) {
    ui.add_space(12.0);
    ui.label(RichText::new(title).size(11.0).color(pal.text_faint));
    ui.add_space(2.0);
    ui.separator();
    ui.add_space(8.0);
}

fn info_line(ui: &mut egui::Ui, label: &str, value: &str, pal: &Palette) {
    ui.horizontal(|ui| {
        ui.add_sized(
            Vec2::new(120.0, 18.0),
            egui::Label::new(RichText::new(label).size(11.5).color(pal.text_dim)),
        );
        ui.label(RichText::new(value).size(11.5).color(pal.text));
    });
}

fn own_dir(path: &str) -> PathBuf {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return config::home_dir();
    }
    let path = PathBuf::from(config::expand_tilde(trimmed));
    if path.is_dir() {
        path
    } else if let Some(parent) = path.parent() {
        if parent.is_dir() {
            return parent.to_path_buf();
        }
        config::home_dir()
    } else {
        config::home_dir()
    }
}

/// Sélecteur de runtime Proton. Renvoie `true` si la valeur a changé.
fn runtime_combo(
    ui: &mut egui::Ui,
    id: &str,
    mode: &mut ProtonMode,
    runtimes: &[ProtonRuntime],
    allow_native: bool,
) -> bool {
    let before = mode.clone();
    let selected_text = match mode {
        ProtonMode::Default => "Automatique (recommandé)".to_string(),
        ProtonMode::Native => "Linux (natif)".to_string(),
        ProtonMode::Runtime(key) => runtimes
            .iter()
            .find(|r| &r.key == key)
            .map(|r| r.label())
            .unwrap_or_else(|| key.clone()),
    };

    egui::ComboBox::from_id_salt(id)
        .selected_text(selected_text)
        .width(360.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(mode, ProtonMode::Default, "Automatique (recommandé)");
            if allow_native {
                ui.selectable_value(mode, ProtonMode::Native, "Linux (natif)");
            }
            if !runtimes.is_empty() {
                ui.separator();
            }
            for runtime in runtimes {
                ui.selectable_value(
                    mode,
                    ProtonMode::Runtime(runtime.key.clone()),
                    runtime.label(),
                );
            }
        });

    *mode != before
}

/// Ligne de résultat de recherche (nom + miniature + source).
fn search_result_row(
    ui: &mut egui::Ui,
    pal: &Palette,
    hit: &crate::artwork::SearchHit,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, 4.0, pal.surface_hover);
    }

    let thumb_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 6.0, rect.center().y - 14.0),
        Vec2::new(52.0, 28.0),
    );
    match hit.thumb.clone() {
        Some(thumb) => {
            ui.put(
                thumb_rect,
                egui::Image::new(thumb).max_size(thumb_rect.size()).maintain_aspect_ratio(true),
            );
        }
        None => {
            widgets::monogram(
                &painter,
                Rect::from_min_size(Pos2::new(thumb_rect.left(), rect.center().y - 14.0), Vec2::splat(28.0)),
                &hit.title.chars().next().unwrap_or('?').to_uppercase().to_string(),
                pal,
            );
        }
    }

    painter.text(
        Pos2::new(rect.left() + 70.0, rect.center().y - 8.0),
        Align2::LEFT_CENTER,
        &hit.title,
        FontId::proportional(14.0),
        pal.text,
    );
    painter.text(
        Pos2::new(rect.left() + 70.0, rect.center().y + 9.0),
        Align2::LEFT_CENTER,
        &hit.detail,
        FontId::proportional(11.0),
        pal.text_faint,
    );
    painter.text(
        Pos2::new(rect.right() - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        hit.source.label(),
        FontId::proportional(11.0),
        pal.text_faint,
    );

    response
}

#[derive(Default)]
pub struct ScanAction {
    pub research: Option<usize>,
    pub back: bool,
    pub submit: bool,
}

/// Ligne de l'analyse d'un dossier : titre corrigeable, source reconnue et
/// verdict « c'est un jeu » / « ce n'est pas un jeu ».
fn scan_row(ui: &mut egui::Ui, pal: &Palette, candidate: &mut crate::app::ScanCandidate) -> bool {
    let mut research = false;
    egui::Frame::new()
        .fill(pal.surface)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut candidate.title)
                            .desired_width(330.0)
                            .font(FontId::proportional(13.5)),
                    );
                    ui.label(
                        RichText::new(candidate.exe.display().to_string())
                            .size(10.5)
                            .color(pal.text_faint),
                    );
                    let detail = if candidate.searching {
                        "recherche en cours…".to_string()
                    } else {
                        candidate.detail.clone()
                    };
                    ui.label(
                        RichText::new(detail)
                            .size(11.0)
                            .color(if candidate.hit.is_some() {
                                pal.text_dim
                            } else {
                                pal.text_faint
                            }),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::ghost_button(ui, "Rechercher", pal, Icon::Refresh).clicked() {
                        research = true;
                    }
                    ui.add_space(6.0);
                    let mut is_game = candidate.include;
                    if ui.selectable_label(is_game, "C'est un jeu").clicked() {
                        is_game = true;
                    }
                    if ui.selectable_label(!candidate.include, "Pas un jeu").clicked() {
                        is_game = false;
                    }
                    candidate.include = is_game;
                });
            });
        });
    ui.add_space(4.0);
    research
}

impl QuarkApp {
    /// Étape « dossier » : liste des jeux détectés avec leur correspondance.
    fn scan_step(&mut self, ui: &mut egui::Ui, pal: &Palette) -> ScanAction {
        let mut action = ScanAction::default();
        let folder = self
            .add
            .scan
            .folder
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Dossier").size(12.0).color(pal.text_dim));
            ui.add_space(6.0);
            ui.label(RichText::new(folder).size(12.0).color(pal.text));
        });
        ui.add_space(8.0);

        if self.add.scan.running {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Analyse du dossier et recherche des titres…")
                        .size(12.5)
                        .color(pal.text_dim),
                );
            });
        } else if let Some(error) = self.add.scan.error.clone() {
            ui.label(RichText::new(error).size(12.0).color(pal.danger));
        } else if self.add.scan.candidates.is_empty() {
            ui.label(
                RichText::new(
                    "Aucun exécutable de jeu trouvé ici. Choisis le dossier qui contient le jeu \
                     lui-même (pas la racine de ta bibliothèque).",
                )
                .size(12.0)
                .color(pal.text_dim),
            );
        } else {
            ui.label(
                RichText::new(
                    "Vérifie les titres proposés, corrige-les si besoin puis relance la recherche.",
                )
                .size(11.0)
                .color(pal.text_faint),
            );
            ui.add_space(8.0);
            let count = self.add.scan.candidates.len();
            egui::ScrollArea::vertical()
                .id_salt("quark_scan_results")
                .max_height(340.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for index in 0..count {
                        if let Some(candidate) = self.add.scan.candidates.get_mut(index) {
                            if scan_row(ui, pal, candidate) {
                                action.research = Some(index);
                            }
                        }
                    }
                });
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if widgets::ghost_button(ui, "← Autre nom", pal, Icon::None).clicked() {
                action.back = true;
            }
            ui.add_space(10.0);
            let selected = self.add.scan.candidates.iter().filter(|c| c.include).count();
            let ready = selected > 0 && !self.add.scan.running;
            let label = format!("Ajouter {selected} jeu(x)");
            if widgets::accent_button(ui, Vec2::new(230.0, 36.0), &label, pal, ready, Icon::Plus)
                .clicked()
                && ready
            {
                action.submit = true;
            }
        });

        action
    }
}
