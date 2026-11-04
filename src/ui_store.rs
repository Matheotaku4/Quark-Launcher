//! Boutique Steam : découverte verticale par sections, sans achat intégré.
//!
//! La page met en avant une offre (grande bannière, dégradé et prix), une
//! rangée de réductions, puis les sections du catalogue. Les cartes se
//! soulèvent au survol et portent leur remise sur l'image : la page se parcourt
//! comme une devanture, pas comme un tableau.

use crate::app::{QuarkApp, SiteSearchState};
use crate::steam_store::{StoreItem, StoreSection};
use crate::theme::Palette;
use crate::widgets::{self, Icon};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2,
};

const PAGE_MARGIN: f32 = 30.0;
const CARD_WIDTH: f32 = 238.0;
const CARD_GAP: f32 = 14.0;
const CARDS_PER_SECTION: usize = 24;
const ART_RATIO: f32 = 1.745;

/// Texture d'une image de boutique, chargée par le loader HTTP d'egui.
fn store_texture(ctx: &egui::Context, url: &str) -> Option<(egui::TextureId, Vec2)> {
    match ctx.try_load_texture(
        url,
        egui::TextureOptions::LINEAR,
        egui::load::SizeHint::Size {
            width: 640,
            height: 360,
            maintain_aspect_ratio: false,
        },
    ) {
        Ok(egui::load::TexturePoll::Ready { texture, .. }) => Some((texture.id, texture.size)),
        Ok(egui::load::TexturePoll::Pending { .. }) => {
            // Le chargement HTTP continue : la prochaine image affichera
            // l'artwork au lieu de le redemander à chaque frame.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
            None
        }
        _ => None,
    }
}

/// Recadrage « cover » : l'image remplit le rectangle sans être déformée.
fn cover_uv(size: Vec2, rect: Rect) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let target = rect.width() / rect.height();
    let image = size.x / size.y;
    if image > target {
        let width = target / image;
        Rect::from_min_max(
            Pos2::new((1.0 - width) / 2.0, 0.0),
            Pos2::new((1.0 + width) / 2.0, 1.0),
        )
    } else {
        let height = image / target;
        Rect::from_min_max(
            Pos2::new(0.0, (1.0 - height) / 2.0),
            Pos2::new(1.0, (1.0 + height) / 2.0),
        )
    }
}

fn paint_store_image(painter: &egui::Painter, ctx: &egui::Context, url: &str, rect: Rect) -> bool {
    if let Some((id, size)) = store_texture(ctx, url) {
        painter.image(id, rect, cover_uv(size, rect), Color32::WHITE);
        true
    } else {
        false
    }
}

