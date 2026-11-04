//! Vue des jeux détectés ou possédés par les boutiques connectées.
//!
//! Un onglet par boutique, une grille de cartes au lieu d'une liste : la page
//! se parcourt d'un coup d'œil, et la synchronisation n'est plus ici (elle se
//! fait au démarrage et dans Paramètres → Boutiques & outils).

use crate::app::{QuarkApp, SettingsTab, Tab};
use crate::providers::{InstallState, Provider, ProviderGame};
use crate::theme;
use eframe::egui::{self, Align, Align2, Layout, Pos2, Rect, RichText, Sense, Vec2};
use std::path::PathBuf;

impl QuarkApp {
    pub(crate) fn my_games_tab(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let mut open_settings = false;
        let mut install: Option<u32> = None;
        let mut preview_save: Option<crate::providers::ProviderGameKey> = None;
        let mut inventory_save: Option<crate::providers::ProviderGameKey> = None;
        let mut choose_save_folder: Option<crate::providers::ProviderGameKey> = None;
        let mut remove_save_folder: Option<(
            crate::providers::ProviderGameKey,
            std::path::PathBuf,
        )> = None;
        let mut merge_keys: Vec<crate::providers::ProviderGameKey> = Vec::new();
        let mut split_key: Option<crate::providers::ProviderGameKey> = None;
        let mut install_request: Option<crate::providers::ProviderGameKey> = None;
        let mut open_game: Option<crate::providers::ProviderGameKey> = None;
        let mut adopt: Option<crate::providers::ProviderGameKey> = None;
        let mut show_achievements: Option<crate::providers::ProviderGameKey> = None;
        let mut uninstall: Option<crate::providers::ProviderGameKey> = None;
        let mut sync_after_handoff = false;

        let busy = self.steam_sync_loading || self.epic_sync_loading || self.gog_sync_loading;
        let all_games = self.lib.provider_library.games.clone();
        let counts = [
            all_games.len(),
            all_games
                .iter()
                .filter(|game| game.key.provider == Provider::Steam)
                .count(),
            all_games
                .iter()
                .filter(|game| game.key.provider == Provider::Epic)
                .count(),
            all_games
                .iter()
                .filter(|game| game.key.provider == Provider::Gog)
                .count(),
        ];

        // Couvertures des jeux Quark correspondants : la même image que dans la
        // bibliothèque, sans requête réseau supplémentaire.
        let selected = self.my_games_store;

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg))
            .show(ctx, |ui| {
                // Entrée de page : fondu de 0,16 s à chaque changement d'onglet.
                let age = self.page_at.elapsed().as_secs_f32();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let fade = (age / 0.16).clamp(0.0, 1.0);
                        if fade < 1.0 {
                            ui.ctx().request_repaint();
                        }
                        ui.set_opacity(fade);
                        ui.horizontal(|ui| {
                            ui.add_space(28.0);
                            ui.vertical(|ui| {
                                ui.set_max_width((ui.available_width() - 28.0).max(240.0));
                                ui.add_space(24.0);
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            crate::i18n::rich("MES JEUX")
                                                .size(11.0)
                                                .strong()
                                                .color(pal.accent),
                                        );
                                        ui.label(
                                            crate::i18n::rich("Vos bibliothèques réunies")
                                                .size(26.0)
                                                .strong()
                                                .color(pal.text),
                                        );
                                        ui.label(
                                            crate::i18n::rich(if busy {
                                                "Synchronisation en cours…"
                                            } else {
                                                "Synchronisées au démarrage · installations locales détectées"
                                            })
                                            .size(12.0)
                                            .color(pal.text_dim),
                                        );
                                    });
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            if busy {
                                                ui.add(egui::Spinner::new().size(14.0));
                                                ui.add_space(6.0);
                                            }
                                            if widgets_button(ui, "Boutiques & outils", &pal) {
                                                open_settings = true;
                                            }
                                            ui.add_space(8.0);
                                            ui.add_sized(
                                                Vec2::new(220.0, 30.0),
                                                egui::TextEdit::singleline(
                                                    &mut self.my_games_query,
                                                )
                                                .id(egui::Id::new("quark_my_games_search"))
                                                .hint_text(crate::i18n::tr("Rechercher un jeu"))
                                                .font(egui::FontId::proportional(12.5)),
                                            );
                                        },
                                    );
                                });
                                ui.add_space(16.0);

                                // Un onglet par boutique, plus « Toutes ».
                                ui.horizontal(|ui| {
                                    for (provider, label) in [
                                        (None, "Toutes"),
                                        (Some(Provider::Steam), "Steam"),
                                        (Some(Provider::Epic), "Epic Games"),
                                        (Some(Provider::Gog), "GOG"),
                                    ] {
                                        let index = match provider {
                                            None => 0,
                                            Some(Provider::Steam) => 1,
                                            Some(Provider::Epic) => 2,
                                            Some(Provider::Gog) => 3,
                                        };
                                        let title = format!("{label} · {}", counts[index]);
                                        if store_tab(
                                            ui,
                                            &pal,
                                            &title,
                                            selected == provider,
                                            index,
                                        ) {
                                            self.my_games_store = provider;
                                        }
                                        ui.add_space(4.0);
                                    }
                                });
                                ui.add_space(14.0);

                                if let Some(error) = &self.steam_sync_error {
                                    ui.label(
                                        crate::i18n::rich(format!("Steam : {error}"))
                                            .size(11.5)
                                            .color(pal.danger),
                                    );
                                }
                                if let Some(error) = &self.epic_sync_error {
                                    ui.label(
                                        crate::i18n::rich(format!("Epic : {error}"))
                                            .size(11.5)
                                            .color(pal.danger),
                                    );
                                }
                                if let Some(error) = &self.gog_sync_error {
                                    ui.label(
                                        crate::i18n::rich(format!("GOG : {error}"))
                                            .size(11.5)
                                            .color(pal.danger),
                                    );
                                }
                                if self.credentials.steam_web_api_key.is_none()
                                    || self.credentials.steam_id.is_none()
                                {
                                    ui.label(
                                        crate::i18n::rich(
                                            "Steam : les jeux installés sont détectés sans clé ; \
                                             les jeux possédés demandent une clé Web API dans \
                                             Paramètres → Boutiques & outils.",
                                        )
                                        .size(11.5)
                                        .color(pal.text_faint),
                                    );
                                }
                                ui.add_space(10.0);

                                self.provider_install_panel(ui, &pal);
                                let needle = self.my_games_query.trim().to_string();
                                let games = visible_games(&all_games, selected, &needle);
                                if games.is_empty() {
                                    ui.add_space(20.0);
                                    ui.label(
                                        crate::i18n::rich(if needle.is_empty() {
                                            "Aucun jeu de boutique synchronisé".to_string()
                                        } else {
                                            format!("Aucun jeu ne correspond à « {needle} »")
                                        })
                                        .size(16.0)
                                        .color(pal.text),
                                    );
                                    ui.add_space(5.0);
                                    ui.label(
                                        crate::i18n::rich(if needle.is_empty() {
                                            "Connecte une boutique dans Paramètres → Boutiques & outils : \
                                             la synchronisation suit automatiquement."
                                                .to_string()
                                        } else {
                                            "Le filtre porte sur les jeux possédés des boutiques \
                                             synchronisées ; efface-le pour tout revoir."
                                                .to_string()
                                        })
                                        .size(12.0)
                                        .color(pal.text_dim),
                                    );
                                } else {
                                    // Seules les lignes visibles sont construites
                                    // et décodées : une bibliothèque de plusieurs
                                    // centaines de jeux garde un défilement fluide.
                                    self.provider_grid(
                                        ui,
                                        &pal,
                                        &games,
                                        &mut install,
                                        &mut install_request,
                                        &mut adopt,
                                        &mut show_achievements,
                                        &mut uninstall,
                                        &mut open_game,
                                        &mut preview_save,
                                        &mut inventory_save,
                                        &mut choose_save_folder,
                                        &mut remove_save_folder,
                                    );
                                }
                                ui.add_space(16.0);
                                for candidate in
                                    crate::providers::operations::match_candidates(&all_games)
                                {
                                    let labels: Vec<String> = candidate
                                        .members
                                        .iter()
                                        .filter_map(|key| {
                                            all_games.iter().find(|game| &game.key == key).map(
                                                |game| {
                                                    format!(
                                                        "{} ({})",
                                                        game.title,
                                                        provider_name(game.key.provider)
                                                    )
                                                },
                                            )
                                        })
                                        .collect();
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            crate::i18n::rich(format!(
                                                "Correspondance exacte : {}",
                                                labels.join(" · ")
                                            ))
                                            .size(11.0)
                                            .color(pal.text_dim),
                                        );
                                        if ui.small_button("Fusionner l’affichage").clicked() {
                                            merge_keys = candidate.members.clone();
                                        }
                                    });
                                }
                                for group in &self.lib.provider_library.manual_groups {
                                    ui.horizontal(|ui| {
                                        let labels: Vec<String> = group
                                            .members
                                            .iter()
                                            .filter_map(|key| {
                                                all_games
                                                    .iter()
                                                    .find(|game| &game.key == key)
                                                    .map(|game| game.title.clone())
                                            })
                                            .collect();
                                        ui.label(
                                            crate::i18n::rich(format!(
                                                "Groupe manuel : {}",
                                                labels.join(" · ")
                                            ))
                                            .size(11.0)
                                            .color(pal.text_dim),
                                        );
                                        for key in &group.members {
                                            if ui
                                                .small_button(format!(
                                                    "Séparer {}",
                                                    key.product_id
                                                ))
                                                .clicked()
                                            {
                                                split_key = Some(key.clone());
                                            }
                                        }
                                    });
                                }
                            });
                        });
                    });
            });

        if open_settings {
            self.settings_tab = SettingsTab::Providers;
            self.settings_open = true;
        }
        if let Some(key) = preview_save {
            self.preview_save_backup(ctx, &key);
        }
        if let Some(key) = inventory_save {
            self.open_save_inventory(ctx, &key);
        }
        if let Some(key) = choose_save_folder {
            self.choose_save_folder(ctx, &key);
        }
        if let Some((key, path)) = remove_save_folder {
            self.remove_save_folder(&key, &path);
        }
        if merge_keys.len() >= 2 {
            match crate::providers::operations::merge_group(&merge_keys) {
                Ok(group) => {
                    self.lib.provider_library.manual_groups.retain(|existing| {
                        !existing
                            .members
                            .iter()
                            .any(|member| merge_keys.contains(member))
                    });
                    self.lib.provider_library.manual_groups.push(group);
                    self.touch();
                }
                Err(error) => self.toast_error(error),
            }
        }
        if let Some(key) = split_key {
            for group in &mut self.lib.provider_library.manual_groups {
                crate::providers::operations::split_member(group, &key);
            }
            self.lib
                .provider_library
                .manual_groups
                .retain(|group| group.members.len() > 1);
            self.touch();
        }
        if let Some(key) = install_request {
            self.open_install_request(ctx, &key);
        }
        if let Some(key) = show_achievements {
            self.achievements_open = Some(key.clone());
            self.request_achievements(ctx, &key);
        }
        if let Some(key) = uninstall {
            self.request_uninstall(&key);
        }
        if let Some(key) = adopt {
            self.adopt_provider_install(ctx, &key);
        }
        if let Some(key) = open_game {
            let quark_id = self
                .lib
                .provider_library
                .games
                .iter()
                .find(|game| game.key == key)
                .and_then(|game| game.quark_game_id.clone())
                .filter(|id| self.lib.get(id).is_some());
            match quark_id {
                Some(id) => {
                    self.selected = Some(id);
                    self.tab = Tab::Library;
                    self.home = false;
                }
                None => self.toast_info(
                    "Ce jeu est installé mais pas encore dans la bibliothèque : installe-le ou \
                     utilise « Ajouter à la bibliothèque »",
                ),
            }
        }
        if let Some(app_id) = install {
            match crate::providers::steam::request_install(app_id) {
                Ok(()) => {
                    self.toast_info(
                        "Demande envoyée au client Steam · l'installation apparaîtra à la \
                         prochaine synchronisation",
                    );
                    sync_after_handoff = true;
                }
                Err(error) => self.toast_error(format!("Installation Steam impossible : {error}")),
            }
        }
        if sync_after_handoff {
            // L'ouverture de l'URI n'étant pas une preuve, cette synchronisation
            // détecte seulement les manifestes déjà présents et ne force aucun état.
            self.start_steam_sync(ctx);
        }
    }

    /// Grille de cartes, trois à huit colonnes selon la largeur disponible.
    #[allow(clippy::too_many_arguments)]
    fn provider_grid(
        &mut self,
        ui: &mut egui::Ui,
        pal: &crate::theme::Palette,
        games: &[ProviderGame],
        install: &mut Option<u32>,
        install_request: &mut Option<crate::providers::ProviderGameKey>,
        adopt: &mut Option<crate::providers::ProviderGameKey>,
        achievements: &mut Option<crate::providers::ProviderGameKey>,
        uninstall: &mut Option<crate::providers::ProviderGameKey>,
        open: &mut Option<crate::providers::ProviderGameKey>,
        preview_save: &mut Option<crate::providers::ProviderGameKey>,
        inventory_save: &mut Option<crate::providers::ProviderGameKey>,
        choose_save_folder: &mut Option<crate::providers::ProviderGameKey>,
        remove_save_folder: &mut Option<(crate::providers::ProviderGameKey, PathBuf)>,
    ) {
        let gap = 12.0;
        let available = ui.available_width();
        let columns = ((available + gap) / (168.0 + gap)).floor().clamp(2.0, 8.0) as usize;
        let tile_w = ((available - gap * (columns as f32 - 1.0)) / columns as f32)
            .floor()
            .max(120.0);
        let cover_h = tile_w * crate::theme::COVER_RATIO;
        // Les 60 px du pied de carte couvrent le titre et la ligne boutique
        // même avec une police agrandie ; le clip du pied empêche tout débord.
        let tile_h = cover_h + 60.0;
        let row_step = tile_h + gap;
        let rows = games.len().div_ceil(columns);
        let grid_top = ui.cursor().top();
        let width = ui.available_width();
        // Le rectangle de clip est en coordonnées écran, comme `grid_top` ; une
        // ligne de marge de chaque côté évite un trou pendant un défilement
        // rapide.
        let clip = ui.clip_rect();
        let first =
            (((clip.top() - grid_top) / row_step).floor().max(0.0) as usize).saturating_sub(1);
        let last =
            ((((clip.bottom() - grid_top) / row_step).ceil().max(0.0) as usize) + 1).min(rows);
        // Même cascade que la bibliothèque, pour que les deux pages se
        // répondent quand on passe de l'une à l'autre.
        let age = self.page_at.elapsed().as_secs_f32();
        let mut visible = Vec::new();

        for row in first..last {
            let index = row * columns;
            let row_rect = Rect::from_min_size(
                Pos2::new(ui.max_rect().left(), grid_top + row as f32 * row_step),
                Vec2::new(width, tile_h),
            );
            let mut row_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(row_rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            row_ui.set_clip_rect(row_rect.intersect(ui.clip_rect()));
            row_ui.spacing_mut().item_spacing.x = 0.0;
            for slot in 0..columns {
                let Some(game) = games.get(index + slot) else {
                    break;
                };
                if slot > 0 {
                    row_ui.add_space(gap);
                }
                let cover = self.cover_for(game);
                let texture = cover
                    .as_ref()
                    .and_then(|path| self.texture_thumb(row_ui.ctx(), path));
                let appear = ((age - slot as f32 * 0.02) / 0.3).clamp(0.0, 1.0);
                if appear < 1.0 {
                    row_ui.ctx().request_repaint();
                }
                visible.push(game.clone());
                provider_card(
                    &mut row_ui,
                    pal,
                    game,
                    Vec2::new(tile_w, tile_h),
                    appear,
                    texture.as_ref(),
                    &mut *install,
                    &mut *install_request,
                    &mut *adopt,
                    &mut *achievements,
                    &mut *uninstall,
                    &mut *open,
                    &mut *preview_save,
                    &mut *inventory_save,
                    &mut *choose_save_folder,
                    &mut *remove_save_folder,
                );
            }
        }

        // Réserve la hauteur totale : la barre de défilement représente toute
        // la bibliothèque, pas seulement les lignes déjà construites.
        ui.allocate_space(Vec2::new(width, rows as f32 * row_step));
        self.queue_provider_covers(ui.ctx(), &visible);
    }

    /// Bandeau des installations fournisseur en cours : pourcentage, étape,
    /// débit, ETA et bouton d'annulation. Sans lui, un téléchargement de
    /// plusieurs gigaoctets ne montrait qu'un « Installation… » figé.
    fn provider_install_panel(&mut self, ui: &mut egui::Ui, pal: &crate::theme::Palette) {
        let jobs: Vec<_> = self
            .provider_install_jobs
            .iter()
            .map(|job| (job.key.clone(), job.progress.clone()))
            .collect();
        for (key, progress) in jobs {
            let title = self
                .lib
                .provider_library
                .games
                .iter()
                .find(|game| game.key == key)
                .map(|game| game.title.clone())
                .unwrap_or_else(|| key.product_id.clone());
            let snapshot = progress
                .lock()
                .ok()
                .map(|state| state.clone())
                .unwrap_or_default();
            egui::Frame::new()
                .fill(pal.surface)
                .stroke(egui::Stroke::new(1.0_f32, pal.border))
                .corner_radius(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                crate::i18n::rich(format!(
                                    "Installation {} · {title}",
                                    provider_name(key.provider)
                                ))
                                .size(12.5)
                                .strong()
                                .color(pal.text),
                            );
                            let mut detail = vec![snapshot.label()];
                            if let Some(sizes) = snapshot.sizes() {
                                detail.push(sizes);
                            }
                            if let Some(speed) = &snapshot.speed {
                                detail.push(speed.clone());
                            }
                            if let Some(eta) = &snapshot.eta {
                                detail.push(format!("reste {eta}"));
                            }
                            ui.label(
                                crate::i18n::rich(detail.join(" · "))
                                    .size(11.0)
                                    .color(pal.text_dim),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if crate::widgets::ghost_button(
                                ui,
                                "Annuler",
                                pal,
                                crate::widgets::Icon::None,
                            )
                            .clicked()
                            {
                                self.cancel_provider_install(&key);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    let (bar, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
                    ui.painter().rect_filled(bar, 3.0, pal.border);
                    match snapshot.percent {
                        Some(percent) => {
                            let fill = (percent / 100.0).clamp(0.0, 1.0);
                            if fill > 0.0 {
                                ui.painter().rect_filled(
                                    Rect::from_min_size(
                                        bar.min,
                                        Vec2::new(bar.width() * fill, bar.height()),
                                    ),
                                    3.0,
                                    pal.accent,
                                );
                            }
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_millis(180));
                        }
                        None => {
                            let phase = ui.input(|input| input.time) as f32;
                            let pulse = (phase * 0.9).sin() * 0.5 + 0.5;
                            let width = bar.width() * 0.22;
                            let x = bar.left() + (bar.width() - width) * pulse;
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    Pos2::new(x, bar.top()),
                                    Vec2::new(width, bar.height()),
                                ),
                                3.0,
                                pal.accent.gamma_multiply(0.75),
                            );
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_millis(60));
                            ui.ctx().request_repaint();
                        }
                    }
                });
            ui.add_space(8.0);
        }
    }

    /// Nombre de téléchargements visibles dans l'icône de la barre du haut.
    pub(crate) fn active_install_count(&self) -> usize {
        self.provider_install_jobs.len()
    }

    /// Panneau de l'icône « Téléchargements » : pourcentage, étape, taille
    /// totale, débit, ETA et annulation, pour Epic comme pour GOG.
    pub(crate) fn downloads_popup(&mut self, ui: &mut egui::Ui, pal: &crate::theme::Palette) {
        ui.set_min_width(330.0);
        let active: Vec<_> = self
            .provider_install_jobs
            .iter()
            .map(|job| (job.key.clone(), job.progress.clone()))
            .collect();
        if active.is_empty() {
            ui.label(
                crate::i18n::rich("Aucun téléchargement en cours.")
                    .size(11.5)
                    .color(pal.text_dim),
            );
            ui.label(
                crate::i18n::rich("Lance une installation depuis Mes jeux.")
                    .size(10.5)
                    .color(pal.text_faint),
            );
            return;
        }
        for (key, progress) in active {
            let title = self
                .lib
                .provider_library
                .games
                .iter()
                .find(|game| game.key == key)
                .map(|game| game.title.clone())
                .unwrap_or_else(|| key.product_id.clone());
            let snapshot = progress
                .lock()
                .ok()
                .map(|state| state.clone())
                .unwrap_or_default();
            egui::Frame::new()
                .fill(pal.surface)
                .stroke(egui::Stroke::new(1.0_f32, pal.border))
                .corner_radius(6.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            crate::i18n::rich(format!("{} · {title}", provider_name(key.provider)))
                                .size(11.5)
                                .strong()
                                .color(pal.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("Annuler").clicked() {
                                self.cancel_provider_install(&key);
                            }
                        });
                    });
                    let mut detail = vec![snapshot.label()];
                    if let Some(sizes) = snapshot.sizes() {
                        detail.push(sizes);
                    }
                    if let Some(speed) = &snapshot.speed {
                        detail.push(speed.clone());
                    }
                    if let Some(eta) = &snapshot.eta {
                        detail.push(format!("reste {eta}"));
                    }
                    ui.label(
                        crate::i18n::rich(detail.join(" · "))
                            .size(10.5)
                            .color(pal.text_dim),
                    );
                    let (bar, _) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 4.0), Sense::hover());
                    ui.painter().rect_filled(bar, 2.0, pal.border);
                    if let Some(percent) = snapshot.percent {
                        let fill = (percent / 100.0).clamp(0.0, 1.0);
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                bar.min,
                                Vec2::new(bar.width() * fill, bar.height()),
                            ),
                            2.0,
                            pal.accent,
                        );
                    }
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(180));
                });
            ui.add_space(4.0);
        }
    }

    /// Couverture d'un jeu de boutique : d'abord celle téléchargée depuis la
    /// boutique (Steam, GOG, Epic), sinon celle du jeu Quark de même identifiant
    /// Steam ou de même titre. Aucun téléchargement n'est déclenché ici.
    fn cover_for(&self, game: &ProviderGame) -> Option<PathBuf> {
        if let Some(path) = crate::providers::covers::cached(&game.key) {
            return Some(path);
        }
        let app_id = game.key.product_id.parse::<u32>().ok();
        let matching = self.lib.games.iter().find(|candidate| {
            candidate
                .steam_app_id
                .zip(app_id)
                .is_some_and(|(left, right)| left == right)
                || candidate.name.eq_ignore_ascii_case(&game.title)
        })?;
        matching
            .artwork
            .get(crate::config::AssetKind::Cover)
            .filter(|path| path.exists())
            .cloned()
    }
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Steam => "Steam",
        Provider::Epic => "Epic Games",
        Provider::Gog => "GOG",
    }
}

