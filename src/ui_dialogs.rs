//! Fenêtres modales : ajout de jeu, paramètres globaux, paramètres du jeu et
//! confirmation de suppression. Les sélecteurs de fichiers sont ceux du système
//! (XDG Desktop Portal), ouverts via `QuarkApp::pick_file` / `pick_directory`.

use crate::app::{AddStep, PickerTarget, QuarkApp, SearchState, SettingsTab, UpdateState};
use crate::config::{self, AssetKind, EnvVar, GameStatus, ProtonMode, ThemeMode, ViewMode};
use crate::disk;
use crate::proton::{self, ProtonRuntime, RuntimeKind};
use crate::stats;
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

        let modal = egui::Modal::new(egui::Id::new("quark_add_game"))
            .frame(theme::modal_frame(&pal))
            .show(ctx, |ui| {
                ui.set_min_width(660.0);
                ui.label(RichText::new(title).size(17.0).color(pal.text));
                ui.add_space(10.0);

                match step {
                    AddStep::Name => {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Nom du jeu").size(12.0).color(pal.text_dim));
                            widgets::info_button(
                                ui,
                                &pal,
                                "Quark cherche le jeu sur Steam puis sur SteamGridDB pour \
                                 récupérer les artworks originaux (bannière, logo, couverture, \
                                 icône). La recherche part dès 2 lettres.",
                            );
                        });
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
                        ui.add_space(12.0);

                        let mut warnings: Vec<String> = Vec::new();
                        match &self.add.state {
                            SearchState::Idle => {
                                ui.label(
                                    RichText::new(
                                        "Tape au moins 2 lettres, la recherche est automatique.",
                                    )
                                    .size(11.5)
                                    .color(pal.text_faint),
                                );
                            }
                            SearchState::Loading => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new("Recherche en cours…")
                                            .size(12.5)
                                            .color(pal.text_dim),
                                    );
                                });
                            }
                            SearchState::Done(hits, warns) => {
                                warnings = warns.clone();
                                if hits.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "Aucun résultat. Tu peux continuer sans artwork.",
                                        )
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
                            widgets::info_button(
                                ui,
                                &pal,
                                "Analyse un dossier, propose les jeux qu'il contient et te laisse \
                                 confirmer chaque titre (ou dire que ce n'est pas un jeu).",
                            );
                        });
                    }
                    AddStep::Artwork => {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Images du jeu")
                                    .size(12.0)
                                    .color(pal.text_dim),
                            );
                            widgets::info_button(
                                ui,
                                &pal,
                                "La première proposition de chaque type est choisie par défaut : \
                                 clique une vignette pour en prendre une autre.",
                            );
                        });
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

                        ui.label(
                            RichText::new("Exécutable du jeu")
                                .size(12.0)
                                .color(pal.text_dim),
                        );
                        ui.horizontal(|ui| {
                            let field = (ui.available_width() - 130.0).max(200.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.add.exe)
                                    .hint_text("/chemin/vers/jeu.exe ou jeu.sh")
                                    .desired_width(field),
                            );
                            if widgets::ghost_button(ui, "Parcourir…", &pal, Icon::Folder).clicked()
                            {
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

                        ui.label(
                            RichText::new("Arguments (optionnel)")
                                .size(12.0)
                                .color(pal.text_dim),
                        );
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

                        ui.checkbox(
                            &mut self.add.custom_prefix,
                            "Préfixe Proton personnalisé pour ce jeu",
                        );
                        if self.add.custom_prefix {
                            ui.horizontal(|ui| {
                                let field = (ui.available_width() - 130.0).max(200.0);
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.add.prefix)
                                        .hint_text("~/Games/prefixes/mon-jeu")
                                        .desired_width(field),
                                );
                                if widgets::ghost_button(ui, "Parcourir…", &pal, Icon::Folder)
                                    .clicked()
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
            // Réglages « par défaut » : recopiés dans chaque nouveau jeu.
            game.anticheat = self.lib.settings.default_anticheat;
            game.launch = self.lib.settings.default_launch.clone();
            game.proton_log = self.lib.settings.default_proton_log;
            game.umu = self.lib.settings.default_umu;
            game.env = self.lib.settings.default_env.clone();
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
        let mut open_steam_import = false;
        let mut save_steam_key = false;
        let mut download_update = false;
        let mut open_url: Option<String> = None;

        // La page défile dans l'espace qui reste dans la fenêtre, une fois
        // l'en-tête, les onglets et le bouton Fermer retirés (~250 px). Plus
        // grand, le pied de page débordait et recouvrait le contenu ; une
        // hauteur calculée sur la place restante ne marchait pas non plus, la
        // fenêtre se dimensionnant elle-même sur son contenu.
        let page_height = (ctx.screen_rect().height() - 250.0).max(200.0);
        let modal = egui::Modal::new(egui::Id::new("quark_settings"))
            .frame(theme::modal_frame(&pal))
            .show(ctx, |ui| {
            ui.set_min_width(860.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Paramètres").size(21.0).strong().color(pal.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("enregistrés automatiquement")
                            .size(10.5)
                            .color(pal.text_faint),
                    );
                });
            });
            ui.add_space(12.0);

            // Une page par famille de réglages (les onglets passent à la ligne
            // sur une fenêtre étroite).
            egui::Frame::new()
                .fill(pal.surface)
                .corner_radius(theme::radius::PILL)
                .inner_margin(egui::Margin::symmetric(4, 3))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.horizontal_wrapped(|ui| {
                        for (tab, label) in SettingsTab::ALL {
                            if widgets::tab(ui, label, self.settings_tab == tab, &pal).clicked() {
                                self.settings_tab = tab;
                            }
                        }
                    });
                });
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .id_salt("quark_settings_scroll")
                .max_height(page_height)
                // Se rétrécit quand la page est courte, mais grandit jusqu'à
                // `max_height` quand les variables s'accumulent.
                .auto_shrink([false, true])
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
                            widgets::info_button(
                                ui,
                                &pal,
                                "AMOLED est noir pur (#000000) ; Graphite adoucit les noirs sur \
                                 les écrans qui ne sont pas OLED.",
                            );
                        });

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
                            widgets::info_button(
                                ui,
                                &pal,
                                "La vue se bascule aussi depuis la barre du haut (icônes liste / \
                                 grille).",
                            );
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if widgets::ghost_button(
                                ui,
                                "Importer la bibliothèque Steam installée…",
                                &pal,
                                Icon::Steam,
                            )
                            .clicked()
                            {
                                open_steam_import = true;
                            }
                            widgets::info_button(
                                ui,
                                &pal,
                                "Lit les fichiers appmanifest_*.acf de toutes tes bibliothèques \
                                 Steam (dossier historique, autres disques, Flatpak et Snap), \
                                 propose les jeux installés et ne duplique pas ceux déjà dans \
                                 Quark. L'exécutable est cherché dans le dossier de chaque jeu.",
                            );
                        });

                        section(ui, "ACCUEIL", &pal);
                        ui.horizontal(|ui| {
                            if ui
                                .checkbox(
                                    &mut self.lib.settings.carousel_auto,
                                    "Faire défiler le carrousel tout seul",
                                )
                                .changed()
                            {
                                self.touch();
                            }
                            widgets::info_button(
                                ui,
                                &pal,
                                "Le carrousel de l'accueil passe d'un jeu récent au suivant \
                                 après le délai ci-dessous. Un clic sur une flèche repart de zéro.",
                            );
                        });
                        if self.lib.settings.carousel_auto {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Changer toutes les")
                                        .size(12.0)
                                        .color(pal.text_dim),
                                );
                                let mut secs = self.lib.settings.carousel_secs;
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut secs)
                                            .range(3.0..=60.0)
                                            .speed(0.5)
                                            .suffix(" s"),
                                    )
                                    .changed()
                                {
                                    self.lib.settings.carousel_secs = secs;
                                    self.touch();
                                }
                            });
                        }

                        section(ui, "ZONE DE NOTIFICATION", &pal);
                        let mut start_tray: Option<bool> = None;
                        ui.horizontal(|ui| {
                            let mut tray_icon = self.lib.settings.tray_icon;
                            if ui
                                .checkbox(
                                    &mut tray_icon,
                                    "Garder Quark dans la zone de notification",
                                )
                                .changed()
                            {
                                start_tray = Some(tray_icon);
                            }
                            widgets::info_button(
                                ui,
                                &pal,
                                "Coché, fermer la fenêtre quitte Quark mais laisse son icône dans \
                                 la zone de notification : un clic dessus relance le launcher, et \
                                 « Quitter Quark » dans son menu ferme tout. Décoché, la croix \
                                 quitte complètement. L'icône vit dans un petit processus dédié, \
                                 donc la fenêtre disparaît vraiment (rien dans la barre des \
                                 tâches), même sous Wayland.",
                            );
                        });
                        if let Some(enabled) = start_tray {
                            self.set_tray_icon(enabled);
                        }
                        if self.lib.settings.tray_icon {
                            match (&crate::tray::helper_pid(), &self.tray_error) {
                                (Some(_), _) => {
                                    ui.label(
                                        RichText::new(
                                            "Icône active : la croix ferme Quark, l'icône reste \
                                             et un clic la relance.",
                                        )
                                        .size(11.0)
                                        .color(pal.text_faint),
                                    );
                                }
                                (None, Some(err)) => {
                                    // Message court dans la page, erreur D-Bus complète
                                    // dans le ⓘ : la ligne brute débordait de la fenêtre.
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(
                                                "Icône indisponible : ce bureau n'a pas de zone \
                                                 de notification.",
                                            )
                                            .size(11.0)
                                            .color(pal.danger),
                                        );
                                        widgets::info_button(ui, &pal, err);
                                    });
                                    ui.label(
                                        RichText::new(
                                            "Le bureau doit fournir un watcher StatusNotifierItem \
                                             (extension AppIndicator sous GNOME). Décoché, la \
                                             croix quitte Quark.",
                                        )
                                        .size(10.5)
                                        .color(pal.text_faint),
                                    );
                                }
                                (None, None) => {}
                            }
                        }
                    }
                    SettingsTab::Defaults => {
                        section_hint(
                            ui,
                            "PAR DÉFAUT",
                            &pal,
                            "Ce ne sont pas des réglages globaux : ces valeurs sont recopiées \
                             dans un jeu au moment où tu l'ajoutes, puis chaque jeu garde les \
                             siennes. Changer un défaut ne modifie donc aucun jeu déjà dans la \
                             bibliothèque, et modifier un jeu ne touche pas les autres.",
                        );

                        ui.label(RichText::new("Préfixe Proton").size(12.5).color(pal.text_dim));
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
                            widgets::info_button(
                                ui,
                                &pal,
                                "Le dossier choisi est utilisé tel quel : Quark n'y crée aucun \
                                 sous-dossier. Vide = un préfixe par jeu sous \
                                 ~/.local/share/quark/prefixes/.",
                            );
                        });

                        section(ui, "ARGUMENTS", &pal);
                        ui.horizontal(|ui| {
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.lib.settings.default_args)
                                    .hint_text("ex. -windowed -novid")
                                    .desired_width((ui.available_width() - 50.0).max(300.0)),
                            );
                            if response.changed() {
                                self.touch();
                            }
                            widgets::info_button(
                                ui,
                                &pal,
                                "Passés après l'exécutable, comme dans les options de lancement \
                                 Steam. Les guillemets sont respectés : --chemin \"un dossier\".",
                            );
                        });

                        section(ui, "RUNTIME PROTON", &pal);
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
                        // Les runtimes détectés et le compte d'anti-triche
                        // vivaient dans leur propre onglet « Proton », qui ne
                        // servait qu'à ça : ils sont ici, à côté du choix.
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
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Anti-triche : {}",
                                    proton::anticheat_summary(&self.runtimes)
                                ))
                                .size(11.0)
                                .color(pal.text_faint),
                            );
                            widgets::info_button(
                                ui,
                                &pal,
                                "BattlEye et EasyAntiCheat sont deux options indépendantes, \
                                 cochables ensemble dans les paramètres du jeu : Quark déclare les \
                                 dossiers trouvés à ton Proton via PROTON_BATTLEYE_RUNTIME et \
                                 PROTON_EAC_RUNTIME. Ces dossiers ne se lancent pas eux-mêmes.",
                            );
                        });
                        if self.runtimes.is_empty() {
                            ui.label(
                                RichText::new(
                                    "Aucun runtime trouvé. Installe Proton (Steam) ou dépose un \
                                     build dans ~/.steam/steam/compatibilitytools.d/.",
                                )
                                .size(11.0)
                                .color(pal.text_faint),
                            );
                        }

                        section_hint(
                            ui,
                            "ANTI-TRICHE",
                            &pal,
                            "Deux options indépendantes, cochables ensemble. Les dossiers trouvés \
                             dans Steam sont déclarés à ton Proton par PROTON_BATTLEYE_RUNTIME et \
                             PROTON_EAC_RUNTIME.",
                        );
                        let mut battleye = self.lib.settings.default_anticheat.battleye();
                        let mut eac = self.lib.settings.default_anticheat.eac();
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
                            self.lib.settings.default_anticheat = self
                                .lib
                                .settings
                                .default_anticheat
                                .with_battleye(battleye)
                                .with_eac(eac);
                            self.touch();
                        }

                        section_hint(
                            ui,
                            "OPTIONS DE LANCEMENT",
                            &pal,
                            "Recopiées dans chaque jeu ajouté, puis chaque jeu garde les siennes. \
                             Un outil absent est ignoré au lancement (le jeu démarre quand même) \
                             et signalé dans les paramètres du jeu.",
                        );
                        if launch_options_editor(
                            ui,
                            &pal,
                            &mut self.lib.settings.default_launch,
                            &mut self.lib.settings.default_proton_log,
                            &mut self.lib.settings.default_umu,
                            None,
                            &self.tools,
                        ) {
                            self.touch();
                        }
                        section_hint(
                            ui,
                            "VARIABLES D'ENVIRONNEMENT",
                            &pal,
                            "Format NOM=valeur, par exemple \
                             WINEDLLOVERRIDES=winmm=n,b;steam_api64. Chaque jeu ajouté en reçoit \
                             une copie qu'il peut modifier sans toucher aux autres.",
                        );
                        if env_editor(ui, &pal, &mut self.lib.settings.default_env) {
                            self.touch();
                        }
                    }

                    SettingsTab::Artworks => {
                        section(ui, "SOURCES", &pal);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Clé API SteamGridDB").size(12.5).color(pal.text_dim),
                            );
                            widgets::info_button(
                                ui,
                                &pal,
                                "Sans clé, Quark se limite aux artworks officiels du CDN Steam. \
                                 Clé gratuite : steamgriddb.com/profile/preferences/api",
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
                    }
                    SettingsTab::Stats => self.stats_page(ui, &pal),
                    SettingsTab::Disk => self.disk_page(ui, &pal, ctx),
                    SettingsTab::Providers => {
                        section(ui, "CONFIDENTIALITÉ DES COMPTES", &pal);
                        ui.label(
                            RichText::new(
                                "Les identifiants restent sur cet ordinateur dans un fichier séparé, protégé par les permissions de ton compte. Ils ne sont pas chiffrés et Quark ne les envoie jamais à un serveur Quark.",
                            )
                            .size(12.0)
                            .color(pal.text_dim),
                        );
                        ui.add_space(8.0);
                        if let Some(error) = &self.credentials_error {
                            ui.label(RichText::new(error).size(11.5).color(pal.danger));
                        }
                        ui.label(
                            RichText::new(if self.credentials.steam_web_api_key.is_some() {
                                "Steam Web API : clé enregistrée (masquée)"
                            } else {
                                "Steam Web API : clé facultative non configurée"
                            })
                            .size(12.0)
                            .color(pal.text),
                        );
                        ui.horizontal_wrapped(|ui| {
                            ui.add_sized(
                                Vec2::new(330.0, 28.0),
                                egui::TextEdit::singleline(&mut self.steam_key_draft)
                                    .password(true)
                                    .hint_text("Clé Web API Steam personnelle · laisser vide pour garder l'actuelle"),
                            );
                            ui.add_sized(
                                Vec2::new(220.0, 28.0),
                                egui::TextEdit::singleline(&mut self.steam_id_draft)
                                    .password(true)
                                    .hint_text(if self.credentials.steam_id.is_some() { "SteamID64 enregistré · remplacement facultatif" } else { "SteamID64 (17 chiffres)" }),
                            );
                            if widgets::ghost_button(ui, "Enregistrer", &pal, Icon::Info).clicked() {
                                save_steam_key = true;
                            }
                            widgets::info_button(
                                ui,
                                &pal,
                                "La clé personnelle et le SteamID64 sont enregistrés dans credentials.json avec permissions privées. GetOwnedGames peut rester inaccessible si les détails Steam sont privés. Une réponse vide est signalée comme ambiguë, jamais interprétée comme une bibliothèque vérifiée vide.",
                            );
                        });
                        ui.add_space(10.0);
                        section(ui, "BOUTIQUES", &pal);
                        self.provider_accounts(ui, ctx, &pal);
                        ui.label(
                            RichText::new("Heroic sera détecté et sa configuration ne sera lue qu'après ton accord explicite.")
                                .size(11.5)
                                .color(pal.text_faint),
                        );

                        ui.add_space(10.0);
                        section(ui, "OUTILS GÉRÉS PAR QUARK", &pal);
                        for tool in crate::providers::tools::ManagedToolId::ALL {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(tool.label()).size(12.0).color(pal.text));
                                if let Some(state) = &self.managed_tools[tool.index()] {
                                    ui.label(RichText::new(state).size(11.0).color(pal.text_dim));
                                } else if tool.installed_path().is_file() {
                                    ui.label(RichText::new("Binaire présent · version non vérifiée à cette session").size(10.5).color(pal.text_faint));
                                } else {
                                    ui.label(RichText::new("Non installé").size(10.5).color(pal.text_faint));
                                }
                                if widgets::ghost_button(ui, "Installer / actualiser", &pal, Icon::Refresh).clicked() {
                                    self.start_managed_tool(ctx, tool);
                                }
                            });
                        }
                        ui.label(
                            RichText::new("Téléchargements depuis les releases officielles Linux x86_64 ; le digest SHA-256 GitHub est vérifié avant exécution (il ne s'agit pas d'une signature indépendante). Les licences et avis de distribution seront inclus dans le paquet.")
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
                                 Ctrl+F pour chercher, Échap pour vider la recherche, \
                                 Ctrl+Q pour quitter complètement.",
                            )
                            .size(11.0)
                            .color(pal.text_faint),
                        );

                        section(ui, "RESSOURCES", &pal);
                        ui.horizontal(|ui| {
                            let memory = config::resident_mb()
                                .map(|mb| format!("{mb} Mo"))
                                .unwrap_or_else(|| "—".to_string());
                            ui.label(
                                RichText::new(format!("Mémoire résidente : {memory}"))
                                    .size(11.5)
                                    .color(pal.text_dim),
                            );
                            ui.add_space(14.0);
                            ui.label(
                                RichText::new(format!(
                                    "{} texture(s) en cache",
                                    self.textures.len()
                                ))
                                .size(11.0)
                                .color(pal.text_faint),
                            );
                            widgets::info_button(
                                ui,
                                &pal,
                                "Les artworks sont décodés en textures : une bannière occupe \
                                 plusieurs mégaoctets. Au-delà de 96 textures, les plus \
                                 anciennes sont libérées, si bien que parcourir la bibliothèque \
                                 ne fait plus grimper la mémoire indéfiniment.",
                            );
                        });

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
            self.toast_info(format!(
                "{} runtime(s) Proton détecté(s)",
                self.runtimes.len()
            ));
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
        if open_steam_import {
            self.open_steam_import(ctx);
        }
        if save_steam_key {
            self.save_steam_api_key();
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
    // Statistiques de jeu
    // -----------------------------------------------------------------------

    /// Résumé, heures par semaine, calendrier de chaleur et jeux les plus joués.
    fn stats_page(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let sessions: Vec<config::PlaySession> = self
            .lib
            .games
            .iter()
            .flat_map(|game| game.sessions.clone())
            .collect();
        let total: u64 = self.lib.games.iter().map(|game| game.playtime_secs).sum();
        let week_start = stats::week_start(stats::day_of(config::now_ts()));
        let this_week: u64 = sessions
            .iter()
            .filter(|session| stats::day_of(session.start) >= week_start)
            .map(|session| session.secs)
            .sum();
        let streak = stats::streak(&sessions);

        section(ui, "RÉSUMÉ", pal);
        ui.horizontal(|ui| {
            metric(
                ui,
                pal,
                "Temps de jeu total",
                &config::human_playtime(total),
            );
            ui.add_space(22.0);
            metric(ui, pal, "Cette semaine", &config::human_playtime(this_week));
            ui.add_space(22.0);
            metric(
                ui,
                pal,
                "Série en cours",
                &format!("{streak} jour{}", if streak > 1 { "s" } else { "" }),
            );
            ui.add_space(22.0);
            metric(ui, pal, "Sessions", &format!("{}", sessions.len()));
        });

        if sessions.is_empty() {
            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    "Aucune session enregistrée : lance un jeu, ses heures apparaîtront ici.",
                )
                .size(12.0)
                .color(pal.text_faint),
            );
            return;
        }

        section_hint(
            ui,
            "HEURES PAR SEMAINE",
            pal,
            "Les douze dernières semaines, du lundi au dimanche. La hauteur de chaque barre \
             donne les heures jouées.",
        );
        week_chart(ui, pal, &sessions);

        section_hint(
            ui,
            "CALENDRIER",
            pal,
            "Un carré par jour sur les 26 dernières semaines : plus il est clair, plus tu as \
             joué ce jour-là. Passe la souris sur un carré pour la durée exacte.",
        );
        heatmap_chart(ui, pal, &sessions);

        section(ui, "JEUX LES PLUS JOUÉS", pal);
        let top = stats::top_games(&self.lib, 6);
        let max = top.iter().map(|(_, secs)| *secs).max().unwrap_or(1).max(1);
        for (name, secs) in top {
            game_bar(ui, pal, &name, secs, max);
        }
    }

    // -----------------------------------------------------------------------
    // Espace disque
    // -----------------------------------------------------------------------

    fn disk_page(&mut self, ui: &mut egui::Ui, pal: &Palette, ctx: &egui::Context) {
        section_hint(
            ui,
            "ESPACE DISQUE",
            pal,
            "Mesure les préfixes Proton, le cache d'artworks, les journaux, les archives de mise \
             à jour et le dossier de chaque jeu. La mesure tourne en arrière-plan : la fenêtre \
             ne se fige jamais, même sur un gros préfixe.",
        );
        let mut recalc = false;
        ui.horizontal(|ui| {
            if widgets::ghost_button(ui, "Recalculer", pal, Icon::Refresh).clicked() {
                recalc = true;
            }
            if let Some(report) = &self.disk_report {
                ui.add_space(14.0);
                ui.label(
                    RichText::new(format!(
                        "Total mesuré : {}",
                        disk::format_bytes(report.total)
                    ))
                    .size(13.0)
                    .strong()
                    .color(pal.text),
                );
            }
        });
        if recalc || (self.disk_report.is_none() && self.disk_job.is_none()) {
            self.start_disk_scan(ctx);
        }
        ui.add_space(8.0);
        if self.disk_job.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Mesure en cours…")
                        .size(12.0)
                        .color(pal.text_dim),
                );
            });
            return;
        }
        let Some(report) = &self.disk_report else {
            return;
        };
        let total = report.total.max(1);
        let rows: Vec<(String, String, PathBuf, u64)> = report
            .rows
            .iter()
            .map(|row| {
                (
                    row.label.clone(),
                    row.detail.clone(),
                    row.path.clone(),
                    row.bytes,
                )
            })
            .collect();
        let mut open: Option<PathBuf> = None;
        for (label, detail, path, bytes) in rows {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(430.0);
                    ui.label(RichText::new(&label).size(12.5).color(pal.text));
                    ui.label(RichText::new(&detail).size(10.5).color(pal.text_faint));
                    ui.label(
                        RichText::new(path.display().to_string())
                            .size(10.0)
                            .color(pal.text_faint),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(disk::format_bytes(bytes))
                            .size(12.5)
                            .color(pal.text),
                    );
                    ui.add_space(10.0);
                    if widgets::icon_button(ui, 24.0, Icon::Folder, pal, "Ouvrir le dossier")
                        .clicked()
                    {
                        open = Some(path.clone());
                    }
                    ui.add_space(10.0);
                    let width = 170.0;
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, pal.surface);
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            rect.min,
                            Vec2::new(width * (bytes as f32 / total as f32), 8.0),
                        ),
                        4.0,
                        pal.accent,
                    );
                });
            });
            ui.add_space(6.0);
            ui.separator();
        }
        if let Some(path) = open {
            if let Err(err) = crate::session::open_folder(&path) {
                self.toast_error(format!("Ouverture impossible : {err}"));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Import de la bibliothèque Steam installée
    // -----------------------------------------------------------------------

    pub(crate) fn steam_import_dialog(&mut self, ctx: &egui::Context) {
        if self.steam_import.is_none() {
            return;
        }
        let pal = self.pal;
        let mut close = false;
        let mut do_import = false;
        let modal = egui::Modal::new(egui::Id::new("quark_steam_import"))
            .frame(theme::modal_frame(&pal))
            .show(ctx, |ui| {
            ui.set_min_width(760.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Importer depuis Steam").size(17.0).color(pal.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("appmanifest_*.acf")
                            .size(10.5)
                            .color(pal.text_faint),
                    );
                });
            });
            ui.label(
                RichText::new(
                    "Jeux installés trouvés dans tes bibliothèques Steam. Les jeux déjà dans \
                     Quark ne sont pas dupliqués ; l'exécutable est cherché dans le dossier du jeu.",
                )
                .size(11.5)
                .color(pal.text_dim),
            );
            ui.add_space(8.0);
            if let Some(import) = self.steam_import.as_mut() {
                if import.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Lecture des bibliothèques Steam…")
                                .size(12.0)
                                .color(pal.text_dim),
                        );
                    });
                }
                if let Some(err) = &import.error {
                    ui.label(RichText::new(err).size(12.0).color(pal.danger));
                }
                if !import.games.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut import.filter)
                                .hint_text("Filtrer par nom…")
                                .desired_width(240.0),
                        );
                        ui.add_space(8.0);
                        let available = import.games.iter().filter(|game| !game.owned).count();
                        ui.label(
                            RichText::new(format!("{available} disponible(s)"))
                                .size(11.0)
                                .color(pal.text_faint),
                        );
                    });
                    ui.add_space(6.0);
                    let needle = import.filter.trim().to_lowercase();
                    egui::ScrollArea::vertical()
                        .max_height(380.0)
                        .show(ui, |ui| {
                            for game in import.games.iter_mut() {
                                if !needle.is_empty()
                                    && !game.name.to_lowercase().contains(&needle)
                                {
                                    continue;
                                }
                                ui.horizontal(|ui| {
                                    if game.owned {
                                        ui.add_enabled(
                                            false,
                                            egui::Checkbox::without_text(&mut game.include),
                                        );
                                    } else {
                                        ui.checkbox(&mut game.include, "");
                                    }
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(&game.name).size(13.0).color(
                                                if game.owned { pal.text_faint } else { pal.text },
                                            ),
                                        )
                                        .on_hover_text(game.dir.display().to_string());
                                        let exe = game
                                            .exe
                                            .as_ref()
                                            .map(|path| path.display().to_string())
                                            .unwrap_or_else(|| {
                                                "exécutable non détecté".to_string()
                                            });
                                        ui.label(
                                            RichText::new(format!(
                                                "AppID {} · {exe}",
                                                game.app_id
                                            ))
                                            .size(10.5)
                                            .color(pal.text_faint),
                                        );
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if game.owned {
                                            ui.label(
                                                RichText::new("déjà importé")
                                                    .size(10.5)
                                                    .color(pal.text_faint),
                                            );
                                        }
                                    });
                                });
                                ui.add_space(2.0);
                            }
                        });
                }
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if widgets::accent_button(
                    ui,
                    Vec2::new(200.0, 34.0),
                    "Importer la sélection",
                    &pal,
                    true,
                    Icon::Steam,
                )
                .clicked()
                {
                    do_import = true;
                }
                ui.add_space(8.0);
                if widgets::ghost_button(ui, "Fermer", &pal, Icon::None).clicked() {
                    close = true;
                }
            });
        });
        if modal.should_close() {
            close = true;
        }
        if do_import {
            self.import_steam(ctx);
            return;
        }
        if close {
            self.steam_import = None;
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
        let mut refresh_meta = false;

        let modal = egui::Modal::new(egui::Id::new("quark_game_settings"))
            .frame(theme::modal_frame(&pal))
            .show(ctx, |ui| {
                ui.set_min_width(660.0);
                ui.label(
                    RichText::new(format!("Paramètres · {}", game.name))
                        .size(20.0)
                        .strong()
                        .color(pal.text),
                );
                ui.add_space(10.0);
                // Même règle que les paramètres globaux : la page laisse la
                // place aux boutons du bas, sans les recouvrir.
                egui::ScrollArea::vertical()
                    .max_height((ctx.screen_rect().height() - 250.0).max(160.0))
                    .auto_shrink([false, true])
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

                section(ui, "BIBLIOTHÈQUE", &pal);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Statut").size(12.5).color(pal.text_dim));
                    widgets::info_button(
                        ui,
                        &pal,
                        "Statut personnel : il sert de filtre sur l'accueil et dans la colonne \
                         de gauche (pastilles de couleur). Aucun effet sur le lancement.",
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    for option in GameStatus::ALL {
                        let selected = game.status == option;
                        let (r, g, b) = option.color();
                        if widgets::chip_colored(
                            ui,
                            option.label(),
                            selected,
                            &pal,
                            egui::Color32::from_rgb(r, g, b),
                        ) {
                            game.status = option;
                        }
                    }
                });
                ui.add_space(8.0);
                tags_editor(ui, &pal, &mut game.tags);
                ui.add_space(8.0);
                ui.label(RichText::new("Notes personnelles").size(12.5).color(pal.text_dim));
                ui.add(
                    egui::TextEdit::multiline(&mut game.notes)
                        .hint_text("Où j'en suis, ce que je veux retenir…")
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );

                section(ui, "FICHE STEAM", &pal);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if game.steam_app_id.is_some() {
                            "Développeur, genres, date de sortie et description viennent de l'API \
                             appdetails."
                                .to_string()
                        } else {
                            "Ce jeu n'est pas lié à une fiche Steam.".to_string()
                        })
                        .size(11.5)
                        .color(pal.text_dim),
                    );
                    if game.steam_app_id.is_some()
                        && widgets::ghost_button(ui, "Récupérer depuis Steam", &pal, Icon::Refresh)
                            .clicked()
                    {
                        // Les champs vides sont remplis par la requête ; on les
                        // vide ici pour forcer la mise à jour au retour.
                        game.developer.clear();
                        game.genres.clear();
                        game.released.clear();
                        game.description.clear();
                        refresh_meta = true;
                    }
                });
                if !game.developer.is_empty() {
                    info_line(ui, "Développeur", &game.developer, &pal);
                }
                if !game.genres.is_empty() {
                    info_line(ui, "Genres", &game.genres.join(" · "), &pal);
                }
                if !game.released.is_empty() {
                    info_line(ui, "Sortie", &game.released, &pal);
                }

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
                let mut custom = game.prefix.is_some();
                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut custom, "Préfixe Proton propre à ce jeu")
                        .changed()
                    {
                        game.prefix = if custom { Some(String::new()) } else { None };
                    }
                    widgets::info_button(
                        ui,
                        &pal,
                        "Sans préfixe propre, le jeu prend celui de l'onglet « Par défaut » (sinon \
                         ~/.local/share/quark/prefixes/<jeu>). Le dossier choisi est utilisé tel \
                         quel, sans sous-dossier.",
                    );
                });

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

                section_hint(
                    ui,
                    "OPTIONS DE LANCEMENT",
                    &pal,
                    "Ces cases remplacent des commandes à taper. Elles enveloppent la commande du \
                     jeu, de l'extérieur vers l'intérieur : gamescope, puis gamemoderun, puis le \
                     jeu. Un outil absent est ignoré — le jeu démarre quand même — et signalé ici.",
                );
                launch_options_editor(
                    ui,
                    &pal,
                    &mut game.launch,
                    &mut game.proton_log,
                    &mut game.umu,
                    Some(&mut game.umu_game_id),
                    &self.tools,
                );
                section_hint(
                    ui,
                    "ANTI-TRICHE",
                    &pal,
                    "Cochés, ces dossiers sont déclarés à ton Proton par PROTON_BATTLEYE_RUNTIME / \
                     PROTON_EAC_RUNTIME : ils ne sont pas lancés directement (Steam ne les livre \
                     pas avec un script `proton`), donc cocher les deux est sans risque.",
                );
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
                section_hint(
                    ui,
                    "VARIABLES D'ENVIRONNEMENT",
                    &pal,
                    "Copiées depuis l'onglet « Par défaut » au moment de l'ajout : les modifier ici \
                     ne touche que ce jeu. Format NOM=valeur.",
                );
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

        if refresh_meta {
            self.refresh_metadata(ctx, &game.id);
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
        let modal = egui::Modal::new(egui::Id::new("quark_customize"))
            .frame(theme::modal_frame(&pal))
            .show(ctx, |ui| {
                ui.set_min_width(700.0);
                ui.label(
                    RichText::new(format!("Personnaliser · {}", game.name))
                        .size(17.0)
                        .color(pal.text),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                ui.label(RichText::new("Images").size(12.0).color(pal.text_dim));
                widgets::info_button(
                    ui,
                    &pal,
                    "Choisis l'image de chaque emplacement : une proposition en ligne, un fichier \
                     de ton disque, ou aucune (rendu de secours).",
                );
            });
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
                                            && widgets::ghost_button(
                                                ui,
                                                "Retirer",
                                                &pal,
                                                Icon::Trash,
                                            )
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
                    if widgets::accent_button(
                        ui,
                        Vec2::new(140.0, 34.0),
                        "Fermer",
                        &pal,
                        true,
                        Icon::None,
                    )
                    .clicked()
                    {
                        close = true;
                    }
                    ui.add_space(8.0);
                    if widgets::ghost_button(ui, "Réglages du jeu…", &pal, Icon::Gear).clicked()
                    {
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

        let modal = egui::Modal::new(egui::Id::new("quark_confirm_remove"))
            .frame(theme::modal_frame(&pal))
            .show(ctx, |ui| {
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
            // Le nom prend un tiers de la ligne, la valeur tout le reste : une
            // valeur longue (WINEDLLOVERRIDES=…) reste lisible sans défiler.
            let name_width = ((width - 96.0) * 0.32).max(110.0);
            let name = ui.add_sized(
                Vec2::new(name_width, 26.0),
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
            let value_width = (width - name_width - 66.0).max(150.0);
            let value = ui.add_sized(
                Vec2::new(value_width, 26.0),
                egui::TextEdit::singleline(&mut vars[index].value).hint_text("valeur"),
            );
            if value.changed() {
                changed = true;
            }
            if widgets::icon_button(ui, 24.0, Icon::Trash, pal, "Retirer cette variable").clicked()
            {
                remove = Some(index);
            }
        });
        ui.add_space(2.0);
    }

    ui.horizontal(|ui| {
        if widgets::ghost_button(ui, "Ajouter une variable", pal, Icon::Plus).clicked() {
            add = true;
        }
        widgets::info_button(
            ui,
            pal,
            "Une ligne par variable, au format NOM=valeur (par exemple \
             WINEDLLOVERRIDES=winmm=n,b;steam_api64). Coller « NOM=valeur » dans le champ du \
             nom le sépare automatiquement.",
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

/// Grand chiffre du résumé (valeur + libellé).
fn metric(ui: &mut egui::Ui, pal: &Palette, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(10.5).color(pal.text_faint));
        ui.label(RichText::new(value).size(19.0).strong().color(pal.text));
    });
}

/// Histogramme des douze dernières semaines.
fn week_chart(ui: &mut egui::Ui, pal: &Palette, sessions: &[config::PlaySession]) {
    let weeks = stats::weeks(sessions, 12);
    let max = weeks
        .iter()
        .map(|(_, secs)| *secs)
        .max()
        .unwrap_or(0)
        .max(1);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 150.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let base = rect.bottom() - 22.0;
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(rect.left(), base), Vec2::new(rect.width(), 1.0)),
        0.0,
        pal.border,
    );
    let slot = rect.width() / weeks.len().max(1) as f32;
    for (index, (start, secs)) in weeks.iter().enumerate() {
        let x = rect.left() + slot * index as f32 + slot * 0.18;
        let width = (slot * 0.64).max(6.0);
        let bar_height = (base - rect.top() - 18.0) * (*secs as f32 / max as f32);
        let bar_height = if *secs > 0 { bar_height.max(3.0) } else { 0.0 };
        let bar = Rect::from_min_size(
            Pos2::new(x, base - bar_height),
            Vec2::new(width, bar_height),
        );
        let hovered = ui.rect_contains_pointer(bar);
        painter.rect_filled(
            bar,
            3.0,
            if hovered {
                pal.accent
            } else {
                pal.accent.gamma_multiply(0.75)
            },
        );
        if *secs > 0 {
            painter.text(
                Pos2::new(bar.center().x, bar.top() - 2.0),
                Align2::CENTER_BOTTOM,
                stats::human(*secs),
                FontId::proportional(9.0),
                pal.text_dim,
            );
        }
        // Une date sur deux : les libellés ne se chevauchent pas.
        if index % 2 == 1 || index + 1 == weeks.len() {
            painter.text(
                Pos2::new(bar.center().x, base + 4.0),
                Align2::CENTER_TOP,
                stats::date_label(*start),
                FontId::proportional(9.0),
                pal.text_faint,
            );
        }
    }
}

/// Calendrier de chaleur : 26 semaines de 7 jours.
fn heatmap_chart(ui: &mut egui::Ui, pal: &Palette, sessions: &[config::PlaySession]) {
    let map = stats::heatmap(sessions, 26);
    let weeks = map.cells.len();
    let cell = 13.0;
    let gap = 3.0;
    let label_width = 46.0;
    let width = label_width + weeks as f32 * (cell + gap);
    let height = 7.0 * (cell + gap) + 20.0;
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(width), height),
        Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    for day in 0..7 {
        painter.text(
            Pos2::new(
                rect.left(),
                rect.top() + 16.0 + day as f32 * (cell + gap) + cell / 2.0,
            ),
            Align2::LEFT_CENTER,
            stats::weekday_label(day),
            FontId::proportional(9.0),
            pal.text_faint,
        );
    }
    let mut last_month = String::new();
    // Un libellé de mois tous les quatre au moins : « septembre 2026 » et
    // « octobre 2026 » se chevauchaient sur la même ligne.
    let mut last_label_week: i64 = -8;
    for (week, days) in map.cells.iter().enumerate() {
        let monday = map.first_day + week as i64 * 7;
        let month = stats::month_short(monday);
        if month != last_month {
            if week as i64 - last_label_week >= 4 {
                painter.text(
                    Pos2::new(
                        rect.left() + label_width + week as f32 * (cell + gap),
                        rect.top(),
                    ),
                    Align2::LEFT_TOP,
                    &month,
                    FontId::proportional(9.0),
                    pal.text_faint,
                );
                last_label_week = week as i64;
            }
            last_month = month;
        }
        for (day, secs) in days.iter().enumerate() {
            let ratio = if map.max > 0 {
                *secs as f32 / map.max as f32
            } else {
                0.0
            };
            let color = if *secs == 0 {
                pal.surface
            } else {
                theme::lerp_color(pal.surface_hover, pal.accent, 0.25 + 0.75 * ratio.sqrt())
            };
            let cell_rect = Rect::from_min_size(
                Pos2::new(
                    rect.left() + label_width + week as f32 * (cell + gap),
                    rect.top() + 16.0 + day as f32 * (cell + gap),
                ),
                Vec2::splat(cell),
            );
            painter.rect_filled(cell_rect, 2.5, color);
            let response = ui.interact(
                cell_rect,
                egui::Id::new(("quark_heatmap", week, day)),
                Sense::hover(),
            );
            if response.hovered() {
                painter.rect_stroke(
                    cell_rect,
                    2.5,
                    egui::Stroke::new(1.0_f32, pal.text),
                    egui::StrokeKind::Inside,
                );
                response.on_hover_text(format!(
                    "{} · joué {}",
                    stats::date_label(monday + day as i64),
                    stats::human(*secs)
                ));
            }
        }
    }
}

