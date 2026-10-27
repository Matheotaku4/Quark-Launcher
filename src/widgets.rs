//! Composants dessinés à la main (icônes vectorielles, boutons, onglets).
//! Aucune dépendance à une police d'icônes : tout est tracé.

use crate::theme::Palette;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    None,
    Play,
    Stop,
    Plus,
    Refresh,
    Trash,
    Gear,
    List,
    Grid,
    Folder,
}

/// Contour rectangulaire (4 bandes) : évite `rect_stroke` et son `StrokeKind`.
pub fn border_rect(painter: &egui::Painter, rect: Rect, width: f32, color: Color32) {
    let w = width;
    painter.rect_filled(
        Rect::from_min_size(rect.left_top(), Vec2::new(rect.width(), w)),
        0.0,
        color,
    );
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(rect.left(), rect.bottom() - w), Vec2::new(rect.width(), w)),
        0.0,
        color,
    );
    painter.rect_filled(
        Rect::from_min_size(rect.left_top(), Vec2::new(w, rect.height())),
        0.0,
        color,
    );
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(rect.right() - w, rect.top()), Vec2::new(w, rect.height())),
        0.0,
        color,
    );
}

fn ring(painter: &egui::Painter, center: Pos2, r_out: f32, r_in: f32, color: Color32, from: f32, to: f32) {
    let segments = 48;
    let step = (to - from) / segments as f32;
    for i in 0..segments {
        let a0 = from + i as f32 * step;
        let a1 = a0 + step * 1.4;
        let pt = |a: f32, r: f32| Pos2::new(center.x + a.cos() * r, center.y + a.sin() * r);
        painter.add(Shape::convex_polygon(
            vec![pt(a0, r_in), pt(a0, r_out), pt(a1, r_out), pt(a1, r_in)],
            color,
            Stroke::NONE,
        ));
    }
}

pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    match icon {
        Icon::None => {}
        Icon::Play => {
            let pts = vec![
                Pos2::new(c.x - s * 0.28, c.y - s * 0.42),
                Pos2::new(c.x + s * 0.42, c.y),
                Pos2::new(c.x - s * 0.28, c.y + s * 0.42),
            ];
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Stop => {
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(s * 0.56)), 1.5, color);
        }
        Icon::Plus => {
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::new(s * 0.78, s * 0.17)),
                1.0,
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::new(s * 0.17, s * 0.78)),
                1.0,
                color,
            );
        }
        Icon::Trash => {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(c.x - s * 0.30, c.y - s * 0.16),
                    Vec2::new(s * 0.60, s * 0.56),
                ),
                1.0,
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x, c.y - s * 0.32), Vec2::new(s * 0.78, s * 0.12)),
                1.0,
                color,
            );
        }
        Icon::Gear => {
            let r = s * 0.5;
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (sa, ca) = a.sin_cos();
                let d = r * 0.66;
                let l = r * 0.3;
                let wi = r * 0.17;
                let pt = |radial: f32, tan: f32| {
                    Pos2::new(
                        c.x + ca * radial - sa * tan,
                        c.y + sa * radial + ca * tan,
                    )
                };
                painter.add(Shape::convex_polygon(
                    vec![
                        pt(d - l, -wi),
                        pt(d + l, -wi),
                        pt(d + l, wi),
                        pt(d - l, wi),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
            ring(painter, c, r * 0.66, r * 0.30, color, 0.0, std::f32::consts::TAU);
        }
        Icon::List => {
            for i in 0..3 {
                let y = c.y - s * 0.28 + i as f32 * s * 0.28;
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(c.x - s * 0.36, y - s * 0.05), Vec2::splat(s * 0.12)),
                    1.0,
                    color,
                );
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(c.x - s * 0.16, y - s * 0.04),
                        Vec2::new(s * 0.54, s * 0.09),
                    ),
                    1.0,
                    color,
                );
            }
        }
        Icon::Grid => {
            for i in 0..4 {
                let x = c.x + if i % 2 == 0 { -s * 0.36 } else { s * 0.06 };
                let y = c.y + if i < 2 { -s * 0.36 } else { s * 0.06 };
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(x, y), Vec2::splat(s * 0.30)),
                    1.0,
                    color,
                );
            }
        }
        Icon::Folder => {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(c.x - s * 0.40, c.y - s * 0.22),
                    Vec2::new(s * 0.80, s * 0.50),
                ),
                1.5,
                color,
            );
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(c.x - s * 0.40, c.y - s * 0.34),
                    Vec2::new(s * 0.36, s * 0.16),
                ),
                1.0,
                color,
            );
        }
        Icon::Refresh => {
            let from = 0.55f32;
            let to = std::f32::consts::TAU + 0.1;
            ring(painter, c, s * 0.40, s * 0.24, color, from, to);
            let a = to;
            let tip = Pos2::new(c.x + a.cos() * s * 0.32, c.y + a.sin() * s * 0.32);
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(tip.x + s * 0.16, tip.y - s * 0.02),
                    Pos2::new(tip.x - s * 0.06, tip.y - s * 0.16),
                    Pos2::new(tip.x - s * 0.06, tip.y + s * 0.14),
                ],
                color,
                Stroke::NONE,
            ));
        }
    }
}

