//! Onglet Bibliothèque : colonne de gauche (liste ou grille) + panneau du jeu.

use crate::app::{NewsState, QuarkApp};
use crate::config::{self, AssetKind, Game, GameStatus, ViewMode};
use crate::images::{
    draw_cover, draw_fit_left, hero_placeholder, rounded_cover, rounded_cover_tinted,
};
use crate::news;
use crate::proton::{self, LaunchKind};
use crate::theme::{self, Palette};
use crate::ui_home::linux_chip;
use crate::widgets::{self, Icon};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2,
};
use std::path::PathBuf;

impl QuarkApp {
    pub(crate) fn library_tab(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let view = self.lib.settings.view;
        // Vue grille : **seulement** la grille. La fiche du jeu s'ouvre en
        // surimpression, avec sa croix, et se ferme d'un clic à côté.
        let grid_only = view == ViewMode::Grid;
        if !grid_only {
            egui::SidePanel::left("quark_library_list")
                .exact_width(292.0)
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(pal.panel)
                        .stroke(egui::Stroke::new(1.0_f32, pal.border)),
                )
                .show(ctx, |ui| self.sidebar(ui));
        }

        // La fiche dessine sa propre bannière floutée : le panneau doit laisser
        // passer le fond au lieu de le repeindre en noir uni.
        let see_through = !self.home && !grid_only;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(if see_through {
                Color32::TRANSPARENT
            } else {
                pal.bg
            }))
            .show(ctx, |ui| {
                if self.home {
                    self.home_pane(ui);
                } else if grid_only {
                    self.grid_page(ui);
                } else {
                    self.detail_pane(ui);
                }
            });

        if grid_only {
            self.detail_overlay(ctx);
        }
    }

    /// Grille pleine largeur (vue grille) : toutes les couvertures, sans fiche.
    fn grid_page(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let ids = self.display_ids();
        // Entrée de page : le contenu se fond et les tuiles montent l'une après
        // l'autre. L'âge est remis à zéro à chaque changement d'onglet ou de vue.
        let age = self.page_at.elapsed().as_secs_f32();
        let total = self.lib.games.len();
        ui.add_space(14.0);
        // Les commandes (ajout, vues) vivent dans la barre du haut, pas ici :
        // la grille ne garde que son compteur.
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(
                RichText::new(format!(
                    "{} jeu{} · clic pour ouvrir la fiche",
                    ids.len(),
                    if ids.len() > 1 { "x" } else { "" }
                ))
                .size(11.0)
                .color(pal.text_faint),
            );
        });
        ui.add_space(12.0);
        if ids.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);
                ui.label(
                    RichText::new(if total == 0 {
                        "Ta bibliothèque est vide."
                    } else {
                        "Aucun jeu ne correspond au filtre."
                    })
                    .size(14.0)
                    .color(pal.text_dim),
                );
            });
            return;
        }

        // Même rendu que l'accueil : couvertures arrondies, titre, temps de jeu.
        let mut paths: Vec<Option<PathBuf>> = Vec::with_capacity(ids.len());
        for id in &ids {
            paths.push(
                self.lib
                    .get(id)
                    .and_then(|game| game.artwork.get(AssetKind::Cover))
                    .filter(|path| path.exists())
                    .cloned(),
            );
        }
        let covers: Vec<Option<egui::TextureHandle>> = paths
            .iter()
            .map(|path| path.as_ref().and_then(|path| self.texture(ui.ctx(), path)))
            .collect();

        let mut opened: Option<String> = None;
        egui::ScrollArea::vertical()
            .id_salt("quark_grid_page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let fade = (age / 0.16).clamp(0.0, 1.0);
                if fade < 1.0 {
                    ui.ctx().request_repaint();
                }
                ui.set_opacity(fade);
                let padding = 24.0;
                let gap = 16.0;
                let available = (ui.available_width() - padding * 2.0).max(240.0);
                let columns = ((available + gap) / 172.0).floor().clamp(2.0, 8.0) as usize;
                // Arrondi vers le bas : les fractions de pixel s'accumulaient
                // de colonne en colonne et poussaient la dernière hors écran.
                let tile_w = ((available - gap * (columns as f32 - 1.0)) / columns as f32)
                    .floor()
                    .max(110.0);
                let cover_h = tile_w * theme::COVER_RATIO;
                let row_h = cover_h + 36.0;
                let mut index = 0usize;
                while index < ids.len() {
                    ui.horizontal(|ui| {
                        // L'espacement par défaut d'egui s'ajoutait à la marge
                        // et aux gouttières : la dernière colonne dépassait.
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(padding);
                        for slot in 0..columns {
                            let Some(id) = ids.get(index + slot) else {
                                break;
                            };
                            let Some(game) = self.lib.get(id).cloned() else {
                                continue;
                            };
                            if slot > 0 {
                                ui.add_space(gap);
                            }
                            let (tile, response) =
                                ui.allocate_exact_size(Vec2::new(tile_w, row_h), Sense::click());
                            let order = (index + slot) as f32;
                            let appear = ((age - order * 0.02) / 0.3).clamp(0.0, 1.0);
                            if appear < 1.0 {
                                ui.ctx().request_repaint();
                            }
                            let tile = tile.translate(Vec2::new(0.0, (1.0 - appear) * 9.0));
                            let mut painter = ui.painter_at(tile);
                            painter.set_opacity(appear);
                            let anim = ui.ctx().animate_bool_with_time(
                                egui::Id::new(("quark_grid_tile", id.as_str())),
                                response.hovered(),
                                0.14,
                            );
                            let missing = game.exe_missing();
                            let cover = Rect::from_min_size(tile.min, Vec2::new(tile_w, cover_h))
                                .shrink(2.0 * anim);
                            painter.rect_filled(
                                tile,
                                12.0,
                                if missing {
                                    Color32::from_rgb(0x2A, 0x10, 0x10)
                                } else {
                                    pal.surface
                                },
                            );
                            match covers.get(index + slot).and_then(|cover| cover.as_ref()) {
                                Some(texture) => {
                                    rounded_cover_tinted(
                                        ui,
                                        cover,
                                        texture,
                                        10.0,
                                        Color32::from_white_alpha((appear * 255.0).round() as u8),
                                    );
                                }
                                None => {
                                    // Monogramme plein cadre, comme une
                                    // couverture : toutes les affiches occupent
                                    // exactement la même surface.
                                    widgets::monogram(
                                        &painter,
                                        cover,
                                        &game.monogram_letter(),
                                        theme::monogram_color(&game.name),
                                        &pal,
                                    );
                                }
                            }
                            if anim > 0.0 {
                                painter.rect_stroke(
                                    tile,
                                    12.0,
                                    egui::Stroke::new(2.0_f32, pal.accent.gamma_multiply(anim)),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if missing {
                                painter.text(
                                    Pos2::new(tile.left() + 6.0, cover.bottom() + 6.0),
                                    Align2::LEFT_TOP,
                                    "exécutable introuvable",
                                    FontId::proportional(10.0),
                                    pal.danger,
                                );
                            }
                            let title_color = if missing {
                                pal.danger
                            } else {
                                theme::lerp_color(pal.text_dim, pal.text, anim)
                            };
                            let job = single_line(&game.name, 12.5, title_color, tile_w);
                            let galley = painter.layout_job(job);
                            painter.galley(
                                Pos2::new(
                                    tile.left(),
                                    cover.bottom() + if missing { 21.0 } else { 7.0 },
                                ),
                                galley,
                                title_color,
                            );
                            if let Some((r, g, b)) = status_color(game.status) {
                                painter.circle_filled(
                                    Pos2::new(tile.left() + 4.0, cover.top() + 8.0),
                                    4.0,
                                    Color32::from_rgb(r, g, b),
                                );
                            }
                            if response.clicked() {
                                opened = Some(id.clone());
                            }
                            let _ = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                        }
                    });
                    ui.add_space(gap);
                    index += columns;
                }
                ui.add_space(20.0);
            });
        if let Some(id) = opened {
            self.overlay = Some(id.clone());
            self.selected = Some(id);
        }
    }

    /// Fiche du jeu en surimpression : croix, clic à côté, ou Échap pour fermer.
    fn detail_overlay(&mut self, ctx: &egui::Context) {
        let Some(id) = self.overlay.clone() else {
            return;
        };
        if self.lib.get(&id).is_none() {
            self.overlay = None;
            return;
        }
        self.selected = Some(id);
        let pal = self.pal;
        let screen = ctx.screen_rect();
        let size = Vec2::new(
            (screen.width() * 0.94).max(720.0),
            (screen.height() * 0.92).max(480.0),
        );
        let name = self
            .lib
            .get(&self.overlay.clone().unwrap_or_default())
            .map(|game| game.name.clone())
            .unwrap_or_default();
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::new("quark_game_overlay"))
            .backdrop_color(Color32::from_black_alpha(200))
            .frame(
                egui::Frame::new()
                    .fill(pal.bg)
                    .stroke(egui::Stroke::new(1.0_f32, pal.border))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                ui.set_min_size(size);
                ui.set_max_size(size);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new(&name).size(13.0).color(pal.text_dim));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(10.0);
                        if widgets::icon_button(ui, 30.0, Icon::Close, &pal, "Fermer (Échap)")
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
                ui.add_space(6.0);
                self.detail_pane(ui);
            });
        if modal.should_close() || close {
            self.overlay = None;
        }
    }

    // -----------------------------------------------------------------------
    // Colonne de gauche
    // -----------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let ids = self.display_ids();
        let view = self.lib.settings.view;
        let total = self.lib.games.len();
        // Sélection multiple (Ctrl+clic) : les actions sont appliquées après la
        // mise en page, pour ne pas emprunter `self` pendant les clôtures.
        let marked_count = self.marked.len();
        let mut remove_marked = false;
        let mut clear_marked = false;

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            if widgets::home_button(ui, self.home, &pal).clicked() {
                self.home = true;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if widgets::icon_button(ui, 26.0, Icon::Plus, &pal, "Ajouter un jeu").clicked() {
                    self.open_add_dialog();
                }
            });
        });

        ui.add_space(6.0);
        if marked_count == 0 {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                let count = if ids.len() == total {
                    format!("{total} jeu{}", if total > 1 { "x" } else { "" })
                } else {
                    format!("{} / {total} jeux", ids.len())
                };
                ui.label(RichText::new(count).size(11.0).color(pal.text_faint));
            });
        } else {
            // Barre de sélection : supprimer plusieurs jeux d'un coup.
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!(
                        "{marked_count} sélectionné{}",
                        if marked_count > 1 { "s" } else { "" }
                    ))
                    .size(11.0)
                    .color(pal.accent),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(12.0);
                    if widgets::icon_button(ui, 26.0, Icon::Trash, &pal, "Supprimer la sélection")
                        .clicked()
                    {
                        remove_marked = true;
                    }
                    ui.add_space(6.0);
                    if widgets::ghost_button(ui, "Désélectionner", &pal, Icon::None).clicked() {
                        clear_marked = true;
                    }
                });
            });
        }
        self.quick_filters(ui);
        ui.add_space(8.0);

        let age = self.page_at.elapsed().as_secs_f32();
        egui::ScrollArea::vertical()
            .id_salt("quark_library_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if ids.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(if total == 0 {
                                "Ta bibliothèque est vide."
                            } else {
                                "Aucun jeu ne correspond au filtre."
                            })
                            .size(12.5)
                            .color(pal.text_dim),
                        );
                        ui.add_space(12.0);
                        if widgets::accent_button(
                            ui,
                            Vec2::new(170.0, 34.0),
                            "Ajouter un jeu",
                            &pal,
                            true,
                            Icon::Plus,
                        )
                        .clicked()
                        {
                            self.open_add_dialog();
                        }
                    });
                    return;
                }

                match view {
                    ViewMode::List => {
                        ui.add_space(2.0);
                        // Comme les colonnes de Steam : une section par statut,
                        // avec son titre et son nombre de jeux. Un clic sur le
                        // titre replie la section.
                        let groups = self.status_groups(&ids);
                        for (position, (status, group)) in groups.iter().enumerate() {
                            if !status_section(ui, &pal, *status, group.len(), position == 0) {
                                continue;
                            }
                            for id in group {
                                self.sidebar_row(ui, id);
                            }
                        }
                        ui.add_space(10.0);
                    }
                    ViewMode::Grid => {
                        ui.add_space(4.0);
                        let padding = 14.0;
                        let gap = 10.0;
                        let available = ui.available_width() - padding * 2.0;
                        let columns = 3;
                        let tile_w = ((available - gap * (columns as f32 - 1.0)) / columns as f32)
                            .floor()
                            .max(80.0);
                        let tile_h = tile_w * theme::COVER_RATIO;

                        let mut index = 0usize;
                        while index < ids.len() {
                            ui.horizontal(|ui| {
                                // Sans cela, l'espacement d'egui s'ajoutait aux
                                // colonnes et la dernière sortait du panneau.
                                ui.spacing_mut().item_spacing.x = 0.0;
                                ui.add_space(padding);
                                for slot in 0..columns {
                                    let Some(id) = ids.get(index + slot) else {
                                        break;
                                    };
                                    let Some(game) = self.lib.get(id).cloned() else {
                                        continue;
                                    };
                                    if slot > 0 {
                                        ui.add_space(gap);
                                    }
                                    let selected = self.selected.as_deref() == Some(id.as_str());
                                    let marked = self.is_marked(id);
                                    let running = self.is_running(id);
                                    let tile_pal = pal.with_accent(self.game_accent(&game));
                                    // Cascade verticale : chaque rangée démarre un
                                    // peu après la précédente.
                                    let appear = ((age - (index + slot) as f32 * 0.02) / 0.3)
                                        .clamp(0.0, 1.0);
                                    if appear < 1.0 {
                                        ui.ctx().request_repaint();
                                    }
                                    let response = game_tile(
                                        ui,
                                        &tile_pal,
                                        &game,
                                        Vec2::new(tile_w, tile_h),
                                        selected,
                                        marked,
                                        running,
                                        appear,
                                        &mut self.textures,
                                    );
                                    let clicked = response.clicked();
                                    let double = response.double_clicked();
                                    match game_context_menu(
                                        &response,
                                        if marked && self.marked.len() > 1 {
                                            self.marked.len()
                                        } else {
                                            0
                                        },
                                        &pal,
                                    ) {
                                        Some(RowAction::Launch) => self.launch(ui.ctx(), id),
                                        Some(RowAction::Customize) => {
                                            self.select(id);
                                            self.open_customize(id);
                                        }
                                        Some(RowAction::Settings) => {
                                            self.select(id);
                                            self.editing = Some(game.clone());
                                        }
                                        Some(RowAction::Remove) => {
                                            self.select(id);
                                            self.confirm_remove = vec![id.clone()];
                                        }
                                        None => {
                                            if clicked {
                                                if ui.input(|i| i.modifiers.command) {
                                                    self.toggle_marked(id);
                                                } else {
                                                    self.select(id);
                                                }
                                            }
                                            if double {
                                                self.launch(ui.ctx(), id);
                                            }
                                        }
                                    }
                                }
                            });
                            ui.add_space(gap);
                            index += columns;
                        }
                        ui.add_space(10.0);
                    }
                }
            });

        if remove_marked {
            self.ask_remove_marked();
        }
        if clear_marked {
            self.marked.clear();
        }

        if let Some(job) = self.job.as_ref() {
            let label = job.label.clone();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.spinner();
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("Artworks · {label}"))
                        .size(11.5)
                        .color(pal.text_dim),
                );
            });
            ui.add_space(8.0);
        }
    }

    /// Une ligne de la vue liste, avec son menu contextuel.
    fn sidebar_row(&mut self, ui: &mut egui::Ui, id: &str) {
        let pal = self.pal;
        let Some(game) = self.lib.get(id).cloned() else {
            return;
        };
        let selected = self.selected.as_deref() == Some(id);
        let marked = self.is_marked(id);
        let running = self.is_running(id);
        let row_pal = pal.with_accent(self.game_accent(&game));
        let response = game_row(
            ui,
            &row_pal,
            &game,
            selected,
            marked,
            running,
            &mut self.textures,
        );
        let clicked = response.clicked();
        let double = response.double_clicked();
        let bulk = if marked && self.marked.len() > 1 {
            self.marked.len()
        } else {
            0
        };
        match game_context_menu(&response, bulk, &pal) {
            Some(RowAction::Launch) => self.launch(ui.ctx(), id),
            Some(RowAction::Customize) => {
                self.select(id);
                self.open_customize(id);
            }
            Some(RowAction::Settings) => {
                self.select(id);
                self.editing = Some(game.clone());
            }
            Some(RowAction::Remove) => self.request_remove(id),
            None => {
                if clicked {
                    if ui.input(|i| i.modifiers.command) {
                        self.toggle_marked(id);
                    } else {
                        self.select(id);
                    }
                }
                if double {
                    self.launch(ui.ctx(), id);
                }
            }
        }
    }

    /// Jeux affichés, répartis en sections par statut (ordre `COLUMNS`).
    fn status_groups(&self, ids: &[String]) -> Vec<(GameStatus, Vec<String>)> {
        let mut groups: Vec<(GameStatus, Vec<String>)> = GameStatus::COLUMNS
            .iter()
            .map(|status| (*status, Vec::new()))
            .collect();
        for id in ids {
            let status = self
                .lib
                .get(id)
                .map(|game| game.status)
                .unwrap_or(GameStatus::None);
            if let Some(slot) = groups
                .iter_mut()
                .find(|(candidate, _)| *candidate == status)
            {
                slot.1.push(id.clone());
            }
        }
        groups.retain(|(_, group)| !group.is_empty());
        groups
    }

    /// Filtres rapides de la colonne : statut puis tags personnels, comme les
    /// catégories Steam. Un second clic sur la pastille active retire le filtre.
    fn quick_filters(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let mut new_status: Option<GameStatus> = None;
        let mut new_tag: Option<String> = None;
        let status = self.lib.settings.home_status;
        let tag = self.lib.settings.home_tag.clone();
        let tags = self.lib.all_tags();

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(12.0);
            for option in GameStatus::ALL {
                let selected = status == option;
                let color = status_color(option)
                    .map(|(r, g, b)| Color32::from_rgb(r, g, b))
                    .unwrap_or(pal.surface_hover);
                if widgets::chip_colored(ui, option.label(), selected, &pal, color) {
                    new_status = Some(if selected { GameStatus::None } else { option });
                }
                ui.add_space(4.0);
            }
        });
        if !tags.is_empty() {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(12.0);
                for (name, count) in tags {
                    let selected = tag.eq_ignore_ascii_case(&name);
                    if widgets::chip(ui, &format!("{name} · {count}"), selected, &pal) {
                        new_tag = Some(if selected {
                            String::new()
                        } else {
                            name.clone()
                        });
                    }
                    ui.add_space(4.0);
                }
            });
        }

        if let Some(status) = new_status {
            self.lib.settings.home_status = status;
            self.touch();
        }
        if let Some(tag) = new_tag {
            self.lib.settings.home_tag = tag;
            self.touch();
        }
    }

    // -----------------------------------------------------------------------
    // Panneau du jeu (bannière, Jouer, temps de jeu, paramètres)
    // -----------------------------------------------------------------------

    fn detail_pane(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected.clone() else {
            self.empty_state(ui);
            return;
        };
        let Some(game) = self.lib.get(&id).cloned() else {
            self.selected = None;
            self.empty_state(ui);
            return;
        };
        // Le jeu impose sa couleur : bouton Jouer, liserés, ligne sélectionnée.
        let pal = self.pal.with_accent(self.game_accent(&game));

        let settings = self.lib.settings.clone();
        let resolved = proton::resolve(&game, &settings, &self.runtimes);
        let is_proton = resolved
            .as_ref()
            .map(|r| matches!(r.kind, LaunchKind::Proton | LaunchKind::Umu))
            .unwrap_or(false);
        // Binaire Linux : le jeu démarre sans Proton (mention sur la bannière).
        let native = resolved
            .as_ref()
            .map(|r| matches!(r.kind, LaunchKind::Native))
            .unwrap_or_else(|_| game.is_native());
        let launch_label = match &resolved {
            Ok(r) => r.label(),
            Err(err) => format!("\u{26a0} {err}"),
        };
        let prefix = game.prefix_path(&settings);
        let running_secs = self.running_since(&id);
        let is_running = self.is_running(&id);
        // Fondu d'entrée à chaque changement de jeu, comme la fiche Steam. La
        // transition ne demande des images que pendant son déroulement.
        let entrance = ui.ctx().animate_bool_with_time(
            egui::Id::new(("quark_detail_entrance", game.id.as_str())),
            true,
            0.22,
        );
        let slide = (1.0 - entrance) * 10.0;
        // Lien externe demandé par un clic (ProtonDB) : ouvert après le rendu,
        // pour ne pas lancer de navigateur au milieu de la mise en page.
        let mut open_url: Option<String> = None;

        // Note ProtonDB du jeu, telle qu'elle sera affichée.
        let protondb_label = match game
            .steam_app_id
            .and_then(|app_id| self.protondb.get(&app_id))
        {
            Some(crate::app::ProtonDbState::Ready(report)) => report.label(),
            Some(crate::app::ProtonDbState::Loading) => "chargement…".to_string(),
            Some(crate::app::ProtonDbState::Failed(err)) => err.clone(),
            None => "—".to_string(),
        };

        // Fond de la fiche : la bannière du jeu, floutée et assombrie, au lieu
        // d'un noir complet. Peinte avant la zone défilante, donc derrière.
        if let Some(hero) = game
            .artwork
            .get(AssetKind::Hero)
            .filter(|path| path.exists())
            .cloned()
        {
            if let Some(texture) = self.blurred_texture(ui.ctx(), &hero) {
                // Toute la zone du panneau, jusqu'en bas : sinon le fond flouté
                // s'arrêtait avant le bord et laissait une bande noire franche
                // sous les actualités. `max_rect` couvre le panneau entier (ou
                // la fiche en surimpression), le painter rogne le reste.
                // À partir du curseur : en surimpression, la barre de titre et
                // sa croix restent visibles au-dessus du fond flouté.
                let area = Rect::from_min_max(ui.cursor().min, ui.max_rect().max);
                let painter = ui.painter_at(area);
                draw_cover(&painter, &texture, area);
                // Voile dégradé (maillage à quatre sommets, donc dégradé lisse
                // sans bandes) : léger derrière la bannière, dense vers le bas
                // pour que les actualités se détachent. La bannière elle-même
                // n'est pas assombrie : elle est peinte par-dessus, sans fondu.
                let mut veil = egui::Mesh::default();
                veil.colored_vertex(area.left_top(), Color32::from_black_alpha(120));
                veil.colored_vertex(area.right_top(), Color32::from_black_alpha(120));
                veil.colored_vertex(area.left_bottom(), Color32::from_black_alpha(238));
                veil.colored_vertex(area.right_bottom(), Color32::from_black_alpha(238));
                veil.add_triangle(0, 1, 2);
                veil.add_triangle(1, 2, 3);
                painter.add(egui::Shape::mesh(veil));
            }
        }

        egui::ScrollArea::vertical()
            .id_salt("quark_detail_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = ui.available_width();
                let hero_height = theme::hero_height(width);
                let hero_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(width, hero_height));

                // La bannière est dessinée dans un enfant dont la zone couvre
                // exactement la bannière. C'est ce qui garantit que les
                // actualités et les informations démarrent *sous* la bannière :
                // sans cet enfant, les éléments positionnés en absolu dans la
                // bannière faisaient reculer le curseur et le contenu suivant
                // s'affichait par-dessus l'image.
                let mut play_clicked = false;
                let mut info_clicked = false;
                let mut gear_clicked = false;
                let mut customize_clicked = false;
                let mut log_clicked = false;
                let mut remove_clicked = false;
                let mut open_protondb = false;
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(hero_rect)
                        .layout(Layout::top_down(Align::Min)),
                    |ui| {
                        let (hero, _) = ui.allocate_exact_size(hero_rect.size(), Sense::hover());
                        let painter = ui.painter_at(hero);
                        let hero_path = game
                            .artwork
                            .get(AssetKind::Hero)
                            .filter(|p| p.exists())
                            .cloned();
                        // Bannière claire : les icônes doivent passer en sombre.
                        let hero_light = hero_path
                            .as_ref()
                            .map(|path| self.is_light_image(path))
                            .unwrap_or(false);
                        let hero_tex = hero_path
                            .as_ref()
                            .and_then(|path| self.texture(ui.ctx(), path));
                        if let Some(texture) = hero_tex {
                            // Recadrée, jamais étirée : quelle que soit la
                            // largeur de la fenêtre (plein écran compris), la
                            // bannière garde ses proportions et les bords qui
                            // dépassent sont rognés par le painter. La bannière
                            // garde son bas net : aucun fondu vers le noir.
                            draw_cover(&painter, &texture, hero);
                        } else {
                            // Sans bannière, un aplat teinté et la lettre du
                            // jeu : l'espace reste habité au lieu de ressembler
                            // à une image qui n'a pas chargé.
                            painter.rect_filled(hero, 0.0, pal.surface);
                            hero_placeholder(
                                &painter,
                                hero,
                                &game.monogram_letter(),
                                pal.accent,
                                0.0,
                            );
                        }

                        // Note ProtonDB et mention « Linux natif », en haut à
                        // gauche de la bannière.
                        let badge_rect = Rect::from_min_size(
                            Pos2::new(hero.left() + 32.0, hero.top() + 14.0),
                            Vec2::new((hero.width() - 240.0).max(120.0), 24.0),
                        );
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(badge_rect)
                                .layout(Layout::left_to_right(Align::Center)),
                            |ui| {
                                if game.exe_missing() {
                                    danger_chip(ui, &pal, "EXÉCUTABLE INTROUVABLE");
                                    ui.add_space(6.0);
                                }
                                if native {
                                    linux_chip(ui, &pal);
                                    ui.add_space(6.0);
                                }
                                let state = game
                                    .steam_app_id
                                    .and_then(|app_id| self.protondb.get(&app_id));
                                if protondb_badge(ui, &pal, state) {
                                    open_protondb = true;
                                }
                            },
                        );

                        // Logo (ou titre) au-dessus du bouton Jouer.
                        let row_height = 44.0;
                        let row_y = hero.bottom() - 26.0 - row_height;
                        let logo_rect = Rect::from_min_size(
                            Pos2::new(
                                hero.left() + 32.0,
                                (row_y - 14.0 - 86.0).max(hero.top() + 14.0) + slide,
                            ),
                            Vec2::new((width * 0.55).min(540.0), 86.0),
                        );
                        let logo_path = game
                            .artwork
                            .get(AssetKind::Logo)
                            .filter(|p| p.exists())
                            .cloned();
                        match logo_path {
                            Some(logo) => {
                                if let Some(texture) = self.texture(ui.ctx(), &logo) {
                                    draw_fit_left(ui, &texture, logo_rect);
                                }
                            }
                            None => {
                                painter.text(
                                    Pos2::new(hero.left() + 34.0, logo_rect.bottom()),
                                    Align2::LEFT_BOTTOM,
                                    &game.name,
                                    FontId::proportional(34.0),
                                    pal.text,
                                );
                            }
                        }

                        // Bouton Jouer + temps de jeu (comme Steam).
                        let (label, icon) = if is_running {
                            ("Arr\u{ea}ter", Icon::Stop)
                        } else {
                            ("Jouer", Icon::Play)
                        };
                        let play_area = Rect::from_min_size(
                            Pos2::new(hero.left() + 32.0, row_y),
                            Vec2::new((hero.width() - 64.0).max(120.0), row_height),
                        );
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(play_area)
                                .layout(Layout::left_to_right(Align::Center)),
                            |ui| {
                                play_clicked = widgets::accent_button(
                                    ui,
                                    Vec2::new(176.0, row_height),
                                    label,
                                    &pal,
                                    true,
                                    icon,
                                )
                                .clicked();
                                ui.add_space(20.0);
                                let text = if is_running {
                                    format!(
                                        "En jeu depuis {}",
                                        config::human_playtime(running_secs.max(1))
                                    )
                                } else {
                                    format!("{} de jeu", game.playtime_label())
                                };
                                ui.label(RichText::new(text).size(13.0).color(pal.text_dim));
                                if game.last_played.is_some() {
                                    ui.add_space(16.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "\u{b7}  jou\u{e9} {}",
                                            game.last_played_label()
                                        ))
                                        .size(12.5)
                                        .color(pal.text_faint),
                                    );
                                }
                                if game.status != GameStatus::None {
                                    ui.add_space(14.0);
                                    status_chip(ui, &pal, game.status);
                                }
                            },
                        );

                        // Actions de la fiche, de droite à gauche : engrenage
                        // bleu (configurer), crayon vert (personnaliser),
                        // poubelle rouge (retirer), puis le journal et les
                        // informations du jeu.
                        let action = |ui: &mut egui::Ui,
                                      slot: f32,
                                      icon: Icon,
                                      color: Color32,
                                      tip: &str| {
                            let rect = Rect::from_min_size(
                                Pos2::new(hero.right() - slot, hero.top() + 14.0),
                                Vec2::splat(34.0),
                            );
                            let mut clicked = false;
                            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                                clicked = widgets::tinted_icon_button(ui, 32.0, icon, color, tip)
                                    .clicked();
                            });
                            clicked
                        };
                        gear_clicked = action(
                            ui,
                            50.0,
                            Icon::Gear,
                            Color32::from_rgb(0x3F, 0x9B, 0xE8),
                            "Configurer ce jeu",
                        );
                        customize_clicked = action(
                            ui,
                            90.0,
                            Icon::Pencil,
                            Color32::from_rgb(0x4C, 0xB0, 0x6A),
                            "Personnaliser (artworks, notes, tags)",
                        );
                        remove_clicked = action(
                            ui,
                            130.0,
                            Icon::Trash,
                            Color32::from_rgb(0xD9, 0x41, 0x41),
                            "Retirer de la bibliothèque",
                        );
                        let log_rect = Rect::from_min_size(
                            Pos2::new(hero.right() - 178.0, hero.top() + 14.0),
                            Vec2::splat(34.0),
                        );
                        ui.scope_builder(egui::UiBuilder::new().max_rect(log_rect), |ui| {
                            log_clicked = widgets::floating_icon_button(
                                ui,
                                34.0,
                                Icon::Log,
                                &pal,
                                "Journal de la session",
                                hero_light,
                            )
                            .clicked();
                        });
                        let info_rect = Rect::from_min_size(
                            Pos2::new(hero.right() - 222.0, hero.top() + 14.0),
                            Vec2::splat(34.0),
                        );
                        ui.scope_builder(egui::UiBuilder::new().max_rect(info_rect), |ui| {
                            info_clicked = widgets::floating_icon_button(
                                ui,
                                34.0,
                                Icon::Info,
                                &pal,
                                "Informations du jeu",
                                hero_light,
                            )
                            .clicked();
                        });
                    },
                );

                if info_clicked {
                    self.show_info = !self.show_info;
                }
                if gear_clicked {
                    self.editing = Some(game.clone());
                }
                if customize_clicked {
                    self.open_customize(&id);
                }
                if log_clicked {
                    self.open_logs(&id);
                }
                if remove_clicked {
                    self.request_remove(&id);
                }
                if open_protondb {
                    if let Some(app_id) = game.steam_app_id {
                        open_url = Some(crate::protondb::Tier::url(app_id));
                    }
                }
                if play_clicked {
                    if is_running {
                        self.stop(&id);
                    } else {
                        self.launch(ui.ctx(), &id);
                    }
                }

                // -----------------------------------------------------------------
                // Actualités du jeu, juste sous la bannière (comme la fiche Steam)
                // -----------------------------------------------------------------
                self.news_section(ui, &game, &pal, width);

                // -----------------------------------------------------------------
                // Bloc d'informations (masqué par défaut, bouton \u{24d8})
                // -----------------------------------------------------------------
                if self.show_info {
                    ui.add_space(22.0);
                    ui.horizontal(|ui| {
                        ui.add_space(32.0);
                        ui.label(
                            RichText::new("INFORMATIONS")
                                .size(11.0)
                                .color(pal.text_faint),
                        );
                    });
                    ui.add_space(12.0);

                    let artworks_label = match (game.steam_app_id, game.sgdb_id) {
                        (Some(appid), Some(sgdb)) => {
                            format!("Steam AppID {appid} \u{b7} SteamGridDB #{sgdb}")
                        }
                        (Some(appid), None) => format!("Steam AppID {appid}"),
                        (None, Some(sgdb)) => format!("SteamGridDB #{sgdb}"),
                        (None, None) => "aucun (assets manuels)".to_string(),
                    };
                    let rows: Vec<(&str, String)> = vec![
                        ("Exécutable", game.exe.clone()),
                        ("Lancement", launch_label),
                        (
                            "Développeur",
                            if game.developer.is_empty() {
                                "—".to_string()
                            } else {
                                game.developer.clone()
                            },
                        ),
                        (
                            "Genres",
                            if game.genres.is_empty() {
                                "—".to_string()
                            } else {
                                game.genres.join(" · ")
                            },
                        ),
                        (
                            "Sortie",
                            if game.released.is_empty() {
                                "—".to_string()
                            } else {
                                game.released.clone()
                            },
                        ),
                        (
                            "Statut",
                            if game.status == GameStatus::None {
                                "aucun".to_string()
                            } else {
                                game.status.label().to_string()
                            },
                        ),
                        (
                            "Tags",
                            if game.tags.is_empty() {
                                "aucun".to_string()
                            } else {
                                game.tags.join(" · ")
                            },
                        ),
                        (
                            "Préfixe Proton",
                            if is_proton {
                                prefix.display().to_string()
                            } else {
                                "\u{2014} (pas de Proton pour ce jeu)".to_string()
                            },
                        ),
                        ("Artworks", artworks_label),
                        ("ProtonDB", protondb_label),
                        ("Temps de jeu", game.playtime_label()),
                        ("Dernière session", game.last_played_label()),
                    ];

                    for (label, value) in &rows {
                        ui.horizontal(|ui| {
                            ui.add_space(32.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((width - 64.0).max(200.0));
                                ui.label(RichText::new(*label).size(11.5).color(pal.text_dim));
                                ui.label(RichText::new(value).size(13.0).color(pal.text));
                                ui.add_space(8.0);
                            });
                        });
                    }

                    if !game.description.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(32.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((width - 64.0).max(200.0));
                                ui.label(
                                    RichText::new(&game.description)
                                        .size(12.5)
                                        .color(pal.text_dim),
                                );
                                ui.add_space(8.0);
                            });
                        });
                    }
                    if !game.notes.is_empty() {
                        ui.horizontal(|ui| {
                            ui.add_space(32.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((width - 64.0).max(200.0));
                                ui.label(RichText::new("NOTES").size(11.0).color(pal.text_faint));
                                ui.label(RichText::new(&game.notes).size(12.5).color(pal.text_dim));
                                ui.add_space(8.0);
                            });
                        });
                    }
                    if let Some(app_id) = game.steam_app_id {
                        ui.horizontal(|ui| {
                            ui.add_space(32.0);
                            if widgets::ghost_button(ui, "Page ProtonDB", &pal, Icon::Info)
                                .clicked()
                            {
                                open_url = Some(crate::protondb::Tier::url(app_id));
                            }
                        });
                        ui.add_space(6.0);
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(32.0);
                        if widgets::tinted_icon_button(
                            ui,
                            28.0,
                            Icon::Gear,
                            Color32::from_rgb(0x3F, 0x9B, 0xE8),
                            "Configurer ce jeu",
                        )
                        .clicked()
                        {
                            self.editing = Some(game.clone());
                        }
                        ui.add_space(4.0);
                        if widgets::tinted_icon_button(
                            ui,
                            28.0,
                            Icon::Pencil,
                            Color32::from_rgb(0x4C, 0xB0, 0x6A),
                            "Personnaliser (artworks, notes, tags)",
                        )
                        .clicked()
                        {
                            self.open_customize(&id);
                        }
                        ui.add_space(4.0);
                        if widgets::tinted_icon_button(
                            ui,
                            28.0,
                            Icon::Trash,
                            Color32::from_rgb(0xD9, 0x41, 0x41),
                            "Retirer de la bibliothèque",
                        )
                        .clicked()
                        {
                            self.request_remove(&id);
                        }
                        ui.add_space(10.0);
                        if widgets::ghost_button(ui, "Paramètres", &pal, Icon::None).clicked() {
                            self.editing = Some(game.clone());
                        }
                        if game.steam_app_id.is_some() || game.sgdb_id.is_some() {
                            if widgets::ghost_button(
                                ui,
                                "Rafraîchir les artworks",
                                &pal,
                                Icon::Refresh,
                            )
                            .clicked()
                            {
                                let hit = crate::artwork::SearchHit::from_game(&game);
                                self.start_asset_job(ui.ctx(), &id, hit);
                            }
                        }
                    });
                }

                // Voile de fondu pendant l'entrée du jeu : rien à faire au repos.
                if entrance < 1.0 {
                    ui.painter().rect_filled(
                        ui.clip_rect(),
                        0.0,
                        Color32::from_black_alpha(((1.0 - entrance) * 255.0).round() as u8),
                    );
                }

                ui.add_space(40.0);
            });

        if let Some(url) = open_url {
            if let Err(err) = news::open_in_browser(&url) {
                self.toast_error(format!("Ouverture du lien impossible : {err}"));
            }
        }
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        ui.vertical_centered(|ui| {
            ui.add_space(140.0);
            ui.label(
                RichText::new("Aucun jeu sélectionné")
                    .size(18.0)
                    .color(pal.text_dim),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Choisis un jeu dans la liste, ou ajoute-en un nouveau.")
                    .size(12.5)
                    .color(pal.text_faint),
            );
            ui.add_space(18.0);
            if widgets::accent_button(
                ui,
                Vec2::new(190.0, 40.0),
                "Ajouter un jeu",
                &pal,
                true,
                Icon::Plus,
            )
            .clicked()
            {
                self.open_add_dialog();
            }
        });
    }

    /// Actualités du jeu (flux public Steam), affichées sous la bannière.
    ///
    /// Un clic déplie le billet sur place : le texte et son image sont déjà
    /// téléchargés, l'article se lit donc dans Quark au lieu d'ouvrir un
    /// navigateur. Le lien Steam reste disponible en un bouton.
    fn news_section(&mut self, ui: &mut egui::Ui, game: &Game, pal: &Palette, width: f32) {
        let (loading, failure, items) = match self.news.get(&game.id) {
            Some(NewsState::Loading) => (true, None, Vec::new()),
            Some(NewsState::Failed(message)) => (false, Some(message.clone()), Vec::new()),
            Some(NewsState::Loaded(items)) => (false, None, items.clone()),
            None => (false, None, Vec::new()),
        };
        let linked = game.steam_app_id.is_some();
        let mut reload = false;
        let mut open: Option<String> = None;
        let mut expanded_change: Option<(egui::Id, bool)> = None;

        ui.add_space(26.0);
        ui.horizontal(|ui| {
            ui.add_space(32.0);
            ui.label(RichText::new("ACTUALITÉS").size(11.0).color(pal.text_faint));
            if linked {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(32.0);
                    let label = if failure.is_some() {
                        "Réessayer"
                    } else {
                        "Actualiser"
                    };
                    if widgets::ghost_button(ui, label, pal, Icon::Refresh).clicked() {
                        reload = true;
                    }
                });
            }
        });
        ui.add_space(10.0);

        if !linked {
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(
                    RichText::new(
                        "Ce jeu n'est pas lié à la fiche Steam : pas d'actualités à afficher.",
                    )
                    .size(12.0)
                    .color(pal.text_faint),
                );
            });
            return;
        }

        if loading {
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.spinner();
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Chargement des actualités\u{2026}")
                        .size(12.0)
                        .color(pal.text_dim),
                );
            });
            return;
        }

        if let Some(message) = failure {
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(
                    RichText::new(format!("Actualités indisponibles \u{b7} {message}"))
                        .size(12.0)
                        .color(pal.danger),
                );
            });
            return;
        }

        if items.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(
                    RichText::new("Aucune actualité pour ce jeu.")
                        .size(12.0)
                        .color(pal.text_faint),
                );
            });
            return;
        }

        let card_width = (width - 64.0).max(220.0);
        for (index, item) in items.iter().enumerate() {
            let id = ui.make_persistent_id(("quark_news", index, item.url.as_str()));
            // L'état de survol est lu du tour précédent (la carte n'est connue
            // qu'après sa mise en page), mais c'est l'animation qui lisse le
            // changement : le décalage d'une image reste invisible.
            let was_hovered = ui.data(|data| data.get_temp::<bool>(id).unwrap_or(false));
            let expanded = ui.data(|data| data.get_temp::<bool>(id.with("open")).unwrap_or(false));
            let anim = ui
                .ctx()
                .animate_bool_with_time(id.with("hover"), was_hovered, 0.14);

            ui.horizontal(|ui| {
                ui.add_space(32.0);
                let frame = egui::Frame::new()
                    .fill(theme::lerp_color(
                        Color32::TRANSPARENT,
                        pal.surface_hover,
                        anim,
                    ))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(12, 10));
                // Le `Frame` hérite du layout horizontal de la rangée : sans ce
                // `vertical`, les lignes se retrouvaient côte à côte.
                let inner = frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(card_width);
                        ui.horizontal_top(|ui| {
                            let text_width = card_width;
                            ui.vertical(|ui| {
                                ui.set_width(text_width);
                                let mut meta = vec![item.feed.clone()];
                                if !item.author.is_empty() {
                                    meta.push(item.author.clone());
                                }
                                meta.push(config::human_ago(item.date));
                                ui.label(
                                    RichText::new(meta.join("  \u{b7}  "))
                                        .size(11.0)
                                        .color(pal.text_faint),
                                );
                                ui.add_space(4.0);
                                ui.label(single_line(&item.title, 14.5, pal.text, text_width));
                                if !expanded && !item.excerpt.is_empty() {
                                    ui.add_space(4.0);
                                    ui.label(paragraph(&item.excerpt, 12.0, pal.text_dim, 2));
                                }
                            });
                        });
                        if expanded {
                            if !item.body.is_empty() {
                                ui.add_space(8.0);
                                // `usize::MAX` = aucune limite de lignes : le
                                // texte entier, dans l'application. (`0`
                                // tronquerait à zéro ligne, donc à rien.)
                                ui.label(paragraph(&item.body, 12.5, pal.text_dim, usize::MAX));
                            }
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if widgets::ghost_button(ui, "Réduire", pal, Icon::None).clicked()
                                {
                                    expanded_change = Some((id.with("open"), false));
                                }
                                ui.add_space(6.0);
                                if widgets::ghost_button(ui, "Lire sur Steam", pal, Icon::None)
                                    .clicked()
                                {
                                    open = Some(item.url.clone());
                                }
                            });
                        }
                    });
                });

                // Liseré d'accent qui s'allume au survol.
                if anim > 0.0 {
                    let rect = inner.response.rect;
                    ui.painter().rect_filled(
                        Rect::from_min_size(rect.left_top(), Vec2::new(2.0, rect.height())),
                        1.0,
                        pal.accent.gamma_multiply(anim),
                    );
                }

                // Déplié, la carte ne se referme plus au clic : le texte et les
                // boutons restent utilisables.
                let response = if expanded {
                    ui.interact(inner.response.rect, id, Sense::click())
                } else {
                    ui.interact(inner.response.rect, id, Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                };
                let hovered = response.hovered();
                if hovered != was_hovered {
                    ui.data_mut(|data| data.insert_temp(id, hovered));
                    // Une seule image de plus : l'animation prend le relais.
                    ui.ctx().request_repaint();
                }
                if !expanded && response.clicked() {
                    expanded_change = Some((id.with("open"), true));
                }
            });
            ui.add_space(4.0);
        }

        if let Some((id, value)) = expanded_change {
            ui.data_mut(|data| data.insert_temp(id, value));
            ui.ctx().request_repaint();
        }

        if reload {
            self.reload_news(ui.ctx());
        }
        if let Some(url) = open {
            if let Err(err) = news::open_in_browser(&url) {
                self.toast_error(format!("Ouverture du lien impossible : {err}"));
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RowAction {
    Launch,
    Customize,
    Settings,
    Remove,
}

/// Menu du clic droit sur un jeu (ligne ou tuile). Renvoie l'action choisie.
///
/// Le menu ne touche à `self` que par la valeur renvoyée : il reste utilisable
/// pendant que la boucle qui dessine la liste emprunte les textures.
fn game_context_menu(response: &egui::Response, bulk: usize, pal: &Palette) -> Option<RowAction> {
    let mut action = None;
    response.context_menu(|ui| {
        ui.set_min_width(212.0);
        // Le menu garde le fond opaque du thème : les icônes colorées restent
        // lisibles même quand il s'ouvre sur une bannière claire.
        if widgets::menu_row(ui, pal, Icon::Play, pal.accent, "Lancer") {
            action = Some(RowAction::Launch);
            ui.close();
        }
        if widgets::menu_row(
            ui,
            pal,
            Icon::Pencil,
            Color32::from_rgb(0x4C, 0xB0, 0x6A),
            "Personnaliser…",
        ) {
            action = Some(RowAction::Customize);
            ui.close();
        }
        if widgets::menu_row(
            ui,
            pal,
            Icon::Gear,
            Color32::from_rgb(0x3F, 0x9B, 0xE8),
            "Paramètres…",
        ) {
            action = Some(RowAction::Settings);
            ui.close();
        }
        ui.add_space(2.0);
        ui.separator();
        let label = if bulk > 1 {
            format!("Retirer la sélection ({bulk} jeux)")
        } else {
            "Retirer de la bibliothèque".to_string()
        };
        if widgets::menu_row(
            ui,
            pal,
            Icon::Trash,
            Color32::from_rgb(0xD9, 0x41, 0x41),
            &label,
        ) {
            action = Some(RowAction::Remove);
            ui.close();
        }
    });
    action
}

/// Titre de section de la vue liste (« En cours », « À jouer »…), avec sa
/// pastille de couleur et le nombre de jeux, comme les colonnes Steam.
///
/// Renvoie vrai si la section est dépliée ; un clic sur le titre la replie, et
/// l'état est mémorisé pour toute la session. Un filet sépare les sections.
fn status_section(
    ui: &mut egui::Ui,
    pal: &Palette,
    status: GameStatus,
    count: usize,
    first: bool,
) -> bool {
    let state_id = egui::Id::new(("quark_status_open", status.label()));
    let open = ui.data(|data| data.get_temp::<bool>(state_id).unwrap_or(true));
    ui.add_space(if first { 4.0 } else { 12.0 });
    if !first {
        let (line, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(line, 0.0, pal.border);
        ui.add_space(6.0);
    }
    let mut toggled = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(12.0);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new((ui.available_width() - 12.0).max(80.0), 24.0),
            Sense::click(),
        );
        let painter = ui.painter_at(rect);
        let hover =
            ui.ctx()
                .animate_bool_with_time(state_id.with("hover"), response.hovered(), 0.12);
        if hover > 0.0 {
            painter.rect_filled(rect, 4.0, pal.surface_hover.gamma_multiply(hover));
        }
        // Chevron : vers le bas quand la section est ouverte, vers la droite
        // quand elle est repliée.
        let c = Pos2::new(rect.left() + 9.0, rect.center().y);
        let points = if open {
            vec![
                Pos2::new(c.x - 4.0, c.y - 2.0),
                Pos2::new(c.x + 4.0, c.y - 2.0),
                Pos2::new(c.x, c.y + 3.0),
            ]
        } else {
            vec![
                Pos2::new(c.x - 2.0, c.y - 4.0),
                Pos2::new(c.x + 3.0, c.y),
                Pos2::new(c.x - 2.0, c.y + 4.0),
            ]
        };
        painter.add(egui::Shape::convex_polygon(
            points,
            pal.text_faint,
            egui::Stroke::NONE,
        ));
        if let Some((r, g, b)) = status_color(status) {
            painter.circle_filled(
                Pos2::new(rect.left() + 22.0, rect.center().y),
                3.5,
                Color32::from_rgb(r, g, b),
            );
        }
        painter.text(
            Pos2::new(
                rect.left()
                    + if status == GameStatus::None {
                        22.0
                    } else {
                        32.0
                    },
                rect.center().y,
            ),
            Align2::LEFT_CENTER,
            status.column_label().to_uppercase(),
            FontId::proportional(10.5),
            pal.text_dim,
        );
        painter.text(
            Pos2::new(rect.right() - 6.0, rect.center().y),
            Align2::RIGHT_CENTER,
            count.to_string(),
            FontId::proportional(10.5),
            pal.text_faint,
        );
        if response.clicked() {
            toggled = Some(!open);
        }
    });
    if let Some(next) = toggled {
        ui.data_mut(|data| data.insert_temp(state_id, next));
        ui.ctx().request_repaint();
        return next;
    }
    open
}

