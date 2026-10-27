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
    // Transitions courtes : assez visibles pour être agréables, assez rapides
    // pour ne jamais retarder un clic. Les animations d'egui n'entretiennent un
    // repaint que tant qu'elles bougent, donc aucun coût au repos.
    style.animation_time = 0.12;

    ctx.set_style(style);
}

/// Rampe de dégradé noir (alpha 0 en haut, presque opaque en bas). Créée une
/// seule fois puis étirée sur la bannière : un seul rectangle à dessiner, donc
/// aucun bord de bande visible et un coût négligeable.
pub fn gradient_texture(ctx: &egui::Context) -> egui::TextureHandle {
    const HEIGHT: usize = 256;
    let mut pixels = Vec::with_capacity(HEIGHT);
    for i in 0..HEIGHT {
        let t = i as f32 / (HEIGHT - 1) as f32;
        // Profil quadratique : le noir ne monte vraiment qu'en bas.
        pixels.push(Color32::from_black_alpha((t * t * 244.0).round() as u8));
    }
    ctx.load_texture(
        "quark-bottom-gradient",
        egui::ColorImage::new([1, HEIGHT], pixels),
        egui::TextureOptions::LINEAR,
    )
}

/// Assombrit le bas du rectangle avec la rampe (lisibilité du titre et des
/// actualités posés sur la bannière).
pub fn bottom_gradient(
    ui: &mut egui::Ui,
    texture: &egui::TextureHandle,
    rect: Rect,
    fraction: f32,
) {
    let height = rect.height() * fraction.clamp(0.0, 1.0);
    if height <= 1.0 {
        return;
    }
    let area = Rect::from_min_max(
        Pos2::new(rect.left(), rect.bottom() - height),
        Pos2::new(rect.right(), rect.bottom()),
    );
    ui.put(
        area,
        egui::Image::new(egui::load::SizedTexture::new(texture.id(), area.size())),
    );
}

/// Hauteur de bannière : ratio 3.1:1 comme les `library_hero` de Steam.
pub fn hero_height(width: f32) -> f32 {
    (width / 3.1).clamp(190.0, 460.0)
}

pub const COVER_RATIO: f32 = 1.5; // 600x900 comme Steam

/// Couleurs de secours des jeux sans icône. Une par jeu, choisie de façon
/// stable : le même titre donne toujours la même couleur.
pub const MONOGRAM_COLORS: [Color32; 10] = [
    rgb(0x2B, 0x4A, 0x7A), // bleu
    rgb(0x6B, 0x2A, 0x4C), // magenta
    rgb(0x2C, 0x59, 0x3C), // vert
    rgb(0x7A, 0x56, 0x1F), // ambre
    rgb(0x3C, 0x33, 0x6E), // violet
    rgb(0x77, 0x32, 0x2A), // rouille
    rgb(0x1C, 0x55, 0x5E), // sarcelle
    rgb(0x5A, 0x2E, 0x63), // prune
    rgb(0x40, 0x4C, 0x22), // olive
    rgb(0x2E, 0x3F, 0x63), // ardoise
];

/// Couleur associée à un jeu (hash FNV-1a du titre, insensible à la casse).
pub fn monogram_color(seed: &str) -> Color32 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.trim().to_lowercase().bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    MONOGRAM_COLORS[(hash % MONOGRAM_COLORS.len() as u64) as usize]
}

/// Mélange linéaire de deux couleurs, pour les transitions (survol, sélection).
pub fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
    let [r1, g1, b1, a1] = from.to_array();
    let [r2, g2, b2, a2] = to.to_array();
    Color32::from_rgba_premultiplied(
        mix(r1, r2),
        mix(g1, g2),
        mix(b1, b2),
        mix(a1, a2),
    )
}