impl QuarkApp {
    pub(crate) fn store_tab(&mut self, ctx: &egui::Context) {
        let pal = self.pal;
        let mut open_item: Option<StoreItem> = None;
        let mut do_search = false;
        let mut refresh = false;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg))
            .show(ctx, |ui| {
                let age = self.page_at.elapsed().as_secs_f32();
                egui::ScrollArea::vertical()
                    .id_salt("quark_store_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Entrée de page en fondu, comme les autres onglets.
                        let fade = (age / 0.16).clamp(0.0, 1.0);
                        if fade < 1.0 {
                            ui.ctx().request_repaint();
                        }
                        ui.set_opacity(fade);
                        ui.horizontal(|ui| {
                            ui.add_space(PAGE_MARGIN);
                            ui.vertical(|ui| {
                                ui.set_max_width(
                                    (ui.available_width() - PAGE_MARGIN).max(240.0),
                                );
                                ui.add_space(20.0);
                                store_heading(ui, &pal);
                                ui.add_space(14.0);
                                ui.horizontal(|ui| {
                                    let field_width =
                                        (ui.available_width() - 250.0).max(150.0);
                                    let field = ui.add_sized(
                                        Vec2::new(field_width, 34.0),
                                        egui::TextEdit::singleline(&mut self.store_query)
                                            .hint_text(crate::i18n::tr("Rechercher un jeu sur Steam…"))
                                            .font(FontId::proportional(14.0)),
                                    );
                                    do_search |= field.lost_focus()
                                        && ui.input(|input| {
                                            input.key_pressed(egui::Key::Enter)
                                        });
                                    if widgets_button(
                                        ui,
                                        "Rechercher",
                                        &pal,
                                        self.store_query.trim().len() >= 2
                                            && !self.store_search_loading,
                                    ) {
                                        do_search = true;
                                    }
                                    if widgets_button(ui, "Actualiser", &pal, true) {
                                        refresh = true;
                                    }
                                });

                                if self.store_search_loading {
                                    spinner_line(ui, &pal, "Recherche dans Steam…");
                                }
                                if let Some(error) = &self.store_search_error {
                                    ui.label(
                                        crate::i18n::rich(error).size(11.5).color(pal.danger),
                                    );
                                }
                                if let Some(results) = self.store_search_results.clone() {
                                    section_heading(
                                        ui,
                                        &pal,
                                        "RÉSULTATS",
                                        Some(&format!("{} correspondance(s)", results.len())),
                                        pal.accent,
                                    );
                                    if results.is_empty() {
                                        empty_line(
                                            ui,
                                            &pal,
                                            "Aucun jeu ne correspond à cette recherche.",
                                        );
                                    } else {
                                        card_grid(
                                            ui,
                                            &pal,
                                            "recherche",
                                            &results,
                                            &mut open_item,
                                        );
                                    }
                                }

                                if self.store_loading {
                                    spinner_line(ui, &pal, "Chargement du catalogue Steam…");
                                }
                                if let Some(error) = &self.store_error {
                                    ui.label(
                                        crate::i18n::rich(format!(
                                            "{error} · les dernières offres en cache restent affichées."
                                        ))
                                        .size(11.5)
                                        .color(pal.text_faint),
                                    );
                                }
                                if let Some(notice) = &self.store_notice {
                                    ui.label(
                                        crate::i18n::rich(notice)
                                            .size(11.0)
                                            .color(pal.text_faint),
                                    );
                                }
                                if self.store_stale && self.store_snapshot.is_some() {
                                    ui.label(
                                        crate::i18n::rich(
                                            "Cache local · données potentiellement périmées",
                                        )
                                        .size(11.0)
                                        .color(pal.text_faint),
                                    );
                                }

                                let snapshot = self.store_snapshot.clone();
                                if let Some(snapshot) = snapshot {
                                    if let Some(hero) = hero_item(&snapshot.sections) {
                                        hero_card(ui, &pal, &hero, &mut open_item);
                                        let deals = top_deals(&snapshot.sections, 8);
                                        if !deals.is_empty() {
                                            section_heading(
                                                ui,
                                                &pal,
                                                "TOP RÉDUCTIONS",
                                                Some(
                                                    "Les plus fortes remises du catalogue, mises à jour avec lui",
                                                ),
                                                pal.accent,
                                            );
                                            deals_rail(ui, &pal, &deals, &mut open_item);
                                        }
                                    }
                                    if let (Some(source), false) = (
                                        self.store_recommendation_source.as_deref(),
                                        self.store_recommendations.is_empty(),
                                    ) {
                                        section_heading(
                                            ui,
                                            &pal,
                                            "RECOMMANDÉ POUR VOUS",
                                            Some(&format!(
                                                "D'après {source} · aucune donnée de navigation n'est envoyée à Steam"
                                            )),
                                            pal.accent,
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
                                        section_heading(
                                            ui,
                                            &pal,
                                            &section.title.to_uppercase(),
                                            section_caption(section).as_deref(),
                                            accent_for(&pal, section),
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
                                    empty_line(
                                        ui,
                                        &pal,
                                        if self.store_error.is_some() {
                                            "Le catalogue Steam est indisponible et aucun cache n'est encore enregistré."
                                        } else {
                                            "Le catalogue Steam apparaîtra ici."
                                        },
                                    );
                                }
                                ui.add_space(34.0);
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
            self.open_site_search(ctx, &item);
        }
        self.site_search_modal(ctx);
    }
}

impl QuarkApp {
    /// Fenêtre de recherche multi-sites ouverte au clic sur un jeu de la
    /// boutique. Chaque source remplit sa section dès que sa réponse arrive.
    pub(crate) fn site_search_modal(&mut self, ctx: &egui::Context) {
        let (close, open_url) = {
            let Some(job) = self.site_search.as_ref() else {
                return;
            };
            let pal = self.pal;
            let mut close = false;
            let mut open_url: Option<String> = None;
            let screen = ctx.screen_rect();
            let width = (screen.width() * 0.82).clamp(640.0, 920.0);
            let height = (screen.height() * 0.80).clamp(440.0, 760.0);
            let modal = egui::Modal::new(egui::Id::new("quark_site_search"))
                .backdrop_color(Color32::from_black_alpha(190))
                .frame(
                    egui::Frame::new()
                        .fill(pal.bg)
                        .stroke(egui::Stroke::new(1.0_f32, pal.border))
                        .corner_radius(14.0)
                        .inner_margin(egui::Margin::same(18)),
                )
                .show(ctx, |ui| {
                    ui.set_min_size(Vec2::new(width, height));
                    ui.set_max_size(Vec2::new(width, height));
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                crate::i18n::rich("RECHERCHE MULTI-SITES")
                                    .size(10.5)
                                    .strong()
                                    .color(pal.accent),
                            );
                            ui.label(
                                crate::i18n::rich(&job.title)
                                    .size(21.0)
                                    .strong()
                                    .color(pal.text),
                            );
                            ui.label(
                                crate::i18n::rich(
                                    "5 sites · chaque résultat s'ouvre dans le navigateur",
                                )
                                .size(11.0)
                                .color(pal.text_dim),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if widgets::icon_button(
                                ui,
                                30.0,
                                Icon::Close,
                                &pal,
                                "Fermer (Échap)",
                            )
                            .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::tr("Voir sur Steam")).clicked() {
                            open_url = Some(job.steam_url.clone());
                        }
                        if job.pending > 0 {
                            ui.add(egui::Spinner::new().size(12.0));
                            ui.label(
                                crate::i18n::rich(format!(
                                    "{} source(s) en cours…",
                                    job.pending
                                ))
                                .size(10.5)
                                .color(pal.text_faint),
                            );
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_millis(140));
                        } else {
                            ui.label(
                                crate::i18n::rich("Recherche terminée")
                                    .size(10.5)
                                    .color(pal.text_faint),
                            );
                        }
                    });
                    ui.add_space(8.0);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("quark_site_search_scroll")
                        .auto_shrink([false, false])
                        .max_height((height - 92.0).max(160.0))
                        .show(ui, |ui| {
                            for (site, state) in &job.states {
                                ui.add_space(12.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        crate::i18n::rich(site.label())
                                            .size(13.5)
                                            .strong()
                                            .color(site_color(*site)),
                                    );
                                    ui.label(
                                        crate::i18n::rich(site.host())
                                            .size(10.0)
                                            .color(pal.text_faint),
                                    );
                                    let status = match state {
                                        SiteSearchState::Pending => {
                                            "recherche…".to_string()
                                        }
                                        SiteSearchState::Ready(hits) => {
                                            format!("{} résultat(s)", hits.len())
                                        }
                                        SiteSearchState::Failed(_) => {
                                            "indisponible".to_string()
                                        }
                                    };
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            ui.label(
                                                crate::i18n::rich(status)
                                                    .size(10.0)
                                                    .color(pal.text_faint),
                                            );
                                        },
                                    );
                                });
                                ui.add_space(3.0);
                                match state {
                                    SiteSearchState::Pending => {
                                        ui.horizontal(|ui| {
                                            ui.add(egui::Spinner::new().size(11.0));
                                            ui.label(
                                                crate::i18n::rich("Interrogation du site…")
                                                    .size(11.5)
                                                    .color(pal.text_dim),
                                            );
                                        });
                                    }
                                    SiteSearchState::Ready(hits) if hits.is_empty() => {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                crate::i18n::rich(
                                                    "Aucun résultat pour ce titre.",
                                                )
                                                .size(11.5)
                                                .color(pal.text_faint),
                                            );
                                            if ui
                                                .small_button(format!(
                                                    "Chercher sur {}",
                                                    site.host()
                                                ))
                                                .clicked()
                                            {
                                                open_url =
                                                    Some(site.search_url(&job.query));
                                            }
                                        });
                                    }
                                    SiteSearchState::Ready(hits) => {
                                        for hit in hits {
                                            if search_hit_row(ui, &pal, hit) {
                                                open_url = Some(hit.url.clone());
                                            }
                                            ui.add_space(3.0);
                                        }
                                    }
                                    SiteSearchState::Failed(error) => {
                                        ui.label(
                                            crate::i18n::rich(error)
                                                .size(11.0)
                                                .color(pal.danger),
                                        );
                                        if ui
                                            .small_button(format!(
                                                "Ouvrir la recherche sur {}",
                                                site.host()
                                            ))
                                            .clicked()
                                        {
                                            open_url =
                                                Some(site.search_url(&job.query));
                                        }
                                    }
                                }
                            }
                            ui.add_space(14.0);
                            ui.label(
                                crate::i18n::rich(
                                    "Quark ne télécharge aucun fichier depuis ces sites : les liens sont ouverts dans ton navigateur.",
                                )
                                .size(10.0)
                                .color(pal.text_faint),
                            );
                        });
                });
            (modal.should_close() || close, open_url)
        };
        if let Some(url) = open_url {
            if let Err(error) = crate::news::open_in_browser(&url) {
                self.toast_error(format!("Ouverture du lien impossible : {error}"));
            }
        }
        if close {
            self.site_search = None;
        }
    }
}