// ---------------------------------------------------------------------------
// Lignes / tuiles
// ---------------------------------------------------------------------------

/// Ligne de la vue liste : vignette à gauche, titre à côté (comme Steam).
fn game_row(
    ui: &mut egui::Ui,
    pal: &Palette,
    game: &Game,
    selected: bool,
    marked: bool,
    running: bool,
    textures: &mut crate::images::TextureCache,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
    let painter = ui.painter_at(rect);
    let id = egui::Id::new(("quark_row", game.id.as_str()));
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), response.hovered(), 0.12);
    let mark = ui
        .ctx()
        .animate_bool_with_time(id.with("selected"), selected || marked, 0.16);
    // Exécutable disparu : la ligne passe en rouge, comme un avertissement.
    let missing = game.exe_missing();

    if hover > 0.0 {
        painter.rect_filled(
            rect,
            4.0,
            theme::lerp_color(Color32::TRANSPARENT, pal.surface_hover, hover),
        );
    }
    if missing {
        painter.rect_filled(
            rect,
            4.0,
            Color32::from_rgba_unmultiplied(0xD9, 0x41, 0x41, 26),
        );
        painter.rect_filled(
            Rect::from_min_size(rect.left_top(), Vec2::new(3.0, rect.height())),
            1.5,
            pal.danger,
        );
    }
    if mark > 0.0 {
        painter.rect_filled(rect, 4.0, pal.row_selected.gamma_multiply(mark));
        // Le trait d'accent grandit depuis le centre de la ligne.
        let height = (rect.height() - 10.0) * mark;
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.center().y - height / 2.0),
                Vec2::new(3.0, height),
            ),
            1.5,
            pal.accent.gamma_multiply(mark),
        );
    }

    // Vignette carrée (1:1) : l'icône du jeu, jamais le logo transparent qui
    // reste réservé à la bannière. Sans icône, la première lettre du titre
    // prend sa place dans un carré identique, donc rien ne bouge d'une ligne à
    // l'autre.
    let thumb_box = Rect::from_min_size(
        Pos2::new(rect.left() + 16.0, rect.center().y - 18.0),
        Vec2::splat(36.0),
    );
    let thumb = game
        .artwork
        .get(AssetKind::Icon)
        .filter(|p| p.exists())
        .cloned();
    let texture = thumb.as_ref().and_then(|path| textures.get(ui.ctx(), path));
    match texture {
        // Icônes 1:1 et capsules larges occupent le même carré : l'image est
        // recadrée au centre (jamais comprimée), grâce au painter limité au
        // carré de la vignette.
        Some(texture) => rounded_cover(ui, thumb_box, texture, 7.0),
        None => widgets::monogram(
            &painter,
            thumb_box,
            &game.monogram_letter(),
            theme::monogram_color(&game.name),
            pal,
        ),
    }

    // Titre sur une seule ligne, coupé par une ellipsis : les noms longs ne
    // débordent plus de la colonne.
    let title_x = rect.left() + 66.0;
    let title_color = if missing {
        pal.danger
    } else {
        theme::lerp_color(pal.text_dim, pal.text, mark.max(hover * 0.7))
    };
    let job = single_line(
        &game.name,
        13.5,
        title_color,
        (rect.right() - 14.0 - title_x).max(24.0),
    );
    let galley = painter.layout_job(job);
    painter.galley(
        Pos2::new(title_x, rect.center().y - galley.size().y / 2.0),
        galley,
        title_color,
    );

    if let Some((r, g, b)) = status_color(game.status) {
        painter.circle_filled(
            Pos2::new(rect.right() - 34.0, rect.center().y),
            3.5,
            Color32::from_rgb(r, g, b),
        );
    }
    if running {
        painter.circle_filled(
            Pos2::new(rect.right() - 18.0, rect.center().y),
            4.0,
            pal.accent,
        );
    }

    let response = if missing {
        response.on_hover_text(format!(
            "{} · exécutable introuvable : {}",
            game.name,
            if game.exe.trim().is_empty() {
                "aucun chemin défini".to_string()
            } else {
                game.exe.clone()
            }
        ))
    } else {
        response
    };
    response
}

