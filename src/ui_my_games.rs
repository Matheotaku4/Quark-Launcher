//! Vue des jeux détectés ou possédés par les boutiques connectées.

use crate::app::QuarkApp;
use crate::providers::{InstallState, Provider, ProviderGame};
use eframe::egui::{self, Align, Layout, RichText};

impl QuarkApp {
    pub(crate) fn my_games_tab(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let mut sync = false;
        let mut sync_epic = false;
        let mut install: Option<u32> = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.add_space(28.0);
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
                        ui.add_space(28.0);
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
                            ui.add_space(28.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("Aucun jeu de boutique synchronisé").size(16.0).color(pal.text));
                                ui.add_space(5.0);
                                ui.label(RichText::new("Lance une synchronisation Steam ou Epic si Legendary est installé. gogdl ne fournit pas de commande de liste de bibliothèque GOG.").size(12.0).color(pal.text_dim));
                            });
                        });
                    } else {                        for game in games {
                            if game.key.provider == Provider::Steam {
                                let app_id = game.key.product_id.parse::<u32>().ok();
                                let can_install = game.owned && game.install_state != InstallState::Installed && app_id.is_some();
                                provider_game_row(ui, &pal, &game, can_install, app_id, &mut install);
                            } else {
                                provider_game_row(ui, &pal, &game, false, None, &mut install);
                            }
                        }
                    }
                });
            });
        if sync {
            self.start_steam_sync(ctx);
        }
        if sync_epic {
            self.start_epic_sync(ctx);
        }
        if let Some(app_id) = install {
            match crate::providers::steam::request_install(app_id) {
                Ok(()) => self.toast_info("Demande envoyée au client Steam · l'installation sera confirmée après resynchronisation"),
                Err(error) => self.toast_error(format!("Installation Steam impossible : {error}")),
            }
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
) {
    egui::Frame::new()
        .fill(pal.panel)
        .stroke(egui::Stroke::new(1.0_f32, pal.border))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_width(ui.available_width());
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&game.title)
                            .size(13.0)
                            .strong()
                            .color(pal.text),
                    );
                    ui.label(
                        RichText::new(provider_name(game.key.provider))
                            .size(10.5)
                            .color(pal.text_faint),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if can_install && ui.small_button("Installer via Steam").clicked() {
                        *install = app_id;
                    }
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
                    ui.label(RichText::new(status).size(11.0).color(
                        if game.install_state == InstallState::Installed {
                            pal.accent
                        } else {
                            pal.text_dim
                        },
                    ));
                });
            });
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
