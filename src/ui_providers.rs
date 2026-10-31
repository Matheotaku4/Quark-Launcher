//! Connexion des comptes de boutiques depuis le launcher.
//!
//! Quark ne stocke aucun mot de passe : l'utilisateur se connecte sur la page
//! officielle de la boutique, copie le code d'autorisation, et l'outil officiel
//! (Legendary pour Epic, gogdl pour GOG) échange ce code contre une session.

use crate::app::{ProviderLogin, QuarkApp};
use crate::providers::tools::ManagedToolId;
use crate::providers::Provider;
use crate::theme::Palette;
use crate::widgets::{self, Icon};
use eframe::egui::{self, RichText, Vec2};

enum LoginAction {
    None,
    Cancel,
    Submit,
    OpenPage(&'static str),
}

impl QuarkApp {
    /// Bloc « Synchronisation » : état de chaque boutique et relance manuelle.
    ///
    /// C'est le seul endroit d'où l'on synchronise : la page « Mes jeux » se
    /// contente d'afficher le résultat, et le démarrage lance tout seul les
    /// boutiques connectées.
    pub(crate) fn sync_section(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, pal: &Palette) {
        let epic_logged_in = crate::providers::legendary::LegendaryAdapter::is_logged_in();
        let gog_logged_in = crate::providers::gog::GogAdapter::is_logged_in();
        let legendary_ready = ManagedToolId::Legendary.installed_path().is_file();
        let steam_owned =
            self.credentials.steam_web_api_key.is_some() && self.credentials.steam_id.is_some();
        // Compteurs et dernière synchronisation, relevés en une fois : le bloc
        // peut ensuite modifier l'application sans conflit d'emprunt.
        let summary: Vec<(usize, Option<i64>)> = [Provider::Steam, Provider::Epic, Provider::Gog]
            .into_iter()
            .map(|provider| {
                let mut count = 0;
                let mut last = None;
                for game in self.lib.provider_library.games.iter() {
                    if game.key.provider == provider {
                        count += 1;
                        if let Some(ts) = game.last_synced {
                            last = Some(last.map_or(ts, |value: i64| value.max(ts)));
                        }
                    }
                }
                (count, last)
            })
            .collect();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(
                    "Lancée automatiquement au démarrage, boutique par boutique. \
                     Chaque synchronisation remplace seulement les jeux de sa boutique.",
                )
                .size(11.5)
                .color(pal.text_dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::ghost_button(ui, "Tout synchroniser", &pal, Icon::Refresh).clicked() {
                    self.start_steam_sync(ctx);
                    if legendary_ready && epic_logged_in {
                        self.start_epic_sync(ctx);
                    }
                    if gog_logged_in {
                        self.start_gog_sync(ctx);
                    }
                }
            });
        });
        ui.add_space(8.0);

        // Erreurs copiées : le tableau ne doit pas emprunter `self`, que les
        // boutons modifient pendant le rendu.
        let steam_error = self.steam_sync_error.clone();
        let epic_error = self.epic_sync_error.clone();
        let gog_error = self.gog_sync_error.clone();
        for (index, provider, label, note, connected, busy, error) in [
            (
                0,
                Provider::Steam,
                "Steam",
                if steam_owned {
                    "Bibliothèque possédée via la clé Web API, plus les jeux installés."
                } else {
                    "Jeux installés localement (clé Web API absente : pas de jeux possédés)."
                },
                true,
                self.steam_sync_loading,
                steam_error.as_deref(),
            ),
            (
                1,
                Provider::Epic,
                "Epic Games",
                if !legendary_ready {
                    "Legendary n'est pas installé : installe-le dans « Outils gérés par Quark »."
                } else if epic_logged_in {
                    "Compte Legendary connecté."
                } else {
                    "Compte Epic non connecté."
                },
                legendary_ready && epic_logged_in,
                self.epic_sync_loading,
                epic_error.as_deref(),
            ),
            (
                2,
                Provider::Gog,
                "GOG",
                if gog_logged_in {
                    "Compte GOG connecté (bibliothèque lue via l'API GOG)."
                } else {
                    "Compte GOG non connecté."
                },
                gog_logged_in,
                self.gog_sync_loading,
                gog_error.as_deref(),
            ),
        ] {
            let (count, last) = summary[index];
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(12.5).strong().color(pal.text));
                ui.label(
                    RichText::new(if count > 1 {
                        format!("{count} jeux")
                    } else if count == 1 {
                        "1 jeu".to_string()
                    } else {
                        "aucun jeu".to_string()
                    })
                    .size(11.0)
                    .color(pal.text_faint),
                );
                if let Some(ts) = last {
                    ui.label(
                        RichText::new(crate::config::human_ago(ts))
                            .size(11.0)
                            .color(pal.text_faint),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let enabled = connected && !busy;
                    if busy {
                        ui.add(egui::Spinner::new().size(11.0));
                    }
                    ui.add_enabled_ui(enabled, |ui| {
                        let text = if busy { "En cours…" } else { "Synchroniser" };
                        if widgets::ghost_button(ui, text, &pal, Icon::Refresh).clicked() {
                            match provider {
                                Provider::Steam => self.start_steam_sync(ctx),
                                Provider::Epic => self.start_epic_sync(ctx),
                                Provider::Gog => self.start_gog_sync(ctx),
                            }
                        }
                    });
                });
            });
            ui.label(RichText::new(note).size(11.0).color(pal.text_dim));
            if let Some(error) = error {
                ui.label(RichText::new(error).size(11.0).color(pal.danger));
            }
            ui.add_space(4.0);
        }
        if self.epic_sync_loading || self.gog_sync_loading || self.steam_sync_loading {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(
                    RichText::new("Synchronisation en cours…")
                        .size(11.0)
                        .color(pal.text_faint),
                );
            });
        }
    }

    /// Bloc « Boutiques » de la page Paramètres → Boutiques & outils.
    pub(crate) fn provider_accounts(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pal: &Palette,
    ) {
        let epic_logged_in = crate::providers::legendary::LegendaryAdapter::is_logged_in();
        let gog_logged_in = crate::providers::gog::GogAdapter::is_logged_in();
        let legendary_ready = ManagedToolId::Legendary.installed_path().is_file();
        let gogdl_ready = ManagedToolId::Gogdl.installed_path().is_file();
        let mut action = LoginAction::None;
        let mut open_login: Option<Provider> = None;

        account_row(
            ui,
            pal,
            "Steam",
            "Jeux installés détectés localement ; bibliothèque possédée facultative via clé API.",
            None,
        );
        let epic_active = self
            .provider_login
            .as_ref()
            .is_some_and(|login| login.provider == Provider::Epic);
        if account_row(
            ui,
            pal,
            "Epic Games",
            epic_status(epic_logged_in, legendary_ready),
            (!epic_active && !epic_logged_in && legendary_ready).then_some("Se connecter"),
        ) {
            open_login = Some(Provider::Epic);
        }
        if epic_active {
            if let Some(login) = self.provider_login.as_mut() {
                action = login_form(
                    ui,
                    pal,
                    login,
                    "Ouvre la page de connexion Epic, connecte-toi, puis copie la valeur \
                     « authorizationCode » de la réponse JSON affichée par la page.",
                    crate::providers::legendary::LOGIN_URL,
                );
            }
        }

        let gog_active = self
            .provider_login
            .as_ref()
            .is_some_and(|login| login.provider == Provider::Gog);
        if account_row(
            ui,
            pal,
            "GOG",
            gog_status(gog_logged_in, gogdl_ready),
            (!gog_active && !gog_logged_in && gogdl_ready).then_some("Se connecter"),
        ) {
            open_login = Some(Provider::Gog);
        }
        if gog_active {
            if let Some(login) = self.provider_login.as_mut() {
                action = login_form(
                    ui,
                    pal,
                    login,
                    "Ouvre la page de connexion GOG, connecte-toi, puis copie le paramètre \
                     « code » de l'adresse de retour (embed.gog.com/on_login_success?code=…).",
                    crate::providers::gog::LOGIN_URL,
                );
            }
        }

        if let Some(provider) = open_login {
            self.open_provider_login(provider);
        }
        match action {
            LoginAction::Cancel => self.cancel_provider_login(),
            LoginAction::Submit => self.submit_provider_login(ctx),
            LoginAction::OpenPage(url) => {
                if let Err(error) = crate::news::open_in_browser(url) {
                    self.toast_error(format!("Ouverture du navigateur impossible : {error}"));
                }
            }
            LoginAction::None => {}
        }
    }
}

