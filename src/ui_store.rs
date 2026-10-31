//! Boutique Steam : découverte verticale par sections, sans achat intégré.

use crate::app::QuarkApp;
use crate::steam_store::{StoreItem, StoreSection};
use crate::theme::{self, Palette};
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align, Align2, FontId, Layout, RichText, Sense, Vec2};

const PAGE_MARGIN: f32 = theme::space::PAGE;
const CARD_WIDTH: f32 = 228.0;
const CARD_GAP: f32 = theme::space::GUTTER;
const CARDS_PER_SECTION: usize = 24;

impl QuarkApp {
    pub(crate) fn store_tab(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let mut open_item: Option<StoreItem> = None;
        let mut do_search = false;
        let mut refresh = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("quark_store_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(PAGE_MARGIN);
                            ui.vertical(|ui| {
                                ui.set_max_width((ui.available_width() - PAGE_MARGIN).max(240.0));
                                ui.add_space(22.0);
                                ui.label(
                                    RichText::new("BOUTIQUE STEAM")
                                        .size(11.0)
                                        .strong()
                                        .color(pal.accent),
                                );
                                ui.add_space(5.0);
                                ui.label(
                                    RichText::new("Découvrir votre prochain jeu")
                                        .size(27.0)
                                        .strong()
                                        .color(pal.text),
                                );
                                ui.label(
                                    RichText::new(
                                        "Catalogue régional Steam · les achats se font sur le site officiel",
                                    )
                                    .size(12.0)
                                    .color(pal.text_dim),
                                );
                                ui.add_space(16.0);

                                ui.horizontal(|ui| {
                                    let field_width = (ui.available_width() - 250.0).max(160.0);
                                    let field = widgets::search_field(
                                        ui,
                                        &mut self.store_query,
                                        "Rechercher un jeu sur Steam…",
                                        field_width,
                                        &pal,
                                    );
                                    do_search |= field.lost_focus()
                                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                                    ui.add_space(8.0);
                                    let ready = self.store_query.trim().len() >= 2
                                        && !self.store_search_loading;
                                    if widgets::accent_button(
                                        ui,
                                        Vec2::new(104.0, 30.0),
                                        "Chercher",
                                        &pal,
                                        ready,
                                        Icon::Search,
                                    )
                                    .clicked()
                                    {
                                        do_search = true;
                                    }
                                    ui.add_space(4.0);
                                    if widgets::ghost_button(ui, "Actualiser", &pal, Icon::Refresh)
                                        .clicked()
                                    {
                                        refresh = true;
                                    }
                                });

                                if self.store_search_loading {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(
                                            RichText::new("Recherche dans Steam…")
                                                .size(12.0)
                                                .color(pal.text_dim),
                                        );
                                    });
                                }
                                if let Some(error) = &self.store_search_error {
                                    ui.label(RichText::new(error).size(11.5).color(pal.danger));
                                }
                                if let Some(results) = self.store_search_results.clone() {
                                    heading(ui, "RÉSULTATS", &pal, None);
                                    if results.is_empty() {
                                        ui.label(
                                            RichText::new("Aucun jeu trouvé.")
                                                .size(12.0)
                                                .color(pal.text_dim),
                                        );
                                    } else {
                                        card_grid(ui, &pal, "recherche", &results, &mut open_item);
                                    }
                                }

                                if self.store_loading {
                                    ui.horizontal(|ui| {
                                        ui.spinner();
                                        ui.label(
                                            RichText::new("Chargement du catalogue Steam…")
                                                .size(12.0)
                                                .color(pal.text_dim),
                                        );
                                    });
                                }
                                if let Some(error) = &self.store_error {
                                    ui.label(
                                        RichText::new(format!(
                                            "{error} · les dernières offres en cache restent affichées."
                                        ))
                                        .size(11.5)
                                        .color(pal.text_faint),
                                    );
                                }
                                if let Some(notice) = &self.store_notice {
                                    ui.label(
                                        RichText::new(notice).size(11.0).color(pal.text_faint),
                                    );
                                }
                                if self.store_stale && self.store_snapshot.is_some() {
                                    ui.label(
                                        RichText::new("Cache local · données potentiellement périmées")
                                            .size(11.0)
                                            .color(pal.text_faint),
                                    );
                                }

                                let snapshot = self.store_snapshot.clone();
                                if let Some(snapshot) = snapshot {
                                    if let Some(hero) = hero_item(&snapshot.sections) {
                                        hero_card(ui, &pal, &hero, &mut open_item);
                                    }
                                    if let (Some(source), false) = (
                                        self.store_recommendation_source.as_deref(),
                                        self.store_recommendations.is_empty(),
                                    ) {
                                        heading(
                                            ui,
                                            "RECOMMANDÉ POUR VOUS",
                                            &pal,
                                            Some(&format!(
                                                "Classées à partir de {source} · aucune donnée de navigation n'est envoyée à Steam"
                                            )),
                                        );
                                        card_grid(
                                            ui,
                                            &pal,
                                            "recommandations",
                                            &self.store_recommendations,
                                            &mut open_item,
                                        );
                                    }
                                    for section in &snapshot.sections {
                                        let caption = section_caption(section);
                                        heading(
                                            ui,
                                            &section.title.to_uppercase(),
                                            &pal,
                                            caption.as_deref(),
                                        );
                                        card_grid(
                                            ui,
                                            &pal,
                                            &section.title,
                                            &section.items,
                                            &mut open_item,
                                        );
                                    }
                                } else if !self.store_loading {
                                    let message = if self.store_error.is_some() {
                                        "Le catalogue Steam est indisponible et aucun cache n'est encore enregistré."
                                    } else {
                                        "Le catalogue Steam apparaîtra ici."
                                    };
                                    ui.label(
                                        RichText::new(message).size(12.0).color(pal.text_dim),
                                    );
                                }
                                ui.add_space(30.0);
                            });
                        });
                    });
            });
        if do_search {
            self.search_store(ctx);
        }
        if refresh {
            self.refresh_store(ctx);
        }
        if let Some(item) = open_item {
            self.open_store_item(&item);
        }
    }
}

