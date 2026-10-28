//! Icône de la zone de notification (StatusNotifierItem, via `ksni`).
//!
//! Elle sert à garder Quark ouvert quand on ferme sa fenêtre : la croix range
//! l'application dans l'icône, un clic la ramène, et « Quitter » dans le menu
//! termine vraiment le processus. Aucun GTK n'est nécessaire : `ksni` parle
//! D-Bus directement, ce qui fonctionne sur KDE, XFCE, Cinnamon… et sur GNOME
//! avec l'extension AppIndicator.

use eframe::egui;
use ksni::blocking::TrayMethods;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Demandes de l'icône, lues par l'application au tick suivant.
#[derive(Default)]
pub struct TrayShared {
    show: AtomicBool,
    quit: AtomicBool,
}

/// Icône vivante : la détruire retire l'icône de la zone de notification.
pub struct TrayHandle {
    shared: Arc<TrayShared>,
    handle: ksni::blocking::Handle<QuarkTray>,
}

impl TrayHandle {
    /// Enregistre l'icône sur le bus de session. L'erreur est renvoyée telle
    /// quelle (pas de D-Bus, pas de watcher…) pour être affichée dans les
    /// paramètres plutôt que de faire croire à une icône présente.
    pub fn spawn(ctx: &egui::Context) -> Result<Self, String> {
        let shared = Arc::new(TrayShared::default());
        let tray = QuarkTray {
            shared: shared.clone(),
            ctx: ctx.clone(),
        };
        let handle = tray.spawn().map_err(|err| err.to_string())?;
        Ok(Self { shared, handle })
    }

    /// Vrai tant que le service n'a pas été arrêté (watcher disparu, etc.).
    pub fn alive(&self) -> bool {
        !self.handle.is_closed()
    }

    pub fn take_show(&self) -> bool {
        self.shared.show.swap(false, Ordering::Relaxed)
    }

    pub fn take_quit(&self) -> bool {
        self.shared.quit.swap(false, Ordering::Relaxed)
    }
}

impl Drop for TrayHandle {
    fn drop(&mut self) {
        let _ = self.handle.shutdown();
    }
}

struct QuarkTray {
    shared: Arc<TrayShared>,
    /// Réveille egui : sans cela, une fenêtre réduite ne redessine jamais et
    /// le clic sur l'icône resterait sans effet.
    ctx: egui::Context,
}

impl QuarkTray {
    fn request_show(&self) {
        self.shared.show.store(true, Ordering::Relaxed);
        self.ctx.request_repaint();
    }

    fn request_quit(&self) {
        self.shared.quit.store(true, Ordering::Relaxed);
        self.ctx.request_repaint();
    }
}

impl ksni::Tray for QuarkTray {
    fn id(&self) -> String {
        "quark".to_string()
    }

    fn title(&self) -> String {
        "Quark".to_string()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![mark_icon(32)]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Quark".to_string(),
            description: "Launcher de jeux".to_string(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.request_show();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Ouvrir Quark".to_string(),
                activate: Box::new(|tray: &mut Self| tray.request_show()),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quitter Quark".to_string(),
                activate: Box::new(|tray: &mut Self| tray.request_quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// La marque de Quark (anneau bleu + point), dessinée en ARGB32 : l'icône ne
/// dépend ainsi d'aucun thème installé.
fn mark_icon(size: i32) -> ksni::Icon {
    let s = size.clamp(8, 256) as usize;
    let mut data = vec![0u8; s * s * 4];
    let center = s as f32 / 2.0;
    let r_out = s as f32 * 0.44;
    let r_in = s as f32 * 0.29;
    let ring_mid = (r_out + r_in) / 2.0;
    let ring_half = (r_out - r_in) / 2.0;
    let dot_radius = s as f32 * 0.13;
    for y in 0..s {
        for x in 0..s {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            let in_ring = (distance - ring_mid).abs() <= ring_half;
            // Le point est décalé comme sur la marque de la barre du haut.
            let ox = dx - s as f32 * 0.16;
            let oy = dy - s as f32 * 0.09;
            let in_dot = (ox * ox + oy * oy).sqrt() <= dot_radius;
            let (r, g, b) = if in_ring {
                (0x1A, 0x9F, 0xFF)
            } else if in_dot {
                (0x3F, 0xB0, 0xFF)
            } else {
                continue;
            };
            let index = (y * s + x) * 4;
            data[index] = 0xFF;
            data[index + 1] = r;
            data[index + 2] = g;
            data[index + 3] = b;
        }
    }
    ksni::Icon {
        width: s as i32,
        height: s as i32,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::mark_icon;

    #[test]
    fn l_icone_a_la_taille_demandee() {
        let icon = mark_icon(32);
        assert_eq!(icon.width, 32);
        assert_eq!(icon.height, 32);
        assert_eq!(icon.data.len(), 32 * 32 * 4);
    }

    #[test]
    fn l_icone_est_transparente_au_centre_sauf_le_point() {
        let icon = mark_icon(32);
        // Coin supérieur gauche : transparent (rien n'est dessiné).
        assert_eq!(icon.data[0], 0);
        // Le point est décalé en bas à droite du centre : il est opaque.
        let index = ((32 * 19 + 21) * 4) as usize;
        assert_eq!(icon.data[index], 0xFF);
    }
}