fn provider_color(provider: Provider) -> egui::Color32 {
    match provider {
        Provider::Steam => egui::Color32::from_rgb(0x66, 0xC0, 0xF4),
        Provider::Epic => egui::Color32::from_rgb(0xC8, 0xC8, 0xD0),
        Provider::Gog => egui::Color32::from_rgb(0xB0, 0x8C, 0xFF),
    }
}

/// Onglet de boutique : pastille pleine pour la sélection, avec un liseré qui
/// glisse d'un onglet à l'autre via l'animation d'egui.
fn store_tab(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    label: &str,
    selected: bool,
    index: usize,
) -> bool {
    let font = egui::FontId::proportional(12.5);
    let width = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), pal.text)
        .size()
        .x;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width + 24.0, 30.0), Sense::click());
    let painter = ui.painter_at(rect);
    let id = egui::Id::new(("quark-store-tab", index));
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), response.hovered(), 0.12);
    let mark = ui
        .ctx()
        .animate_bool_with_time(id.with("selected"), selected, 0.18);
    let fill = if selected {
        pal.surface
    } else {
        pal.panel.gamma_multiply(0.5 + 0.5 * hover)
    };
    painter.rect_filled(rect, 15.0, fill);
    if hover > 0.0 && !selected {
        painter.rect_stroke(
            rect,
            15.0,
            egui::Stroke::new(1.0_f32, pal.border.gamma_multiply(hover)),
            egui::StrokeKind::Inside,
        );
    }
    let color = if selected {
        pal.text
    } else {
        theme::lerp_color(pal.text_dim, pal.text, hover)
    };
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, color);
    if mark > 0.0 {
        let dot = Pos2::new(rect.left() + 10.0, rect.center().y);
        painter.circle_filled(dot, 2.5 * mark, pal.accent.gamma_multiply(mark));
    }
    response.clicked()
}