fn section_caption(section: &StoreSection) -> Option<String> {
    let with_players = section
        .items
        .iter()
        .filter(|item| item.players_peak.is_some())
        .count();
    (with_players > 0).then(|| format!("{with_players} jeux avec leur pic de joueurs simultanés"))
}

/// Première offre en promotion, mise en avant en haut de page.
fn hero_item(sections: &[StoreSection]) -> Option<StoreItem> {
    sections
        .iter()
        .flat_map(|section| section.items.iter())
        .filter(|item| item.artwork_url.is_some())
        .max_by_key(|item| {
            item.price
                .as_ref()
                .and_then(|price| price.discount_percent)
                .unwrap_or(0)
        })
        .cloned()
}

fn heading(ui: &mut egui::Ui, text: &str, pal: &Palette, caption: Option<&str>) {
    ui.add_space(theme::space::SECTION);
    widgets::section_title(ui, text, pal);
    if let Some(caption) = caption {
        ui.label(RichText::new(caption).size(10.5).color(pal.text_faint));
    }
    ui.add_space(10.0);
}

fn hero_card(
    ui: &mut egui::Ui,
    pal: &Palette,
    item: &StoreItem,
    open_item: &mut Option<StoreItem>,
) {
    let width = ui.available_width().min(1000.0);
    let image_width = (width * 0.58).clamp(240.0, 620.0);
    ui.add_space(18.0);
    egui::Frame::new()
        .fill(pal.panel)
        .stroke(egui::Stroke::new(1.0_f32, pal.border))
        .corner_radius(theme::radius::XL)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(url) = &item.artwork_url {
                    ui.add(
                        egui::Image::new(url)
                            .corner_radius(theme::radius::MD)
                            .fit_to_exact_size(Vec2::new(image_width, image_width / 1.745)),
                    );
                }
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("À LA UNE")
                            .size(10.5)
                            .strong()
                            .color(pal.accent),
                    );
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&item.name)
                                .size(24.0)
                                .strong()
                                .color(pal.text),
                        )
                        .truncate(),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Achat et téléchargement sur le site officiel de Steam.")
                            .size(11.5)
                            .color(pal.text_faint),
                    );
                    // Les mêmes repères que les cartes : la colonne de droite
                    // du bandeau ne reste pas vide sous le bouton.
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if item.linux_available == Some(true) {
                            chip_label(ui, pal, "Linux natif");
                        }
                        if let Some(peak) = item.players_peak {
                            chip_label(ui, pal, &format!("{} joueurs simultanés", thousands(peak)));
                        }
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        price_widgets(ui, pal, item, true);
                        ui.add_space(6.0);
                        if widgets::ghost_button(ui, "Voir sur Steam", pal, Icon::None).clicked() {
                            *open_item = Some(item.clone());
                        }
                    });
                });
            });
        });
}

