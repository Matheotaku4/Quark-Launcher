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