fn widgets_button(ui: &mut egui::Ui, label: &str, pal: &crate::theme::Palette) -> bool {
    crate::widgets::ghost_button(ui, label, pal, crate::widgets::Icon::Gear).clicked()
}

/// Carte d'un jeu de boutique : couverture, titre, boutique, état, et un menu
/// d'actions qui ne s'ouvre qu'au clic sur « … ».
#[allow(clippy::too_many_arguments)]
fn provider_card(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    game: &ProviderGame,
    size: Vec2,
    appear: f32,
    texture: Option<&egui::TextureHandle>,
    install: &mut Option<u32>,
    install_request: &mut Option<crate::providers::ProviderGameKey>,
    adopt: &mut Option<crate::providers::ProviderGameKey>,
    achievements: &mut Option<crate::providers::ProviderGameKey>,
    uninstall: &mut Option<crate::providers::ProviderGameKey>,
    open: &mut Option<crate::providers::ProviderGameKey>,
    preview_save: &mut Option<crate::providers::ProviderGameKey>,
    inventory_save: &mut Option<crate::providers::ProviderGameKey>,
    choose_save_folder: &mut Option<crate::providers::ProviderGameKey>,
    remove_save_folder: &mut Option<(crate::providers::ProviderGameKey, PathBuf)>,
) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    // La carte monte et se fond en apparaissant.
    let rect = rect.translate(Vec2::new(0.0, (1.0 - appear) * 9.0));
    let mut painter = ui.painter_at(rect);
    painter.set_opacity(appear);
    let id = egui::Id::new(("quark-provider-card", game.key.product_id.as_str()));
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), response.hovered(), 0.12);
    painter.rect_filled(rect, 8.0, pal.panel);
    // Le survol soulève la couverture de 3 px, en plus de la resserrer.
    let cover_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), rect.height() - 60.0))
        .shrink(2.0 * hover)
        .translate(Vec2::new(0.0, -3.0 * hover));
    match texture {
        Some(texture) => crate::images::rounded_cover_tinted(
            ui,
            cover_rect,
            texture,
            7.0,
            egui::Color32::from_white_alpha((appear * 255.0).round() as u8),
        ),
        None => crate::widgets::monogram(
            &painter,
            cover_rect,
            &game.title.chars().next().unwrap_or('?').to_string(),
            theme::monogram_color(&game.title),
            pal,
        ),
    }
    if hover > 0.0 {
        painter.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(1.5_f32, pal.accent.gamma_multiply(hover * 0.8)),
            egui::StrokeKind::Inside,
        );
    }
    // Pastille de boutique, en haut à gauche de la couverture.
    let badge = Rect::from_min_size(
        Pos2::new(cover_rect.left() + 6.0, cover_rect.top() + 6.0),
        Vec2::new(18.0, 18.0),
    );
    painter.circle_filled(badge.center(), 9.0, egui::Color32::from_black_alpha(170));
    painter.circle_filled(badge.center(), 4.0, provider_color(game.key.provider));

    // État d'installation : liseré et point sur la couverture.
    let installed = game.install_state == InstallState::Installed;
    let status = install_label(game.install_state);
    let status_color = if installed { pal.accent } else { pal.text_dim };
    painter.circle_filled(
        Pos2::new(cover_rect.right() - 10.0, cover_rect.top() + 10.0),
        4.0,
        if installed { pal.accent } else { pal.border },
    );

    let footer = Rect::from_min_max(
        Pos2::new(rect.left() + 8.0, cover_rect.bottom() + 4.0),
        Pos2::new(rect.right() - 6.0, rect.bottom() - 2.0),
    );
    // Un enfant dédié ne fait pas avancer le curseur de la rangée : avec un
    // `scope_builder`, le pied de carte décalait la carte suivante et les
    // couvertures se chevauchaient.
    let mut footer_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(footer)
            .layout(Layout::top_down(Align::Min)),
    );
    footer_ui.set_clip_rect(footer);
    footer_ui.spacing_mut().item_spacing.y = 1.0;
    footer_ui.add(
        egui::Label::new(
            crate::i18n::rich(&game.title)
                .size(12.0)
                .strong()
                .color(pal.text),
        )
        .truncate(),
    );
    footer_ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        ui.label(
            crate::i18n::rich(buffer_or(&game.key.provider))
                .size(10.0)
                .color(pal.text_faint),
        );
        ui.label(crate::i18n::rich("·").size(10.0).color(pal.text_faint));
        if installed {
            ui.colored_label(status_color, crate::i18n::rich(status).size(10.0));
        } else {
            ui.label(crate::i18n::rich(status).size(10.0).color(status_color));
        }
    });

    // Clic gauche ou droit sur un jeu possédé mais absent : confirmation
    // d'installation (taille + dossier + Installer). Sur un jeu installé (ou
    // en cours), le clic droit ouvre le menu complet des actions, sans bouton
    // « … » dans la carte.
    let busy = matches!(
        game.install_state,
        InstallState::Installing | InstallState::Updating | InstallState::Repairing
    );
    let direct_install = game.owned && !installed && !busy;
    if !direct_install {
        response.context_menu(|ui| {
            ui.set_min_width(210.0);
            actions_menu_items(
                ui,
                pal,
                game,
                installed,
                install,
                install_request,
                adopt,
                achievements,
                uninstall,
                preview_save,
                inventory_save,
                choose_save_folder,
                remove_save_folder,
            );
        });
    }
    if (response.clicked() || response.secondary_clicked()) && direct_install {
        if game.key.provider == Provider::Steam {
            if let Some(app_id) = game.key.product_id.parse::<u32>().ok() {
                *install = Some(app_id);
            }
        } else {
            *install_request = Some(game.key.clone());
        }
    }

    // Double-clic : ouvre le jeu installé, prépare l'installation sinon.
    if response.double_clicked() {
        if installed {
            *open = Some(game.key.clone());
        } else if busy {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(400));
        } else if game.owned && game.key.provider == Provider::Steam {
            if let Some(app_id) = game.key.product_id.parse::<u32>().ok() {
                *install = Some(app_id);
            }
        } else if game.owned {
            *install_request = Some(game.key.clone());
        } else {
            *open = Some(game.key.clone());
        }
    }
    let hint = if direct_install {
        "clic gauche ou droit pour installer"
    } else {
        "clic droit pour les actions"
    };
    response.on_hover_text(format!(
        "{} · {} · {hint} · double-clic pour ouvrir",
        game.title,
        provider_name(game.key.provider)
    ));
}