/// Tuile de la vue grille (couverture verticale 2:3).
fn game_tile(
    ui: &mut egui::Ui,
    pal: &Palette,
    game: &Game,
    size: Vec2,
    selected: bool,
    marked: bool,
    running: bool,
    // Progression de l'apparition de la page (0 → 1), décalée par tuile.
    appear: f32,
    textures: &mut crate::images::TextureCache,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    // Les tuiles montent de quelques pixels en apparaissant ; la place réservée
    // ne bouge pas, seule la peinture est décalée.
    let rect = rect.translate(Vec2::new(0.0, (1.0 - appear) * 9.0));
    let mut painter = ui.painter_at(rect);
    painter.set_opacity(appear);
    let id = egui::Id::new(("quark_tile", game.id.as_str()));
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), response.hovered(), 0.12);
    let mark = ui
        .ctx()
        .animate_bool_with_time(id.with("selected"), selected || marked, 0.16);

    // Le survol « soulève » la tuile : la couverture se resserre d'un ou deux
    // pixels et un liseré s'allume. Tout est peint, la grille ne bouge pas.
    let cover_rect = rect.shrink(2.0 * hover);
    painter.rect_filled(rect, 6.0, pal.surface);
    let cover = game
        .artwork
        .get(AssetKind::Cover)
        .filter(|p| p.exists())
        .cloned();
    let texture = cover.as_ref().and_then(|path| textures.get(ui.ctx(), path));
    match texture {
        // Couverture recadrée au centre quand le format diffère (jamais
        // déformée), grâce au painter limité à la tuile.
        Some(texture) => rounded_cover_tinted(
            ui,
            cover_rect,
            texture,
            8.0,
            Color32::from_white_alpha((appear * 255.0).round() as u8),
        ),
        None => widgets::monogram(
            &painter,
            cover_rect,
            &game.monogram_letter(),
            theme::monogram_color(&game.name),
            pal,
        ),
    }

    let accent = mark.max(hover);
    if accent > 0.0 {
        painter.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new((2.0 + mark).min(3.0), pal.accent.gamma_multiply(accent)),
            egui::StrokeKind::Inside,
        );
    }
    if game.exe_missing() {
        painter.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(2.0_f32, pal.danger),
            egui::StrokeKind::Inside,
        );
    }
    if running {
        let badge = Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.top() + 6.0),
            Vec2::new(34.0, 18.0),
        );
        // La pastille respire tant qu'un jeu tourne : c'est le seul état
        // animé en continu, et il ne dure que le temps de la partie.
        let time = ui.input(|i| i.time) as f32;
        let pulse = 0.62 + 0.38 * (time * 2.6).sin().abs();
        ui.ctx().request_repaint();
        painter.rect_filled(badge, 3.0, pal.accent.gamma_multiply(0.55 + 0.25 * pulse));
        painter.text(
            badge.center(),
            Align2::CENTER_CENTER,
            "EN JEU",
            FontId::proportional(9.5),
            eframe::egui::Color32::BLACK,
        );
    }

    response.on_hover_text(format!("{} · clic droit : paramètres", game.name))
}