/// Une ligne de boutique ; renvoie vrai quand « Se connecter » a été cliqué.
fn account_row(
    ui: &mut egui::Ui,
    pal: &Palette,
    provider: &str,
    status: &str,
    action: Option<&str>,
) -> bool {
    let mut clicked = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(provider).size(12.0).color(pal.text));
        ui.label(RichText::new(status).size(11.0).color(pal.text_dim));
        if let Some(label) = action {
            if crate::widgets::ghost_button(ui, label, pal, crate::widgets::Icon::Plus).clicked() {
                clicked = true;
            }
        }
    });
    clicked
}

fn epic_status(logged_in: bool, tool_ready: bool) -> &'static str {
    if logged_in {
        "Connecté · session Legendary enregistrée"
    } else if tool_ready {
        "Non connecté · connexion avec un code Epic"
    } else {
        "Non connecté · installe Legendary dans « Outils gérés »"
    }
}

fn gog_status(logged_in: bool, tool_ready: bool) -> &'static str {
    if logged_in {
        "Connecté · session gogdl enregistrée"
    } else if tool_ready {
        "Non connecté · connexion avec un code GOG"
    } else {
        "Non connecté · installe gogdl dans « Outils gérés »"
    }
}

/// Formulaire de connexion : page officielle, champ de code, validation.
fn login_form(
    ui: &mut egui::Ui,
    pal: &Palette,
    login: &mut ProviderLogin,
    instructions: &str,
    page: &'static str,
) -> LoginAction {
    let mut action = LoginAction::None;
    let busy = login.rx.is_some();
    egui::Frame::new()
        .fill(pal.surface)
        .stroke(egui::Stroke::new(1.0_f32, pal.border))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.label(RichText::new(instructions).size(11.5).color(pal.text_dim));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if crate::widgets::ghost_button(
                    ui,
                    "Ouvrir la page de connexion",
                    pal,
                    crate::widgets::Icon::Info,
                )
                .clicked()
                {
                    action = LoginAction::OpenPage(page);
                }
                ui.add_space(10.0);
                let field = ui.add_sized(
                    Vec2::new(320.0, 28.0),
                    egui::TextEdit::singleline(&mut login.code)
                        .password(true)
                        .hint_text("Code d'autorisation"),
                );
                let submitted =
                    field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if ui
                    .add_enabled(
                        !busy && !login.code.trim().is_empty(),
                        egui::Button::new("Connecter"),
                    )
                    .clicked()
                    || (submitted && !busy && !login.code.trim().is_empty())
                {
                    action = LoginAction::Submit;
                }
                if ui
                    .add_enabled(!busy, egui::Button::new("Annuler"))
                    .clicked()
                {
                    action = LoginAction::Cancel;
                }
                if busy {
                    ui.spinner();
                }
            });
            if let Some(error) = &login.error {
                ui.label(RichText::new(error).size(11.5).color(pal.danger));
            } else if let Some(status) = &login.status {
                ui.label(RichText::new(status).size(11.0).color(pal.accent));
            }
            ui.label(
                RichText::new(
                    "Le code est échangé par l'outil officiel ; Quark ne conserve que la session \
                     écrite par cet outil, et n'affiche jamais le code ni les jetons.",
                )
                .size(10.5)
                .color(pal.text_faint),
            );
        });
    ui.add_space(4.0);
    action
}
