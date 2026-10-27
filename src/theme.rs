//! Thème de Quark : noir profond (AMOLED) par défaut, ambiance Steam.

use crate::config::ThemeMode;
use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Stroke, Vec2};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Fond général de la fenêtre.
    pub bg: Color32,
    /// Fond des panneaux (barre du haut, colonne de gauche).
    pub panel: Color32,
    /// Fond des éléments posés (champs de saisie, cartes).
    pub surface: Color32,
    pub surface_hover: Color32,
    pub border: Color32,
    /// Ligne sélectionnée dans la liste.
    pub row_selected: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub danger: Color32,
    pub header: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn palette(mode: ThemeMode) -> Palette {
    let accent = rgb(0x1A, 0x9F, 0xFF); // bleu Steam
    match mode {
        ThemeMode::Amoled => Palette {
            bg: rgb(0, 0, 0),
            panel: rgb(0x05, 0x05, 0x05),
            surface: rgb(0x11, 0x11, 0x11),
            surface_hover: rgb(0x1C, 0x1C, 0x1C),
            border: rgb(0x1E, 0x1E, 0x1E),
            row_selected: rgb(0x14, 0x22, 0x2E),
            text: rgb(0xE8, 0xE8, 0xE8),
            text_dim: rgb(0x9A, 0xA0, 0xA6),
            text_faint: rgb(0x66, 0x6C, 0x72),
            accent,
            accent_hover: rgb(0x3F, 0xB0, 0xFF),
            accent_pressed: rgb(0x0F, 0x7F, 0xD0),
            danger: rgb(0xD9, 0x41, 0x41),
            header: rgb(0, 0, 0),
        },
        ThemeMode::Graphite => Palette {
            bg: rgb(0x0B, 0x0E, 0x11),
            panel: rgb(0x12, 0x16, 0x1B),
            surface: rgb(0x1A, 0x20, 0x27),
            surface_hover: rgb(0x23, 0x2B, 0x34),
            border: rgb(0x2A, 0x32, 0x3C),
            row_selected: rgb(0x1B, 0x2A, 0x38),
            text: rgb(0xE3, 0xE7, 0xEB),
            text_dim: rgb(0x98, 0xA2, 0xAC),
            text_faint: rgb(0x6E, 0x7A, 0x86),
            accent,
            accent_hover: rgb(0x3F, 0xB0, 0xFF),
            accent_pressed: rgb(0x0F, 0x7F, 0xD0),
            danger: rgb(0xD9, 0x41, 0x41),
            header: rgb(0x12, 0x16, 0x1B),
        },
    }
}

pub fn apply(ctx: &egui::Context, mode: ThemeMode) {
    let p = palette(mode);
    let mut style = (*ctx.style()).clone();

    let mut v = egui::Visuals::dark();
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.surface;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, p.accent);

    let widgets = &mut v.widgets;
    widgets.noninteractive.bg_fill = p.panel;
    widgets.noninteractive.weak_bg_fill = p.surface;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);

    widgets.inactive.bg_fill = p.surface;
    widgets.inactive.weak_bg_fill = p.surface;
    widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.border);
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    widgets.inactive.expansion = 0.0;
    widgets.inactive.corner_radius = egui::CornerRadius::same(4);

    widgets.hovered.bg_fill = p.surface_hover;
    widgets.hovered.weak_bg_fill = p.surface_hover;
    widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent.gamma_multiply(0.6));
    widgets.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
    widgets.hovered.expansion = 0.0;
    widgets.hovered.corner_radius = egui::CornerRadius::same(4);

    widgets.active.bg_fill = p.accent_pressed;
    widgets.active.weak_bg_fill = p.accent_pressed;
    widgets.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    widgets.active.expansion = 0.0;
    widgets.active.corner_radius = egui::CornerRadius::same(4);

    widgets.open.bg_fill = p.surface_hover;
    widgets.open.weak_bg_fill = p.surface_hover;
    widgets.open.bg_stroke = Stroke::new(1.0_f32, p.border);
    widgets.open.fg_stroke = Stroke::new(1.0_f32, p.text);
    widgets.open.expansion = 0.0;
    widgets.open.corner_radius = egui::CornerRadius::same(4);

    style.visuals = v;
    style.text_styles = [
        (egui::TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
        (egui::TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (egui::TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        (egui::TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
        (egui::TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
    ]
    .into();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.scroll.bar_width = 8.0;

    ctx.set_style(style);
}

/// Dégradé noir vers le bas du rectangle (lisibilité du titre sur la bannière).
pub fn bottom_gradient(painter: &egui::Painter, rect: Rect, fraction: f32) {
    let steps = 40;
    let h = rect.height() * fraction;
    let top = rect.bottom() - h;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let a = (t * t * 235.0) as u8;
        let y0 = top + h * (i as f32 / steps as f32);
        let y1 = top + h * ((i + 1) as f32 / steps as f32);
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(rect.left(), y0), Pos2::new(rect.right(), y1)),
            0.0,
            Color32::from_black_alpha(a),
        );
    }
}

/// Hauteur de bannière : ratio 3.1:1 comme les `library_hero` de Steam.
pub fn hero_height(width: f32) -> f32 {
    (width / 3.1).clamp(190.0, 460.0)
}

pub const COVER_RATIO: f32 = 1.5; // 600x900 comme Steam
