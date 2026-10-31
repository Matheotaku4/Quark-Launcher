//! Vue des jeux détectés ou possédés par les boutiques connectées.
//!
//! Un onglet par boutique, une grille de cartes au lieu d'une liste : la page
//! se parcourt d'un coup d'œil, et la synchronisation n'est plus ici (elle se
//! fait au démarrage et dans Paramètres → Boutiques & outils).

use crate::app::{QuarkApp, SettingsTab};
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
        let mut install_epic: Option<crate::providers::ProviderGameKey> = None;
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
                                            RichText::new("MES JEUX")
                                                .size(11.0)
                                                .strong()
                                                .color(pal.accent),
                                        );
                                        ui.label(
                                            RichText::new("Vos bibliothèques réunies")
                                                .size(26.0)
                                                .strong()
                                                .color(pal.text),
                                        );
                                        ui.label(
                                            RichText::new(if busy {
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
                                        RichText::new(format!("Steam : {error}"))
                                            .size(11.5)
                                            .color(pal.danger),
                                    );
                                }
                                if let Some(error) = &self.epic_sync_error {
                                    ui.label(
                                        RichText::new(format!("Epic : {error}"))
                                            .size(11.5)
                                            .color(pal.danger),
                                    );
                                }
                                if let Some(error) = &self.gog_sync_error {
                                    ui.label(
                                        RichText::new(format!("GOG : {error}"))
                                            .size(11.5)
                                            .color(pal.danger),
                                    );
                                }
                                if self.credentials.steam_web_api_key.is_none()
                                    || self.credentials.steam_id.is_none()
                                {
                                    ui.label(
                                        RichText::new(
                                            "Steam : les jeux installés sont détectés sans clé ; \
                                             les jeux possédés demandent une clé Web API dans \
                                             Paramètres → Boutiques & outils.",
                                        )
                                        .size(11.5)
                                        .color(pal.text_faint),
                                    );
                                }
                                ui.add_space(10.0);

                                let games: Vec<ProviderGame> = all_games
                                    .iter()
                                    .filter(|game| selected.is_none_or(|p| game.key.provider == p))
                                    .cloned()
                                    .collect();
                                if games.is_empty() {
                                    ui.add_space(20.0);
                                    ui.label(
                                        RichText::new("Aucun jeu de boutique synchronisé")
                                            .size(16.0)
                                            .color(pal.text),
                                    );
                                    ui.add_space(5.0);
                                    ui.label(
                                        RichText::new(
                                            "Connecte une boutique dans Paramètres → Boutiques & outils : \
                                             la synchronisation suit automatiquement.",
                                        )
                                        .size(12.0)
                                        .color(pal.text_dim),
                                    );
                                } else {
                                    self.provider_grid(
                                        ui,
                                        &pal,
                                        &games,
                                        &mut install,
                                        &mut install_epic,
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
                                            RichText::new(format!(
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
                                            RichText::new(format!(
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
        if let Some(key) = install_epic {
            self.request_epic_install(ctx, &key);
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
        install_epic: &mut Option<crate::providers::ProviderGameKey>,
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
        let tile_h = cover_h + 54.0;
        // Même cascade que la bibliothèque, pour que les deux pages se
        // répondent quand on passe de l'une à l'autre.
        let age = self.page_at.elapsed().as_secs_f32();

        let mut index = 0;
        while index < games.len() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for slot in 0..columns {
                    let Some(game) = games.get(index + slot) else {
                        break;
                    };
                    if slot > 0 {
                        ui.add_space(gap);
                    }
                    let cover = self.cover_for(game);
                    let texture = cover.as_ref().and_then(|path| self.texture(ui.ctx(), path));
                    let appear = ((age - (index + slot) as f32 * 0.02) / 0.3).clamp(0.0, 1.0);
                    if appear < 1.0 {
                        ui.ctx().request_repaint();
                    }
                    let choice = provider_card(
                        ui,
                        pal,
                        game,
                        Vec2::new(tile_w, tile_h),
                        appear,
                        texture.as_ref(),
                        &mut *install,
                        &mut *install_epic,
                        &mut *preview_save,
                        &mut *inventory_save,
                        &mut *choose_save_folder,
                        &mut *remove_save_folder,
                    );
                    let _ = choice;
                }
            });
            ui.add_space(gap);
            index += columns;
        }
    }

    /// Couverture d'un jeu de boutique : celle du jeu Quark de même identifiant
    /// Steam, ou de même titre. Aucun téléchargement n'est déclenché ici.
    fn cover_for(&self, game: &ProviderGame) -> Option<PathBuf> {
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
    install_epic: &mut Option<crate::providers::ProviderGameKey>,
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
    let cover_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), rect.height() - 54.0))
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
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(footer)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add(
                egui::Label::new(
                    RichText::new(&game.title)
                        .size(12.0)
                        .strong()
                        .color(pal.text),
                )
                .truncate(),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(
                    RichText::new(buffer_or(&game.key.provider))
                        .size(10.0)
                        .color(pal.text_faint),
                );
                ui.label(RichText::new("·").size(10.0).color(pal.text_faint));
                if installed {
                    ui.colored_label(status_color, RichText::new(status).size(10.0));
                } else {
                    ui.label(RichText::new(status).size(10.0).color(status_color));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    actions_menu(
                        ui,
                        pal,
                        game,
                        installed,
                        install,
                        install_epic,
                        preview_save,
                        inventory_save,
                        choose_save_folder,
                        remove_save_folder,
                    );
                });
            });
        },
    );

    response.on_hover_text(format!(
        "{} · {} · clic droit pour les actions",
        game.title,
        provider_name(game.key.provider)
    ));
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
fn actions_menu(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    game: &ProviderGame,
    installed: bool,
    install: &mut Option<u32>,
    install_epic: &mut Option<crate::providers::ProviderGameKey>,
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
    let has_row_actions =
        steam_installable || epic_installable || installed || !game.custom_save_paths.is_empty();
    if !has_row_actions {
        let _ = pal;
        return;
    }
    ui.menu_button(
        RichText::new("Actions").size(10.0).color(pal.text_dim),
        |ui| {
            ui.set_min_width(180.0);
            if steam_installable && ui.button("Installer via Steam").clicked() {
                *install = app_id;
                ui.close();
            }
            if epic_installable && ui.button("Installer via Legendary").clicked() {
                *install_epic = Some(game.key.clone());
                ui.close();
            }
            if installed && ludusavi_ready {
                if ui.button("Aperçu sauvegarde").clicked() {
                    *preview_save = Some(game.key.clone());
                    ui.close();
                }
                if ui.button("Restaurer…").clicked() {
                    *inventory_save = Some(game.key.clone());
                    ui.close();
                }
            }
            if installed && ui.button("Dossiers de sauvegarde…").clicked() {
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
        },
    );
}
