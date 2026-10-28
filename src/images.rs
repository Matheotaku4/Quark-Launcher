//! Chargement des artworks en textures egui.
//!
//! On décode nous-mêmes les fichiers (JPEG/PNG/WebP) au lieu de passer par le
//! chargeur `file://` d'`egui_extras` : celui-ci n'est pas compilé par défaut
//! et sa dépendance `image` n'active aucun codec, ce qui affichait une icône
//! d'avertissement à la place des bannières et des logos.

use crate::app::QuarkApp;
use eframe::egui::{self, Align2, Color32, FontId, TextureHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Default)]
pub struct TextureCache {
    entries: HashMap<PathBuf, (Option<SystemTime>, TextureHandle)>,
}

impl TextureCache {
    /// Texture d'un fichier local, rechargée si le fichier a changé.
    pub fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<&TextureHandle> {
        let modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        let stale = match self.entries.get(path) {
            Some((stamp, _)) => *stamp != modified,
            None => true,
        };

        if stale {
            let bytes = std::fs::read(path).ok()?;
            let decoded = image::load_from_memory(&bytes).ok()?;
            let rgba = decoded.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw().as_slice());
            let texture =
                ctx.load_texture(path.to_string_lossy(), color_image, egui::TextureOptions::LINEAR);
            self.entries.insert(path.to_path_buf(), (modified, texture));
        }

        self.entries.get(path).map(|(_, texture)| texture)
    }
}

impl QuarkApp {
    /// Récupère la texture d'un artwork (clone d'un handle, donc peu coûteux).
    pub fn texture(&mut self, ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
        self.textures.get(ctx, path).cloned()
    }
}

/// Versions floutées des bannières (fond de la fiche jeu).
///
/// L'image est réduite à `BLUR_WIDTH` pixels, floutée, puis agrandie par la
/// carte graphique : le résultat est un vrai flou doux, coûteux une seule fois
/// par fichier.
#[derive(Default)]
pub struct BlurCache {
    entries: HashMap<PathBuf, (Option<SystemTime>, TextureHandle)>,
}

const BLUR_WIDTH: u32 = 96;

impl BlurCache {
    pub fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<&TextureHandle> {
        let modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        let stale = match self.entries.get(path) {
            Some((stamp, _)) => *stamp != modified,
            None => true,
        };
        if stale {
            let bytes = std::fs::read(path).ok()?;
            let decoded = image::load_from_memory(&bytes).ok()?;
            let (width, height) = (decoded.width().max(1), decoded.height().max(1));
            let scaled_height = ((BLUR_WIDTH as f32 * height as f32) / width as f32)
                .round()
                .clamp(1.0, 4096.0) as u32;
            let small = decoded
                .resize(BLUR_WIDTH, scaled_height, image::imageops::FilterType::Triangle)
                .to_rgba8();
            let blurred = image::imageops::blur(&small, 4.0);
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [blurred.width() as usize, blurred.height() as usize],
                blurred.as_raw(),
            );
            let texture = ctx.load_texture(
                format!("blur:{}", path.display()),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            self.entries.insert(path.to_path_buf(), (modified, texture));
        }
        self.entries.get(path).map(|(_, texture)| texture)
    }
}

impl QuarkApp {
    /// Texture floutée d'une bannière, pour le fond de la fiche.
    pub fn blurred_texture(&mut self, ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
        self.blurs.get(ctx, path).cloned()
    }
}

/// Dessine la texture dans `rect` en conservant ses proportions.

/// Dessine la texture dans `rect` en conservant ses proportions, collée en bas
/// à gauche : le logo de la fiche occupe ainsi la même place que le titre de
/// secours, au lieu de flotter au centre de la bannière.
pub fn draw_fit_left(ui: &mut egui::Ui, texture: &TextureHandle, rect: egui::Rect) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let target = size * scale;
    let fitted =
        egui::Rect::from_min_size(egui::pos2(rect.left(), rect.bottom() - target.y), target);
    ui.put(
        fitted,
        egui::Image::new(egui::load::SizedTexture::new(texture.id(), target)),
    );
}

/// Dessine la texture en remplissant `rect` sans jamais la déformer : l'image
/// est mise à l'échelle pour couvrir la zone, puis rognée (centrée) sur les
/// bords qui dépassent. C'est le comportement d'une bannière en plein écran :
/// `rect` doit être le rectangle de découpe du painter appelant.
pub fn draw_cover(painter: &egui::Painter, texture: &TextureHandle, rect: egui::Rect) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let scale = (rect.width() / size.x).max(rect.height() / size.y);
    let fitted = egui::Rect::from_center_size(rect.center(), size * scale);
    painter.image(
        texture.id(),
        fitted,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// Remplissage de secours d'une bannière : un aplat teinté par la couleur du
/// jeu et sa lettre, plutôt qu'un grand rectangle noir qui ressemble à une
/// image manquante.
pub fn hero_placeholder(
    painter: &egui::Painter,
    rect: egui::Rect,
    letter: &str,
    accent: Color32,
    radius: f32,
) {
    painter.rect_filled(rect, radius, accent.gamma_multiply(0.16));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        FontId::proportional((rect.height() * 0.45).clamp(40.0, 200.0)),
        Color32::from_white_alpha(20),
    );
}

