//! Types communs à la bibliothèque unifiée des boutiques.
//!
//! Les adaptateurs concrets (Steam, Epic, GOG) restent dans des modules
//! séparés ; ces types ne contiennent aucune donnée d'authentification.

pub mod achievements;
pub mod covers;
pub mod credentials;
pub mod gog;
pub mod gog_library;
pub mod legendary;
pub mod model;
pub mod operations;
pub mod playtime;
pub mod runner;
pub mod steam;
pub mod tools;

pub use achievements::{AchievementInfo, AchievementRow, SessionUnlock};
pub use model::{
    CapabilityState, GameCapabilities, InstallState, Provider, ProviderGame, ProviderGameKey,
    ProviderLibrary,
};
pub use playtime::PlaytimeState;
/// Vrai quand un fichier de session a été créé ou réécrit entre deux relevés.
///
/// Legendary et gogdl sortent parfois avec le code 0 en cas de refus : c'est
/// l'écriture du fichier de session qui prouve la connexion. Le contenu est
/// comparé plutôt que l'horodatage, dont la résolution peut être grossière.
/// Taille d'installation publiée par `legendary info` ou `gogdl info`.
///
/// Les deux outils changent de forme selon leur version : on cherche donc les
/// clés connues au premier niveau, puis récursivement, en acceptant un nombre
/// ou une chaîne de chiffres. Aucune valeur n'est inventée quand la réponse ne
/// la contient pas.
pub fn install_size_from_json(text: &str) -> Option<u64> {
    let start = text.find('{')?;
    let root: serde_json::Value = serde_json::from_str(&text[start..]).ok()?;
    fn find(value: &serde_json::Value, depth: usize) -> Option<u64> {
        if depth > 6 {
            return None;
        }
        match value {
            serde_json::Value::Object(map) => {
                for key in [
                    "install_size",
                    "download_size",
                    "disk_size",
                    "total_size",
                    "size",
                ] {
                    if let Some(size) = map.get(key).and_then(|value| match value {
                        serde_json::Value::Number(number) => number.as_u64(),
                        serde_json::Value::String(text) => text.trim().parse::<u64>().ok(),
                        _ => None,
                    }) {
                        if size > 0 {
                            return Some(size);
                        }
                    }
                }
                map.values().find_map(|value| find(value, depth + 1))
            }
            serde_json::Value::Array(items) => {
                items.iter().find_map(|value| find(value, depth + 1))
            }
            _ => None,
        }
    }
    find(&root, 0)
}

pub fn session_written(before: Option<&[u8]>, after: Option<&[u8]>) -> bool {
    match (before, after) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(before), Some(after)) => before != after,
    }
}

#[cfg(test)]
mod tests {
    use super::{install_size_from_json, session_written};

    #[test]
    fn la_taille_d_installation_est_lue_dans_les_formes_connues() {
        assert_eq!(
            install_size_from_json(r#"{"install_size": 6442450944}"#),
            Some(6_442_450_944)
        );
        assert_eq!(
            install_size_from_json(r#"{"folder_name":"Game","size":"4294967296"}"#),
            Some(4_294_967_296)
        );
        assert_eq!(
            install_size_from_json(r#"{"data":{"download_size":123456}}"#),
            Some(123_456)
        );
        assert_eq!(install_size_from_json(r#"{"title":"sans taille"}"#), None);
        assert_eq!(install_size_from_json("pas du json"), None);
    }

    #[test]
    fn une_session_absente_ou_inchangee_ne_prouve_pas_une_connexion() {
        assert!(!session_written(None, None));
        assert!(!session_written(Some(b"a"), Some(b"a")));
        assert!(!session_written(Some(b"a"), None));
        assert!(session_written(None, Some(b"a")));
        assert!(session_written(Some(b"a"), Some(b"b")));
    }
}