/// Couleur d'une pastille de statut (`None` : pas de pastille).
pub(crate) fn status_color(status: GameStatus) -> Option<(u8, u8, u8)> {
    if status == GameStatus::None {
        None
    } else {
        Some(status.color())
    }
}

/// Pastille de statut (À jouer, En cours, Terminé, Abandonné).
pub(crate) fn status_chip(ui: &mut egui::Ui, pal: &Palette, status: GameStatus) {
    let Some((r, g, b)) = status_color(status) else {
        return;
    };
    let color = Color32::from_rgb(r, g, b);
    ui.add(
        egui::Button::new(
            RichText::new(status.label())
                .size(10.5)
                .strong()
                .color(theme::on_accent(color)),
        )
        .fill(color)
        .stroke(egui::Stroke::NONE)
        .corner_radius(9.0)
        .min_size(Vec2::new(0.0, 20.0)),
    )
    .on_hover_text("Statut personnel du jeu (modifiable dans ses paramètres)");
    let _ = pal;
}

/// Pastille d'alerte rouge (exécutable introuvable).
pub(crate) fn danger_chip(ui: &mut egui::Ui, pal: &Palette, label: &str) {
    ui.add(
        egui::Button::new(
            RichText::new(label)
                .size(10.0)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(pal.danger)
        .stroke(egui::Stroke::NONE)
        .corner_radius(9.0)
        .min_size(Vec2::new(0.0, 20.0)),
    )
    .on_hover_text(
        "Le fichier n'existe plus à ce chemin : corrige l'exécutable dans les paramètres du jeu.",
    );
}

/// Texte sur une seule ligne, coupé par une ellipsis s'il dépasse `max_width`.
pub(crate) fn single_line(
    text: &str,
    size: f32,
    color: Color32,
    max_width: f32,
) -> egui::text::LayoutJob {
    let mut job = paragraph(text, size, color, 1);
    job.wrap.max_width = max_width;
    job
}

/// Paragraphe limité à `max_rows` lignes, la dernière terminée par « \u{2026} ».
fn paragraph(text: &str, size: f32, color: Color32, max_rows: usize) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_rows = max_rows;
    job.wrap.overflow_character = Some('\u{2026}');
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: FontId::proportional(size),
            color,
            ..Default::default()
        },
    );
    job
}