fn site_color(site: crate::game_search::SearchSite) -> Color32 {
    match site {
        crate::game_search::SearchSite::AnkerGames => Color32::from_rgb(0xFF, 0x8A, 0x4C),
        crate::game_search::SearchSite::AstralGames => Color32::from_rgb(0xC6, 0x8C, 0xFF),
        crate::game_search::SearchSite::GameBounty => Color32::from_rgb(0xF5, 0xC2, 0x4B),
        crate::game_search::SearchSite::Kryoto => Color32::from_rgb(0x5D, 0xD4, 0xE8),
        crate::game_search::SearchSite::SceneCat => Color32::from_rgb(0x7C, 0xD9, 0x92),
    }
}

fn search_hit_row(ui: &mut egui::Ui, pal: &Palette, hit: &crate::game_search::SiteHit) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 46.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(
        egui::Id::new(("quark-site-hit", hit.url.as_str())),
        response.hovered(),
        0.12,
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        7.0,
        crate::theme::lerp_color(pal.surface, pal.surface_hover, hover),
    );
    painter.rect_stroke(
        rect,
        7.0,
        egui::Stroke::new(1.0_f32, pal.border.gamma_multiply(0.9 + 0.1 * hover)),
        egui::StrokeKind::Inside,
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(11.0, 0.0)))
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    crate::i18n::rich(&hit.title)
                        .size(12.5)
                        .strong()
                        .color(pal.text),
                )
                .truncate(),
            );
            ui.label(
                crate::i18n::rich(if hit.detail.is_empty() {
                    "Voir la fiche du site"
                } else {
                    &hit.detail
                })
                .size(10.0)
                .color(pal.text_dim),
            );
        },
    );
    widgets::paint_icon(
        ui,
        Rect::from_center_size(
            Pos2::new(rect.right() - 18.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        Icon::ExternalLink,
        pal.text_faint,
    );
    let clicked = response.clicked();
    let _ = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    clicked
}

/// Titre de page avec filet d'accent et une ligne d'explication.
fn store_heading(ui: &mut egui::Ui, pal: &Palette) {
    ui.horizontal(|ui| {
        let (bar, _) = ui.allocate_exact_size(Vec2::new(3.0, 44.0), Sense::hover());
        ui.painter().rect_filled(bar, 1.5, pal.accent);
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.label(
                crate::i18n::rich("BOUTIQUE STEAM")
                    .size(11.0)
                    .strong()
                    .color(pal.accent),
            );
            ui.label(
                crate::i18n::rich("Découvrir votre prochain jeu")
                    .size(28.0)
                    .strong()
                    .color(pal.text),
            );
            ui.label(
                crate::i18n::rich(
                    "Catalogue régional Steam · les achats se font sur le site officiel",
                )
                .size(12.0)
                .color(pal.text_dim),
            );
        });
    });
}

