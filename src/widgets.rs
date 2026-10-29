//! Composants dessinés à la main (icônes vectorielles, boutons, onglets).
//! Aucune dépendance à une police d'icônes : tout est tracé.

use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, Vec2,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    None,
    Play,
    Stop,
    Plus,
    Refresh,
    Trash,
    Gear,
    Info,
    /// Journal de session (page lignée).
    Log,
    List,
    Grid,
    Folder,
    /// Crayon (modifier la fiche, les tags, les notes).
    Pencil,
    /// Logo Steam, pour l'import de la bibliothèque.
    Steam,
    /// Croix de fermeture.
    Close,
    /// Maison de la page d'accueil.
    Home,
    ArrowLeft,
    ArrowRight,
}

/// Identifiant stable d'un contrôle animé : sa place et son libellé. Le bloc de
/// mise en page ne change pas, donc l'état de l'animation n'est jamais perdu.
fn control_id(ui: &egui::Ui, rect: Rect, label: &str, kind: &str) -> egui::Id {
    ui.id()
        .with((kind, label, rect.left().to_bits(), rect.top().to_bits()))
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
        Icon::Info => {
            ring(painter, c, s * 0.40, s * 0.31, color, 0.0, std::f32::consts::TAU);
            painter.circle_filled(Pos2::new(c.x, c.y - s * 0.17), s * 0.055, color);
            painter.rect_filled(
                Rect::from_center_size(
                    Pos2::new(c.x, c.y + s * 0.06),
                    Vec2::new(s * 0.11, s * 0.32),
                ),
                1.0,
                color,
            );
        }
        Icon::Log => {
            let page = Rect::from_center_size(c, Vec2::new(s * 0.60, s * 0.74));
            border_rect(painter, page, (s * 0.08).max(1.0), color);
            for i in 0..3 {
                let y = page.top() + page.height() * (0.30 + i as f32 * 0.19);
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(page.left() + s * 0.10, y),
                        Vec2::new(page.width() - s * 0.20, s * 0.06),
                    ),
                    0.5,
                    color,
                );
            }
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
        Icon::Pencil => {
            // Corps diagonal du crayon, puis la pointe.
            let dir = Vec2::new(0.707, -0.707);
            let normal = Vec2::new(0.707, 0.707);
            let body = |along: f32, across: f32| {
                Pos2::new(
                    c.x + dir.x * s * along + normal.x * s * across,
                    c.y + dir.y * s * along + normal.y * s * across,
                )
            };
            painter.add(Shape::convex_polygon(
                vec![body(0.30, 0.13), body(-0.16, 0.13), body(-0.16, -0.13), body(0.30, -0.13)],
                color,
                Stroke::NONE,
            ));
            painter.add(Shape::convex_polygon(
                vec![body(-0.16, 0.13), body(-0.40, 0.0), body(-0.16, -0.13)],
                color,
                Stroke::NONE,
            ));
            // Bague du haut, pour distinguer le crayon d'un simple trait.
            painter.rect_filled(
                Rect::from_center_size(body(0.30, 0.0), Vec2::splat(s * 0.10)),
                1.0,
                color,
            );
        }
        Icon::Steam => {
            ring(painter, c, s * 0.40, s * 0.30, color, 0.0, std::f32::consts::TAU);
            // Petit cercle en bas à droite et le trait qui le relie au centre.
            let dot = Pos2::new(c.x + s * 0.14, c.y + s * 0.14);
            painter.circle_filled(dot, s * 0.11, color);
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.28, c.y - s * 0.10),
                    Pos2::new(c.x - s * 0.02, c.y + s * 0.02),
                    Pos2::new(c.x - s * 0.10, c.y + s * 0.16),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Close => {
            let arm = s * 0.34;
            let thickness = s * 0.15;
            for sign in [-1.0_f32, 1.0] {
                let from = Pos2::new(c.x - arm, c.y - arm * sign);
                let to = Pos2::new(c.x + arm, c.y + arm * sign);
                painter.add(Shape::convex_polygon(
                    vec![
                        Pos2::new(from.x, from.y - thickness / 2.0),
                        Pos2::new(to.x, to.y - thickness / 2.0),
                        Pos2::new(to.x, to.y + thickness / 2.0),
                        Pos2::new(from.x, from.y + thickness / 2.0),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
        }
        Icon::Home => {
            // Toit (triangle) puis corps (rectangle plein).
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - s * 0.44, c.y - s * 0.02),
                    Pos2::new(c.x, c.y - s * 0.42),
                    Pos2::new(c.x + s * 0.44, c.y - s * 0.02),
                ],
                color,
                Stroke::NONE,
            ));
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(c.x - s * 0.30, c.y - s * 0.06),
                    Vec2::new(s * 0.60, s * 0.46),
                ),
                1.0,
                color,
            );
        }
        Icon::ArrowLeft | Icon::ArrowRight => {
            let direction = if icon == Icon::ArrowLeft { -1.0 } else { 1.0 };
            painter.add(Shape::convex_polygon(
                vec![
                    Pos2::new(c.x - direction * s * 0.26, c.y - s * 0.34),
                    Pos2::new(c.x + direction * s * 0.30, c.y),
                    Pos2::new(c.x - direction * s * 0.26, c.y + s * 0.34),
                ],
                color,
                Stroke::NONE,
            ));
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
    let pressed = enabled && resp.is_pointer_button_down_on();
    // Transition douce vers l'état survolé. egui ne redemande une image que
    // pendant la transition : au repos, aucun repaint supplémentaire.
    let hover = ui.ctx().animate_bool_with_time(
        control_id(ui, rect, label, "accent"),
        enabled && resp.hovered(),
        0.12,
    );
    let base = if enabled { pal.accent } else { pal.surface };
    let bg = if pressed {
        pal.accent_pressed
    } else {
        theme::lerp_color(base, pal.accent_hover, hover)
    };
    // Texte noir sur un accent clair, blanc sur un accent sombre.
    let fg = if enabled { theme::on_accent(bg) } else { pal.text_faint };
    let painter = ui.painter_at(rect);
    // L'enfoncement est peint, jamais alloué : la mise en page ne bouge pas.
    painter.rect_filled(rect.shrink(if pressed { 1.0 } else { 0.0 }), 4.0, bg);
    if enabled && hover > 0.0 && !pressed {
        border_rect(
            &painter,
            rect.shrink(0.5),
            1.0,
            pal.accent_hover.gamma_multiply(0.30 * hover),
        );
    }

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
    let pressed = enabled && resp.is_pointer_button_down_on();
    let hover = ui.ctx().animate_bool_with_time(
        control_id(ui, rect, label, "danger"),
        enabled && resp.hovered(),
        0.12,
    );
    let bg = if pressed {
        pal.danger.gamma_multiply(0.80)
    } else {
        theme::lerp_color(pal.danger, pal.danger.gamma_multiply(1.30), hover)
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect.shrink(if pressed { 1.0 } else { 0.0 }), 4.0, bg);

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
    let hover = ui.ctx().animate_bool_with_time(
        control_id(ui, rect, label, "ghost"),
        resp.hovered(),
        0.12,
    );
    if hover > 0.0 {
        painter.rect_filled(rect, 4.0, pal.surface_hover.gamma_multiply(hover));
        border_rect(&painter, rect, 1.0, pal.border.gamma_multiply(hover));
    }
    let color = theme::lerp_color(pal.text_dim, pal.text, hover);
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
    let hover = ui.ctx().animate_bool_with_time(
        control_id(ui, rect, tooltip, "icon"),
        resp.hovered(),
        0.12,
    );
    if hover > 0.0 {
        painter.rect_filled(rect, 4.0, pal.surface_hover.gamma_multiply(hover));
    }
    // Le symbole se resserre légèrement au survol : un effet de « prise », sans
    // aucune conséquence sur la mise en page.
    paint_icon(
        &painter,
        rect.shrink(size * (0.24 + 0.03 * hover)),
        icon,
        theme::lerp_color(pal.text_dim, pal.text, hover),
    );
    if tooltip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tooltip)
    }
}

