//! Composants dessinés à la main (icônes vectorielles, boutons, onglets).
//! Aucune dépendance à une police d'icônes : tout est tracé.

use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, Vec2,
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
    /// Téléchargements en cours.
    Download,
    ArrowLeft,
    ArrowRight,
    /// Loupe pour la recherche et la liaison d'un jeu.
    Search,
    /// Chaîne pour relier un jeu à une source d'images.
    Link,
    /// Flèche sortante d'un résultat web.
    ExternalLink,
}

impl Icon {
    /// Glyphe privé de la police Lucide embarquée. Les valeurs viennent de
    /// `lucide-static/font/lucide.css` (version 1.0) ; ne pas les remplacer par
    /// des caractères Unicode ordinaires, qui n'existent pas dans cette police.
    pub fn glyph(self) -> Option<&'static str> {
        Some(match self {
            Icon::None => return None,
            Icon::Play => "\u{e13c}",
            Icon::Stop => "\u{e167}",
            Icon::Plus => "\u{e13d}",
            Icon::Refresh => "\u{e145}",
            Icon::Trash => "\u{e18e}",
            Icon::Gear => "\u{e154}",
            Icon::Info => "\u{e0f9}",
            Icon::Log => "\u{e45f}",
            Icon::List => "\u{e106}",
            Icon::Grid => "\u{e0ff}",
            Icon::Folder => "\u{e0d7}",
            Icon::Pencil => "\u{e1f9}",
            // Lucide n'a pas de logo Steam : la manette représente la boutique.
            Icon::Steam => "\u{e0df}",
            Icon::Close => "\u{e1b2}",
            Icon::Home => "\u{e0f5}",
            Icon::Download => "\u{e0b2}",
            Icon::ArrowLeft => "\u{e048}",
            Icon::ArrowRight => "\u{e049}",
            Icon::Search => "\u{e151}",
            Icon::Link => "\u{e102}",
            Icon::ExternalLink => "\u{e0b9}",
        })
    }
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
        Rect::from_min_size(
            Pos2::new(rect.left(), rect.bottom() - w),
            Vec2::new(rect.width(), w),
        ),
        0.0,
        color,
    );
    painter.rect_filled(
        Rect::from_min_size(rect.left_top(), Vec2::new(w, rect.height())),
        0.0,
        color,
    );
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.right() - w, rect.top()),
            Vec2::new(w, rect.height()),
        ),
        0.0,
        color,
    );
}