/// Jeux de boutique à afficher : onglet de boutique, recherche par titre (sans
/// tenir compte des accents ni de la casse) et tri alphabétique. « Toutes » se
/// lit donc dans le même ordre que chaque onglet de boutique.
fn visible_games(
    games: &[ProviderGame],
    provider: Option<Provider>,
    query: &str,
) -> Vec<ProviderGame> {
    let needle = fold_case(query.trim());
    let mut visible: Vec<ProviderGame> = games
        .iter()
        .filter(|game| provider.is_none_or(|p| game.key.provider == p))
        .filter(|game| needle.is_empty() || fold_case(&game.title).contains(&needle))
        .cloned()
        .collect();
    visible.sort_by(|left, right| {
        fold_case(&left.title)
            .cmp(&fold_case(&right.title))
            .then_with(|| left.key.product_id.cmp(&right.key.product_id))
    });
    visible
}

/// Minuscules sans accents, pour que « Hollow Knight » se trouve en tapant
/// « hollow » comme « pokemon » trouve « Pokémon ».
fn fold_case(text: &str) -> String {
    // Minuscules d'abord : « Échos » doit devenir « echos » comme « échos ».
    text.to_lowercase()
        .chars()
        .flat_map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => "a".chars().collect::<Vec<_>>(),
            'ç' => vec!['c'],
            'è' | 'é' | 'ê' | 'ë' => vec!['e'],
            'ì' | 'í' | 'î' | 'ï' => vec!['i'],
            'ñ' => vec!['n'],
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => vec!['o'],
            'ù' | 'ú' | 'û' | 'ü' => vec!['u'],
            'ý' | 'ÿ' => vec!['y'],
            other => other.to_lowercase().collect(),
        })
        .collect()
}