fn widgets_button(ui: &mut egui::Ui, label: &str, pal: &Palette, enabled: bool) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(crate::i18n::rich(label).size(12.0))
            .fill(pal.surface)
            .stroke(egui::Stroke::new(1.0_f32, pal.border))
            .corner_radius(7.0)
            .min_size(Vec2::new(0.0, 34.0)),
    )
    .clicked()
}

fn spinner_line(ui: &mut egui::Ui, pal: &Palette, text: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(13.0));
        ui.add_space(6.0);
        ui.label(crate::i18n::rich(text).size(12.0).color(pal.text_dim));
    });
}

fn empty_line(ui: &mut egui::Ui, pal: &Palette, text: &str) {
    ui.label(crate::i18n::rich(text).size(12.0).color(pal.text_dim));
}

fn accent_for(pal: &Palette, section: &StoreSection) -> Color32 {
    // Une teinte par grande famille de sections, pour que la page ne soit pas
    // un mur d'une seule couleur.
    let title = section.title.to_lowercase();
    if title.contains("promo") || title.contains("réduc") || title.contains("special") {
        Color32::from_rgb(0xFF, 0x6B, 0x35)
    } else if title.contains("nouveaut") || title.contains("new") {
        Color32::from_rgb(0x62, 0xD8, 0x8B)
    } else if title.contains("populaire") || title.contains("vente") {
        Color32::from_rgb(0xB0, 0x8C, 0xFF)
    } else {
        pal.accent
    }
}