/// Pastille de la note ProtonDB : couleur officielle, texte qui la contraste.
pub(crate) fn protondb_badge(
    ui: &mut egui::Ui,
    pal: &Palette,
    state: Option<&crate::app::ProtonDbState>,
) -> bool {
    use crate::app::ProtonDbState;
    let (label, color, tip) = match state {
        Some(ProtonDbState::Loading) => (
            "ProtonDB…".to_string(),
            pal.surface_hover,
            "Note ProtonDB en cours de chargement".to_string(),
        ),
        Some(ProtonDbState::Failed(err)) => (
            "ProtonDB · ?".to_string(),
            pal.surface_hover,
            format!("{err} · cliquer pour ouvrir la page"),
        ),
        Some(ProtonDbState::Ready(report)) => {
            let (r, g, b) = report.tier.color();
            (
                format!("{} · ProtonDB", report.tier.label().to_uppercase()),
                Color32::from_rgb(r, g, b),
                report.label(),
            )
        }
        None => return false,
    };
    let ink = theme::on_accent(color);
    ui.add(
        egui::Button::new(RichText::new(label).size(10.5).strong().color(ink))
            .fill(color)
            .stroke(egui::Stroke::NONE)
            .corner_radius(10.0)
            .min_size(Vec2::new(0.0, 20.0)),
    )
    .on_hover_text(tip)
    .clicked()
}
