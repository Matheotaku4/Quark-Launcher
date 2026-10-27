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

/// Dessine la texture en remplissant exactement `rect` (ratio déjà vérifié au
/// téléchargement pour les bannières et les couvertures).
pub fn draw_fill(ui: &mut egui::Ui, texture: &TextureHandle, rect: egui::Rect) {
    let target = rect.size();
    if target.x <= 0.0 || target.y <= 0.0 {
        return;
    }
    ui.put(
        rect,
        egui::Image::new(egui::load::SizedTexture::new(texture.id(), target)),
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
}
