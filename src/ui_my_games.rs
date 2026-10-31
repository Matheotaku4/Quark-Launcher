//! Vue des jeux détectés ou possédés par les boutiques connectées.

use crate::app::QuarkApp;
use crate::providers::{InstallState, Provider, ProviderGame};
use eframe::egui::{self, Align, Layout, RichText, Vec2};

impl QuarkApp {
    pub(crate) fn my_games_tab(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let mut sync = false;
        let mut sync_epic = false;
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
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // Marge à droite comprise : les boutons ne touchent jamais le
                    // bord de la fenêtre, même en 1024 de large.
                    ui.horizontal(|ui| {
                        ui.add_space(28.0);
                        ui.vertical(|ui| {
                            ui.set_max_width((ui.available_width() - 28.0).max(240.0));
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("MES JEUX").size(11.0).strong().color(pal.accent));
                            ui.label(RichText::new("Vos bibliothèques réunies").size(26.0).strong().color(pal.text));
                            ui.label(RichText::new("Les installations locales sont détectées indépendamment des données de compte.").size(12.0).color(pal.text_dim));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(!self.steam_sync_loading, egui::Button::new(if self.steam_sync_loading { "Synchronisation…" } else { "Synchroniser Steam" })).clicked() { sync = true; }
                            let epic_available = crate::providers::tools::ManagedToolId::Legendary.installed_path().is_file();
                            let epic_label = if self.epic_sync_loading { "Synchronisation Epic…" } else { "Synchroniser Epic" };
                            if ui.add_enabled(epic_available && !self.epic_sync_loading, egui::Button::new(epic_label)).clicked() { sync_epic = true; }
                        });
                    });
                    ui.add_space(16.0);
                    ui.horizontal_wrapped(|ui| {
                        for provider in [Provider::Steam, Provider::Epic, Provider::Gog] {
                            let name = provider_name(provider);
                            ui.label(RichText::new(format!("{name} · {}", self.lib.provider_library.games.iter().filter(|game| game.key.provider == provider).count())).size(11.0).color(pal.text_dim));
                            ui.add_space(12.0);
                        }
                    });
                    if let Some(error) = &self.steam_sync_error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(error).size(11.5).color(pal.danger));
                    }
                    if let Some(error) = &self.epic_sync_error {
                        ui.add_space(8.0);
                        ui.label(RichText::new(error).size(11.5).color(pal.danger));
                    }
                    if self.credentials.steam_web_api_key.is_none() || self.credentials.steam_id.is_none() {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Steam : les jeux installés sont détectables sans clé. Pour les jeux possédés non installés, ajoute une clé Web API et un SteamID64 dans Paramètres → Boutiques & outils.").size(11.5).color(pal.text_faint));
                    }
                    ui.add_space(10.0);
                    let games = self.lib.provider_library.games.clone();
                    if games.is_empty() {
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new("Aucun jeu de boutique synchronisé").size(16.0).color(pal.text));
                                ui.add_space(5.0);
                                ui.label(RichText::new("Lance une synchronisation Steam ou Epic si Legendary est installé. gogdl ne fournit pas de commande de liste de bibliothèque GOG.").size(12.0).color(pal.text_dim));
                            });
                        });
                    } else {
                        for game in &games {
                            if game.key.provider == Provider::Steam {
                                let app_id = game.key.product_id.parse::<u32>().ok();
                                let can_install = game.owned && game.install_state != InstallState::Installed && app_id.is_some();
                                provider_game_row(ui, &pal, game, can_install, app_id, &mut install, &mut install_epic, &mut preview_save, &mut inventory_save, &mut choose_save_folder, &mut remove_save_folder);
                            } else {
                                provider_game_row(ui, &pal, game, false, None, &mut install, &mut install_epic, &mut preview_save, &mut inventory_save, &mut choose_save_folder, &mut remove_save_folder);
                            }
                        }
                        for candidate in crate::providers::operations::match_candidates(&games) {
                            let labels: Vec<String> = candidate.members.iter().filter_map(|key| games.iter().find(|game| &game.key == key).map(|game| format!("{} ({})", game.title, provider_name(game.key.provider)))).collect();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("Correspondance exacte : {}", labels.join(" · "))).size(11.0).color(pal.text_dim));
                                if ui.small_button("Fusionner l’affichage").clicked() { merge_keys = candidate.members.clone(); }
                            });
                        }
                        for group in &self.lib.provider_library.manual_groups {
                            ui.horizontal(|ui| {
                                let labels: Vec<String> = group.members.iter().filter_map(|key| games.iter().find(|game| &game.key == key).map(|game| game.title.clone())).collect();
                                ui.label(RichText::new(format!("Groupe manuel : {}", labels.join(" · "))).size(11.0).color(pal.text_dim));
                                for key in &group.members { if ui.small_button(format!("Séparer {}", key.product_id)).clicked() { split_key = Some(key.clone()); } }
                            });
                        }
                    }
                        });
                    });
                });
            });
        if sync {
            self.start_steam_sync(ctx);
        }
        if sync_epic {
            self.start_epic_sync(ctx);
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
                    self.toast_info("Demande envoyée au client Steam · vérifie ensuite l'installation avec « Synchroniser Steam »");
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
}

