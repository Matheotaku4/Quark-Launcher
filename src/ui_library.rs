//! Onglet Bibliothèque : colonne de gauche (liste ou grille) + panneau du jeu.

use crate::app::{NewsState, QuarkApp};
use crate::config::{self, AssetKind, Game, ViewMode};
use crate::images::{draw_cover, draw_fit};
use crate::news;
use crate::proton::{self, LaunchKind};
use crate::theme::{self, Palette};
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2};

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
        // Sélection multiple (Ctrl+clic) : les actions sont appliquées après la
        // mise en page, pour ne pas emprunter `self` pendant les clôtures.
        let marked_count = self.marked.len();
        let mut remove_marked = false;
        let mut clear_marked = false;

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
        if marked_count == 0 {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                let count = if ids.len() == total {
                    format!("{total} jeu{}", if total > 1 { "x" } else { "" })
                } else {
                    format!("{} / {total} jeux", ids.len())
                };
                ui.label(RichText::new(count).size(11.0).color(pal.text_faint));
                ui.add_space(10.0);
                ui.label(
                    RichText::new("Ctrl+clic : sélection multiple")
                        .size(10.5)
                        .color(pal.text_faint),
                );
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
                            let marked = self.is_marked(id);
                            let running = self.is_running(id);
                            let response = game_row(
                                ui,
                                &pal,
                                &game,
                                selected,
                                marked,
                                running,
                                &mut self.textures,
                            );
                            if response.secondary_clicked() {
                                // Clic droit : les paramètres du jeu, comme Steam.
                                self.select(id);
                                self.editing = Some(game.clone());
                            } else if response.clicked() {
                                if ui.input(|i| i.modifiers.command) {
                                    self.toggle_marked(id);
                                } else {
                                    self.select(id);
                                }
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
                                    let marked = self.is_marked(id);
                                    let running = self.is_running(id);
                                    let response = game_tile(
                                        ui,
                                        &pal,
                                        &game,
                                        Vec2::new(tile_w, tile_h),
                                        selected,
                                        marked,
                                        running,
                                        &mut self.textures,
                                    );
                                    if response.secondary_clicked() {
                                        self.select(id);
                                        self.editing = Some(game.clone());
                                    } else if response.clicked() {
                                        if ui.input(|i| i.modifiers.command) {
                                            self.toggle_marked(id);
                                        } else {
                                            self.select(id);
                                        }
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
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(hero_rect)
                        .layout(Layout::top_down(Align::Min)),
                    |ui| {
                        let (hero, _) = ui.allocate_exact_size(hero_rect.size(), Sense::hover());
                        let painter = ui.painter_at(hero);
                        painter.rect_filled(hero, 0.0, pal.surface);
                        let hero_path = game
                            .artwork
                            .get(AssetKind::Hero)
                            .filter(|p| p.exists())
                            .cloned();
                        let hero_tex = hero_path
                            .as_ref()
                            .and_then(|path| self.texture(ui.ctx(), path));
                        if let Some(texture) = hero_tex {
                            // Recadrée, jamais étirée : quelle que soit la
                            // largeur de la fenêtre (plein écran compris), la
                            // bannière garde ses proportions et les bords qui
                            // dépassent sont rognés par le painter.
                            draw_cover(&painter, &texture, hero);
                        } else {
                            painter.rect_filled(hero, 0.0, pal.surface_hover);
                        }
                        let gradient = self.gradient_texture(ui.ctx());
                        theme::bottom_gradient(ui, &gradient, hero, 0.62);

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
                                    draw_fit(ui, &texture, logo_rect);
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
                            },
                        );

                        // Infos (\u{24d8}) puis engrenage, à droite de la bannière (comme Steam).
                        let info_rect = Rect::from_min_size(
                            Pos2::new(hero.right() - 92.0, hero.top() + 14.0),
                            Vec2::splat(34.0),
                        );
                        ui.scope_builder(egui::UiBuilder::new().max_rect(info_rect), |ui| {
                            info_clicked =
                                widgets::icon_button(ui, 34.0, Icon::Info, &pal, "Informations du jeu")
                                    .clicked();
                        });

                        // Personnaliser (images du jeu), puis Infos, puis Paramètres.
                        let custom_rect = Rect::from_min_size(
                            Pos2::new(hero.right() - 134.0, hero.top() + 14.0),
                            Vec2::splat(34.0),
                        );
                        ui.scope_builder(egui::UiBuilder::new().max_rect(custom_rect), |ui| {
                            customize_clicked = widgets::icon_button(
                                ui,
                                34.0,
                                Icon::Image,
                                &pal,
                                "Personnaliser (artworks)",
                            )
                            .clicked();
                        });

                        let gear_rect = Rect::from_min_size(
                            Pos2::new(hero.right() - 50.0, hero.top() + 14.0),
                            Vec2::splat(34.0),
                        );
                        ui.scope_builder(egui::UiBuilder::new().max_rect(gear_rect), |ui| {
                            gear_clicked =
                                widgets::icon_button(ui, 34.0, Icon::Gear, &pal, "Paramètres du jeu")
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
                        ui.label(RichText::new("INFORMATIONS").size(11.0).color(pal.text_faint));
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
                            "Préfixe Proton",
                            if is_proton {
                                prefix.display().to_string()
                            } else {
                                "\u{2014} (pas de Proton pour ce jeu)".to_string()
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
                        if widgets::ghost_button(ui, "Personnaliser", &pal, Icon::Image).clicked() {
                            self.open_customize(&id);
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
            if widgets::accent_button(ui, Vec2::new(190.0, 40.0), "Ajouter un jeu", &pal, true, Icon::Plus)                    .clicked()
            {
                self.open_add_dialog();
            }
        });
    }

    /// Actualités du jeu (flux public Steam), affichées sous la bannière.
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

        ui.add_space(26.0);
        ui.horizontal(|ui| {
            ui.add_space(32.0);
            ui.label(RichText::new("ACTUALIT\u{c9}S").size(11.0).color(pal.text_faint));
            if linked {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(32.0);
                    let label = if failure.is_some() { "Réessayer" } else { "Actualiser" };
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
                // `vertical`, les trois lignes se retrouvaient côte à côte et
                // débordaient de la fenêtre.
                let inner = frame.show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(card_width);
                        let mut meta = vec![item.feed.clone()];
                        if !item.author.is_empty() {
                            meta.push(item.author.clone());
                        }
                        meta.push(config::human_ago(item.date));
                        ui.label(RichText::new(meta.join("  \u{b7}  ")).size(11.0).color(pal.text_faint));
                        ui.add_space(4.0);
                        ui.label(single_line(&item.title, 14.5, pal.text, card_width));
                        if !item.excerpt.is_empty() {
                            ui.add_space(4.0);
                            ui.label(paragraph(&item.excerpt, 12.0, pal.text_dim, 3));
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

                let response = ui
                    .interact(inner.response.rect, id, Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let hovered = response.hovered();
                if hovered != was_hovered {
                    ui.data_mut(|data| data.insert_temp(id, hovered));
                    // Une seule image de plus : l'animation prend le relais.
                    ui.ctx().request_repaint();
                }
                if response.clicked() {
                    open = Some(item.url.clone());
                }
            });
            ui.add_space(4.0);
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
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
    let painter = ui.painter_at(rect);
    let id = egui::Id::new(("quark_row", game.id.as_str()));
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), response.hovered(), 0.12);
    let mark = ui
        .ctx()
        .animate_bool_with_time(id.with("selected"), selected || marked, 0.16);

    if hover > 0.0 {
        painter.rect_filled(
            rect,
            4.0,
            theme::lerp_color(Color32::TRANSPARENT, pal.surface_hover, hover),
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
        Some(texture) => draw_cover(&ui.painter_at(thumb_box), texture, thumb_box),
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
    let title_color = theme::lerp_color(pal.text_dim, pal.text, mark.max(hover * 0.7));
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
    marked: bool,
    running: bool,
    textures: &mut crate::images::TextureCache,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
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
        Some(texture) => draw_cover(&ui.painter_at(cover_rect), texture, cover_rect),
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
        widgets::border_rect(
            &painter,
            rect,
            (2.0 + mark).min(3.0),
            pal.accent.gamma_multiply(accent),
        );
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

    response.on_hover_text(format!("{} · clic droit : paramètres", game.name))
}

/// Texte sur une seule ligne, coupé par une ellipsis s'il dépasse `max_width`.
fn single_line(text: &str, size: f32, color: Color32, max_width: f32) -> egui::text::LayoutJob {
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