/// Ligne « jeu · durée » avec une barre proportionnelle.
fn game_bar(ui: &mut egui::Ui, pal: &Palette, name: &str, secs: u64, max: u64) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).size(12.0).color(pal.text));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(config::human_playtime(secs))
                    .size(11.5)
                    .color(pal.text_dim),
            );
            let width = 260.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, pal.surface);
            ui.painter().rect_filled(
                Rect::from_min_size(
                    rect.min,
                    Vec2::new(width * (secs as f32 / max.max(1) as f32), 8.0),
                ),
                4.0,
                pal.accent,
            );
        });
    });
}

/// Éditeur de tags : pastilles supprimables et champ d'ajout. Le champ garde
/// sa saisie dans la mémoire d'egui, donc une virgule fraîche n'efface rien.
fn tags_editor(ui: &mut egui::Ui, pal: &Palette, tags: &mut Vec<String>) {
    let buffer_id = ui.id().with("quark_tag_field");
    let mut buffer = ui
        .data(|data| data.get_temp::<String>(buffer_id))
        .unwrap_or_default();

    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Tags personnels")
                .size(12.5)
                .color(pal.text_dim),
        );
        widgets::info_button(
            ui,
            pal,
            "Les tags servent de catégories : sur l'accueil et dans la colonne de gauche, \
             cliquer sur « coop » n'affiche que les jeux qui le portent.",
        );
    });
    let mut remove: Option<usize> = None;
    if tags.is_empty() {
        ui.label(RichText::new("aucun tag").size(11.0).color(pal.text_faint));
    } else {
        ui.horizontal_wrapped(|ui| {
            for (index, tag) in tags.iter().enumerate() {
                if widgets::chip(ui, &format!("{tag} ×"), true, pal) {
                    remove = Some(index);
                }
            }
        });
    }
    if let Some(index) = remove {
        tags.remove(index);
    }
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut buffer)
                .hint_text("coop, détente, à finir…")
                .desired_width(240.0),
        );
        let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if widgets::ghost_button(ui, "Ajouter", pal, Icon::Plus).clicked() || enter {
            let tag = buffer.trim().to_string();
            if !tag.is_empty()
                && !tags
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&tag))
            {
                tags.push(tag);
            }
            buffer.clear();
        }
    });
    ui.data_mut(|data| data.insert_temp(buffer_id, buffer));
}