/// Dessine une image dans `rect` en la recadrant (« cover » : jamais déformée)
/// et avec des coins arrondis.
///
/// Le widget est posé dans un enfant du `Ui` reçu : le curseur de `ui` ne bouge
/// pas, on peut donc dessiner vignette par vignette dans une liste sans casser
/// la mise en page.
pub fn rounded_cover(ui: &mut egui::Ui, rect: egui::Rect, texture: &TextureHandle, radius: f32) {
    rounded_cover_tinted(ui, rect, texture, radius, egui::Color32::WHITE);
}

/// Même chose avec une teinte (fondu d'entrée du carrousel).
pub fn rounded_cover_tinted(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    texture: &TextureHandle,
    radius: f32,
    tint: egui::Color32,
) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    // Fenêtre de la texture qui garde le rapport de la zone, centrée : c'est
    // exactement un recadrage, l'image n'est jamais étirée.
    let target = rect.width() / rect.height();
    let image = size.x / size.y;
    let uv = if image > target {
        let width = target / image;
        egui::Rect::from_min_max(
            egui::pos2((1.0 - width) / 2.0, 0.0),
            egui::pos2((1.0 + width) / 2.0, 1.0),
        )
    } else {
        let height = image / target;
        egui::Rect::from_min_max(
            egui::pos2(0.0, (1.0 - height) / 2.0),
            egui::pos2(1.0, (1.0 + height) / 2.0),
        )
    };
    let widget = egui::Image::new(egui::load::SizedTexture::new(texture.id(), rect.size()))
        .uv(uv)
        .corner_radius(radius)
        .tint(tint);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.put(rect, widget);
}

/// Voile noir dégradé posé sur le bas d'une image arrondie.
///
/// Les coins du bas sont retirés bande par bande : un simple rectangle de
/// dégradé déborderait des coins arrondis de la vignette.
pub fn rounded_veil(
    painter: &egui::Painter,
    rect: egui::Rect,
    radius: f32,
    strength: f32,
    fraction: f32,
) {
    const ROWS: usize = 26;
    if strength <= 0.0 || rect.height() <= 2.0 {
        return;
    }
    let veil = rect.height() * fraction.clamp(0.0, 1.0);
    if veil <= 1.0 {
        return;
    }
    let radius = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    for row in 0..ROWS {
        let top = row as f32 / ROWS as f32;
        let bottom = (row + 1) as f32 / ROWS as f32;
        let y0 = rect.bottom() - veil * (1.0 - top);
        let y1 = rect.bottom() - veil * (1.0 - bottom);
        let band = egui::Rect::from_min_max(
            egui::pos2(rect.left(), y0.min(y1)),
            egui::pos2(rect.right(), y0.max(y1)),
        );
        // Retrait latéral dans la zone des coins arrondis.
        let corner_start = rect.bottom() - radius;
        let inset = if band.bottom() > corner_start {
            let dy = (band.bottom() - corner_start).clamp(0.0, radius);
            radius - (radius * radius - dy * dy).max(0.0).sqrt()
        } else {
            0.0
        };
        let alpha = (top * top * 235.0 * strength).round().clamp(0.0, 255.0) as u8;
        painter.rect_filled(
            band.shrink2(egui::vec2(inset, 0.0)),
            0.0,
            egui::Color32::from_black_alpha(alpha),
        );
    }
}

/// Couleur d'accent déduite d'une image : teinte dominante des pixels
/// suffisamment colorés, ramenée à une luminosité lisible sur le noir.
/// Renvoie `None` si l'image est grise ou noire (aucune teinte exploitable).
pub fn accent_from_path(path: &Path) -> Option<egui::Color32> {
    let bytes = std::fs::read(path).ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgb8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return None;
    }

    // Grille d'échantillonnage d'environ 32 points par axe : assez pour une
    // teinte stable, assez peu pour un coût négligeable.
    let step_x = (width / 32).max(1) as usize;
    let step_y = (height / 32).max(1) as usize;
    let mut buckets = [0.0_f32; 36];
    for y in (0..height).step_by(step_y) {
        for x in (0..width).step_by(step_x) {
            let pixel = image.get_pixel(x, y);
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;
            let max = r.max(g).max(b);
            let min = r.min(g).min(b);
            let saturation = if max > 0.0 { (max - min) / max } else { 0.0 };
            // Les gris et les noirs ne portent pas de teinte : les ignorer évite
            // de sortir un accent terne sur une couverture sombre.
            if saturation < 0.18 || max < 0.12 {
                continue;
            }
            let bucket = ((hue_degrees(r, g, b) / 10.0).round() as usize).min(35);
            buckets[bucket] += saturation * max;
        }
    }

    let (index, weight) = buckets
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    if *weight <= 0.0 {
        return None;
    }
    Some(crate::theme::accent_from_hue(index as f32 * 10.0))
}