fn provider_game_row(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    game: &ProviderGame,
    can_install: bool,
    app_id: Option<u32>,
    install: &mut Option<u32>,
    install_epic: &mut Option<crate::providers::ProviderGameKey>,
    preview_save: &mut Option<crate::providers::ProviderGameKey>,
    inventory_save: &mut Option<crate::providers::ProviderGameKey>,
    choose_save_folder: &mut Option<crate::providers::ProviderGameKey>,
    remove_save_folder: &mut Option<(crate::providers::ProviderGameKey, std::path::PathBuf)>,
) {
    let status = match game.install_state {
        InstallState::Installed => "Installé",
        InstallState::NotInstalled => "Non installé",
        InstallState::Queued => "En attente",
        InstallState::Installing => "Installation…",
        InstallState::Updating => "Mise à jour…",
        InstallState::Repairing => "Réparation…",
        InstallState::Failed => "Échec",
        InstallState::Unknown => "État inconnu",
    };
    let status_color = if game.install_state == InstallState::Installed {
        pal.accent
    } else {
        pal.text_dim
    };
    egui::Frame::new()
        .fill(pal.panel)
        .stroke(egui::Stroke::new(1.0_f32, pal.border))
        .corner_radius(crate::theme::radius::LG)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            // Le titre garde une largeur bornée et le statut reste au bord
            // droit : rien ne déborde quand la fenêtre est étroite.
            ui.horizontal(|ui| {
                let status_width = 96.0;
                let title_width = (ui.available_width() - status_width).max(120.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(title_width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&game.title)
                                    .size(13.0)
                                    .strong()
                                    .color(pal.text),
                            )
                            .truncate(),
                        );
                        // Nom de la boutique dans une petite pastille : il ne se
                        // confond plus avec le statut, écrit en gris lui aussi.
                        egui::Frame::new()
                            .fill(pal.surface)
                            .corner_radius(crate::theme::radius::SM)
                            .inner_margin(egui::Margin::symmetric(6, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(provider_name(game.key.provider))
                                        .size(10.0)
                                        .color(pal.text_dim),
                                );
                            });
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(status).size(11.0).color(status_color));
                });
            });
            let installed = game.install_state == InstallState::Installed;
            let legendary_ready = crate::providers::tools::ManagedToolId::Legendary
                .installed_path()
                .is_file();
            let ludusavi_ready = crate::providers::tools::ManagedToolId::Ludusavi
                .installed_path()
                .is_file();
            let epic_installable =
                game.owned && game.key.provider == Provider::Epic && !installed && legendary_ready;
            let has_actions = (can_install && game.key.provider == Provider::Steam)
                || epic_installable
                || (installed && ludusavi_ready)
                || installed
                || !game.custom_save_paths.is_empty();
            if has_actions {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if can_install
                        && game.key.provider == Provider::Steam
                        && ui.small_button("Installer via Steam").clicked()
                    {
                        *install = app_id;
                    }
                    if epic_installable && ui.small_button("Installer via Legendary").clicked() {
                        *install_epic = Some(game.key.clone());
                    }
                    if installed && ludusavi_ready && ui.small_button("Aperçu sauvegarde").clicked()
                    {
                        *preview_save = Some(game.key.clone());
                    }
                    if installed && ludusavi_ready && ui.small_button("Restaurer…").clicked() {
                        *inventory_save = Some(game.key.clone());
                    }
                    if installed && ui.small_button("Dossiers de sauvegarde…").clicked() {
                        *choose_save_folder = Some(game.key.clone());
                    }
                    for path in &game.custom_save_paths {
                        ui.label(
                            RichText::new(format!("Dossier sauvegarde · {}", path.display()))
                                .size(10.0)
                                .color(pal.text_faint),
                        );
                        if ui.small_button("Retirer").clicked() {
                            *remove_save_folder = Some((game.key.clone(), path.clone()));
                        }
                    }
                });
            }
        });
    ui.add_space(5.0);
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Steam => "Steam",
        Provider::Epic => "Epic Games",
        Provider::Gog => "GOG",
    }
}