fn section(ui: &mut egui::Ui, title: &str, pal: &Palette) {
    // Même titre de section que les pages (petites capitales + filet).
    ui.add_space(14.0);
    widgets::section_title(ui, title, pal);
    ui.add_space(6.0);
}

/// Titre de section avec son ⓘ : l'explication reste disponible au survol au
/// lieu d'occuper une ligne de texte gris sous chaque réglage.
fn section_hint(ui: &mut egui::Ui, title: &str, pal: &Palette, hint: &str) {
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 22.0).max(80.0);
        ui.scope(|ui| {
            ui.set_max_width(width);
            widgets::section_title(ui, title, pal);
        });
        widgets::info_button(ui, pal, hint);
    });
    ui.add_space(6.0);
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
    let (rect, response) = ui.allocate_exact_size(Vec2::new(132.0, 76.0), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, pal.surface);
    // Chargement explicite : egui affiche une icône d'avertissement rouge quand
    // une image distante échoue, alors que la proposition reste utilisable. On
    // dessine donc nos propres états (chargement / aperçu indisponible).
    let area = rect.shrink(3.0);
    match ui.ctx().try_load_texture(
        url,
        egui::TextureOptions::LINEAR,
        egui::load::SizeHint::Scale(1.0.into()),
    ) {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            let size = texture.size;
            if size.x > 0.0 && size.y > 0.0 {
                let scale = (area.width() / size.x).max(area.height() / size.y);
                let fitted = Rect::from_center_size(area.center(), size * scale);
                ui.painter_at(area).image(
                    texture.id,
                    fitted,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        }
        Ok(egui::load::TexturePoll::Pending { .. }) => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "chargement…",
                FontId::proportional(10.5),
                pal.text_faint,
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(120));
        }
        Err(_) => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "aperçu indisponible",
                FontId::proportional(10.5),
                pal.text_faint,
            );
        }
    }
    if selected || response.hovered() {
        widgets::border_rect(
            &painter,
            rect,
            if selected { 2.0 } else { 1.0 },
            if selected { pal.accent } else { pal.border },
        );
    }
    if selected {
        painter.circle_filled(
            Pos2::new(rect.right() - 13.0, rect.top() + 13.0),
            7.0,
            pal.accent,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Ligne de résultat de recherche (nom + miniature + source).
fn search_result_row(
    ui: &mut egui::Ui,
    pal: &Palette,
    hit: &crate::artwork::SearchHit,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
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
                egui::Image::new(thumb)
                    .max_size(thumb_rect.size())
                    .maintain_aspect_ratio(true),
            );
        }
        None => {
            widgets::monogram(
                &painter,
                Rect::from_min_size(
                    Pos2::new(thumb_rect.left(), rect.center().y - 14.0),
                    Vec2::splat(28.0),
                ),
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
                    if ui
                        .selectable_label(!candidate.include, "Pas un jeu")
                        .clicked()
                    {
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
            let selected = self
                .add
                .scan
                .candidates
                .iter()
                .filter(|c| c.include)
                .count();
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

impl QuarkApp {
    // -----------------------------------------------------------------------
    // Journal de session
    // -----------------------------------------------------------------------

    pub(crate) fn logs_dialog(&mut self, ctx: &egui::Context) {
        let Some((game_id, lines, truncated, error, path)) = self.logs.as_ref().map(|logs| {
            (
                logs.game_id.clone(),
                logs.lines.clone(),
                logs.truncated,
                logs.error.clone(),
                logs.path.clone(),
            )
        }) else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.logs = None;
            return;
        }
        let pal = self.pal;
        let name = self
            .lib
            .get(&game_id)
            .map(|game| game.name.clone())
            .unwrap_or_else(|| game_id.clone());
        let running = self.is_running(&game_id);
        let mut close = false;
        let mut refresh = false;
        let mut open_folder = false;
        let max_height = (ctx.screen_rect().height() * 0.58).max(220.0);

        // Fond opaque explicite : sans lui, une fenêtre plus large que l'écran
        // laissait voir la bibliothèque au travers.
        let log_frame = egui::Frame::new()
            .fill(pal.bg)
            .stroke(egui::Stroke::new(1.0_f32, pal.border))
            .corner_radius(12.0)
            .inner_margin(14.0);
        let modal = egui::Modal::new(egui::Id::new("quark_logs")).frame(log_frame).show(ctx, |ui| {
            ui.set_min_width(780.0);
            // Les lignes d'un journal sont longues : la fenêtre se limite à
            // l'écran et le texte défile horizontalement au lieu de pousser le
            // cadre hors de la fenêtre.
            ui.set_max_width((ctx.screen_rect().width() - 120.0).clamp(520.0, 980.0));
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Journal · {name}"))
                        .size(17.0)
                        .color(pal.text),
                );
                if running {
                    ui.add_space(8.0);
                    ui.spinner();
                    ui.label(
                        RichText::new("session en cours")
                            .size(11.0)
                            .color(pal.text_dim),
                    );
                }
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(path.display().to_string())
                    .size(10.5)
                    .color(pal.text_faint),
            );
            widgets::info_button(
                ui,
                &pal,
                "Tout ce que le jeu et Wine affichent (stdout et stderr) est écrit ici, précédé de \
                 la commande lancée et de ses variables. Avec « Journal Proton détaillé », \
                 PROTON_LOG écrit son propre fichier dans ce même dossier.",
            );
            ui.add_space(8.0);

            egui::ScrollArea::both()
                .id_salt("quark_logs_scroll")
                .max_height(max_height)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if let Some(message) = &error {
                        ui.label(RichText::new(message).size(11.5).color(pal.danger));
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Le journal est créé au premier lancement du jeu.")
                                .size(11.5)
                                .color(pal.text_faint),
                        );
                        return;
                    }
                    if truncated {
                        ui.label(
                            RichText::new("début du journal tronqué à l'affichage")
                                .size(10.5)
                                .color(pal.text_faint),
                        );
                        ui.add_space(4.0);
                    }
                    if lines.is_empty() {
                        ui.label(RichText::new("journal vide").size(11.5).color(pal.text_faint));
                    } else {
                        ui.add(
                            egui::Label::new(
                                RichText::new(lines.join("\n"))
                                    .monospace()
                                    .size(11.0)
                                    .color(pal.text_dim),
                            )
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .selectable(true),
                        );
                    }
                });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if widgets::accent_button(ui, Vec2::new(130.0, 32.0), "Fermer", &pal, true, Icon::None)
                    .clicked()
                {
                    close = true;
                }
                ui.add_space(8.0);
                if widgets::ghost_button(ui, "Actualiser", &pal, Icon::Refresh).clicked() {
                    refresh = true;
                }
                if widgets::ghost_button(ui, "Ouvrir le dossier", &pal, Icon::Folder).clicked() {
                    open_folder = true;
                }
            });
        });
        if modal.should_close() {
            close = true;
        }
        if refresh {
            self.refresh_log();
        }
        if open_folder {
            let dir = path.parent().map(|parent| parent.to_path_buf());
            if let Some(dir) = dir {
                if let Err(err) = crate::session::open_folder(&dir) {
                    self.toast_error(format!("Ouverture du dossier impossible : {err}"));
                }
            }
        }
        if close {
            self.logs = None;
        }
    }
}

