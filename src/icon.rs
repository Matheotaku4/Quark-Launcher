//! Icône de Quark, embarquée dans le binaire.
//!
//! La même image sert pour la fenêtre (X11 et Wayland, via `app_id` et le
//! `.desktop` fourni) et pour le fichier exécutable : aucun thème d'icônes
//! n'est nécessaire.

use eframe::egui;

/// PNG 512 × 512, généré par `tools/make-icon.py`.
pub const PNG: &[u8] = include_bytes!("../assets/quark.png");

/// Icône de fenêtre (RGBA non prémultiplié, ce qu'attend egui).
pub fn window_icon() -> egui::IconData {
    decode().unwrap_or(egui::IconData {
        rgba: vec![0, 0, 0, 0],
        width: 1,
        height: 1,
    })
}

fn decode() -> Option<egui::IconData> {
    let image = image::load_from_memory(PNG).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_icone_est_decodee() {
        let icon = window_icon();
        assert_eq!(icon.width, 512);
        assert_eq!(icon.height, 512);
        assert_eq!(icon.rgba.len(), 512 * 512 * 4);
        // Le centre de la bague est transparent : l'icône n'est pas un carré uni.
        let center = (256 * 512 + 256) * 4;
        assert!(icon.rgba[center + 3] < 200 || icon.rgba[center] < 250);
    }
}
