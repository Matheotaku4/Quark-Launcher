//! Onglet Bibliothèque : colonne de gauche (liste ou grille) + panneau du jeu.

use crate::app::QuarkApp;
use crate::config::{self, AssetKind, Game, ViewMode, file_uri};
use crate::proton::{self, LaunchKind};
use crate::theme::{self, Palette};
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Align2, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2};

impl QuarkApp {
    pub(crate) fn library_tab(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let view = self.lib.settings.view;
        let sidebar_width = match view {
            ViewMode::List => 292.0,
            ViewMode::Grid => 468.0,
        };

        egui::SidePanel::left("quark_library_list")
            .exact_width(sidebar_width)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(pal.panel)
                    .stroke(egui::Stroke::new(1.0_f32, pal.border)),
            )
            .show(ctx, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg))
            .show(ctx, |ui| self.detail_pane(ui));
    }

    // -----------------------------------------------------------------------
    // Colonne de gauche
    // -----------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let ids = self.display_ids();
        let view = self.lib.settings.view;
        let total = self.lib.games.len();

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(RichText::new("BIBLIOTHÈQUE").size(11.0).color(pal.text_dim));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                if widgets::icon_button(ui, 26.0, Icon::Plus, &pal, "Ajouter un jeu").clicked() {
                    self.open_add_dialog();
                }
                ui.add_space(8.0);
                if widgets::icon_button(ui, 26.0, Icon::Grid, &pal, "Affichage en grille").clicked() {
                    self.set_view(ViewMode::Grid);
                }
                if widgets::icon_button(ui, 26.0, Icon::List, &pal, "Affichage en liste (Steam)").clicked() {
                    self.set_view(ViewMode::List);
                }
            });
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let count = if ids.len() == total {
                format!("{total} jeu{}", if total > 1 { "x" } else { "" })
            } else {
                format!("{} / {total} jeux", ids.len())
            };
            ui.label(RichText::new(count).size(11.0).color(pal.text_faint));
        });
        ui.add_space(8.0);

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
                        for id in &ids {
                            let Some(game) = self.lib.get(id).cloned() else {
                                continue;
                            };
                            let selected = self.selected.as_deref() == Some(id.as_str());
                            let running = self.is_running(id);
                            let response = game_row(ui, &pal, &game, selected, running);
                            if response.clicked() {
                                self.select(id);
                            }
                            if response.double_clicked() {
                                self.launch(ui.ctx(), id);
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
                        let tile_w = ((available - gap * (columns as f32 - 1.0)) / columns as f32).max(80.0);
                        let tile_h = tile_w * theme::COVER_RATIO;

                        let mut index = 0usize;
                        while index < ids.len() {
                            ui.horizontal(|ui| {
                                ui.add_space(padding);
                                for slot in 0..columns {
                                    let Some(id) = ids.get(index + slot) else {
                                        break;
                                    };
                                    let Some(game) = self.lib.get(id).cloned() else {
                                        continue;
                                    };
                                    let selected = self.selected.as_deref() == Some(id.as_str());
                                    let running = self.is_running(id);
                                    let response = game_tile(
                                        ui,
                                        &pal,
                                        &game,
                                        Vec2::new(tile_w, tile_h),
                                        selected,
                                        running,
                                    );
                                    if response.clicked() {
                                        self.select(id);
                                    }
                                    if response.double_clicked() {
                                        self.launch(ui.ctx(), id);
                                    }
                                    ui.add_space(gap);
                                }
                            });
                            ui.add_space(gap);
                            index += columns;
                        }
                        ui.add_space(10.0);
                    }
                }
            });

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

    // -----------------------------------------------------------------------
    // Panneau du jeu (bannière, Jouer, temps de jeu, paramètres)
    // -----------------------------------------------------------------------

    fn detail_pane(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let Some(id) = self.selected.clone() else {
            self.empty_state(ui);
            return;
        };
        let Some(game) = self.lib.get(&id).cloned() else {
            self.selected = None;
            self.empty_state(ui);
            return;
        };

        let settings = self.lib.settings.clone();
        let resolved = proton::resolve(&game, &settings, &self.runtimes);
        let is_proton = resolved
            .as_ref()
            .map(|r| r.kind == LaunchKind::Proton)
            .unwrap_or(false);
        let launch_label = match &resolved {
            Ok(r) => r.label(),
            Err(err) => format!("⚠ {err}"),
        };
        let prefix = game.prefix_path(&settings);
        let running_secs = self.running_since(&id);
        let is_running = self.is_running(&id);

        egui::ScrollArea::vertical()
            .id_salt("quark_detail_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = ui.available_width();
                let hero_height = theme::hero_height(width);
                let (hero_rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, hero_height), Sense::hover());
                let painter = ui.painter_at(hero_rect);

                painter.rect_filled(hero_rect, 0.0, pal.surface);
                let hero_path = game
                    .artwork
                    .get(AssetKind::Hero)
                    .filter(|p| p.exists())
                    .cloned();
                if let Some(hero) = hero_path {
                    ui.put(
                        hero_rect,
                        egui::Image::new(file_uri(&hero)).fit_to_exact_size(hero_rect.size()),
                    );
                } else {
                    painter.rect_filled(hero_rect, 0.0, pal.surface_hover);
                }
                theme::bottom_gradient(&painter, hero_rect, if hero_rect.height() > 0.0 { 0.62 } else { 1.0 });

                // Logo (ou titre) au-dessus du bouton Jouer.
                let row_height = 44.0;
                let row_y = hero_rect.bottom() - 26.0 - row_height;
                let logo_rect = Rect::from_min_size(
                    Pos2::new(hero_rect.left() + 32.0, (row_y - 14.0 - 86.0).max(hero_rect.top() + 14.0)),
                    Vec2::new((width * 0.55).min(540.0), 86.0),
                );
                let logo_path = game
                    .artwork
                    .get(AssetKind::Logo)
                    .filter(|p| p.exists())
                    .cloned();
                match logo_path {
                    Some(logo) => {
                        ui.put(
                            logo_rect,
                            egui::Image::new(file_uri(&logo))
                                .max_size(logo_rect.size())
                                .maintain_aspect_ratio(true),
                        );
                    }
                    None => {
                        painter.text(
                            Pos2::new(hero_rect.left() + 34.0, logo_rect.bottom()),
                            Align2::LEFT_BOTTOM,
                            &game.name,
                            FontId::proportional(34.0),
                            pal.text,
                        );
                    }
                }

                // Bouton Jouer + temps de jeu (comme Steam).
                let (label, icon) = if is_running {
                    ("Arrêter", Icon::Stop)
                } else {
                    ("Jouer", Icon::Play)
                };
                let play_area = Rect::from_min_size(
                    Pos2::new(hero_rect.left() + 32.0, row_y),
                    Vec2::new((hero_rect.width() - 64.0).max(120.0), row_height),
                );
                let mut play_clicked = false;
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(play_area)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        let response =
                            widgets::accent_button(ui, Vec2::new(176.0, row_height), label, &pal, true, icon);
                        play_clicked = response.clicked();
                        ui.add_space(20.0);
                        let text = if is_running {
                            format!("En jeu depuis {}", config::human_playtime(running_secs.max(1)))
                        } else {
                            format!("{} de jeu", game.playtime_label())
                        };
                        ui.label(RichText::new(text).size(13.0).color(pal.text_dim));
                    },
                );
                if play_clicked {
                    if is_running {
                        self.stop(&id);
                    } else {
                        self.launch(ui.ctx(), &id);
                    }
                }

                // Infos (ⓘ) puis engrenage, à droite de la bannière (comme Steam).
                let info_rect = Rect::from_min_size(
                    Pos2::new(hero_rect.right() - 92.0, hero_rect.top() + 14.0),
                    Vec2::splat(34.0),
                );
                let mut info_clicked = false;
                ui.scope_builder(egui::UiBuilder::new().max_rect(info_rect), |ui| {
                    info_clicked =
                        widgets::icon_button(ui, 34.0, Icon::Info, &pal, "Informations du jeu")
                            .clicked();
                });
                if info_clicked {
                    self.show_info = !self.show_info;
                }

                // Engrenage : paramètres du jeu, à droite (comme Steam).
                let gear_rect = Rect::from_min_size(
                    Pos2::new(hero_rect.right() - 50.0, hero_rect.top() + 14.0),
                    Vec2::splat(34.0),
                );
                let mut gear_clicked = false;
                ui.scope_builder(egui::UiBuilder::new().max_rect(gear_rect), |ui| {
                    gear_clicked =
                        widgets::icon_button(ui, 34.0, Icon::Gear, &pal, "Paramètres du jeu").clicked();
                });
                if gear_clicked {
                    self.editing = Some(game.clone());
                }

                // -----------------------------------------------------------------
                // Bloc d'informations (masqué par défaut, bouton ⓘ)
                if self.show_info {
                    // Bloc d'informations
                    // -----------------------------------------------------------------
                    ui.add_space(22.0);
                    ui.horizontal(|ui| {
                        ui.add_space(32.0);
                        ui.label(RichText::new("INFORMATIONS").size(11.0).color(pal.text_faint));
                    });
                    ui.add_space(12.0);

                    let artworks_label = match (game.steam_app_id, game.sgdb_id) {
                        (Some(appid), Some(sgdb)) => format!("Steam AppID {appid} · SteamGridDB #{sgdb}"),
                        (Some(appid), None) => format!("Steam AppID {appid}"),
                        (None, Some(sgdb)) => format!("SteamGridDB #{sgdb}"),
                        (None, None) => "aucun (assets manuels)".to_string(),
                    };
                    let rows: Vec<(&str, String)> = vec![
                        ("Exécutable", game.exe.clone()),
                        ("Lancement", launch_label),
                        (
                            "Préfixe Proton",
                            if is_proton {
                                prefix.display().to_string()
                            } else {
                                "— (pas de Proton pour ce jeu)".to_string()
                            },
                        ),
                        ("Artworks", artworks_label),
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

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(32.0);
                        if widgets::ghost_button(ui, "Paramètres du jeu", &pal, Icon::Gear).clicked() {
                            self.editing = Some(game.clone());
                        }
                        if game.steam_app_id.is_some() || game.sgdb_id.is_some() {
                            if widgets::ghost_button(ui, "Rafraîchir les artworks", &pal, Icon::Refresh)
                                .clicked()
                            {
                                let hit = crate::artwork::SearchHit::from_game(&game);
                                self.start_asset_job(ui.ctx(), &id, hit);
                            }
                        }
                    });
                }

                ui.add_space(40.0);
            });
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        ui.vertical_centered(|ui| {
            ui.add_space(140.0);
            ui.label(RichText::new("Aucun jeu sélectionné").size(18.0).color(pal.text_dim));
            ui.add_space(8.0);
            ui.label(
                RichText::new("Choisis un jeu dans la liste, ou ajoute-en un nouveau.")
                    .size(12.5)
                    .color(pal.text_faint),
            );
            ui.add_space(18.0);
            if widgets::accent_button(ui, Vec2::new(190.0, 40.0), "Ajouter un jeu", &pal, true, Icon::Plus)
                .clicked()
            {
                self.open_add_dialog();
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Lignes / tuiles
// ---------------------------------------------------------------------------

/// Ligne de la vue liste : logo à gauche, titre à côté (comme Steam).
fn game_row(
    ui: &mut egui::Ui,
    pal: &Palette,
    game: &Game,
    selected: bool,
    running: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
    let painter = ui.painter_at(rect);

    if selected {
        painter.rect_filled(rect, 4.0, pal.row_selected);
    } else if response.hovered() {
        painter.rect_filled(rect, 4.0, pal.surface_hover);
    }
    if selected {
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(rect.left(), rect.top() + 5.0), Vec2::new(3.0, rect.height() - 10.0)),
            1.5,
            pal.accent,
        );
    }

    let logo_box = Rect::from_min_size(
        Pos2::new(rect.left() + 18.0, rect.center().y - 15.0),
        Vec2::new(86.0, 30.0),
    );
    let logo = game.artwork.get(AssetKind::Logo).filter(|p| p.exists()).cloned();
    match logo {
        Some(path) => {
            ui.put(
                logo_box,
                egui::Image::new(file_uri(&path))
                    .max_size(logo_box.size())
                    .maintain_aspect_ratio(true),
            );
        }
        None => {
            let icon = game.artwork.get(AssetKind::Icon).filter(|p| p.exists()).cloned();
            match icon {
                Some(path) => {
                    let box_size = Vec2::splat(28.0);
                    ui.put(
                        Rect::from_min_size(
                            Pos2::new(logo_box.left(), rect.center().y - box_size.y / 2.0),
                            box_size,
                        ),
                        egui::Image::new(file_uri(&path)).max_size(box_size),
                    );
                }
                None => {
                    widgets::monogram(
                        &painter,
                        Rect::from_min_size(
                            Pos2::new(logo_box.left(), rect.center().y - 14.0),
                            Vec2::splat(28.0),
                        ),
                        &game.initials(),
                        pal,
                    );
                }
            }
        }
    }

    painter.text(
        Pos2::new(rect.left() + 118.0, rect.center().y),
        Align2::LEFT_CENTER,
        &game.name,
        FontId::proportional(13.5),
        if selected { pal.text } else { pal.text_dim },
    );

    if running {
        painter.circle_filled(Pos2::new(rect.right() - 18.0, rect.center().y), 4.0, pal.accent);
    }

    response
}

/// Tuile de la vue grille (couverture verticale 2:3).
fn game_tile(
    ui: &mut egui::Ui,
    pal: &Palette,
    game: &Game,
    size: Vec2,
    selected: bool,
    running: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 6.0, pal.surface);
    let cover = game.artwork.get(AssetKind::Cover).filter(|p| p.exists()).cloned();
    match cover {
        Some(path) => {
            ui.put(
                rect,
                egui::Image::new(file_uri(&path))
                    .fit_to_exact_size(rect.size())
                    .corner_radius(6.0),
            );
        }
        None => {
            widgets::monogram(&painter, rect.shrink(12.0), &game.initials(), pal);
        }
    }

    if selected || response.hovered() {
        widgets::border_rect(&painter, rect, if selected { 3.0 } else { 2.0 }, pal.accent);
    }
    if running {
        let badge = Rect::from_min_size(
            Pos2::new(rect.left() + 6.0, rect.top() + 6.0),
            Vec2::new(34.0, 18.0),
        );
        painter.rect_filled(badge, 3.0, pal.accent);
        painter.text(
            badge.center(),
            Align2::CENTER_CENTER,
            "EN JEU",
            FontId::proportional(9.5),
            eframe::egui::Color32::BLACK,
        );
    }

    response.on_hover_text(game.name.clone())
}