/// Icône Lucide, peinte depuis la police embarquée. `ui` est nécessaire pour
/// que le texte soit posé dans le bon calque et découpé avec le reste du widget.
pub fn paint_icon(ui: &egui::Ui, rect: Rect, icon: Icon, color: Color32) {
    let Some(glyph) = icon.glyph() else {
        return;
    };
    let painter = ui.painter_at(rect);
    let size = rect.width().min(rect.height());
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(size, crate::fonts::icon_family()),
        color,
    );
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
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
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
    let fg = if enabled {
        theme::on_accent(bg)
    } else {
        pal.text_faint
    };
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
        paint_icon(ui,
            Rect::from_center_size(
                Pos2::new(x0 + icon_w / 2.0, rect.center().y),
                Vec2::splat(icon_w),
            ),
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
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
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
        paint_icon(ui,
            Rect::from_center_size(
                Pos2::new(x0 + icon_w / 2.0, rect.center().y),
                Vec2::splat(icon_w),
            ),
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
    let hover =
        ui.ctx()
            .animate_bool_with_time(control_id(ui, rect, label, "ghost"), resp.hovered(), 0.12);
    if hover > 0.0 {
        painter.rect_filled(rect, 4.0, pal.surface_hover.gamma_multiply(hover));
        border_rect(&painter, rect, 1.0, pal.border.gamma_multiply(hover));
    }
    let color = theme::lerp_color(pal.text_dim, pal.text, hover);
    let x0 = rect.center().x - (icon_w + gap + galley.size().x) / 2.0;
    if icon != Icon::None {
        paint_icon(ui,
            Rect::from_center_size(
                Pos2::new(x0 + icon_w / 2.0, rect.center().y),
                Vec2::splat(icon_w),
            ),
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
pub fn icon_button(
    ui: &mut egui::Ui,
    size: f32,
    icon: Icon,
    pal: &Palette,
    tooltip: &str,
) -> Response {
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
    paint_icon(ui,
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
    paint_icon(ui,
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
pub fn menu_row(ui: &mut egui::Ui, pal: &Palette, icon: Icon, color: Color32, label: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(206.0), 28.0),
        Sense::click(),
    );
    let painter = ui.painter_at(rect);
    let hover =
        ui.ctx()
            .animate_bool_with_time(ui.id().with(("quark_menu", label)), resp.hovered(), 0.10);
    if hover > 0.0 {
        painter.rect_filled(rect, 5.0, pal.surface_hover.gamma_multiply(hover));
    }
    let center = Pos2::new(rect.left() + 18.0, rect.center().y);
    painter.circle_filled(center, 10.0, Color32::from_black_alpha(130));
    painter.circle_stroke(center, 9.5, Stroke::new(1.0_f32, color.gamma_multiply(0.6)));
    paint_icon(ui,
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
    let mark =
        ui.ctx()
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

/// Ligne de réglage complète : intitulé et explication à gauche, interrupteur à
/// droite, fond qui s'éclaire au survol. Toute la ligne répond au clic.
pub fn setting_toggle(
    ui: &mut egui::Ui,
    pal: &Palette,
    title: &str,
    hint: Option<&str>,
    value: &mut bool,
    id_salt: impl std::hash::Hash,
) -> Response {
    let id = ui.id().with(("quark-setting", &id_salt));
    let width = ui.available_width();
    let height = if hint.is_some() { 46.0 } else { 34.0 };
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("row-hover"), response.hovered(), 0.12);
    let on = ui
        .ctx()
        .animate_bool_with_time(id.with("toggle"), *value, 0.18);
    if hover > 0.0 {
        ui.painter()
            .rect_filled(rect, 7.0, pal.surface_hover.gamma_multiply(hover));
    }
    // Texte à gauche, largeur limitée pour laisser la place à l'interrupteur.
    let text_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 10.0, rect.top() + 4.0),
        Pos2::new(rect.right() - 66.0, rect.bottom() - 4.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 1.0;
    child.label(RichText::new(title).size(12.5).color(pal.text));
    if let Some(hint) = hint {
        child.add(egui::Label::new(RichText::new(hint).size(10.5).color(pal.text_faint)).wrap());
    }
    // Interrupteur peint, sans widget interactif imbriqué : toute la ligne
    // réagit au clic, sans double bascule.
    let track = Rect::from_center_size(
        Pos2::new(rect.right() - 31.0, rect.center().y),
        Vec2::new(42.0, 23.0),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        track,
        track.height() / 2.0,
        theme::lerp_color(pal.border, pal.accent, on),
    );
    let knob_x = track.left() + track.height() / 2.0 + (track.width() - track.height()) * on;
    painter.circle_filled(
        Pos2::new(knob_x, track.center().y),
        track.height() / 2.0 - 3.0,
        if on > 0.5 {
            theme::on_accent(pal.accent)
        } else {
            pal.text_dim
        },
    );
    if response.clicked()
        || (response.has_focus()
            && ui.input(|input| {
                input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
            }))
    {
        *value = !*value;
        response.mark_changed();
    }
    response
}

/// Bouton « Accueil » posé au-dessus de la liste des jeux : icône maison,
/// libellé, et un trait d'accent quand la page est affichée.
pub fn home_button(ui: &mut egui::Ui, selected: bool, pal: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(112.0, 30.0), Sense::click());
    let painter = ui.painter_at(rect);
    let hover =
        ui.ctx()
            .animate_bool_with_time(ui.id().with("quark-home-hover"), resp.hovered(), 0.12);
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
    paint_icon(ui,
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
    paint_icon(ui,
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
