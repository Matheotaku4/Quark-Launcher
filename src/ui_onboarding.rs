//! Premier lancement et résumé de mise à jour.
//!
//! Le parcours de bienvenue n'est pas un tutoriel bloquant : il présente les
//! endroits où Quark se règle, laisse déjà changer la langue et le thème, puis
//! se termine en un clic. Le résumé de version est volontairement court et
//! n'apparaît qu'une fois par version.

use crate::app::QuarkApp;
use crate::config::{StartPage, ThemeMode, UiLanguage};
use crate::i18n;
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, Sense, Stroke, Vec2};

impl QuarkApp {
    pub(crate) fn onboarding_dialog(&mut self, ctx: &egui::Context) {
        if !self.onboarding_open {
            return;
        }
        let pal = self.pal;
        let screen = ctx.screen_rect();
        let width = (screen.width() - 48.0).clamp(420.0, 920.0);
        let mut finish = false;
        let mut skip = false;
        let mut back = false;
        let mut next = false;

        egui::Area::new(egui::Id::new("quark_onboarding_backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::hover());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(220));
            });

        egui::Area::new(egui::Id::new("quark_onboarding"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(pal.panel)
                    .stroke(Stroke::new(1.0_f32, pal.border))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::symmetric(30, 24))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    i18n::rich("Bienvenue dans Quark")
                                        .size(25.0)
                                        .strong()
                                        .color(pal.text),
                                );
                                ui.label(
                                    i18n::rich("Quelques réglages, puis votre bibliothèque.")
                                        .size(12.0)
                                        .color(pal.text_dim),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                for language in UiLanguage::ALL {
                                    let selected = self.lib.settings.ui_language == language;
                                    if ui.selectable_label(selected, language.short()).clicked() {
                                        self.lib.settings.ui_language = language;
                                        i18n::set_language(language);
                                        self.touch();
                                    }
                                }
                                ui.label(i18n::rich("Langue").size(10.5).color(pal.text_faint));
                            });
                        });
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(12.0);

                        // Trois étapes : découvrir, régler, commencer.
                        ui.horizontal(|ui| {
                            for (index, label) in ["Découvrir", "Personnaliser", "Commencer"]
                                .iter()
                                .enumerate()
                            {
                                let selected = self.onboarding_step == index;
                                let color = if selected { pal.accent } else { pal.text_faint };
                                let (dot, _) =
                                    ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                                ui.painter().circle_filled(dot.center(), 4.0, color);
                                ui.label(i18n::rich(*label).size(10.5).color(if selected {
                                    pal.text
                                } else {
                                    pal.text_faint
                                }));
                                ui.add_space(8.0);
                            }
                        });
                        ui.add_space(12.0);

                        egui::ScrollArea::vertical()
                            .id_salt("quark_onboarding_scroll")
                            .max_height((screen.height() - 300.0).max(280.0))
                            .auto_shrink([false, true])
                            .show(ui, |ui| match self.onboarding_step {
                                0 => self.onboarding_welcome(ui),
                                1 => self.onboarding_customize(ui),
                                _ => self.onboarding_ready(ui),
                            });

                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if widgets::ghost_button(ui, "Passer", &pal, Icon::None).clicked() {
                                skip = true;
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if self.onboarding_step < 2 {
                                    if widgets::accent_button(
                                        ui,
                                        Vec2::new(130.0, 34.0),
                                        "Suivant",
                                        &pal,
                                        true,
                                        Icon::None,
                                    )
                                    .clicked()
                                    {
                                        next = true;
                                    }
                                } else if widgets::accent_button(
                                    ui,
                                    Vec2::new(150.0, 34.0),
                                    "Terminer",
                                    &pal,
                                    true,
                                    Icon::None,
                                )
                                .clicked()
                                {
                                    finish = true;
                                }
                                if self.onboarding_step > 0
                                    && widgets::ghost_button(ui, "Retour", &pal, Icon::ArrowLeft)
                                        .clicked()
                                {
                                    back = true;
                                }
                            });
                        });
                    });
            });

        if skip || finish {
            self.finish_onboarding();
        } else if next {
            self.onboarding_step = (self.onboarding_step + 1).min(2);
        } else if back {
            self.onboarding_step = self.onboarding_step.saturating_sub(1);
        }
    }

    fn onboarding_welcome(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        ui.label(i18n::rich("Ce que Quark réunit").size(16.0).color(pal.text));
        ui.add_space(4.0);
        ui.label(
            i18n::rich(
                "Une seule bibliothèque pour les jeux locaux, Steam, Epic et GOG, avec les mêmes artworks, tags, notes et statistiques.",
            )
            .size(12.0)
            .color(pal.text_dim),
        );
        ui.add_space(12.0);
        ui.columns(2, |columns| {
            onboarding_card(
                &mut columns[0],
                &pal,
                Icon::Grid,
                "Bibliothèque",
                "Ajoutez un jeu, importez Steam, classez par statut, tag ou genre, puis enregistrez vos vues.",
            );
            onboarding_card(
                &mut columns[0],
                &pal,
                Icon::Steam,
                "Boutiques",
                "Steam, Epic et GOG : comptes, installations, temps de jeu et sauvegardes cloud.",
            );
            onboarding_card(
                &mut columns[1],
                &pal,
                Icon::Play,
                "Lancement",
                "Proton, umu, GameMode, MangoHud, gamescope, PRIME et variables d'environnement.",
            );
            onboarding_card(
                &mut columns[1],
                &pal,
                Icon::Info,
                "Personnalisation",
                "Treize thèmes, page d'ouverture, fond flouté, langue française ou anglaise.",
            );
        });
    }

    fn onboarding_customize(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        ui.label(
            i18n::rich("Rendez Quark à votre image")
                .size(16.0)
                .color(pal.text),
        );
        ui.add_space(8.0);
        ui.label(
            i18n::rich("Langue de l'interface")
                .size(12.0)
                .color(pal.text_dim),
        );
        ui.horizontal(|ui| {
            for language in UiLanguage::ALL {
                if ui
                    .radio_value(
                        &mut self.lib.settings.ui_language,
                        language,
                        language.label(),
                    )
                    .changed()
                {
                    i18n::set_language(language);
                    self.touch();
                }
            }
        });
        ui.add_space(12.0);
        ui.label(
            i18n::rich("Page d'ouverture")
                .size(12.0)
                .color(pal.text_dim),
        );
        ui.horizontal_wrapped(|ui| {
            for page in StartPage::ALL {
                if ui
                    .radio_value(&mut self.lib.settings.start_page, page, page.label())
                    .changed()
                {
                    self.touch();
                }
            }
        });
        ui.add_space(12.0);
        ui.label(i18n::rich("Thème").size(12.0).color(pal.text_dim));
        ui.horizontal_wrapped(|ui| {
            for mode in [
                ThemeMode::Amoled,
                ThemeMode::CatppuccinMocha,
                ThemeMode::Dracula,
                ThemeMode::TokyoNight,
            ] {
                if theme_preview(ui, mode, self.lib.settings.theme == mode) {
                    self.lib.settings.theme = mode;
                    self.touch();
                }
            }
        });
        ui.add_space(12.0);
        if widgets::setting_toggle(
            ui,
            &pal,
            "Fond hero aléatoire",
            Some(
                "Une bannière de la bibliothèque, floutée, derrière l'accueil et la bibliothèque. Un nouveau tirage à chaque démarrage.",
            ),
            &mut self.lib.settings.hero_background,
            "onboarding_hero",
        )
        .changed()
        {
            if self.lib.settings.hero_background {
                self.pick_random_background();
            }
            self.touch();
        }
        if self.lib.settings.hero_background {
            ui.horizontal(|ui| {
                ui.label(i18n::rich("Flou").size(11.5).color(pal.text_dim));
                let mut blur = self.lib.settings.hero_background_blur;
                if ui
                    .add(egui::Slider::new(&mut blur, 2.0..=30.0).suffix(" px"))
                    .changed()
                {
                    self.lib.settings.hero_background_blur = blur;
                    self.touch();
                }
            });
        }
    }

    fn onboarding_ready(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal;
        ui.label(i18n::rich("Tout est prêt").size(16.0).color(pal.text));
        ui.add_space(5.0);
        ui.label(
            i18n::rich(
                "La bibliothèque est vide pour l'instant. Ajoutez un jeu, importez votre bibliothèque Steam, ou explorez les boutiques.",
            )
            .size(12.0)
            .color(pal.text_dim),
        );
        ui.add_space(12.0);
        onboarding_line(
            ui,
            &pal,
            "Ctrl+K",
            "Ouvre la palette : pages, actions et jeux.",
        );
        onboarding_line(
            ui,
            &pal,
            "Ctrl+F",
            "Cherche dans la bibliothèque ; Échap vide la recherche.",
        );
        onboarding_line(
            ui,
            &pal,
            "F12",
            "Capture d'écran en jeu ; Maj+Retour arrière ouvre l'overlay.",
        );
        ui.add_space(12.0);
        if widgets::accent_button(
            ui,
            Vec2::new(220.0, 38.0),
            "Ajouter mon premier jeu",
            &pal,
            true,
            Icon::Plus,
        )
        .clicked()
        {
            self.finish_onboarding();
            self.open_add_dialog();
        }
    }

    fn finish_onboarding(&mut self) {
        self.onboarding_open = false;
        self.onboarding_step = 0;
        self.lib.settings.onboarding_done = true;
        self.lib.settings.last_seen_version = crate::update::current_version().to_string();
        self.whats_new_open = false;
        self.touch();
    }

    pub(crate) fn whats_new_dialog(&mut self, ctx: &egui::Context) {
        if !self.whats_new_open {
            return;
        }
        let pal = self.pal;
        let mut close = false;
        egui::Modal::new(egui::Id::new("quark_whats_new")).show(ctx, |ui| {
            ui.set_width(620.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        i18n::rich(i18n::release_title())
                            .size(21.0)
                            .strong()
                            .color(pal.text),
                    );
                    ui.label(
                        i18n::rich("Les changements concrets de cette version.")
                            .size(11.5)
                            .color(pal.text_faint),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::icon_button(ui, 28.0, Icon::Close, &pal, "Fermer").clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            for (index, sentence) in i18n::release_summary().iter().enumerate() {
                ui.horizontal_top(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::hover());
                    ui.painter().circle_filled(
                        rect.center(),
                        10.0,
                        pal.accent.gamma_multiply(0.18),
                    );
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        format!("{}", index + 1),
                        FontId::proportional(10.5),
                        pal.accent,
                    );
                    ui.add_space(8.0);
                    ui.add(
                        egui::Label::new(i18n::rich(*sentence).size(12.5).color(pal.text_dim))
                            .wrap(),
                    );
                });
                ui.add_space(6.0);
            }
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if widgets::accent_button(
                    ui,
                    Vec2::new(130.0, 34.0),
                    "Compris",
                    &pal,
                    true,
                    Icon::None,
                )
                .clicked()
                {
                    close = true;
                }
                ui.add_space(8.0);
                if widgets::ghost_button(ui, "Voir la release", &pal, Icon::Info).clicked() {
                    let _ = crate::news::open_in_browser(
                        "https://github.com/Matheotaku4/Quark-Launcher/releases/latest",
                    );
                }
            });
        });
        if close {
            self.whats_new_open = false;
            self.lib.settings.last_seen_version = crate::update::current_version().to_string();
            self.touch();
        }
    }
}