fn card_grid(
    ui: &mut egui::Ui,
    pal: &Palette,
    salt: &str,
    items: &[StoreItem],
    open_item: &mut Option<StoreItem>,
) {
    let available = ui.available_width();
    let per_row = (((available + CARD_GAP) / (CARD_WIDTH + CARD_GAP)).floor() as usize).max(1);
    let shown: Vec<&StoreItem> = items.iter().take(CARDS_PER_SECTION).collect();
    for (row, chunk) in shown.chunks(per_row).enumerate() {
        ui.push_id((salt, row), |ui| {
            // Alignement en haut : les cartes d'une même rangée ne se
            // décalent pas quand l'une d'elles a une ligne de plus.
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                for item in chunk {
                    card(ui, pal, item, open_item);
                    ui.add_space(CARD_GAP);
                }
            });
        });
        ui.add_space(CARD_GAP);
    }
}

fn card(ui: &mut egui::Ui, pal: &Palette, item: &StoreItem, open_item: &mut Option<StoreItem>) {
    egui::Frame::new()
        .fill(pal.panel)
        .stroke(egui::Stroke::new(1.0_f32, pal.border))
        .corner_radius(theme::radius::LG)
        .inner_margin(9.0)
        .show(ui, |ui| {
            // La grille est dessinée dans un `ui.horizontal` : sans ce
            // `vertical`, le contenu de la carte s'étalerait sur la ligne.
            ui.vertical(|ui| {
                ui.set_width(CARD_WIDTH - 16.0);
                card_body(ui, pal, item, open_item);
            });
        });
}

fn card_body(
    ui: &mut egui::Ui,
    pal: &Palette,
    item: &StoreItem,
    open_item: &mut Option<StoreItem>,
) {
    let art_size = Vec2::new(CARD_WIDTH - 16.0, (CARD_WIDTH - 16.0) / 1.745);
    match &item.artwork_url {
        Some(url) => {
            ui.add(
                egui::Image::new(url)
                    .corner_radius(theme::radius::MD)
                    .fit_to_exact_size(art_size),
            );
        }
        None => {
            let (rect, _) = ui.allocate_exact_size(art_size, Sense::hover());
            ui.painter().rect_filled(rect, 4.0, pal.surface);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Steam",
                FontId::proportional(13.0),
                pal.text_faint,
            );
        }
    }
    ui.add_space(6.0);
    ui.allocate_ui_with_layout(
        Vec2::new(art_size.x, 34.0),
        Layout::top_down(Align::Min),
        |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(&item.name)
                        .size(12.0)
                        .strong()
                        .color(pal.text),
                )
                .truncate(),
            );
            if item.linux_available == Some(true) {
                ui.label(RichText::new("Linux natif").size(10.5).color(pal.accent));
            }
        },
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        price_widgets(ui, pal, item, false);
    });
    if let Some(peak) = item.players_peak {
        ui.label(
            RichText::new(format!("{} joueurs simultanés", thousands(peak)))
                .size(10.0)
                .color(pal.text_faint),
        );
    }
    ui.add_space(4.0);
    if widgets::ghost_button(ui, "Voir sur Steam", pal, Icon::None).clicked() {
        *open_item = Some(item.clone());
    }
}

fn price_widgets(ui: &mut egui::Ui, pal: &Palette, item: &StoreItem, large: bool) {
    let Some(price) = &item.price else {
        ui.label(
            RichText::new("Prix indisponible")
                .size(11.0)
                .color(pal.text_faint),
        );
        return;
    };
    let amount = |minor: u64| format!("{}.{:02}", minor / 100, minor % 100);
    if let Some(discount) = price.discount_percent {
        egui::Frame::new()
            .fill(pal.accent)
            .corner_radius(theme::radius::SM)
            .inner_margin(egui::Margin::symmetric(6, 1))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("-{discount} %"))
                        .size(if large { 12.0 } else { 10.5 })
                        .strong()
                        .color(pal.bg),
                );
            });
        ui.label(
            RichText::new(format!("{} {}", price.currency, amount(price.final_price)))
                .size(if large { 13.0 } else { 11.0 })
                .color(pal.text),
        );
        ui.label(
            RichText::new(format!("{} {}", price.currency, amount(price.initial)))
                .size(if large { 11.5 } else { 10.0 })
                .color(pal.text_faint)
                .strikethrough(),
        );
    } else {
        ui.label(
            RichText::new(format!("{} {}", price.currency, amount(price.final_price)))
                .size(if large { 13.0 } else { 11.0 })
                .color(pal.text),
        );
    }
}

/// Petite pastille d'information posée sur une carte (Linux natif, joueurs).
fn chip_label(ui: &mut egui::Ui, pal: &Palette, text: &str) {
    egui::Frame::new()
        .fill(pal.surface)
        .corner_radius(theme::radius::SM)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).color(pal.text_dim));
        });
}

fn thousands(value: u32) -> String {
    let digits = value.to_string();
    let mut with_spaces = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            with_spaces.push('\u{202f}');
        }
        with_spaces.push(character);
    }
    with_spaces
}