/// Bouton principal plein (bleu Steam), avec Icône optionnelle.
pub fn accent_button(
    ui: &mut egui::Ui,
    size: Vec2,
    label: &str,
    pal: &Palette,
    enabled: bool,
    icon: Icon,
) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let fg = if enabled { Color32::WHITE } else { pal.text_faint };
    let bg = if !enabled {
        pal.surface
    } else if resp.is_pointer_button_down_on() {
        pal.accent_pressed
    } else if resp.hovered() {
        pal.accent_hover
    } else {
        pal.accent
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, bg);

    let galley = painter.layout_no_wrap(label.to_string(), FontId::proportional(15.0), fg);
    let icon_w = if icon == Icon::None { 0.0 } else { 15.0 };
    let gap = if icon_w > 0.0 { 9.0 } else { 0.0 };
    let total = icon_w + gap + galley.size().x;
    let x0 = rect.center().x - total / 2.0;
    if icon != Icon::None {
        paint_icon(
            &painter,
            Rect::from_center_size(Pos2::new(x0 + icon_w / 2.0, rect.center().y), Vec2::splat(icon_w)),
            icon,
            fg,
        );
    }
    painter.galley(
        Pos2::new(x0 + icon_w + gap, rect.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );
    resp
}

/// Bouton destructif (rouge).
pub fn danger_button(
    ui: &mut egui::Ui,
    size: Vec2,
    label: &str,
    pal: &Palette,
    enabled: bool,
    icon: Icon,
) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let fg = Color32::WHITE;
    let bg = if !enabled {
        pal.surface
    } else if resp.is_pointer_button_down_on() {
        pal.danger.gamma_multiply(0.8)
    } else if resp.hovered() {
        pal.danger.gamma_multiply(1.25)
    } else {
        pal.danger
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, bg);

    let galley = painter.layout_no_wrap(label.to_string(), FontId::proportional(14.0), fg);
    let icon_w = if icon == Icon::None { 0.0 } else { 15.0 };
    let gap = if icon_w > 0.0 { 9.0 } else { 0.0 };
    let total = icon_w + gap + galley.size().x;
    let x0 = rect.center().x - total / 2.0;
    if icon != Icon::None {
        paint_icon(
            &painter,
            Rect::from_center_size(Pos2::new(x0 + icon_w / 2.0, rect.center().y), Vec2::splat(icon_w)),
            icon,
            fg,
        );
    }
    painter.galley(
        Pos2::new(x0 + icon_w + gap, rect.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );
    resp
}

/// Bouton secondaire discret (contour au survol), largeur automatique.
pub fn ghost_button(ui: &mut egui::Ui, label: &str, pal: &Palette, icon: Icon) -> Response {
    let font = FontId::proportional(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font.clone(), pal.text);
    let icon_w = if icon == Icon::None { 0.0 } else { 13.0 };
    let gap = if icon_w > 0.0 { 8.0 } else { 0.0 };
    let size = Vec2::new(galley.size().x + icon_w + gap + 22.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, pal.surface_hover);
        border_rect(&painter, rect, 1.0, pal.border);
    }
    let color = if resp.hovered() { pal.text } else { pal.text_dim };
    let x0 = rect.center().x - (icon_w + gap + galley.size().x) / 2.0;
    if icon != Icon::None {
        paint_icon(
            &painter,
            Rect::from_center_size(Pos2::new(x0 + icon_w / 2.0, rect.center().y), Vec2::splat(icon_w)),
            icon,
            color,
        );
    }
    painter.galley(
        Pos2::new(x0 + icon_w + gap, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    resp
}

/// Bouton icône carré (engrenage, +, corbeille…).
pub fn icon_button(ui: &mut egui::Ui, size: f32, icon: Icon, pal: &Palette, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, pal.surface_hover);
    }
    paint_icon(
        &painter,
        rect.shrink(size * 0.24),
        icon,
        if resp.hovered() { pal.text } else { pal.text_dim },
    );
    if tooltip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tooltip)
    }
}

/// Onglet de la barre supérieure (Boutique / Bibliothèque).
pub fn tab(ui: &mut egui::Ui, label: &str, selected: bool, pal: &Palette) -> Response {
    let text = label.to_uppercase();
    let font = FontId::proportional(12.5);
    let width = ui
        .painter()
        .layout_no_wrap(text.clone(), font.clone(), pal.text)
        .size()
        .x;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width + 26.0, 34.0), Sense::click());
    let painter = ui.painter_at(rect);
    let color = if selected || resp.hovered() { pal.text } else { pal.text_dim };
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, color);
    if selected {
        painter.rect_filled(
            Rect::from_min_size(Pos2::new(rect.left() + 3.0, rect.bottom() - 2.0), Vec2::new(rect.width() - 6.0, 2.0)),
            1.0,
            pal.accent,
        );
    }
    resp
}

/// Vignette de secours quand le jeu n'a pas d'artwork.
pub fn monogram(painter: &egui::Painter, rect: Rect, initials: &str, pal: &Palette) {
    painter.rect_filled(rect, 5.0, pal.surface_hover);
    border_rect(painter, rect, 1.0, pal.border);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        initials,
        FontId::proportional((rect.height() * 0.40).clamp(10.0, 34.0)),
        pal.text_dim,
    );
}

/// Ligne sélectionnable (explorateur de fichiers).
pub fn list_entry(ui: &mut egui::Ui, label: &str, icon: Icon, pal: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, pal.surface_hover);
    }
    paint_icon(
        &painter,
        Rect::from_min_size(Pos2::new(rect.left() + 6.0, rect.center().y - 8.0), Vec2::splat(16.0)),
        icon,
        pal.text_dim,
    );
    painter.text(
        Pos2::new(rect.left() + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        pal.text,
    );
    resp
}