/// Bouton icône carré coloré, pour les actions d'un jeu : poubelle rouge,
/// crayon vert, engrenage bleu. La couleur reste lisible sur le noir AMOLED et
/// sur une bannière claire grâce à une pastille sombre puis un liseré coloré.
pub fn tinted_icon_button(
    ui: &mut egui::Ui,
    size: f32,
    icon: Icon,
    color: Color32,
    tooltip: &str,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let painter = ui.painter_at(rect);
    let hover = ui.ctx().animate_bool_with_time(
        control_id(ui, rect, tooltip, "tinted"),
        resp.hovered(),
        0.12,
    );
    // Pastille sombre sous le symbole : posée sur une bannière claire, l'icône
    // colorée gardait peu de contraste. Le fond sombre la détache de l'image.
    painter.circle_filled(
        rect.center(),
        size * 0.5,
        Color32::from_black_alpha((105.0 + 45.0 * hover).round() as u8),
    );
    // Liseré de la couleur de l'action : le bouton se lit même de loin.
    painter.circle_stroke(
        rect.center(),
        size * 0.5 - 0.5,
        Stroke::new(1.0_f32, color.gamma_multiply(0.65 + 0.35 * hover)),
    );
    paint_icon(
        &painter,
        rect.shrink(size * (0.29 + 0.02 * hover)),
        icon,
        theme::lerp_color(color, Color32::WHITE, hover * 0.30),
    );
    if tooltip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tooltip)
    }
}

