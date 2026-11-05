//! Thème de Quark : noir profond (AMOLED) par défaut, ambiance Steam.

use crate::config::ThemeMode;
use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, Vec2};

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

impl Palette {
    /// Même palette avec la couleur d'accent d'un jeu (bouton Jouer, liseré de
    /// sélection, ligne sélectionnée). Le reste du thème ne bouge pas.
    pub fn with_accent(self, accent: Color32) -> Palette {
        let mut pal = self;
        pal.accent = accent;
        pal.accent_hover = lerp_color(accent, Color32::WHITE, 0.22);
        pal.accent_pressed = lerp_color(accent, Color32::BLACK, 0.30);
        pal.row_selected = lerp_color(self.surface, accent, 0.18);
        pal
    }
}

/// Couleur d'accent lisible sur le noir AMOLED à partir d'une teinte (degrés).
pub fn accent_from_hue(hue: f32) -> Color32 {
    hsv(hue, 0.78, 0.90)
}

/// Couleur de texte lisible sur un fond `accent` : noir sur un accent clair
/// (jaune, vert clair…), blanc sur un accent sombre. Sans ça, un accent déduit
/// d'une couverture claire donnerait un libellé blanc illisible.
pub fn on_accent(accent: Color32) -> Color32 {
    let [r, g, b, _] = accent.to_array();
    let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if luminance > 150.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// Couleur d'un symbole posé sur `bg` : la même teinte, très assombrie, sur un
/// fond clair (une icône blanche sur un bleu clair disparaît), et un blanc cassé
/// sur un fond sombre. C'est ce qui rend les icônes lisibles sur une bannière
/// claire comme sur le noir AMOLED.
pub fn on_surface(bg: Color32) -> Color32 {
    let [r, g, b, _] = bg.to_array();
    let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if luminance > 150.0 {
        // Même teinte, gardée très sombre : bleu clair -> bleu foncé. L'alpha est
        // remis à 255 : `gamma_multiply` le réduit aussi, et une icône à 22 %
        // d'opacité disparaîtrait sur son fond.
        let c = bg.gamma_multiply(0.22);
        Color32::from_rgb(c.r(), c.g(), c.b())
    } else {
        rgb(0xF2, 0xF5, 0xF9)
    }
}

/// Conversion teinte/saturation/valeur -> RVB.
fn hsv(hue: f32, saturation: f32, value: f32) -> Color32 {
    let h = hue.rem_euclid(360.0) / 60.0;
    let c = value * saturation;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    rgb(
        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

pub fn palette(mode: ThemeMode) -> Palette {
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
            accent: rgb(0x1A, 0x9F, 0xFF),
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
            accent: rgb(0x1A, 0x9F, 0xFF),
            accent_hover: rgb(0x3F, 0xB0, 0xFF),
            accent_pressed: rgb(0x0F, 0x7F, 0xD0),
            danger: rgb(0xD9, 0x41, 0x41),
            header: rgb(0x12, 0x16, 0x1B),
        },
        ThemeMode::CatppuccinMocha => Palette {
            bg: rgb(0x11, 0x11, 0x1B),
            panel: rgb(0x18, 0x18, 0x25),
            surface: rgb(0x31, 0x32, 0x44),
            surface_hover: rgb(0x45, 0x47, 0x5A),
            border: rgb(0x58, 0x5B, 0x70),
            row_selected: rgb(0x30, 0x38, 0x58),
            text: rgb(0xCD, 0xD6, 0xF4),
            text_dim: rgb(0xA6, 0xAD, 0xC8),
            text_faint: rgb(0x7F, 0x84, 0x9C),
            accent: rgb(0x89, 0xB4, 0xFA),
            accent_hover: rgb(0xB4, 0xBE, 0xFE),
            accent_pressed: rgb(0x74, 0x97, 0xD4),
            danger: rgb(0xF3, 0x8B, 0xA8),
            header: rgb(0x1E, 0x1E, 0x2E),
        },
        ThemeMode::CatppuccinMacchiato => Palette {
            bg: rgb(0x18, 0x19, 0x26),
            panel: rgb(0x1E, 0x20, 0x30),
            surface: rgb(0x36, 0x3A, 0x4F),
            surface_hover: rgb(0x49, 0x4D, 0x64),
            border: rgb(0x5B, 0x60, 0x78),
            row_selected: rgb(0x35, 0x40, 0x62),
            text: rgb(0xCA, 0xD3, 0xF5),
            text_dim: rgb(0xA5, 0xAD, 0xCB),
            text_faint: rgb(0x80, 0x87, 0xA0),
            accent: rgb(0x8A, 0xAD, 0xF4),
            accent_hover: rgb(0xB7, 0xBD, 0xF8),
            accent_pressed: rgb(0x72, 0x8E, 0xCC),
            danger: rgb(0xED, 0x87, 0x96),
            header: rgb(0x24, 0x27, 0x3A),
        },
        ThemeMode::CatppuccinFrappe => Palette {
            bg: rgb(0x23, 0x26, 0x34),
            panel: rgb(0x29, 0x2C, 0x3C),
            surface: rgb(0x41, 0x45, 0x59),
            surface_hover: rgb(0x51, 0x56, 0x70),
            border: rgb(0x62, 0x68, 0x80),
            row_selected: rgb(0x3E, 0x4B, 0x6B),
            text: rgb(0xC6, 0xD0, 0xF5),
            text_dim: rgb(0xA5, 0xAD, 0xCE),
            text_faint: rgb(0x83, 0x8B, 0xA7),
            accent: rgb(0x8C, 0xAA, 0xEE),
            accent_hover: rgb(0xB5, 0xBF, 0xF4),
            accent_pressed: rgb(0x73, 0x8B, 0xC4),
            danger: rgb(0xE7, 0x82, 0x8B),
            header: rgb(0x30, 0x34, 0x46),
        },
        ThemeMode::Dracula => Palette {
            bg: rgb(0x1E, 0x1F, 0x29),
            panel: rgb(0x21, 0x22, 0x2C),
            surface: rgb(0x34, 0x37, 0x46),
            surface_hover: rgb(0x44, 0x47, 0x5A),
            border: rgb(0x55, 0x58, 0x6B),
            row_selected: rgb(0x38, 0x35, 0x52),
            text: rgb(0xF8, 0xF8, 0xF2),
            text_dim: rgb(0xBF, 0xC7, 0xD5),
            text_faint: rgb(0x8B, 0x91, 0xA5),
            accent: rgb(0xBD, 0x93, 0xF9),
            accent_hover: rgb(0xCA, 0xA9, 0xFA),
            accent_pressed: rgb(0x9B, 0x76, 0xD0),
            danger: rgb(0xFF, 0x55, 0x55),
            header: rgb(0x28, 0x2A, 0x36),
        },
        ThemeMode::Nord => Palette {
            bg: rgb(0x24, 0x29, 0x33),
            panel: rgb(0x2E, 0x34, 0x40),
            surface: rgb(0x3B, 0x42, 0x52),
            surface_hover: rgb(0x43, 0x4C, 0x5E),
            border: rgb(0x4C, 0x56, 0x6A),
            row_selected: rgb(0x3A, 0x4B, 0x5A),
            text: rgb(0xEC, 0xEF, 0xF4),
            text_dim: rgb(0xD8, 0xDE, 0xE9),
            text_faint: rgb(0x9A, 0xA4, 0xB5),
            accent: rgb(0x88, 0xC0, 0xD0),
            accent_hover: rgb(0x8F, 0xBC, 0xBB),
            accent_pressed: rgb(0x5E, 0x87, 0x97),
            danger: rgb(0xBF, 0x61, 0x6A),
            header: rgb(0x2E, 0x34, 0x40),
        },
        ThemeMode::GruvboxDark => Palette {
            bg: rgb(0x1D, 0x20, 0x21),
            panel: rgb(0x28, 0x28, 0x28),
            surface: rgb(0x32, 0x30, 0x2F),
            surface_hover: rgb(0x3C, 0x38, 0x36),
            border: rgb(0x50, 0x49, 0x45),
            row_selected: rgb(0x3D, 0x38, 0x2A),
            text: rgb(0xEB, 0xDB, 0xB2),
            text_dim: rgb(0xBD, 0xAE, 0x93),
            text_faint: rgb(0x92, 0x86, 0x74),
            accent: rgb(0xFA, 0xBD, 0x2F),
            accent_hover: rgb(0xFB, 0xD2, 0x4A),
            accent_pressed: rgb(0xD7, 0x99, 0x21),
            danger: rgb(0xFB, 0x49, 0x34),
            header: rgb(0x28, 0x28, 0x28),
        },
        ThemeMode::TokyoNight => Palette {
            bg: rgb(0x16, 0x16, 0x1E),
            panel: rgb(0x1A, 0x1B, 0x26),
            surface: rgb(0x24, 0x28, 0x3B),
            surface_hover: rgb(0x2F, 0x35, 0x4F),
            border: rgb(0x41, 0x48, 0x68),
            row_selected: rgb(0x28, 0x3A, 0x5B),
            text: rgb(0xC0, 0xCA, 0xF5),
            text_dim: rgb(0xA9, 0xB1, 0xD6),
            text_faint: rgb(0x73, 0x7A, 0xA2),
            accent: rgb(0x7A, 0xA2, 0xF7),
            accent_hover: rgb(0x9A, 0xB8, 0xFA),
            accent_pressed: rgb(0x5E, 0x7F, 0xC9),
            danger: rgb(0xF7, 0x76, 0x8E),
            header: rgb(0x1A, 0x1B, 0x26),
        },
        ThemeMode::RosePine => Palette {
            bg: rgb(0x19, 0x17, 0x24),
            panel: rgb(0x1F, 0x1D, 0x2E),
            surface: rgb(0x26, 0x23, 0x3A),
            surface_hover: rgb(0x31, 0x2D, 0x48),
            border: rgb(0x40, 0x3D, 0x52),
            row_selected: rgb(0x36, 0x2F, 0x50),
            text: rgb(0xE0, 0xDE, 0xF4),
            text_dim: rgb(0x90, 0x8C, 0xAA),
            text_faint: rgb(0x6E, 0x6A, 0x86),
            accent: rgb(0xC4, 0xA7, 0xE7),
            accent_hover: rgb(0xD5, 0xBE, 0xF0),
            accent_pressed: rgb(0xA3, 0x86, 0xC4),
            danger: rgb(0xEB, 0x6F, 0x92),
            header: rgb(0x1F, 0x1D, 0x2E),
        },
        ThemeMode::Everforest => Palette {
            bg: rgb(0x23, 0x2A, 0x2E),
            panel: rgb(0x2B, 0x33, 0x39),
            surface: rgb(0x32, 0x3C, 0x41),
            surface_hover: rgb(0x3D, 0x48, 0x4D),
            border: rgb(0x47, 0x52, 0x58),
            row_selected: rgb(0x3A, 0x4A, 0x40),
            text: rgb(0xD3, 0xC6, 0xAA),
            text_dim: rgb(0x9D, 0xA9, 0xA0),
            text_faint: rgb(0x7A, 0x84, 0x7C),
            accent: rgb(0xA7, 0xC0, 0x80),
            accent_hover: rgb(0xBA, 0xD1, 0x92),
            accent_pressed: rgb(0x86, 0x9D, 0x62),
            danger: rgb(0xE6, 0x7E, 0x80),
            header: rgb(0x2B, 0x33, 0x39),
        },
        ThemeMode::OneDark => Palette {
            bg: rgb(0x21, 0x25, 0x2B),
            panel: rgb(0x28, 0x2C, 0x34),
            surface: rgb(0x2C, 0x31, 0x3A),
            surface_hover: rgb(0x35, 0x3B, 0x45),
            border: rgb(0x3E, 0x44, 0x51),
            row_selected: rgb(0x2F, 0x3E, 0x52),
            text: rgb(0xAB, 0xB2, 0xBF),
            text_dim: rgb(0x82, 0x89, 0x97),
            text_faint: rgb(0x5C, 0x63, 0x70),
            accent: rgb(0x61, 0xAF, 0xEF),
            accent_hover: rgb(0x8A, 0xC7, 0xF5),
            accent_pressed: rgb(0x4A, 0x8C, 0xC4),
            danger: rgb(0xE0, 0x6C, 0x75),
            header: rgb(0x28, 0x2C, 0x34),
        },
        ThemeMode::Kanagawa => Palette {
            bg: rgb(0x16, 0x16, 0x1D),
            panel: rgb(0x1F, 0x1F, 0x28),
            surface: rgb(0x2A, 0x2A, 0x37),
            surface_hover: rgb(0x36, 0x36, 0x46),
            border: rgb(0x44, 0x44, 0x55),
            row_selected: rgb(0x2D, 0x35, 0x52),
            text: rgb(0xDC, 0xD7, 0xBA),
            text_dim: rgb(0xC8, 0xC0, 0x93),
            text_faint: rgb(0x8A, 0x89, 0x80),
            accent: rgb(0x7E, 0x9C, 0xD8),
            accent_hover: rgb(0x9B, 0xB4, 0xE8),
            accent_pressed: rgb(0x5F, 0x79, 0xAE),
            danger: rgb(0xE4, 0x68, 0x76),
            header: rgb(0x1F, 0x1F, 0x28),
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
    // Les suggestions des champs (`--fullscreen`, `coop, détente…`) restent
    // discrètes : sans couleur faible explicite, elles prendraient la couleur
    // du texte normal à cause de `override_text_color`.
    v.weak_text_color = Some(p.text_faint);
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
        (
            egui::TextStyle::Heading,
            FontId::new(20.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            FontId::new(13.5, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
        (
            egui::TextStyle::Button,
            FontId::new(13.5, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
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

/// Hauteur de bannière : plus haute que le 3.1:1 des `library_hero` de Steam,
/// pour laisser respirer le logo, le bouton et les actualités.
pub fn hero_height(width: f32) -> f32 {
    (width / 2.7).clamp(230.0, 520.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_accent_d_un_jeu_ne_touche_pas_au_reste() {
        let base = palette(ThemeMode::Amoled);
        let jeu = base.with_accent(accent_from_hue(150.0));
        assert_ne!(jeu.accent, base.accent);
        assert_ne!(jeu.accent_hover, jeu.accent_pressed);
        assert_eq!(jeu.bg, base.bg);
        assert_eq!(jeu.text, base.text);
    }

    #[test]
    fn le_texte_s_adapte_a_la_couleur_d_accent() {
        // Jaune : texte noir ; bleu : texte blanc.
        assert_eq!(on_accent(accent_from_hue(60.0)), Color32::BLACK);
        assert_eq!(on_accent(accent_from_hue(230.0)), Color32::WHITE);
    }

    #[test]
    fn une_icone_s_oppose_au_fond_qui_la_porte() {
        // Fond clair (bleu ciel) : l'icône est de la même teinte, très foncée.
        let ciel = rgb(0x7F, 0xC8, 0xFF);
        let icone = on_surface(ciel);
        let [r, g, b, a] = icone.to_array();
        assert_eq!(a, 255, "l'icône doit rester opaque : {icone:?}");
        assert!(
            r < 90 && g < 120 && b < 140,
            "icône trop claire : {icone:?}"
        );
        assert!(b > r, "la teinte doit rester bleutée : {icone:?}");
        // Fond sombre : l'icône repasse en blanc cassé.
        let sombre = on_surface(rgb(0x0B, 0x0E, 0x11));
        assert!(sombre.r() > 200, "{sombre:?}");
    }

    #[test]
    fn les_treize_themes_sont_distincts_et_lisibles() {
        assert_eq!(ThemeMode::ALL.len(), 13);
        for (index, mode) in ThemeMode::ALL.iter().enumerate() {
            let pal = palette(*mode);
            assert_ne!(pal.bg, pal.panel, "{mode:?} : fond et panneau confondus");
            assert_ne!(pal.text, pal.bg, "{mode:?} : texte illisible sur le fond");
            assert!(pal.text.r() > pal.bg.r(), "{mode:?} : texte trop sombre");
            for other in ThemeMode::ALL.iter().skip(index + 1) {
                assert_ne!(
                    pal.bg,
                    palette(*other).bg,
                    "{mode:?} et {other:?} identiques"
                );
            }
        }
    }

    #[test]
    fn une_teinte_verte_donne_un_accent_vert() {
        let color = accent_from_hue(140.0);
        assert!(color.g() > color.r(), "{color:?}");
        assert!(color.g() > color.b(), "{color:?}");
    }
}

/// Mélange linéaire de deux couleurs, pour les transitions (survol, sélection).
pub fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| {
        (a as f32 + (b as f32 - a as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    let [r1, g1, b1, a1] = from.to_array();
    let [r2, g2, b2, a2] = to.to_array();
    Color32::from_rgba_premultiplied(mix(r1, r2), mix(g1, g2), mix(b1, b2), mix(a1, a2))
}