/// Vrai si l'image est globalement claire.
///
/// Sert à poser des icônes sombres sur une bannière claire : une icône blanche
/// sur une image bleu ciel devient invisible. Le calcul échantillonne l'image
/// (32 × 32 points au plus) et n'a lieu qu'une fois par fichier côté appelant.
pub fn is_light(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(image) = image::load_from_memory(&bytes) else {
        return false;
    };
    let image = image.to_rgb8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return false;
    }
    let step_x = (width / 32).max(1);
    let step_y = (height / 32).max(1);
    let mut total = 0.0_f32;
    let mut count = 0.0_f32;
    for y in (0..height).step_by(step_y as usize) {
        for x in (0..width).step_by(step_x as usize) {
            let pixel = image.get_pixel(x, y);
            total += 0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32;
            count += 1.0;
        }
    }
    count > 0.0 && total / count > 150.0
}

/// Teinte en degrés d'une couleur RVB (0-360).
fn hue_degrees(r: f32, g: f32, b: f32) -> f32 {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta <= f32::EPSILON {
        return 0.0;
    }
    let h = if (max - r).abs() < 1e-6 {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-6 {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    h * 60.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_les_artworks_locaux() {
        let dir = std::env::temp_dir().join("quark-images-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cover.png");
        let image = image::RgbaImage::from_pixel(8, 12, image::Rgba([200, 40, 40, 255]));
        image.save(&path).unwrap();

        let ctx = eframe::egui::Context::default();
        let mut cache = TextureCache::default();
        let texture = cache.get(&ctx, &path).expect("texture décodée");
        assert_eq!(texture.size(), [8, 12]);
        assert!(cache.get(&ctx, &path).is_some(), "le cache doit resservir");
    }

    #[test]
    fn accent_deduit_d_une_couverture() {
        let dir = std::env::temp_dir().join("quark-accent-test");
        std::fs::create_dir_all(&dir).unwrap();
        let rouge = dir.join("rouge.png");
        image::RgbImage::from_pixel(64, 96, image::Rgb([205, 35, 45]))
            .save(&rouge)
            .unwrap();
        let accent = accent_from_path(&rouge).expect("accent calculé");
        assert!(
            accent.r() > 180 && accent.g() < 120,
            "accent rouge attendu, obtenu {accent:?}"
        );
        // Une image grise ne donne aucune teinte : l'appelant garde le thème.
        let gris = dir.join("gris.png");
        image::RgbImage::from_pixel(64, 96, image::Rgb([128, 128, 128]))
            .save(&gris)
            .unwrap();
        assert!(accent_from_path(&gris).is_none());
    }

    #[test]
    fn une_image_claire_est_reconnue() {
        let dir = std::env::temp_dir().join("quark-light-test");
        std::fs::create_dir_all(&dir).unwrap();
        let clair = dir.join("clair.png");
        image::RgbImage::from_pixel(32, 18, image::Rgb([0x7F, 0xC8, 0xFF]))
            .save(&clair)
            .unwrap();
        assert!(is_light(&clair));
        let sombre = dir.join("sombre.png");
        image::RgbImage::from_pixel(32, 18, image::Rgb([12, 16, 22]))
            .save(&sombre)
            .unwrap();
        assert!(!is_light(&sombre));
        // Fichier absent : on ne casse rien, on suppose sombre.
        assert!(!is_light(&dir.join("absent.png")));
    }

    #[test]
    fn recadrage_conserve_les_proportions() {
        // Une image 2:1 dans une zone 1:1 : elle couvre la zone et déborde en
        // largeur (jamais étirée).
        let source = egui::Vec2::new(400.0, 200.0);
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(100.0, 100.0));
        let scale = (rect.width() / source.x).max(rect.height() / source.y);
        let fitted = source * scale;
        assert_eq!(fitted, egui::Vec2::new(200.0, 100.0));
        assert_eq!(fitted.x / fitted.y, source.x / source.y);
        assert!(fitted.x > rect.width() && fitted.y >= rect.height());
    }
}