/// Entrée de menu contextuel : petite pastille sombre, icône colorée, libellé.
///
/// Un menu egui standard ne porte que du texte ; ces lignes donnent au clic
/// droit les mêmes icônes que la fiche. La pastille sombre garde le symbole
/// lisible même quand la fenêtre est posée sur une bannière claire.
pub fn menu_row(
    ui: &mut egui::Ui,
    pal: &Palette,
    icon: Icon,
    color: Color32,
    label: &str,
) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(206.0), 28.0),
        Sense::click(),
    );
    let painter = ui.painter_at(rect);
    let hover = ui.ctx().animate_bool_with_time(
        ui.id().with(("quark_menu", label)),
        resp.hovered(),
        0.10,
    );
    if hover > 0.0 {
        painter.rect_filled(rect, 5.0, pal.surface_hover.gamma_multiply(hover));
    }
    let center = Pos2::new(rect.left() + 18.0, rect.center().y);
    painter.circle_filled(center, 10.0, Color32::from_black_alpha(130));
    painter.circle_stroke(
        center,
        9.5,
        Stroke::new(1.0_f32, color.gamma_multiply(0.6)),
    );
    paint_icon(
        &painter,
        Rect::from_center_size(center, Vec2::splat(12.0)),
        icon,
        color,
    );
    painter.text(
        Pos2::new(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        theme::lerp_color(pal.text_dim, pal.text, 0.55 + 0.45 * hover),
    );
    resp.clicked()
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
    let hover = ui.ctx().animate_bool_with_time(
        ui.id().with(("quark-tab-hover", label)),
        resp.hovered(),
        0.12,
    );
    // Le trait d'onglet s'étire depuis le centre quand l'onglet est choisi.
    let mark = ui
        .ctx()
        .animate_bool_with_time(ui.id().with(("quark-tab-mark", label)), selected, 0.16);
    let color = if selected {
        pal.text
    } else {
        theme::lerp_color(pal.text_dim, pal.text, hover)
    };
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, color);
    if mark > 0.0 {
        let width = (rect.width() - 6.0) * (0.55 + 0.45 * mark);
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(
                    rect.center().x - width / 2.0,
                    rect.bottom() - 2.0 - (1.0 - mark) * 2.0,
                ),
                Vec2::new(width, 2.0),
            ),
            1.0,
            pal.accent.gamma_multiply(mark),
        );
    }
    resp
}

/// Vignette de secours quand le jeu n'a pas d'artwork.
/// Vignette de secours d'un jeu sans artwork : la première lettre du titre sur
/// la couleur assignée au jeu. Le rectangle reçu est exactement celui de
/// l'artwork, donc la vignette occupe toujours la même place.
pub fn monogram(painter: &egui::Painter, rect: Rect, text: &str, fill: Color32, _pal: &Palette) {
    // Coins arrondis comme les vignettes d'images : la pastille de secours ne
    // se distingue plus du reste de la liste.
    painter.rect_filled(rect, 7.0, fill);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional((rect.height() * 0.36).clamp(13.0, 120.0)),
        // Sur une couleur claire, une lettre blanche serait illisible.
        theme::on_surface(fill),
    );
}