fn section_caption(section: &StoreSection) -> Option<String> {
    let with_players = section
        .items
        .iter()
        .filter(|item| item.players_peak.is_some())
        .count();
    let mut parts = vec![format!("{} jeux", section.items.len())];
    if with_players > 0 {
        parts.push(format!("{with_players} pics de joueurs"));
    }
    if section.title.contains("Populaires") {
        parts.push("classement officiel Steam".to_string());
    }
    Some(parts.join(" · "))
}

fn section_heading(
    ui: &mut egui::Ui,
    pal: &Palette,
    text: &str,
    caption: Option<&str>,
    accent: Color32,
) {
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        let (bar, _) = ui.allocate_exact_size(Vec2::new(3.0, 15.0), Sense::hover());
        ui.painter().rect_filled(bar, 1.5, accent);
        ui.add_space(8.0);
        ui.label(crate::i18n::rich(text).size(12.0).strong().color(pal.text));
    });
    if let Some(caption) = caption {
        ui.add_space(2.0);
        ui.label(crate::i18n::rich(caption).size(10.5).color(pal.text_faint));
    }
    ui.add_space(9.0);
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

/// Meilleures remises, sans doublon.
fn top_deals(sections: &[StoreSection], count: usize) -> Vec<StoreItem> {
    let mut items: Vec<StoreItem> = sections
        .iter()
        .flat_map(|section| section.items.iter())
        .filter(|item| {
            item.price
                .as_ref()
                .and_then(|price| price.discount_percent)
                .is_some_and(|discount| discount > 0)
        })
        .cloned()
        .collect();
    items.sort_by(|left, right| {
        right
            .price
            .as_ref()
            .and_then(|price| price.discount_percent)
            .cmp(&left.price.as_ref().and_then(|price| price.discount_percent))
    });
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.app_id));
    items.truncate(count);
    items
}