fn onboarding_card(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    icon: Icon,
    title: &str,
    detail: &str,
) {
    egui::Frame::new()
        .fill(pal.surface)
        .stroke(Stroke::new(1.0_f32, pal.border))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_height(82.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                ui.painter()
                    .circle_filled(rect.center(), 12.0, pal.accent.gamma_multiply(0.16));
                widgets::paint_icon(ui, rect.shrink(5.0), icon, pal.accent);
                ui.add_space(6.0);
                ui.label(i18n::rich(title).size(13.0).strong().color(pal.text));
            });
            ui.add_space(4.0);
            ui.label(i18n::rich(detail).size(11.0).color(pal.text_dim));
        });
    ui.add_space(8.0);
}

fn onboarding_line(ui: &mut egui::Ui, pal: &crate::theme::Palette, key: &str, detail: &str) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(92.0, 24.0), Sense::hover());
        ui.painter().rect_filled(rect, 4.0, pal.surface);
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            key,
            FontId::monospace(10.5),
            pal.accent,
        );
        ui.add_space(10.0);
        ui.label(i18n::rich(detail).size(11.5).color(pal.text_dim));
    });
}

fn theme_preview(ui: &mut egui::Ui, mode: ThemeMode, selected: bool) -> bool {
    let preview = crate::theme::palette(mode);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(150.0, 66.0), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, preview.bg);
    painter.rect_stroke(
        rect,
        6.0,
        Stroke::new(
            if selected { 2.0 } else { 1.0 },
            if selected {
                preview.accent
            } else {
                preview.border
            },
        ),
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left() + 12.0, rect.top() + 11.0),
            Vec2::new(58.0, 7.0),
        ),
        3.0,
        preview.accent,
    );
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left() + 12.0, rect.top() + 25.0),
            Vec2::new(92.0, 5.0),
        ),
        2.0,
        preview.surface_hover,
    );
    painter.text(
        Pos2::new(rect.left() + 12.0, rect.bottom() - 9.0),
        Align2::LEFT_BOTTOM,
        mode.label(),
        FontId::proportional(10.5),
        preview.text,
    );
    response.clicked()
}