fn buffer_or(provider: &Provider) -> &'static str {
    provider_name(*provider)
}

fn install_label(state: InstallState) -> &'static str {
    match state {
        InstallState::Installed => "Installé",
        InstallState::NotInstalled => "Non installé",
        InstallState::Queued => "En attente",
        InstallState::Installing => "Installation…",
        InstallState::Updating => "Mise à jour…",
        InstallState::Repairing => "Réparation…",
        InstallState::Failed => "Échec",
        InstallState::Unknown => "État inconnu",
    }
}

#[allow(clippy::too_many_arguments)]
fn actions_menu_items(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    game: &ProviderGame,
    installed: bool,
    install: &mut Option<u32>,
    install_request: &mut Option<crate::providers::ProviderGameKey>,
    adopt: &mut Option<crate::providers::ProviderGameKey>,
    achievements: &mut Option<crate::providers::ProviderGameKey>,
    uninstall: &mut Option<crate::providers::ProviderGameKey>,
    preview_save: &mut Option<crate::providers::ProviderGameKey>,
    inventory_save: &mut Option<crate::providers::ProviderGameKey>,
    choose_save_folder: &mut Option<crate::providers::ProviderGameKey>,
    remove_save_folder: &mut Option<(crate::providers::ProviderGameKey, PathBuf)>,
) {
    let app_id = game.key.product_id.parse::<u32>().ok();
    let legendary_ready = crate::providers::tools::ManagedToolId::Legendary
        .installed_path()
        .is_file();
    let ludusavi_ready = crate::providers::tools::ManagedToolId::Ludusavi
        .installed_path()
        .is_file();
    let steam_installable =
        game.owned && game.key.provider == Provider::Steam && !installed && app_id.is_some();
    let epic_installable =
        game.owned && game.key.provider == Provider::Epic && !installed && legendary_ready;
    let gogdl_ready = crate::providers::tools::ManagedToolId::Gogdl
        .installed_path()
        .is_file();
    let gog_installable =
        game.owned && game.key.provider == Provider::Gog && !installed && gogdl_ready;
    let has_row_actions = game.owned || installed || !game.custom_save_paths.is_empty();
    if !has_row_actions {
        let _ = pal;
        return;
    }
    ui.set_min_width(210.0);
    if steam_installable && ui.button(crate::i18n::tr("Installer via Steam")).clicked() {
        *install = app_id;
        ui.close();
    }
    if epic_installable && ui.button(crate::i18n::tr("Installer Epic…")).clicked() {
        *install_request = Some(game.key.clone());
        ui.close();
    }
    if gog_installable && ui.button(crate::i18n::tr("Installer GOG…")).clicked() {
        *install_request = Some(game.key.clone());
        ui.close();
    }
    if installed
        && game.quark_game_id.is_none()
        && ui
            .button(crate::i18n::tr("Ajouter à la bibliothèque"))
            .clicked()
    {
        *adopt = Some(game.key.clone());
        ui.close();
    }
    if installed
        && matches!(game.key.provider, Provider::Epic | Provider::Gog)
        && ui.button(crate::i18n::tr("Désinstaller…")).clicked()
    {
        *uninstall = Some(game.key.clone());
        ui.close();
    }
    if installed && ludusavi_ready {
        if ui.button(crate::i18n::tr("Aperçu sauvegarde")).clicked() {
            *preview_save = Some(game.key.clone());
            ui.close();
        }
        if ui.button(crate::i18n::tr("Restaurer…")).clicked() {
            *inventory_save = Some(game.key.clone());
            ui.close();
        }
    }
    ui.separator();
    match game.key.provider {
        Provider::Epic if legendary_ready => {
            if ui.button(crate::i18n::tr("Succès…")).clicked() {
                *achievements = Some(game.key.clone());
                ui.close();
            }
        }
        Provider::Epic => {
            ui.add_enabled(false, egui::Button::new(crate::i18n::tr("Succès…")))
                .on_disabled_hover_text("Legendary n'est pas installé");
        }
        Provider::Gog if gogdl_ready => {
            if ui.button(crate::i18n::tr("Succès…")).clicked() {
                *achievements = Some(game.key.clone());
                ui.close();
            }
        }
        Provider::Gog => {
            ui.add_enabled(false, egui::Button::new(crate::i18n::tr("Succès…")))
                .on_disabled_hover_text(
                    "gogdl n'est pas installé : Paramètres → Boutiques & outils",
                );
        }
        Provider::Steam => {}
    }
    if installed
        && ui
            .button(crate::i18n::tr("Dossiers de sauvegarde…"))
            .clicked()
    {
        *choose_save_folder = Some(game.key.clone());
        ui.close();
    }
    for path in &game.custom_save_paths {
        let label = format!("Retirer {}", path.display());
        if ui.button(label).clicked() {
            *remove_save_folder = Some((game.key.clone(), path.clone()));
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, ProviderGame, ProviderGameKey};

    fn game(provider: Provider, id: &str, title: &str) -> ProviderGame {
        ProviderGame {
            key: ProviderGameKey {
                provider,
                product_id: id.into(),
            },
            title: title.into(),
            owned: true,
            ..ProviderGame::default()
        }
    }

    #[test]
    fn la_liste_est_triee_par_nom_et_filtree_par_recherche() {
        let games = vec![
            game(Provider::Steam, "1", "The Witcher 3"),
            game(Provider::Gog, "2", "Cyberpunk 2077"),
            game(Provider::Steam, "3", "Échos du passé"),
        ];
        let titles = |list: Vec<ProviderGame>| -> Vec<String> {
            list.into_iter().map(|game| game.title).collect()
        };
        assert_eq!(
            titles(visible_games(&games, None, "")),
            vec!["Cyberpunk 2077", "Échos du passé", "The Witcher 3"]
        );
        // Accents et casse ignorés, sur le titre seulement.
        assert_eq!(
            titles(visible_games(&games, None, "ECHOS")),
            vec!["Échos du passé"]
        );
        assert_eq!(
            titles(visible_games(&games, Some(Provider::Steam), "")),
            vec!["Échos du passé", "The Witcher 3"]
        );
        assert!(visible_games(&games, None, "zelda").is_empty());
    }
}
