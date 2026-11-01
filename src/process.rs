//! Petits utilitaires de processus.
//!
//! Les lancements « feu et oublie » (notification, son, ouverture de dossier,
//! relance de l'icône) doivent être récoltés : sur Unix, `Child::drop` ne le
//! fait pas et chaque appel laisserait un zombie dans un processus longue
//! durée comme Quark.

use std::process::{Child, Command};

/// Lance une commande dont on n'attend pas la fin, puis la récolte dans un
/// thread dédié. Le thread se termine dès que le processus auxiliaire sort.
pub fn spawn_and_reap(command: &mut Command) -> std::io::Result<()> {
    let child = command.spawn()?;
    reap(child);
    Ok(())
}

/// Récolte un enfant déjà lancé sans bloquer l'appelant.
pub fn reap(mut child: Child) {
    std::thread::Builder::new()
        .name("quark-child-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        })
        .ok();
}
