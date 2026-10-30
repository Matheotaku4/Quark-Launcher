//! Boutique Steam : découverte et recommandations locales, sans achat intégré.

use crate::app::QuarkApp;
use crate::steam_store::StoreItem;
use eframe::egui::{self, Align, Align2, FontId, Layout, RichText, Sense, Vec2};

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
                        ui.add_space(22.0);
                        ui.horizontal(|ui| {
                            ui.add_space(28.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("BOUTIQUE STEAM").size(11.0).strong().color(pal.accent));
                                ui.add_space(5.0);
                                ui.label(RichText::new("Découvrir votre prochain jeu").size(27.0).strong().color(pal.text));
                                ui.label(RichText::new("Catalogue régional Steam · achat sur le site officiel").size(12.0).color(pal.text_dim));
                            });
                        });
                        ui.add_space(18.0);
                        ui.horizontal(|ui| {
                            ui.add_space(28.0);
                            let field = ui.add_sized(
                                Vec2::new((ui.available_width() - 140.0).max(180.0), 34.0),
                                egui::TextEdit::singleline(&mut self.store_query)
                                    .hint_text("Rechercher un jeu sur Steam…")
                                    .font(FontId::proportional(14.0)),
                            );
                            do_search |= field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.add_enabled(self.store_query.trim().len() >= 2 && !self.store_search_loading, egui::Button::new("Rechercher")).clicked() {
                                do_search = true;
                            }
                            if ui.button("Actualiser").clicked() { refresh = true; }
                        });

                        if self.store_search_loading {
                            ui.horizontal(|ui| { ui.add_space(30.0); ui.spinner(); ui.label("Recherche dans Steam…"); });
                        }
                        if let Some(error) = &self.store_search_error {
                            ui.label(RichText::new(error).size(11.0).color(pal.danger));
                        }
                        if let Some(results) = self.store_search_results.clone() {
                            heading(ui, "RÉSULTATS", &pal);
                            if results.is_empty() {
                                ui.label(RichText::new("Aucun jeu trouvé.").size(12.0).color(pal.text_dim));
                            } else {
                                item_row(ui, &pal, "store_search_cards", &results, &mut open_item);
                            }
                        }

                        if self.store_loading {
                            ui.horizontal(|ui| { ui.add_space(30.0); ui.spinner(); ui.label("Chargement des offres Steam…"); });
                        }
                        if let Some(error) = &self.store_error {
                            ui.label(RichText::new(format!("{error} · les dernières offres en cache restent affichées.")).size(11.0).color(pal.text_faint));
                        }
                        let snapshot = self.store_snapshot.clone();
                        if self.store_stale && snapshot.is_some() {
                            ui.label(RichText::new("Cache local · données potentiellement périmées").size(11.0).color(pal.text_faint));
                        }
                        if let Some(snapshot) = snapshot {
                            for section in &snapshot.sections {
                                heading(ui, &section.title.to_uppercase(), &pal);
                                item_row(ui, &pal, &section.title, &section.items, &mut open_item);
                            }
                        } else if !self.store_loading {
                            let message = if self.store_error.is_some() { "Le catalogue Steam est indisponible et aucun cache n'est encore enregistré." } else { "Les offres Steam apparaîtront ici." };
                            ui.label(RichText::new(message).size(12.0).color(pal.text_dim));
                        }

                        let mut recommendation_pool: Vec<StoreItem> = self
                            .store_snapshot
                            .as_ref()
                            .into_iter()
                            .flat_map(|snapshot| snapshot.sections.iter())
                            .flat_map(|section| section.items.iter().cloned())
                            .collect();
                        if let Some(search_results) = &self.store_search_results {
                            recommendation_pool.extend(search_results.iter().cloned());
                        }
                        recommendation_pool.sort_by_key(|item| item.app_id);
                        recommendation_pool.dedup_by_key(|item| item.app_id);
                        let recs = crate::steam_store::recommend(
                            &recommendation_pool,
                            &self.lib.games,
                        );
                        if !recs.is_empty() {
                            heading(ui, "POUR VOUS · RECOMMANDATIONS LOCALES", &pal);
                            ui.label(RichText::new("Classées à partir des titres de votre bibliothèque Quark, sans envoyer votre profil à Steam.").size(11.0).color(pal.text_faint));
                            item_row(ui, &pal, "store_recommendation_cards", &recs, &mut open_item);
                        }
                        ui.add_space(28.0);
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

fn heading(ui: &mut egui::Ui, text: &str, pal: &crate::theme::Palette) {
    ui.add_space(22.0);
    ui.horizontal(|ui| {
        ui.add_space(28.0);
        ui.label(RichText::new(text).size(12.0).strong().color(pal.text_dim));
    });
    ui.add_space(9.0);
}

fn item_row(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    salt: &str,
    items: &[StoreItem],
    open_item: &mut Option<StoreItem>,
) {
    egui::ScrollArea::horizontal()
        .id_salt(("quark_store_cards", salt))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(28.0);
                for item in items.iter().take(16) {
                    egui::Frame::new()
                        .fill(pal.panel)
                        .stroke(egui::Stroke::new(1.0_f32, pal.border))
                        .corner_radius(6.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                ui.set_width(224.0);
                                if let Some(url) = &item.artwork_url {
                                    ui.add(
                                        egui::Image::new(url)
                                            .fit_to_exact_size(Vec2::new(208.0, 98.0)),
                                    );
                                } else {
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::new(208.0, 98.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 4.0, pal.surface);
                                    ui.painter().text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        "Steam",
                                        FontId::proportional(13.0),
                                        pal.text_faint,
                                    );
                                }
                                ui.add_space(5.0);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(208.0, 42.0),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        ui.add_sized(
                                            Vec2::new(208.0, 20.0),
                                            egui::Label::new(
                                                RichText::new(&item.name)
                                                    .size(12.0)
                                                    .strong()
                                                    .color(pal.text),
                                            )
                                            .truncate(),
                                        );
                                        if item.linux_available == Some(true) {
                                            ui.label(
                                                RichText::new("Linux natif")
                                                    .size(10.5)
                                                    .color(pal.accent),
                                            );
                                        }
                                    },
                                );
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(price_label(item))
                                            .size(11.0)
                                            .color(pal.text_dim),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.small_button("Voir sur Steam").clicked() {
                                            *open_item = Some(item.clone());
                                        }
                                    });
                                });
                            });
                        });
                    ui.add_space(8.0);
                }
            });
        });
}

fn price_label(item: &StoreItem) -> String {
    let Some(price) = &item.price else {
        return "Prix indisponible".to_string();
    };
    let amount = |minor: u64| format!("{}.{:02}", minor / 100, minor % 100);
    if price.initial > price.final_price {
        format!(
            "{} {} · -{}%",
            price.currency,
            amount(price.final_price),
            price.discount_percent.unwrap_or(0)
        )
    } else {
        format!("{} {}", price.currency, amount(price.final_price))
    }
}
