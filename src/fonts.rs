//! Polices de l'interface.
//!
//! Archivo et Lucide sont embarquées dans le binaire : l'interface et ses
//! icônes gardent la même allure sur toutes les distributions, sans dépendre
//! d'une police installée.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

const ARCHIVO: &[u8] = include_bytes!("../assets/fonts/Archivo-Regular.ttf");
const LUCIDE: &[u8] = include_bytes!("../assets/fonts/lucide.ttf");

/// Famille réservée aux glyphes Lucide ; le texte courant ne l'utilise jamais.
pub fn icon_family() -> FontFamily {
    FontFamily::Name("lucide".into())
}

/// Remplace la police proportionnelle par Archivo, en gardant les polices
/// egui en secours pour les glyphes absents. Lucide est installée à part pour
/// que ses glyphes privés ne soient jamais choisis pour du texte normal.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "archivo".to_owned(),
        Arc::new(FontData::from_static(ARCHIVO)),
    );
    fonts.font_data.insert(
        "lucide".to_owned(),
        Arc::new(FontData::from_static(LUCIDE)),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "archivo".to_owned());
    fonts
        .families
        .insert(icon_family(), vec!["lucide".to_owned()]);
    ctx.set_fonts(fonts);
}
