//! Succès communs aux boutiques, et succès rapportés par une session de jeu.
//!
//! Epic (`legendary achievements --json`) et GOG (`gameplay.gog.com`) publient
//! des listes différentes mais la même information utile : un identifiant, un
//! nom, une description et une date de déblocage. Ce module donne une forme
//! unique aux deux, plus la fusion des succès envoyés pendant une partie par le
//! pont local (`QUARK_ACHIEVEMENTS_URL`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AchievementRow {
    /// Identifiant propre à la boutique (nom Epic, identifiant GOG).
    pub key: String,
    /// Identifiant utilisé pour écrire chez la boutique (`achievement_id` GOG).
    pub id: String,
    pub name: String,
    pub description: String,
    pub unlocked: bool,
    /// Progression 0..1 pour les succès à paliers (Epic la publie).
    pub progress: f32,
    /// Débloqué pendant une session et rapporté par le pont local.
    pub session: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AchievementInfo {
    pub total: u32,
    pub unlocked: u32,
    pub rows: Vec<AchievementRow>,
}

impl AchievementInfo {
    /// Recale les totaux sur la liste réelle : les boutiques les publient
    /// parfois séparément, et un total inférieur au nombre de lignes donnerait
    /// une barre de progression à plus de 100 %.
    pub fn normalize(&mut self) {
        self.total = self.total.max(self.rows.len() as u32);
        let unlocked = self.rows.iter().filter(|row| row.unlocked).count() as u32;
        self.unlocked = unlocked;
    }
}

/// Succès envoyé par un jeu en cours d'exécution, via le pont local.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionUnlock {
    pub key: String,
    pub name: String,
    pub description: String,
    /// Horodatage UNIX du déblocage (0 si le jeu n'en fournit pas).
    pub at: i64,
    /// Vrai quand la boutique a confirmé l'écriture (GOG seulement).
    pub synced: bool,
}

impl Default for SessionUnlock {
    fn default() -> Self {
        Self {
            key: String::new(),
            name: String::new(),
            description: String::new(),
            at: 0,
            synced: false,
        }
    }
}

/// Fusionne les déblocages de session dans une liste de boutique. Un succès
/// inconnu de la boutique est ajouté : c'est la seule trace disponible pour un
/// jeu qui n'expose pas ses succès côté fournisseur.
pub fn merge_session(info: &mut AchievementInfo, unlocks: &[SessionUnlock]) -> bool {
    if unlocks.is_empty() {
        return false;
    }
    let mut changed = false;
    for unlock in unlocks {
        if unlock.key.trim().is_empty() && unlock.name.trim().is_empty() {
            continue;
        }
        let found = info.rows.iter_mut().find(|row| {
            (!unlock.key.trim().is_empty() && row.key.eq_ignore_ascii_case(unlock.key.trim()))
                || (!unlock.name.trim().is_empty()
                    && row.name.eq_ignore_ascii_case(unlock.name.trim()))
        });
        match found {
            Some(row) => {
                if !row.unlocked {
                    row.unlocked = true;
                    changed = true;
                }
                if !row.session {
                    row.session = true;
                    changed = true;
                }
            }
            None => {
                info.rows.push(AchievementRow {
                    key: unlock.key.clone(),
                    id: String::new(),
                    name: if unlock.name.trim().is_empty() {
                        unlock.key.clone()
                    } else {
                        unlock.name.clone()
                    },
                    description: unlock.description.clone(),
                    unlocked: true,
                    progress: 1.0,
                    session: true,
                });
                changed = true;
            }
        }
    }
    if changed {
        info.normalize();
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, name: &str, unlocked: bool) -> AchievementRow {
        AchievementRow {
            key: key.into(),
            id: key.into(),
            name: name.into(),
            description: String::new(),
            unlocked,
            progress: 0.0,
            session: false,
        }
    }

    #[test]
    fn les_totaux_suivent_la_liste_reelle() {
        let mut info = AchievementInfo {
            total: 1,
            unlocked: 9,
            rows: vec![row("a", "A", true), row("b", "B", false)],
        };
        info.normalize();
        assert_eq!(info.total, 2);
        assert_eq!(info.unlocked, 1);
    }

    #[test]
    fn une_session_debloque_les_lignes_connues_et_ajoute_les_inconnues() {
        let mut info = AchievementInfo {
            total: 3,
            unlocked: 1,
            rows: vec![
                row("ACH_FIRST", "Premier pas", false),
                row("ACH_SECOND", "Deuxième", false),
                row("ACH_THIRD", "Troisième", true),
            ],
        };
        let unlocks = [
            SessionUnlock {
                key: "ach_first".into(),
                name: "Premier pas".into(),
                ..SessionUnlock::default()
            },
            SessionUnlock {
                key: "ACH_LOCAL".into(),
                name: "Succès local".into(),
                description: "Rapporté par le jeu".into(),
                ..SessionUnlock::default()
            },
        ];
        assert!(merge_session(&mut info, &unlocks));
        assert_eq!(info.unlocked, 3);
        assert_eq!(info.total, 4);
        assert!(info.rows[0].unlocked && info.rows[0].session);
        assert!(!info.rows[1].unlocked);
        assert_eq!(info.rows[3].key, "ACH_LOCAL");
        // Fusionner deux fois ne duplique rien.
        assert!(!merge_session(&mut info, &unlocks));
        assert_eq!(info.rows.len(), 4);
    }
}
