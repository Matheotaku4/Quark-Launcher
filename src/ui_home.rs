//! Page d'accueil : carrousel des jeux récemment joués, puis la bibliothèque.
//!
//! Même esprit que l'accueil de Steam : la bannière du dernier jeu joué en
//! grand, son nom, un bouton **Continuer**, le temps de jeu et la dernière
//! session à côté, puis tous les jeux, triés selon le choix du joueur.

use crate::app::QuarkApp;
use crate::config::{self, AssetKind, HomeSort};
use crate::images::{self, draw_fit_left};
use crate::proton::{self, LaunchKind};
use crate::theme::{self, Palette};
use crate::ui_library::{protondb_badge, single_line};
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2};
use std::path::PathBuf;
use std::time::Duration;

/// Durée du glissement d'une vignette du carrousel.
const SLIDE_SECS: f32 = 0.34;
/// Rayon des coins du carrousel.
const CAROUSEL_RADIUS: f32 = 16.0;
/// Rayon des coins des vignettes de la grille.
const TILE_RADIUS: f32 = 10.0;
/// Marge latérale de la page.
const PADDING: f32 = 26.0;

impl QuarkApp {
    pub(crate) fn home_pane(&mut self, ui: &mut egui::Ui) {
        let recent = self.recent_ids();
        if self.carousel >= recent.len() {
            self.carousel = 0;
        }
        // Défilement automatique : chaque vignette reste `carousel_secs` à
        // l'écran, puis glisse vers la suivante. Le minuteur ne demande des
        // images qu'à intervalles larges, la vitesse de l'interface ne change
        // donc pas.
        if self.lib.settings.carousel_auto && recent.len() > 1 {
            let period = self.lib.settings.carousel_secs.clamp(3.0, 120.0);
            let elapsed = self.carousel_at.elapsed().as_secs_f32();
            if elapsed >= period {
                let next = (self.carousel + 1) % recent.len();
                self.show_slide(next, recent.len());
            } else {
                let remaining = ((period - elapsed) * 1000.0).max(16.0) as u64;
                ui.ctx()
                    .request_repaint_after(Duration::from_millis(remaining.min(250)));
            }
        }
        egui::ScrollArea::vertical()
            .id_salt("quark_home_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(18.0);
                if recent.is_empty() {
                    self.home_empty(ui);
                } else {
                    self.carousel_section(ui, &recent);
                    ui.add_space(30.0);
                    self.home_grid(ui);
                }
                ui.add_space(34.0);
            });
    }

    /// Change de vignette et relance l'animation d'entrée.
    pub fn show_slide(&mut self, index: usize, len: usize) {
        if len == 0 {
            return;
        }
        self.carousel = index % len;
        self.carousel_at = std::time::Instant::now();
    }

    fn home_empty(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.label(
                RichText::new("Bienvenue dans Quark")
                    .size(22.0)
                    .color(pal.text),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Ajoute un jeu pour le voir apparaître ici.")
                    .size(12.5)
                    .color(pal.text_dim),
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

    // -----------------------------------------------------------------------
    // Carrousel
    // -----------------------------------------------------------------------

    fn carousel_section(&mut self, ui: &mut egui::Ui, recent: &[String]) {
        let base = self.pal;
        // Position dans la période de défilement automatique (0 = début).
        let auto_progress = if self.lib.settings.carousel_auto {
            (self.carousel_at.elapsed().as_secs_f32()
                / self.lib.settings.carousel_secs.clamp(3.0, 120.0))
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let width = (ui.available_width() - PADDING * 2.0).max(320.0);
        let height = (width / 2.7).clamp(240.0, 410.0);

        // Progression du glissement : l'instant du changement est posé par
        // `show_slide`, puis chaque image demandée avance l'animation. Au repos,
        // plus aucune image n'est réclamée.
        let progress = (self.carousel_at.elapsed().as_secs_f32() / SLIDE_SECS).clamp(0.0, 1.0);
        if progress < 1.0 {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        let ease = 1.0 - (1.0 - progress).powi(3);
        let slide = (1.0 - ease) * 40.0;

        let Some(id) = recent.get(self.carousel).cloned() else {
            return;
        };
        let Some(game) = self.lib.get(&id).cloned() else {
            return;
        };
        let pal = base.with_accent(self.game_accent(&game));

        let hero_path = game
            .artwork
            .get(AssetKind::Hero)
            .filter(|path| path.exists())
            .cloned();
        let hero_light = hero_path
            .as_ref()
            .map(|path| self.is_light_image(path))
            .unwrap_or(false);
        let hero_tex = hero_path
            .as_ref()
            .and_then(|path| self.texture(ui.ctx(), path));
        let logo_tex = game
            .artwork
            .get(AssetKind::Logo)
            .filter(|path| path.exists())
            .cloned()
            .and_then(|path| self.texture(ui.ctx(), &path));

        let settings = self.lib.settings.clone();
        let native = proton::resolve(&game, &settings, &self.runtimes)
            .map(|resolved| matches!(resolved.kind, LaunchKind::Native))
            .unwrap_or_else(|_| game.is_native());
        let running = self.is_running(&id);
        let running_secs = self.running_since(&id);

        let mut continue_clicked = false;
        let mut prev_clicked = false;
        let mut next_clicked = false;
        let mut goto: Option<usize> = None;
        let mut open_game = false;
        let mut open_protondb = false;

        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            let rect = Rect::from_min_size(ui.cursor().min, Vec2::new(width, height));
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    let (hero, _) = ui.allocate_exact_size(rect.size(), Sense::hover());
                    let painter = ui.painter_at(hero);
                    // Carte vide puis image arrondie : si un jeu n'a pas de
                    // bannière, le cadre reste propre au lieu d'un trou.
                    painter.rect_filled(hero, CAROUSEL_RADIUS, pal.surface);
                    if hero_tex.is_none() {
                        images::hero_placeholder(
                            &painter,
                            hero,
                            &game.monogram_letter(),
                            pal.accent,
                            CAROUSEL_RADIUS,
                        );
                    }
                    if let Some(texture) = &hero_tex {
                        images::rounded_cover_tinted(
                            ui,
                            hero,
                            texture,
                            CAROUSEL_RADIUS,
                            Color32::from_white_alpha((ease * 255.0).round() as u8),
                        );
                    }
                    images::rounded_veil(&painter, hero, CAROUSEL_RADIUS, ease * 0.95, 0.72);
                    // Fine ligne de progression du défilement automatique.
                    if self.lib.settings.carousel_auto && recent.len() > 1 {
                        painter.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(hero.left(), hero.bottom() - 3.0),
                                Vec2::new(hero.width() * auto_progress, 2.0),
                            ),
                            1.0,
                            pal.accent.gamma_multiply(0.85),
                        );
                    }

                    // Toute la vignette ouvre la fiche du jeu ; les boutons
                    // posés dessus restent prioritaires car créés après.
                    let card = ui.interact(
                        hero,
                        egui::Id::new(("quark_carousel", id.as_str())),
                        Sense::click(),
                    );
                    if card.clicked() {
                        open_game = true;
                    }

                    // Note ProtonDB et mention « natif Linux » en haut à gauche.
                    let top_rect = Rect::from_min_size(
                        Pos2::new(hero.left() + 30.0, hero.top() + 16.0),
                        Vec2::new(hero.width() - 120.0, 24.0),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(top_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                        |ui| {
                            if native {
                                linux_chip(ui, &base);
                                ui.add_space(6.0);
                            }
                            if let Some(app_id) = game.steam_app_id {
                                if protondb_badge(ui, &pal, self.protondb.get(&app_id)) {
                                    open_protondb = true;
                                }
                            }
                        },
                    );

                    // Logo (ou titre) juste au-dessus du bouton.
                    let row_height = 42.0;
                    let row_y = hero.bottom() - 26.0 - row_height;
                    let logo_rect = Rect::from_min_size(
                        Pos2::new(
                            hero.left() + 30.0,
                            (row_y - 12.0 - 74.0).max(hero.top() + 52.0) + slide,
                        ),
                        Vec2::new((width * 0.52).min(520.0), 74.0),
                    );
                    match &logo_tex {
                        Some(texture) => draw_fit_left(ui, texture, logo_rect),
                        None => {
                            painter.text(
                                Pos2::new(hero.left() + 32.0, logo_rect.bottom() + slide),
                                Align2::LEFT_BOTTOM,
                                &game.name,
                                FontId::proportional(32.0),
                                pal.text.gamma_multiply(ease),
                            );
                        }
                    }

                    // Continuer, puis le temps de jeu et la dernière session.
                    let row_rect = Rect::from_min_size(
                        Pos2::new(hero.left() + 30.0, row_y),
                        Vec2::new(hero.width() - 60.0, row_height),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new()
                            .max_rect(row_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                        |ui| {
                            let (label, icon) = if running {
                                ("Arr\u{ea}ter", Icon::Stop)
                            } else {
                                ("Continuer", Icon::Play)
                            };
                            continue_clicked = widgets::accent_button(
                                ui,
                                Vec2::new(160.0, row_height),
                                label,
                                &pal,
                                true,
                                icon,
                            )
                            .clicked();
                            ui.add_space(18.0);
                            let playtime = if running {
                                format!(
                                    "En jeu depuis {}",
                                    config::human_playtime(running_secs.max(1))
                                )
                            } else {
                                format!("{} de jeu", game.playtime_label())
                            };
                            ui.label(
                                RichText::new(playtime)
                                    .size(13.0)
                                    .color(pal.text_dim.gamma_multiply(ease)),
                            );
                            if game.last_played.is_some() {
                                ui.add_space(14.0);
                                ui.label(
                                    RichText::new(format!(
                                        "\u{b7}  jou\u{e9} {}",
                                        game.last_played_label()
                                    ))
                                    .size(12.5)
                                    .color(pal.text_faint.gamma_multiply(ease)),
                                );
                            }
                        },
                    );

                    // Flèches, au bord de la carte.
                    let nav_y = hero.center().y - 17.0;
                    let left_rect = Rect::from_min_size(
                        Pos2::new(hero.left() + 12.0, nav_y),
                        Vec2::splat(34.0),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(left_rect), |ui| {
                        prev_clicked = widgets::floating_icon_button(
                            ui,
                            34.0,
                            Icon::ArrowLeft,
                            &pal,
                            "Jeu précédent",
                            hero_light,
                        )
                        .clicked();
                    });
                    let right_rect = Rect::from_min_size(
                        Pos2::new(hero.right() - 46.0, nav_y),
                        Vec2::splat(34.0),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(right_rect), |ui| {
                        next_clicked = widgets::floating_icon_button(
                            ui,
                            34.0,
                            Icon::ArrowRight,
                            &pal,
                            "Jeu suivant",
                            hero_light,
                        )
                        .clicked();
                    });

                    // Pastilles de position, en bas à droite.
                    if recent.len() > 1 {
                        let dots_width = recent.len() as f32 * 16.0;
                        let dots_rect = Rect::from_min_size(
                            Pos2::new(hero.right() - 26.0 - dots_width, hero.bottom() - 24.0),
                            Vec2::new(dots_width, 16.0),
                        );
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .max_rect(dots_rect)
                                .layout(Layout::left_to_right(Align::Center)),
                            |ui| {
                                for index in 0..recent.len() {
                                    let active = index == self.carousel;
                                    let (dot, response) =
                                        ui.allocate_exact_size(Vec2::splat(10.0), Sense::click());
                                    let radius = if active { 4.0 } else { 3.0 };
                                    ui.painter().circle_filled(
                                        dot.center(),
                                        radius,
                                        if active {
                                            pal.text
                                        } else {
                                            pal.text_faint.gamma_multiply(0.9)
                                        },
                                    );
                                    if response.clicked() {
                                        goto = Some(index);
                                    }
                                    ui.add_space(6.0);
                                }
                            },
                        );
                    }
                },
            );
        });

        if continue_clicked {
            if running {
                self.stop(&id);
            } else {
                self.launch(ui.ctx(), &id);
            }
        }
        if prev_clicked {
            let len = recent.len();
            self.show_slide((self.carousel + len - 1) % len, len);
        }
        if next_clicked {
            let len = recent.len();
            self.show_slide((self.carousel + 1) % len, len);
        }
        if let Some(index) = goto {
            self.show_slide(index, recent.len());
        }
        if open_game {
            self.selected = Some(id.clone());
            // En vue grille, la fiche s'ouvre par-dessus le carrousel.
            if self.lib.settings.view == crate::config::ViewMode::Grid {
                self.overlay = Some(id);
            } else {
                self.home = false;
            }
        }
        if open_protondb {
            if let Some(app_id) = game.steam_app_id {
                let url = crate::protondb::Tier::url(app_id);
                if let Err(err) = crate::news::open_in_browser(&url) {
                    self.toast_error(format!("Ouverture du lien impossible : {err}"));
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Toute la bibliothèque
    // -----------------------------------------------------------------------

    fn home_grid(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let sort = self.lib.settings.home_sort;
        let mut new_sort: Option<HomeSort> = None;

        self.home_filters(ui);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            ui.label(
                RichText::new(match sort {
                    HomeSort::Recent => "TOUS LES JEUX \u{b7} DERNI\u{c8}RE SESSION",
                    HomeSort::Added => "TOUS LES JEUX \u{b7} AJOUT\u{c9}S R\u{c9}C\u{c9}MMENT",
                    HomeSort::Playtime => "TOUS LES JEUX \u{b7} LES PLUS JOU\u{c9}S",
                })
                .size(11.0)
                .color(pal.text_faint),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(PADDING);
                for option in HomeSort::ALL.iter().rev() {
                    if sort_chip(ui, option.label(), sort == *option, &pal) {
                        new_sort = Some(*option);
                    }
                    ui.add_space(6.0);
                }
            });
        });
        ui.add_space(14.0);

        let ids = self.lib.filtered_ids(sort);
        // Chemins puis textures, en deux passes : `texture` a besoin de `&mut
        // self`, que la lecture de la bibliothèque interdit en même temps.
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

        let available = (ui.available_width() - PADDING * 2.0).max(240.0);
        let gap = 14.0;
        let columns = ((available + gap) / 154.0).floor().clamp(2.0, 7.0) as usize;
        let tile_w = ((available - gap * (columns as f32 - 1.0)) / columns as f32)
            .floor()
            .max(96.0);
        let cover_h = tile_w * theme::COVER_RATIO;
        let row_h = cover_h + 34.0;

        // Entrée en cascade : chaque tuile apparaît un peu après la précédente.
        // Une fois l'animation finie, plus aucune image n'est demandée.
        let age = self.home_at.elapsed().as_secs_f32();
        let mut opened: Option<String> = None;
        let mut index = 0usize;
        while index < ids.len() {
            ui.horizontal(|ui| {
                // L'espacement automatique d'egui s'ajoutait à la marge et aux
                // gouttières : la dernière colonne dépassait de la fenêtre.
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(PADDING);
                for slot in 0..columns {
                    let Some(id) = ids.get(index + slot) else {
                        break;
                    };
                    let Some(game) = self.lib.get(id).cloned() else {
                        continue;
                    };
                    // Séparateur *avant* la tuile : une marge en fin de rangée
                    // ferait déborder la dernière colonne de la fenêtre.
                    if slot > 0 {
                        ui.add_space(gap);
                    }
                    let (tile, response) =
                        ui.allocate_exact_size(Vec2::new(tile_w, row_h), Sense::click());
                    // Apparition en cascade (décalage par tuile, 0,35 s).
                    let order = (index + slot) as f32;
                    let appear = ((age - order * 0.025) / 0.35).clamp(0.0, 1.0);
                    if appear < 1.0 {
                        ui.ctx().request_repaint();
                    }
                    let tile = tile.translate(Vec2::new(0.0, (1.0 - appear) * 10.0));
                    let painter = ui.painter_at(tile);
                    let anim = ui.ctx().animate_bool_with_time(
                        egui::Id::new(("quark_home_tile", id.as_str())),
                        response.hovered(),
                        0.14,
                    );
                    let cover = Rect::from_min_size(tile.min, Vec2::new(tile_w, cover_h))
                        .shrink(2.0 * anim);
                    painter.rect_filled(tile, TILE_RADIUS, pal.surface);
                    match covers.get(index + slot).and_then(|cover| cover.as_ref()) {
                        Some(texture) => {
                            images::rounded_cover_tinted(
                                ui,
                                cover,
                                texture,
                                TILE_RADIUS,
                                Color32::from_white_alpha((appear * 255.0).round() as u8),
                            );
                        }
                        None => {
                            // Monogramme plein cadre : une tuile sans couverture
                            // garde exactement la géométrie des autres.
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
                            TILE_RADIUS,
                            egui::Stroke::new(2.0_f32, pal.accent.gamma_multiply(anim)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let title_color = theme::lerp_color(pal.text_dim, pal.text, anim)
                        .gamma_multiply(appear);
                    let job = single_line(&game.name, 12.5, title_color, tile_w);
                    let galley = painter.layout_job(job);
                    painter.galley(
                        Pos2::new(tile.left(), cover.bottom() + 7.0),
                        galley,
                        title_color,
                    );
                    painter.text(
                        Pos2::new(tile.left(), cover.bottom() + 22.0),
                        Align2::LEFT_TOP,
                        if game.playtime_secs == 0 {
                            "jamais lanc\u{e9}".to_string()
                        } else {
                            game.playtime_label()
                        },
                        FontId::proportional(10.5),
                        pal.text_faint.gamma_multiply(appear),
                    );
                    if game.exe_missing() {
                        painter.rect_stroke(
                            cover,
                            TILE_RADIUS,
                            egui::Stroke::new(2.0_f32, pal.danger),
                            egui::StrokeKind::Inside,
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

        if let Some(id) = opened {
            // Vue grille : la fiche s'ouvre en surimpression, la grille reste
            // en place derrière. Vue liste : la fiche prend la place du panneau.
            if self.lib.settings.view == crate::config::ViewMode::Grid {
                self.selected = Some(id.clone());
                self.overlay = Some(id);
            } else {
                self.selected = Some(id);
                self.home = false;
            }
        }
        if let Some(option) = new_sort {
            self.lib.settings.home_sort = option;
            self.touch();
        }
    }

    /// Filtres de la bibliothèque : statut, genre Steam, tag personnel.
    /// Un second clic sur une pastille active retire son filtre.
    fn home_filters(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        let tags = self.lib.all_tags();
        let genres = self.lib.all_genres();
        if tags.is_empty() && genres.is_empty() {
            return;
        }
        let mut new_status: Option<crate::config::GameStatus> = None;
        let mut new_tag: Option<String> = None;
        let mut new_genre: Option<String> = None;
        let status = self.lib.settings.home_status;
        let tag = self.lib.settings.home_tag.clone();
        let genre = self.lib.settings.home_genre.clone();

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(PADDING);
            ui.label(RichText::new("FILTRES").size(10.5).color(pal.text_faint));
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(PADDING);
            if widgets::chip_colored(ui, "Tout", status == crate::config::GameStatus::None
                && tag.is_empty()
                && genre.is_empty(), &pal, pal.accent)
            {
                new_status = Some(crate::config::GameStatus::None);
                new_tag = Some(String::new());
                new_genre = Some(String::new());
            }
            for option in crate::config::GameStatus::ALL.iter().skip(1) {
                let selected = status == *option;
                let (r, g, b) = option.color();
                if widgets::chip_colored(
                    ui,
                    option.label(),
                    selected,
                    &pal,
                    Color32::from_rgb(r, g, b),
                ) {
                    new_status = Some(if selected {
                        crate::config::GameStatus::None
                    } else {
                        *option
                    });
                }
            }
            for (name, count) in &genres {
                let selected = genre.eq_ignore_ascii_case(name);
                if widgets::chip(
                    ui,
                    &format!("{name}"),
                    selected,
                    &pal,
                ) {
                    new_genre = Some(if selected { String::new() } else { name.clone() });
                }
                let _ = count;
            }
            for (name, count) in &tags {
                let selected = tag.eq_ignore_ascii_case(name);
                if widgets::chip(ui, &format!("#{name} · {count}"), selected, &pal) {
                    new_tag = Some(if selected { String::new() } else { name.clone() });
                }
            }
        });
        if let Some(status) = new_status {
            self.lib.settings.home_status = status;
            self.touch();
        }
        if let Some(tag) = new_tag {
            self.lib.settings.home_tag = tag;
            self.touch();
        }
        if let Some(genre) = new_genre {
            self.lib.settings.home_genre = genre;
            self.touch();
        }
    }
}

/// Pastille « Linux natif » : le jeu démarre sans Proton.
pub(crate) fn linux_chip(ui: &mut egui::Ui, pal: &Palette) {
    let color = Color32::from_rgb(0x3C, 0xA5, 0x62);
    ui.add(
        egui::Button::new(
            RichText::new("LINUX NATIF")
                .size(10.0)
                .strong()
                .color(theme::on_accent(color)),
        )
        .fill(color)
        .stroke(egui::Stroke::NONE)
        .corner_radius(9.0)
        .min_size(Vec2::new(0.0, 20.0)),
    )
    .on_hover_text("Ce jeu est un binaire Linux : il démarre sans Proton.");
    let _ = pal;
}

/// Pastille de tri de la grille.
fn sort_chip(ui: &mut egui::Ui, label: &str, selected: bool, pal: &Palette) -> bool {
    let ink = if selected {
        theme::on_accent(pal.accent)
    } else {
        pal.text_dim
    };
    ui.add(
        egui::Button::new(RichText::new(label).size(11.0).color(ink))
            .fill(if selected { pal.accent } else { pal.surface })
            .stroke(egui::Stroke::new(
                1.0_f32,
                if selected { pal.accent } else { pal.border },
            ))
            .corner_radius(9.0)
            .min_size(Vec2::new(0.0, 22.0)),
    )
    .clicked()
}
