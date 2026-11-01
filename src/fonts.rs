//! Police de l'interface.
//!
//! Archivo est embarquée dans le binaire : l'interface garde la même allure
//! sur toutes les distributions, sans dépendre d'une police installée.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use std::sync::Arc;

const ARCHIVO: &[u8] = include_bytes!("../assets/fonts/Archivo-Regular.ttf");

/// Remplace la police proportionnelle par Archivo, en gardant les polices
/// egui en secours pour les glyphes absents.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "archivo".to_owned(),
        Arc::new(FontData::from_static(ARCHIVO)),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "archivo".to_owned());
    ctx.set_fonts(fonts);
}
