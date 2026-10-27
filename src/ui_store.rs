//! Onglet Boutique : volontairement vide pour l'instant (structure en place).

use crate::app::QuarkApp;
use crate::theme;
use eframe::egui::{self, Align2, FontId, RichText, Vec2};

impl QuarkApp {
    pub(crate) fn store_tab(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("quark_store_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let width = ui.available_width();
                        let height = theme::hero_height(width);
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 0.0, pal.surface);
                        let gradient = self.gradient_texture(ui.ctx());
                        theme::bottom_gradient(ui, &gradient, rect, 0.85);
                        painter.text(
                            egui::pos2(rect.left() + 34.0, rect.bottom() - 78.0),
                            Align2::LEFT_BOTTOM,
                            "BOUTIQUE",
                            FontId::proportional(34.0),
                            pal.text,
                        );
                        painter.text(
                            egui::pos2(rect.left() + 36.0, rect.bottom() - 44.0),
                            Align2::LEFT_BOTTOM,
                            "Découvrir, acheter et installer — en préparation",
                            FontId::proportional(13.5),
                            pal.text_dim,
                        );

                        ui.add_space(26.0);
                        ui.horizontal(|ui| {
                            ui.add_space(34.0);
                            ui.add_enabled(
                                false,
                                egui::TextEdit::singleline(&mut self.store_query)
                                    .hint_text("Rechercher dans la boutique")
                                    .desired_width(420.0),
                            );
                        });
                        ui.add_space(22.0);
                        ui.horizontal(|ui| {
                            ui.add_space(34.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("La boutique arrive bientôt.")
                                        .size(17.0)
                                        .color(pal.text),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(
                                        "Cet onglet est encore vide : la bibliothèque, les artworks \
                                         officiels et le lancement via Proton sont la priorité. \
                                         Les jeux ajoutés depuis la bibliothèque restent jouables sans boutique.",
                                    )
                                    .size(13.0)
                                    .color(pal.text_dim),
                                );
                            });
                        });
                        ui.add_space(60.0);
                    });
            });
    }
}