/// Pastille de filtre (vue, tri, tag, statut) : pleine quand elle est active.
pub fn chip(ui: &mut egui::Ui, label: &str, selected: bool, pal: &Palette) -> bool {
    chip_colored(ui, label, selected, pal, pal.accent)
}

/// Même pastille avec une couleur propre (statuts, tags).
pub fn chip_colored(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    pal: &Palette,
    accent: Color32,
) -> bool {
    let ink = if selected {
        theme::on_accent(accent)
    } else {
        pal.text_dim
    };
    ui.add(
        egui::Button::new(RichText::new(label).size(11.0).color(ink))
            .fill(if selected { accent } else { pal.surface })
            .stroke(egui::Stroke::new(
                1.0_f32,
                if selected { accent } else { pal.border },
            ))
            .corner_radius(9.0)
            .min_size(Vec2::new(0.0, 22.0)),
    )
    .clicked()
}

/// Bouton « Accueil » posé au-dessus de la liste des jeux : icône maison,
/// libellé, et un trait d'accent quand la page est affichée.
pub fn home_button(ui: &mut egui::Ui, selected: bool, pal: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(112.0, 30.0), Sense::click());
    let painter = ui.painter_at(rect);
    let hover = ui.ctx().animate_bool_with_time(
        ui.id().with("quark-home-hover"),
        resp.hovered(),
        0.12,
    );
    let mark = ui
        .ctx()
        .animate_bool_with_time(ui.id().with("quark-home-mark"), selected, 0.16);
    let lift = hover.max(mark);
    if lift > 0.0 {
        painter.rect_filled(rect, 5.0, pal.surface_hover.gamma_multiply(lift * 0.8));
    }
    let color = if selected {
        pal.text
    } else {
        theme::lerp_color(pal.text_dim, pal.text, hover)
    };
    paint_icon(
        &painter,
        Rect::from_center_size(
            Pos2::new(rect.left() + 16.0, rect.center().y),
            Vec2::splat(15.0),
        ),
        Icon::Home,
        color,
    );
    painter.text(
        Pos2::new(rect.left() + 29.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Accueil",
        FontId::proportional(12.5),
        color,
    );
    if mark > 0.0 {
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left() + 8.0, rect.bottom() - 2.0),
                Vec2::new((rect.width() - 16.0) * mark, 2.0),
            ),
            1.0,
            pal.accent,
        );
    }
    resp
}

/// Bouton icône posé **sur une image** (bannière) : un fond translucide rend le
/// symbole lisible quelle que soit l'image, et sa couleur s'oppose au fond
/// (sombre sur une bannière claire, clair sur une bannière sombre).
pub fn floating_icon_button(
    ui: &mut egui::Ui,
    size: f32,
    icon: Icon,
    pal: &Palette,
    tooltip: &str,
    on_light: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let painter = ui.painter_at(rect);
    let hover = ui.ctx().animate_bool_with_time(
        control_id(ui, rect, tooltip, "floating"),
        resp.hovered(),
        0.12,
    );
    let backdrop = if on_light {
        Color32::from_rgba_unmultiplied(255, 255, 255, 190)
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 0, 120)
    };
    painter.rect_filled(rect, 5.0, backdrop);
    if hover > 0.0 {
        painter.rect_filled(
            rect,
            5.0,
            if on_light {
                Color32::from_rgba_unmultiplied(255, 255, 255, 40)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 30)
            },
        );
    }
    let fg = if on_light {
        theme::on_surface(Color32::from_rgb(0xF4, 0xF7, 0xFB))
    } else {
        theme::lerp_color(pal.text, Color32::WHITE, hover)
    };
    paint_icon(
        &painter,
        rect.shrink(size * (0.26 + 0.02 * hover)),
        icon,
        fg,
    );
    if tooltip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tooltip)
    }
}

/// Petit ⓘ ) côté d'un réglage : l'explication remplace les lignes de conseil
/// qui encombraient les fenêtres. Le texte complet s'affiche au survol.
pub fn info_button(ui: &mut egui::Ui, pal: &Palette, text: &str) -> Response {
    icon_button(ui, 16.0, Icon::Info, pal, text)
}
