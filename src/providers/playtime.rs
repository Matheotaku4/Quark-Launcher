//! Temps de jeu publié par les boutiques.
//!
//! Quark ne remplace jamais le temps d'Epic ou de GOG par ses propres sessions :
//! une fiche de jeu liée à une boutique affiche la valeur de la boutique. Epic
//! expose `library-service` (même jeton que Legendary, lu dans `user.json`) ;
//! GOG expose les sessions Galaxy avec le jeton gogdl. Les deux lectures sont
//! ponctuelles, uniquement pour le jeu affiché.

use super::{Provider, ProviderGameKey};

#[derive(Clone, Debug, Default)]
pub enum PlaytimeState {
    Loading,
    Ready(u64),
    Failed(String),
    #[default]
    Unknown,
}

/// Lit le temps de jeu d'un jeu de boutique. Aucune valeur locale n'est
/// inventée : une erreur est renvoyée et la fiche l'affiche telle quelle.
pub fn fetch(key: &ProviderGameKey) -> Result<u64, String> {
    match key.provider {
        Provider::Epic => super::legendary::epic_playtime(&key.product_id),
        Provider::Gog => super::gog_library::gog_playtime(
            &super::gog::auth_config_path(),
            &key.product_id,
            crate::config::now_ts(),
        ),
        Provider::Steam => Err("Steam ne publie pas le temps de jeu sans sa session".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_temps_de_boutique_ne_devient_jamais_une_valeur_locale() {
        assert!(matches!(PlaytimeState::Unknown, PlaytimeState::Unknown));
        assert!(matches!(
            PlaytimeState::Failed("x".into()),
            PlaytimeState::Failed(_)
        ));
        assert!(matches!(
            PlaytimeState::Ready(3600),
            PlaytimeState::Ready(3600)
        ));
    }
}