fn hero_card(
    ui: &mut egui::Ui,
    pal: &Palette,
    item: &StoreItem,
    open_item: &mut Option<StoreItem>,
) {
    let width = ui.available_width().min(1100.0);
    let height = (width / 2.6).clamp(210.0, 380.0);
    ui.add_space(20.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let painter = ui.painter_at(rect);
    let hover = ui.ctx().animate_bool_with_time(
        egui::Id::new(("quark-hero-hover", item.app_id)),
        response.hovered(),
        0.18,
    );
    // Image qui zoome très légèrement au survol et se recadre sans déforme.
    let zoom = Rect::from_center_size(rect.center(), rect.size() * (1.0 + 0.015 * hover));
    let painted = item
        .artwork_url
        .as_deref()
        .is_some_and(|url| paint_store_image(&painter, ui.ctx(), url, zoom));
    if !painted {
        painter.rect_filled(rect, 12.0, pal.surface);
    }
    // Dégradé vers le bas : le texte reste lisible sur n'importe quelle image.
    let mut veil = egui::Mesh::default();
    veil.colored_vertex(rect.left_top(), Color32::from_black_alpha(30));
    veil.colored_vertex(rect.right_top(), Color32::from_black_alpha(30));
    veil.colored_vertex(rect.left_bottom(), Color32::from_black_alpha(235));
    veil.colored_vertex(rect.right_bottom(), Color32::from_black_alpha(235));
    veil.add_triangle(0, 1, 2);
    veil.add_triangle(1, 2, 3);
    painter.add(egui::Shape::mesh(veil));
    painter.rect_stroke(
        rect,
        12.0,
        egui::Stroke::new(1.0_f32, pal.accent.gamma_multiply(0.35 + 0.5 * hover)),
        egui::StrokeKind::Inside,
    );

    // Bandeau « À LA UNE » et remise, en haut à gauche.
    let pill = Rect::from_min_size(
        Pos2::new(rect.left() + 16.0, rect.top() + 16.0),
        Vec2::new(98.0, 24.0),
    );
    painter.rect_filled(pill, 12.0, Color32::from_black_alpha(150));
    painter.text(
        pill.center(),
        Align2::CENTER_CENTER,
        crate::i18n::tr("À LA UNE"),
        FontId::proportional(10.5),
        pal.accent,
    );
    if let Some(discount) = item.price.as_ref().and_then(|price| price.discount_percent) {
        let badge = Rect::from_min_size(
            Pos2::new(rect.left() + 16.0, rect.top() + 48.0),
            Vec2::new(58.0, 24.0),
        );
        painter.rect_filled(badge, 5.0, pal.accent);
        painter.text(
            badge.center(),
            Align2::CENTER_CENTER,
            format!("-{discount} %"),
            FontId::proportional(11.0),
            crate::theme::on_accent(pal.accent),
        );
    }

    // Bloc texte, posé en bas de la bannière.
    let text_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 22.0, rect.bottom() - 150.0),
        Pos2::new(rect.right() - 22.0, rect.bottom() - 18.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.label(
        crate::i18n::rich(&item.name)
            .size(24.0)
            .strong()
            .color(Color32::WHITE),
    );
    child.add_space(4.0);
    if item.linux_available == Some(true) {
        child.label(
            crate::i18n::rich("Linux natif")
                .size(10.5)
                .strong()
                .color(pal.accent),
        );
    }
    if let Some(peak) = item.players_peak {
        child.label(
            crate::i18n::rich(format!("{} joueurs simultanés au pic", thousands(peak)))
                .size(10.5)
                .color(Color32::from_white_alpha(190)),
        );
    }
    child.add_space(8.0);
    child.horizontal(|ui| {
        price_widgets(ui, pal, item, true);
        ui.add_space(16.0);
        if ui
            .add(
                egui::Button::new(
                    crate::i18n::rich("Voir sur Steam")
                        .size(12.5)
                        .strong()
                        .color(crate::theme::on_accent(pal.accent)),
                )
                .fill(pal.accent)
                .corner_radius(7.0)
                .min_size(Vec2::new(0.0, 32.0)),
            )
            .clicked()
        {
            *open_item = Some(item.clone());
        }
    });
    if response.clicked() {
        *open_item = Some(item.clone());
    }
    response.on_hover_text("Chercher ce jeu sur les sites de téléchargement");
}

/// Rangée de cartes compactes, une par remise.
fn deals_rail(
    ui: &mut egui::Ui,
    pal: &Palette,
    items: &[StoreItem],
    open_item: &mut Option<StoreItem>,
) {
    egui::ScrollArea::horizontal()
        .id_salt("quark_store_deals")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (index, item) in items.iter().enumerate() {
                    deal_card(ui, pal, item, index + 1, open_item);
                    ui.add_space(10.0);
                }
            });
        });
}

