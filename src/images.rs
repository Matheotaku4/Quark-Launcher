//! Chargement des artworks en textures egui.
//!
//! On décode nous-mêmes les fichiers (JPEG/PNG/WebP) au lieu de passer par le
//! chargeur `file://` d'`egui_extras` : celui-ci n'est pas compilé par défaut
//! et sa dépendance `image` n'active aucun codec, ce qui affichait une icône
//! d'avertissement à la place des bannières et des logos.

use crate::app::QuarkApp;
use eframe::egui::{self, TextureHandle};
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

/// Dessine la texture dans `rect` en conservant ses proportions.
pub fn draw_fit(ui: &mut egui::Ui, texture: &TextureHandle, rect: egui::Rect) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    let target = size * scale;
    let fitted = egui::Rect::from_center_size(rect.center(), target);
    ui.put(
        fitted,
        egui::Image::new(egui::load::SizedTexture::new(texture.id(), target)),
    );
}

/// Dessine la texture dans `rect` en conservant ses proportions.

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
