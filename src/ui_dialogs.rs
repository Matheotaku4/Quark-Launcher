//! Fenêtres modales : ajout de jeu, paramètres globaux, paramètres du jeu et
//! confirmation de suppression. Les sélecteurs de fichiers sont ceux du système
//! (XDG Desktop Portal), ouverts via `QuarkApp::pick_file` / `pick_directory`.

use crate::app::{AddStep, PickerTarget, QuarkApp, SearchState, SettingsTab, UpdateState};
use crate::config::{self, AssetKind, EnvVar, ProtonMode, ThemeMode, ViewMode};
use crate::proton::{self, ProtonRuntime, RuntimeKind};
use crate::theme::{self, Palette};
use crate::update;
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
        let mut pick_artwork: Option<(AssetKind, usize)> = None;
        let mut finish_artwork = false;
        let mut scan_action = ScanAction::default();

        let title = match step {
            AddStep::Name => "Ajouter un jeu",
            AddStep::Artwork => "Ajouter un jeu · artwork",
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
                    AddStep::Artwork => {
                        ui.label(
                            RichText::new(
                                "Choisis les images téléchargées pour ce jeu. La première \
                                 proposition de chaque type est choisie par défaut.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );
                        ui.add_space(10.0);

                        let plan = self.add.artwork.plan.clone();
                        if self.add.artwork.loading {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new("Recherche des artworks\u{2026}")
                                        .size(12.5)
                                        .color(pal.text_dim),
                                );
                            });
                        } else if let Some(plan) = &plan {
                            egui::ScrollArea::vertical()
                                .id_salt("quark_artwork_choices")
                                .max_height(330.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for kind in AssetKind::ALL {
                                        let urls = plan.candidates(kind);
                                        ui.label(
                                            RichText::new(kind.label().to_uppercase())
                                                .size(11.0)
                                                .color(pal.text_faint),
                                        );
                                        ui.add_space(3.0);
                                        if urls.is_empty() {
                                            ui.label(
                                                RichText::new("aucune proposition")
                                                    .size(11.5)
                                                    .color(pal.text_faint),
                                            );
                                        } else {
                                            let selected = self
                                                .add
                                                .artwork
                                                .chosen
                                                .get(&kind)
                                                .copied()
                                                .unwrap_or(0);
                                            ui.horizontal_wrapped(|ui| {
                                                for (index, url) in urls.iter().enumerate() {
                                                    if artwork_choice(
                                                        ui,
                                                        &pal,
                                                        url,
                                                        index == selected,
                                                    )
                                                    .clicked()
                                                    {
                                                        pick_artwork = Some((kind, index));
                                                    }
                                                }
                                            });
                                        }
                                        ui.add_space(10.0);
                                    }
                                });
                        } else {
                            ui.label(
                                RichText::new("Aucun artwork proposé : tu peux continuer sans.")
                                    .size(12.0)
                                    .color(pal.text_dim),
                            );
                        }
                        if let Some(error) = self.add.artwork.error.clone() {
                            ui.add_space(4.0);
                            ui.label(RichText::new(error).size(11.0).color(pal.text_faint));
                        }

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if widgets::ghost_button(ui, "← Retour", &pal, Icon::None).clicked() {
                                back = true;
                            }
                            ui.add_space(10.0);
                            if widgets::accent_button(
                                ui,
                                Vec2::new(230.0, 36.0),
                                "Choisir l'exécutable",
                                &pal,
                                true,
                                Icon::Plus,
                            )
                            .clicked()
                            {
                                finish_artwork = true;
                            }
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
                            &pal,
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

        if let Some(index) = pick_hit {
            if let SearchState::Done(hits, _) = &self.add.state {
                if let Some(hit) = hits.get(index).cloned() {
                    self.add.name = hit.title.clone();
                    self.add.hit = Some(hit);
                }
            }
            self.add.error = None;
            // Un jeu reconnu a des artworks à proposer : on les montre avant
            // d'indiquer l'exécutable.
            self.start_artwork_plan(ctx);
        } else if continue_to_exe {
            // Continue sans choisir de résultat : pas d'artwork à proposer.
            self.add.hit = None;
            self.add.artwork = crate::app::ArtworkPicker::default();
            self.add.step = AddStep::Exe;
            self.add.error = None;
        } else if finish_artwork {
            self.add.step = AddStep::Exe;
        }

        if let Some((kind, index)) = pick_artwork {
            self.add.artwork.chosen.insert(kind, index);
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
            let chosen = self.add.artwork.plan.clone().map(|plan| {
                let choices: Vec<(AssetKind, usize)> = self
                    .add
                    .artwork
                    .chosen
                    .iter()
                    .map(|(kind, index)| (*kind, *index))
                    .collect();
                plan.with_choices(&choices)
            });
            match hit {
                // Les artworks choisis passent en premier, le reste sert de secours.
                Some(hit) => match chosen {
                    Some(plan) => self.start_chosen_job(ctx, &id, hit.title.clone(), plan),
                    None => self.start_asset_job(ctx, &id, hit),
                },
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
        let mut check_updates = false;
        let mut download_update = false;
        let mut open_url: Option<String> = None;

        let max_height = (ctx.screen_rect().height() * 0.66).max(300.0);
        let modal = egui::Modal::new(egui::Id::new("quark_settings")).show(ctx, |ui| {
            ui.set_min_width(720.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Paramètres").size(17.0).color(pal.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("enregistrés automatiquement")
                            .size(10.5)
                            .color(pal.text_faint),
                    );
                });
            });
            ui.add_space(12.0);

            // Une page par famille de réglages.
            ui.horizontal(|ui| {
                for (tab, label) in SettingsTab::ALL {
                    if widgets::tab(ui, label, self.settings_tab == tab, &pal).clicked() {
                        self.settings_tab = tab;
                    }
                }
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(2.0);

            egui::ScrollArea::vertical()
                .id_salt("quark_settings_scroll")
                .max_height(max_height)
                .auto_shrink([false, false])
                .show(ui, |ui| match self.settings_tab {
                    SettingsTab::Appearance => {
                        section(ui, "THÈME", &pal);
                        ui.horizontal(|ui| {
                            for mode in [ThemeMode::Amoled, ThemeMode::Graphite] {
                                if ui
                                    .radio_value(&mut self.lib.settings.theme, mode, mode.label())
                                    .changed()
                                {
                                    self.touch();
                                }
                            }
                        });
                        ui.label(
                            RichText::new(
                                "AMOLED est noir pur (#000000) ; Graphite adoucit les noirs sur \
                                 les écrans qui ne sont pas OLED.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );

                        section(ui, "BIBLIOTHÈQUE", &pal);
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
                    }
                    SettingsTab::Proton => {
                        section(ui, "PRÉFIXE", &pal);
                        ui.horizontal(|ui| {
                            let mut text =
                                self.lib.settings.default_prefix.clone().unwrap_or_default();
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
                                "Utilisé par tous les jeux qui n'ont pas leur propre préfixe. Le \
                                 dossier est utilisé tel quel : Quark n'y crée aucun sous-dossier.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );

                        section(ui, "RUNTIME", &pal);
                        let mut mode = self
                            .lib
                            .settings
                            .default_proton
                            .clone()
                            .map(ProtonMode::Runtime)
                            .unwrap_or(ProtonMode::Default);
                        if runtime_combo(
                            ui,
                            &pal,
                            "default_runtime",
                            &mut mode,
                            &self.runtimes,
                            false,
                        ) {
                            self.lib.settings.default_proton = match mode {
                                ProtonMode::Runtime(key) => Some(key),
                                _ => None,
                            };
                            self.touch();
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let protons = self
                                .runtimes
                                .iter()
                                .filter(|runtime| runtime.kind == RuntimeKind::Proton)
                                .count();
                            let anticheat = self.runtimes.len() - protons;
                            ui.label(
                                RichText::new(format!(
                                    "{protons} runtime(s) Proton · {anticheat} runtime(s) anti-triche"
                                ))
                                .size(11.0)
                                .color(pal.text_faint),
                            );
                            if widgets::ghost_button(ui, "Redétecter", &pal, Icon::Refresh).clicked() {
                                redetect = true;
                            }
                        });
                        ui.label(
                            RichText::new(format!(
                                "Anti-triche : {}",
                                proton::anticheat_summary(&self.runtimes)
                            ))
                            .size(11.0)
                            .color(pal.text_faint),
                        );
                        ui.label(
                            RichText::new(
                                "BattlEye et EasyAntiCheat sont deux options indépendantes, cochables \
                                 ensemble dans les paramètres du jeu : Quark les déclare à Proton via \
                                 PROTON_BATTLEYE_RUNTIME / PROTON_EAC_RUNTIME.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );
                        if self.runtimes.is_empty() {
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(
                                    "Aucun runtime trouvé. Installe Proton (Steam) ou dépose un \
                                     build dans ~/.steam/steam/compatibilitytools.d/.",
                                )
                                .size(11.0)
                                .color(pal.text_faint),
                            );
                        }
                    }
                    SettingsTab::Environment => {
                        section(ui, "VARIABLES GLOBALES", &pal);
                        ui.label(
                            RichText::new(
                                "Appliquées à tous les lancements. Un jeu qui définit la même \
                                 variable dans ses propres paramètres écrase celle-ci.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );
                        ui.add_space(6.0);
                        if env_editor(ui, &pal, &mut self.lib.settings.env) {
                            self.touch();
                        }
                    }
                    SettingsTab::Artworks => {
                        section(ui, "SOURCES", &pal);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Clé API SteamGridDB").size(12.5).color(pal.text_dim),
                            );
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
                            ui.label(
                                RichText::new("Langue / pays (recherche Steam)")
                                    .size(12.5)
                                    .color(pal.text_dim),
                            );
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
                                        ui.selectable_value(
                                            &mut language,
                                            option.to_string(),
                                            option,
                                        );
                                    }
                                });
                            if language != self.lib.settings.language {
                                self.lib.settings.language = language;
                                self.touch();
                            }
                            let mut country = self.lib.settings.country.clone();
                            let response = ui.add_sized(
                                Vec2::new(70.0, 24.0),
                                egui::TextEdit::singleline(&mut country),
                            );
                            if response.changed() {
                                self.lib.settings.country = country;
                                self.touch();
                            }
                        });
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(
                                "Pour changer les images d'un jeu : bouton « Personnaliser » sur sa \
                                 fiche, ou clic droit sur sa ligne.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );
                    }
                    SettingsTab::About => {
                        section(ui, "VERSION", &pal);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Quark {}", update::current_version()))
                                    .size(12.5)
                                    .color(pal.text),
                            );
                            ui.add_space(14.0);
                            match self.update.clone() {
                                UpdateState::Idle => {
                                    if widgets::ghost_button(
                                        ui,
                                        "Vérifier les mises à jour",
                                        &pal,
                                        Icon::Refresh,
                                    )
                                    .clicked()
                                    {
                                        check_updates = true;
                                    }
                                }
                                UpdateState::Checking => {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new("vérification\u{2026}")
                                            .size(12.0)
                                            .color(pal.text_dim),
                                    );
                                }
                                UpdateState::UpToDate => {
                                    ui.label(
                                        RichText::new("à jour").size(12.0).color(pal.text_dim),
                                    );
                                    if widgets::ghost_button(ui, "Revérifier", &pal, Icon::Refresh)
                                        .clicked()
                                    {
                                        check_updates = true;
                                    }
                                }
                                UpdateState::Available(release) => {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} disponible",
                                            release.tag_name
                                        ))
                                        .size(12.0)
                                        .color(pal.accent),
                                    );
                                    let size = release
                                        .archive()
                                        .map(|asset| asset.size as f64 / 1_048_576.0)
                                        .unwrap_or(0.0);
                                    let label = if size > 0.0 {
                                        format!("Télécharger ({size:.1} Mo)")
                                    } else {
                                        "Télécharger".to_string()
                                    };
                                    if widgets::ghost_button(ui, &label, &pal, Icon::Plus).clicked() {
                                        download_update = true;
                                    }
                                    if !release.html_url.is_empty()
                                        && widgets::ghost_button(
                                            ui,
                                            "Page de la release",
                                            &pal,
                                            Icon::None,
                                        )
                                        .clicked()
                                    {
                                        open_url = Some(release.html_url.clone());
                                    }
                                }
                                UpdateState::Downloading(release) => {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new(format!(
                                            "téléchargement de {}",
                                            release.tag_name
                                        ))
                                        .size(12.0)
                                        .color(pal.text_dim),
                                    );
                                }
                                UpdateState::Downloaded(path) => {
                                    ui.label(
                                        RichText::new(format!(
                                            "téléchargée : {}",
                                            path.display()
                                        ))
                                        .size(11.5)
                                        .color(pal.text_dim),
                                    );
                                }
                                UpdateState::Failed(error) => {
                                    ui.label(RichText::new(error).size(11.5).color(pal.danger));
                                    if widgets::ghost_button(ui, "Réessayer", &pal, Icon::Refresh)
                                        .clicked()
                                    {
                                        check_updates = true;
                                    }
                                }
                            }
                        });
                        ui.label(
                            RichText::new(
                                "L'archive est enregistrée dans ~/.local/share/quark/updates/ : \
                                 décompresse-la et remplace le binaire pour installer la \
                                 nouvelle version.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(
                                "Clavier : flèches pour changer de jeu, Entrée pour lancer, \
                                 Ctrl+F pour chercher, Échap pour vider la recherche.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );

                        section(ui, "DOSSIERS", &pal);
                        info_line(
                            ui,
                            "Configuration",
                            &config::config_path().display().to_string(),
                            &pal,
                        );
                        info_line(
                            ui,
                            "Artworks",
                            &config::artwork_root().display().to_string(),
                            &pal,
                        );
                        info_line(
                            ui,
                            "Préfixes",
                            &config::prefix_root().display().to_string(),
                            &pal,
                        );
                    }
                });

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                if widgets::accent_button(ui, Vec2::new(140.0, 34.0), "Fermer", &pal, true, Icon::None)
                    .clicked()
                {
                    close_requested = true;
                }
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
        if check_updates {
            self.check_updates(ctx);
        }
        if download_update {
            self.download_update(ctx);
        }
        if let Some(url) = open_url {
            if let Err(err) = crate::news::open_in_browser(&url) {
                self.toast_error(format!("Ouverture du lien impossible : {err}"));
            }
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
                runtime_combo(
                    ui,
                    &pal,
                    "game_runtime",
                    &mut game.proton,
                    &self.runtimes,
                    true,
                );
                ui.add_space(8.0);
                let default_prefix = self
                    .lib
                    .settings
                    .default_prefix
                    .clone()
                    .unwrap_or_else(|| format!("auto : {}", config::prefix_root().display()));
                ui.label(
                    RichText::new(format!(
                        "Préfixe par défaut : {default_prefix} (utilisé tel quel, sans sous-dossier)"
                    ))
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

                section(ui, "ANTI-TRICHE", &pal);
                // Deux cases indépendantes, comme dans Heroic : un jeu peut avoir
                // besoin des deux runtimes.
                let mut battleye = game.anticheat.battleye();
                let mut eac = game.anticheat.eac();
                let mut changed = false;
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut battleye, "BattlEye AntiCheat Runtime")
                        .changed()
                    {
                        changed = true;
                    }
                    ui.add_space(10.0);
                    if ui.checkbox(&mut eac, "EasyAntiCheat Runtime").changed() {
                        changed = true;
                    }
                });
                if changed {
                    game.anticheat = game.anticheat.with_battleye(battleye).with_eac(eac);
                }
                // Disponibilité réelle : on montre ce que Quark a trouvé (chemin
                // compris) au lieu d'inviter à installer ce qui l'est déjà.
                for (keyword, label, enabled) in [
                    ("battleye", "BattlEye", game.anticheat.battleye()),
                    ("easyanticheat", "EasyAntiCheat", game.anticheat.eac()),
                ] {
                    if !enabled {
                        continue;
                    }
                    match proton::anticheat_runtime(&self.runtimes, keyword) {
                        Some(runtime) => ui.label(
                            RichText::new(format!(
                                "{label} : trouvé — {} (variable {})",
                                runtime.dir().display(),
                                proton::anticheat_env_name(keyword)
                            ))
                            .size(11.0)
                            .color(pal.text_dim),
                        ),
                        None => ui.label(
                            RichText::new(format!(
                                "{label} : non détecté (dossiers vus : {})",
                                proton::anticheat_summary(&self.runtimes)
                            ))
                            .size(11.0)
                            .color(pal.danger),
                        ),
                    };
                }
                ui.label(
                    RichText::new(
                        "Cochés, ces dossiers sont déclarés à ton Proton par PROTON_BATTLEYE_RUNTIME / \
                         PROTON_EAC_RUNTIME : ils ne sont pas lancés directement (Steam ne les livre \
                         pas avec un script `proton`), donc cocher les deux est sans risque.",
                    )
                    .size(11.0)
                    .color(pal.text_faint),
                );

                section(ui, "VARIABLES D'ENVIRONNEMENT", &pal);
                ui.label(
                    RichText::new(
                        "Passées au jeu au lancement, après celles des réglages généraux : une \
                         variable du même nom ici écrase la variable globale.",
                    )
                    .size(11.0)
                    .color(pal.text_faint),
                );
                ui.add_space(6.0);
                env_editor(ui, &pal, &mut game.env);

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
            self.confirm_remove = vec![game.id.clone()];
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
    // Personnaliser un jeu (artworks)
    // -----------------------------------------------------------------------

    pub(crate) fn customize_dialog(&mut self, ctx: &egui::Context) {
        let Some((game_id, kind, loading, plan, error)) =
            self.customize.as_ref().map(|customize| {
                (
                    customize.game_id.clone(),
                    customize.kind,
                    customize.loading,
                    customize.plan.clone(),
                    customize.error.clone(),
                )
            })
        else {
            return;
        };
        let Some(game) = self.lib.get(&game_id).cloned() else {
            self.customize = None;
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.customize = None;
            return;
        }
        let pal = self.pal;
        let mut close = false;
        let mut open_settings = false;
        let mut propose: Option<AssetKind> = None;
        let mut browse: Option<AssetKind> = None;
        let mut clear: Option<AssetKind> = None;
        let mut pick: Option<(AssetKind, usize)> = None;

        // Miniatures actuelles, décodées une seule fois par image.
        let handles: Vec<(AssetKind, Option<egui::TextureHandle>)> = AssetKind::ALL
            .iter()
            .map(|asset| {
                let path = game.artwork.get(*asset).filter(|p| p.exists()).cloned();
                let texture = path.and_then(|path| self.textures.get(ctx, &path)).cloned();
                (*asset, texture)
            })
            .collect();

        // Le calcul de la couleur n'a lieu qu'ici (une fois par couverture).
        let accent = self.game_accent(&game);

        let max_height = (ctx.screen_rect().height() * 0.56).max(240.0);
        let modal = egui::Modal::new(egui::Id::new("quark_customize")).show(ctx, |ui| {
            ui.set_min_width(700.0);
            ui.label(
                RichText::new(format!("Personnaliser · {}", game.name))
                    .size(17.0)
                    .color(pal.text),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Choisis l'image de chaque emplacement : une proposition en ligne, un fichier \
                     de ton disque, ou aucune (rendu de secours).",
                )
                .size(11.0)
                .color(pal.text_faint),
            );
            ui.add_space(10.0);

            // Couleur de jeu : déduite de la couverture, rien à régler.
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                ui.painter_at(rect).rect_filled(rect, 3.0, accent);
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Couleur du jeu, déduite de la couverture")
                        .size(11.0)
                        .color(pal.text_faint),
                );
            });
            ui.add_space(10.0);

            egui::ScrollArea::vertical()
                .id_salt("quark_customize_scroll")
                .max_height(max_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (asset, texture) in &handles {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::new(120.0, 68.0), Sense::hover());
                            let painter = ui.painter_at(rect);
                            painter.rect_filled(rect, 4.0, pal.surface);
                            match texture {
                                Some(texture) => {
                                    crate::images::draw_cover(&painter, texture, rect);
                                }
                                None => {
                                    painter.text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        "aucune image",
                                        FontId::proportional(10.5),
                                        pal.text_faint,
                                    );
                                }
                            }
                            ui.add_space(12.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(asset.label().to_uppercase())
                                        .size(11.0)
                                        .color(pal.text_faint),
                                );
                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    if widgets::ghost_button(
                                        ui,
                                        "Proposer d'autres…",
                                        &pal,
                                        Icon::Refresh,
                                    )
                                    .clicked()
                                    {
                                        propose = Some(*asset);
                                    }
                                    if widgets::ghost_button(
                                        ui,
                                        "Choisir un fichier…",
                                        &pal,
                                        Icon::Folder,
                                    )
                                    .clicked()
                                    {
                                        browse = Some(*asset);
                                    }
                                    if texture.is_some()
                                        && widgets::ghost_button(ui, "Retirer", &pal, Icon::Trash)
                                            .clicked()
                                    {
                                        clear = Some(*asset);
                                    }
                                });
                            });
                        });
                        ui.add_space(6.0);

                        // Propositions du type en cours de modification.
                        if kind == Some(*asset) {
                            if loading {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new("Recherche des propositions\u{2026}")
                                            .size(12.0)
                                            .color(pal.text_dim),
                                    );
                                });
                            } else if let Some(plan) = &plan {
                                let urls = plan.candidates(*asset);
                                if urls.is_empty() {
                                    ui.label(
                                        RichText::new("aucune proposition trouvée")
                                            .size(11.5)
                                            .color(pal.text_faint),
                                    );
                                } else {
                                    ui.horizontal_wrapped(|ui| {
                                        for (index, url) in urls.iter().enumerate() {
                                            if artwork_choice(ui, &pal, url, false).clicked() {
                                                pick = Some((*asset, index));
                                            }
                                        }
                                    });
                                }
                            }
                        }
                        ui.add_space(8.0);
                    }

                    if let Some(message) = &error {
                        ui.label(RichText::new(message).size(11.5).color(pal.danger));
                    }
                });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if widgets::accent_button(ui, Vec2::new(140.0, 34.0), "Fermer", &pal, true, Icon::None)
                    .clicked()
                {
                    close = true;
                }
                ui.add_space(8.0);
                if widgets::ghost_button(ui, "Réglages du jeu…", &pal, Icon::Gear).clicked() {
                    open_settings = true;
                }
            });
        });
        if modal.should_close() {
            close = true;
        }

        if let Some(asset) = propose {
            self.start_customize_plan(ctx, asset);
        }
        if let Some(asset) = browse {
            let start = game
                .artwork
                .get(asset)
                .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
                .unwrap_or_else(config::home_dir);
            self.pick_file_for_artwork(ctx, start, asset);
        }
        if let Some(asset) = clear {
            self.remove_artwork(&game_id, asset);
        }
        if let Some((asset, index)) = pick {
            self.apply_customize_pick(ctx, asset, index);
        }
        if open_settings {
            self.customize = None;
            self.editing = Some(game.clone());
        }
        if close {
            self.customize = None;
        }
    }

    // -----------------------------------------------------------------------
    // Confirmation de suppression
    // -----------------------------------------------------------------------

    pub(crate) fn confirm_remove_dialog(&mut self, ctx: &egui::Context) {
        if self.confirm_remove.is_empty() {
            return;
        }
        let pal = self.pal;
        let ids = self.confirm_remove.clone();
        let names: Vec<String> = ids
            .iter()
            .filter_map(|id| self.lib.get(id).map(|g| g.name.clone()))
            .collect();
        let title = match names.len() {
            0 => "Retirer ce jeu de la bibliothèque ?".to_string(),
            1 => format!("Retirer « {} » de la bibliothèque ?", names[0]),
            count => format!("Retirer {count} jeux de la bibliothèque ?"),
        };
        let mut cancel = false;
        let mut confirm = false;

        let modal = egui::Modal::new(egui::Id::new("quark_confirm_remove")).show(ctx, |ui| {
                ui.set_min_width(520.0);
                ui.label(RichText::new(title).size(14.0));
                if names.len() > 1 {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(names.join("  \u{b7}  "))
                            .size(12.0)
                            .color(pal.text_dim),
                    );
                }
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
            for id in &ids {
                self.lib.remove(id);
                self.news.remove(id);
                let _ = std::fs::remove_dir_all(config::artwork_root().join(id));
                // Plus rien à télécharger pour ce jeu, et le job en cours est ignoré.
                self.job_queue.retain(|(queued, _)| queued != id);
            }
            self.marked.clear();
            let selected_gone = self
                .selected
                .as_ref()
                .map(|selected| ids.iter().any(|id| id == selected))
                .unwrap_or(false);
            if selected_gone {
                self.selected = self.lib.games.first().map(|g| g.id.clone());
            }
            let count = ids.len();
            self.confirm_remove.clear();
            self.touch();
            self.toast_info(if count > 1 {
                format!("{count} jeux retirés de la bibliothèque")
            } else {
                "Jeu retiré de la bibliothèque".to_string()
            });
        } else if cancel {
            self.confirm_remove.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// Aides
// ---------------------------------------------------------------------------

/// Éditeur de variables d'environnement : une ligne « NOM=valeur » par
/// variable, avec ajout et retrait en place. Renvoie `true` si quelque chose a
/// changé (pour déclencher la sauvegarde).
fn env_editor(ui: &mut egui::Ui, pal: &Palette, vars: &mut Vec<EnvVar>) -> bool {
    let mut changed = false;
    let mut remove: Option<usize> = None;
    let mut add = false;

    if vars.is_empty() {
        ui.label(
            RichText::new("aucune variable")
                .size(11.5)
                .color(pal.text_faint),
        );
        ui.add_space(4.0);
    }

    for index in 0..vars.len() {
        ui.horizontal(|ui| {
            let width = ui.available_width();
            let name_width = ((width - 70.0) * 0.40).max(110.0);
            let name = ui.add_sized(
                Vec2::new(name_width, 24.0),
                egui::TextEdit::singleline(&mut vars[index].name).hint_text("NOM"),
            );
            if name.changed() {
                changed = true;
            }
            // « NOM=valeur » collé d'un bloc : la séparation se fait quand le
            // champ est validé, jamais pendant la frappe.
            if name.lost_focus() && vars[index].value.trim().is_empty() {
                if let Some(var) = EnvVar::parse(&vars[index].name.clone()) {
                    vars[index] = var;
                    changed = true;
                }
            }
            ui.label(RichText::new("=").size(12.0).color(pal.text_faint));
            let value_width = (width - name_width - 70.0).max(90.0);
            let value = ui.add_sized(
                Vec2::new(value_width, 24.0),
                egui::TextEdit::singleline(&mut vars[index].value).hint_text("valeur"),
            );
            if value.changed() {
                changed = true;
            }
            if widgets::icon_button(ui, 24.0, Icon::Trash, pal, "Retirer cette variable").clicked() {
                remove = Some(index);
            }
        });
        ui.add_space(2.0);
    }

    ui.horizontal(|ui| {
        if widgets::ghost_button(ui, "Ajouter une variable", pal, Icon::Plus).clicked() {
            add = true;
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new("format NOM=valeur, ex. WINEDLLOVERRIDES=winmm=n,b;steam_api64")
                .size(11.0)
                .color(pal.text_faint),
        );
    });

    if let Some(index) = remove {
        vars.remove(index);
        changed = true;
    }
    if add {
        vars.push(EnvVar::default());
        changed = true;
    }
    changed
}

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

/// Sélecteur de runtime Proton ; les runtimes anti-triche sont regroupés à part.
/// Renvoie `true` si la valeur a changé.
fn runtime_combo(
    ui: &mut egui::Ui,
    pal: &Palette,
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
            .find(|runtime| &runtime.key == key)
            .map(|runtime| runtime.label())
            .unwrap_or_else(|| key.clone()),
    };

    let protons: Vec<&ProtonRuntime> = runtimes
        .iter()
        .filter(|runtime| runtime.kind == RuntimeKind::Proton)
        .collect();
    // Un runtime anti-triche sans script `proton` ne peut pas être lancé : il
    // s'ajoute à un Proton par variable d'environnement (options du jeu).
    let anticheat: Vec<&ProtonRuntime> = runtimes
        .iter()
        .filter(|runtime| runtime.kind == RuntimeKind::AntiCheat && runtime.has_proton_script)
        .collect();

    egui::ComboBox::from_id_salt(id)
        .selected_text(selected_text)
        .width(380.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(mode, ProtonMode::Default, "Automatique (recommandé)");
            if allow_native {
                ui.selectable_value(mode, ProtonMode::Native, "Linux (natif)");
            }
            if !protons.is_empty() {
                ui.separator();
            }
            for runtime in &protons {
                ui.selectable_value(
                    mode,
                    ProtonMode::Runtime(runtime.key.clone()),
                    runtime.label(),
                );
            }
            if !anticheat.is_empty() {
                ui.separator();
                ui.label(
                    RichText::new("ANTI-TRICHE (BattlEye / EasyAntiCheat)")
                        .size(10.0)
                        .color(pal.text_faint),
                );
                for runtime in &anticheat {
                    ui.selectable_value(
                        mode,
                        ProtonMode::Runtime(runtime.key.clone()),
                        runtime.label(),
                    );
                }
            }
        });

    *mode != before
}

/// Proposition d'artwork : miniature distante, sélection au clic.
fn artwork_choice(ui: &mut egui::Ui, pal: &Palette, url: &str, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(132.0, 76.0), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, pal.surface);
    ui.put(
        rect.shrink(3.0),
        egui::Image::new(url)
            .max_size(rect.size() - Vec2::splat(6.0))
            .maintain_aspect_ratio(true),
    );
    if selected || response.hovered() {
        widgets::border_rect(
            &painter,
            rect,
            if selected { 2.0 } else { 1.0 },
            if selected { pal.accent } else { pal.border },
        );
    }
    if selected {
        painter.circle_filled(Pos2::new(rect.right() - 13.0, rect.top() + 13.0), 7.0, pal.accent);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
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
                &config::monogram_letter(&hit.title),
                theme::monogram_color(&hit.title),
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