/// Éditeur des options de lancement, partagé par les paramètres du jeu et
/// l'onglet « Par défaut » : mêmes cases et mêmes explications des deux côtés.
/// Renvoie vrai si un réglage a changé.
fn launch_options_editor(
    ui: &mut egui::Ui,
    pal: &Palette,
    launch: &mut config::LaunchOptions,
    proton_log: &mut bool,
    umu: &mut bool,
    mut umu_game_id: Option<&mut String>,
    tools: &proton::Tools,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.checkbox(&mut launch.gamemode, "gamemoderun").changed();
        if !tools.gamemoderun {
            ui.label(RichText::new("(absent)").size(10.5).color(pal.danger));
        }
        ui.add_space(16.0);
        changed |= ui.checkbox(&mut launch.mangohud, "MangoHud").changed();
        ui.add_space(16.0);
        changed |= ui.checkbox(&mut launch.prime, "PRIME (NVIDIA)").changed();
        widgets::info_button(
            ui,
            pal,
            "MangoHud passe par MANGOHUD=1, rien à taper. PRIME force le rendu sur la \
             carte NVIDIA dédiée des portables hybrides (DRI_PRIME pour les autres).",
        );
    });
    ui.horizontal(|ui| {
        changed |= ui.checkbox(&mut launch.gamescope, "gamescope").changed();
        if !tools.gamescope {
            ui.label(RichText::new("(absent)").size(10.5).color(pal.danger));
        }
        if launch.gamescope {
            ui.add_space(10.0);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut launch.gamescope_width)
                        .range(320..=7680)
                        .speed(4.0)
                        .suffix(" px"),
                )
                .changed();
            ui.label(RichText::new("×").size(11.0).color(pal.text_dim));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut launch.gamescope_height)
                        .range(200..=4320)
                        .speed(4.0)
                        .suffix(" px"),
                )
                .changed();
            ui.add_space(10.0);
            changed |= ui.checkbox(&mut launch.gamescope_fsr, "FSR").changed();
            ui.add_space(10.0);
            ui.label(RichText::new("max").size(11.5).color(pal.text_dim));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut launch.gamescope_fps)
                        .range(0..=1000)
                        .speed(1.0)
                        .suffix(" fps"),
                )
                .changed();
            widgets::info_button(
                ui,
                pal,
                "gamescope affiche le jeu dans son propre compositeur, à cette \
                 résolution. FSR agrandit une image plus petite (gain de performance) ; \
                 « max » limite les images par seconde (--framerate-limit, 0 = aucune limite).",
            );
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Commande wrapper")
                .size(12.5)
                .color(pal.text_dim),
        );
        widgets::info_button(
            ui,
            pal,
            "Comme les options de lancement Steam : %command% est remplacé par la \
             commande du jeu. Exemple : env FOO=1 %command%. Sans %command%, la \
             commande est placée devant. La ligne passe par un shell, donc env, pipes \
             et redirections fonctionnent.",
        );
    });
    changed |= ui
        .add(
            egui::TextEdit::singleline(&mut launch.wrapper)
                .hint_text("%command%")
                .desired_width(f32::INFINITY),
        )
        .changed();
    ui.horizontal(|ui| {
        changed |= ui
            .checkbox(proton_log, "Journal Proton détaillé (PROTON_LOG=1)")
            .changed();
        widgets::info_button(
            ui,
            pal,
            "Proton écrit un journal détaillé dans le dossier de logs du jeu, à côté de \
             la sortie standard capturée par Quark. Utile quand un jeu refuse de démarrer.",
        );
    });
    ui.horizontal(|ui| {
        changed |= ui.checkbox(umu, "Lancer via umu-launcher").changed();
        if !tools.umu_run {
            ui.label(
                RichText::new("(umu-run absent)")
                    .size(10.5)
                    .color(pal.danger),
            );
        }
        widgets::info_button(
            ui,
            pal,
            "umu-launcher lance un jeu Windows avec Proton sans Steam : il gère le \
             préfixe et transmet GAMEID à Proton. Sans umu-run installé, Quark refuse le \
             lancement avec un message clair plutôt que d'échouer en silence.",
        );
    });
    if launch.any() || *proton_log || *umu {
        ui.add_space(4.0);
        if widgets::ghost_button(ui, "Tout réinitialiser", pal, Icon::None).clicked() {
            *launch = config::LaunchOptions::default();
            *proton_log = false;
            *umu = false;
            if let Some(id) = umu_game_id.as_deref_mut() {
                id.clear();
            }
            changed = true;
        }
    }
    if *umu {
        if let Some(id) = umu_game_id.as_deref_mut() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("GAMEID").size(11.5).color(pal.text_dim));
                ui.add(
                    egui::TextEdit::singleline(id)
                        .hint_text("0")
                        .desired_width(120.0),
                );
                widgets::info_button(
                    ui,
                    pal,
                    "Identifiant ProtonDB du jeu (son AppID Steam). « 0 » suffit pour un \
                     jeu inconnu.",
                );
            });
        }
    }
    changed
}
