//! Types communs à la bibliothèque unifiée des boutiques.
//!
//! Les adaptateurs concrets (Steam, Epic, GOG) restent dans des modules
//! séparés ; ces types ne contiennent aucune donnée d'authentification.

pub mod credentials;
pub mod gog;
pub mod legendary;
pub mod model;
pub mod operations;
pub mod runner;
pub mod steam;
pub mod tools;

pub use model::{
    CapabilityState, GameCapabilities, InstallState, Provider, ProviderGame, ProviderGameKey,
    ProviderLibrary,
};
/// Vrai quand un fichier de session a été créé ou réécrit entre deux relevés.
///
/// Legendary et gogdl sortent parfois avec le code 0 en cas de refus : c'est
/// l'écriture du fichier de session qui prouve la connexion. Le contenu est
/// comparé plutôt que l'horodatage, dont la résolution peut être grossière.
pub fn session_written(before: Option<&[u8]>, after: Option<&[u8]>) -> bool {
    match (before, after) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(before), Some(after)) => before != after,
    }
}

#[cfg(test)]
mod tests {
    use super::session_written;

    #[test]
    fn une_session_absente_ou_inchangee_ne_prouve_pas_une_connexion() {
        assert!(!session_written(None, None));
        assert!(!session_written(Some(b"a"), Some(b"a")));
        assert!(!session_written(Some(b"a"), None));
        assert!(session_written(None, Some(b"a")));
        assert!(session_written(Some(b"a"), Some(b"b")));
    }
}