fn deal_card(
    ui: &mut egui::Ui,
    pal: &Palette,
    item: &StoreItem,
    rank: usize,
    open_item: &mut Option<StoreItem>,
) {
    let width = 196.0;
    let art = Vec2::new(width, width / ART_RATIO);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, art.y + 52.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(
        egui::Id::new(("quark-deal-hover", item.app_id)),
        response.hovered(),
        0.14,
    );
    let lift = -3.0 * hover;
    let card = rect.translate(Vec2::new(0.0, lift));
    let painter = ui.painter_at(card);
    painter.rect_filled(card, 9.0, pal.panel);
    let image_rect = Rect::from_min_size(card.min, art);
    let painted = item
        .artwork_url
        .as_deref()
        .is_some_and(|url| paint_store_image(&painter, ui.ctx(), url, image_rect));
    if !painted {
        painter.rect_filled(image_rect, 9.0, pal.surface);
    }
    let mut veil = egui::Mesh::default();
    veil.colored_vertex(image_rect.left_top(), Color32::from_black_alpha(0));
    veil.colored_vertex(image_rect.right_top(), Color32::from_black_alpha(0));
    veil.colored_vertex(image_rect.left_bottom(), Color32::from_black_alpha(170));
    veil.colored_vertex(image_rect.right_bottom(), Color32::from_black_alpha(170));
    veil.add_triangle(0, 1, 2);
    veil.add_triangle(1, 2, 3);
    painter.add(egui::Shape::mesh(veil));
    // Rang et remise, superposés à l'image.
    let rank_badge = Rect::from_min_size(
        Pos2::new(image_rect.left() + 8.0, image_rect.top() + 8.0),
        Vec2::new(22.0, 22.0),
    );
    painter.circle_filled(rank_badge.center(), 11.0, Color32::from_black_alpha(180));
    painter.text(
        rank_badge.center(),
        Align2::CENTER_CENTER,
        rank.to_string(),
        FontId::proportional(11.0),
        Color32::WHITE,
    );
    if let Some(discount) = item.price.as_ref().and_then(|price| price.discount_percent) {
        let badge = Rect::from_min_size(
            Pos2::new(image_rect.right() - 54.0, image_rect.top() + 8.0),
            Vec2::new(46.0, 20.0),
        );
        painter.rect_filled(badge, 5.0, pal.accent);
        painter.text(
            badge.center(),
            Align2::CENTER_CENTER,
            format!("-{discount}%"),
            FontId::proportional(10.5),
            crate::theme::on_accent(pal.accent),
        );
    }
    let footer = Rect::from_min_max(
        Pos2::new(card.left() + 9.0, image_rect.bottom() + 6.0),
        Pos2::new(card.right() - 8.0, card.bottom() - 4.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(footer)
            .layout(Layout::top_down(Align::Min)),
    );
    child.add(
        egui::Label::new(
            crate::i18n::rich(&item.name)
                .size(11.5)
                .strong()
                .color(pal.text),
        )
        .truncate(),
    );
    child.horizontal(|ui| price_widgets(ui, pal, item, false));
    painter.rect_stroke(
        card,
        9.0,
        egui::Stroke::new(1.0_f32, pal.accent.gamma_multiply(hover)),
        egui::StrokeKind::Inside,
    );
    if response.clicked() {
        *open_item = Some(item.clone());
    }
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
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                for item in chunk {
                    card(ui, pal, item, open_item);
                    ui.add_space(CARD_GAP);
                }
            });
        });
    }
}

fn card(ui: &mut egui::Ui, pal: &Palette, item: &StoreItem, open_item: &mut Option<StoreItem>) {
    let art = Vec2::new(CARD_WIDTH, CARD_WIDTH / ART_RATIO);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(CARD_WIDTH, art.y + 68.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(
        egui::Id::new(("quark-store-card", item.app_id)),
        response.hovered(),
        0.14,
    );
    let card = rect.translate(Vec2::new(0.0, -3.0 * hover));
    let painter = ui.painter_at(card);
    painter.rect_filled(card, 9.0, pal.panel);
    painter.rect_stroke(
        card,
        9.0,
        egui::Stroke::new(1.0_f32, pal.border),
        egui::StrokeKind::Inside,
    );
    let image_rect = Rect::from_min_size(card.min, art);
    let painted = item
        .artwork_url
        .as_deref()
        .is_some_and(|url| paint_store_image(&painter, ui.ctx(), url, image_rect));
    if !painted {
        painter.rect_filled(image_rect, 9.0, pal.surface);
        painter.text(
            image_rect.center(),
            Align2::CENTER_CENTER,
            "Steam",
            FontId::proportional(13.0),
            pal.text_faint,
        );
    }
    // Remise sur l'image, comme une étiquette de rayon.
    if let Some(discount) = item.price.as_ref().and_then(|price| price.discount_percent) {
        let badge = Rect::from_min_size(
            Pos2::new(image_rect.left() + 8.0, image_rect.top() + 8.0),
            Vec2::new(48.0, 21.0),
        );
        painter.rect_filled(badge, 5.0, pal.accent);
        painter.text(
            badge.center(),
            Align2::CENTER_CENTER,
            format!("-{discount}%"),
            FontId::proportional(10.5),
            crate::theme::on_accent(pal.accent),
        );
    }
    if item.linux_available == Some(true) {
        let chip = Rect::from_min_size(
            Pos2::new(image_rect.right() - 70.0, image_rect.top() + 8.0),
            Vec2::new(62.0, 20.0),
        );
        painter.rect_filled(chip, 10.0, Color32::from_black_alpha(160));
        painter.text(
            chip.center(),
            Align2::CENTER_CENTER,
            "Linux",
            FontId::proportional(9.5),
            pal.accent,
        );
    }

    let footer = Rect::from_min_max(
        Pos2::new(card.left() + 10.0, image_rect.bottom() + 7.0),
        Pos2::new(card.right() - 9.0, card.bottom() - 5.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(footer)
            .layout(Layout::top_down(Align::Min)),
    );
    child.add(
        egui::Label::new(
            crate::i18n::rich(&item.name)
                .size(12.0)
                .strong()
                .color(pal.text),
        )
        .truncate(),
    );
    child.horizontal(|ui| price_widgets(ui, pal, item, false));
    if let Some(peak) = item.players_peak {
        child.label(
            crate::i18n::rich(format!("{} joueurs simultanés", thousands(peak)))
                .size(9.5)
                .color(pal.text_faint),
        );
    }
    painter.rect_stroke(
        card,
        9.0,
        egui::Stroke::new(1.2_f32, pal.accent.gamma_multiply(hover * 0.9)),
        egui::StrokeKind::Inside,
    );
    if response.clicked() {
        *open_item = Some(item.clone());
    }
    response.on_hover_text("Chercher ce jeu sur les sites de téléchargement");
}

fn price_widgets(ui: &mut egui::Ui, pal: &Palette, item: &StoreItem, large: bool) {
    let Some(price) = &item.price else {
        ui.label(
            crate::i18n::rich("Prix indisponible")
                .size(11.0)
                .color(pal.text_faint),
        );
        return;
    };
    let amount = |minor: u64| format!("{}.{:02}", minor / 100, minor % 100);
    if let Some(discount) = price.discount_percent {
        ui.label(
            crate::i18n::rich(format!("-{discount} %"))
                .size(if large { 12.0 } else { 10.5 })
                .strong()
                .color(pal.accent),
        );
        ui.label(
            crate::i18n::rich(format!("{} {}", price.currency, amount(price.final_price)))
                .size(if large { 14.0 } else { 11.5 })
                .strong()
                .color(if large { Color32::WHITE } else { pal.text }),
        );
        ui.label(
            crate::i18n::rich(format!("{} {}", price.currency, amount(price.initial)))
                .size(if large { 11.5 } else { 10.0 })
                .color(pal.text_faint)
                .strikethrough(),
        );
    } else {
        ui.label(
            crate::i18n::rich(format!("{} {}", price.currency, amount(price.final_price)))
                .size(if large { 14.0 } else { 11.5 })
                .color(if large { Color32::WHITE } else { pal.text }),
        );
    }
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
