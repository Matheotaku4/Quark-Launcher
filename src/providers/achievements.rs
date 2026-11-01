//! Succès communs aux boutiques, et succès rapportés par une session de jeu.
//!
//! Epic (`legendary achievements --json`) et GOG (`gameplay.gog.com`) publient
//! des listes différentes mais la même information utile : un identifiant, un
//! nom, une description et une date de déblocage. Ce module donne une forme
//! unique aux deux, plus la fusion des succès envoyés pendant une partie par le
//! pont local (`QUARK_ACHIEVEMENTS_URL`).

use super::model::Provider;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    /// Pourcentage de joueurs ayant obtenu le succès (0..100). GOG le publie
    /// sous `rarity`; Epic ne l'expose pas toujours.
    pub rarity: Option<f32>,
    /// Image publiée par la boutique (URL distante).
    pub icon_url: Option<String>,
    /// Copie locale de l'image, remplie par le cache de succès.
    pub icon_path: Option<PathBuf>,
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

/// Lit un pourcentage d'obtention depuis un champ JSON de boutique. GOG
/// publie un nombre dans `rarity`; certaines réponses Epic l'enveloppent dans
/// un objet (`percent`, `percentage`) ou l'écrivent avec un « % ». Une valeur
/// hors de 0..100 est ignorée plutôt qu'affichée faussement.
pub fn rarity_percent(value: Option<&serde_json::Value>) -> Option<f32> {
    let value = value?;
    let number = match value {
        serde_json::Value::Number(number) => number.as_f64()?,
        serde_json::Value::String(text) => text
            .trim()
            .trim_end_matches('%')
            .trim()
            .replace(',', ".")
            .parse::<f64>()
            .ok()?,
        serde_json::Value::Object(map) => {
            let mut found = None;
            for key in ["percent", "percentage", "completionPercent", "value"] {
                if let Some(number) = map.get(key).and_then(serde_json::Value::as_f64) {
                    found = Some(number);
                    break;
                }
            }
            found?
        }
        _ => return None,
    };
    (number.is_finite() && (0.0..=100.0).contains(&number)).then_some(number as f32)
}

/// Ordre d'affichage des succès : les plus rares d'abord (pourcentage
/// d'obtention croissant), puis les succès débloqués, puis le nom. Les succès
/// sans pourcentage connu restent en fin de liste.
pub fn sorted_rows(rows: &[AchievementRow]) -> Vec<&AchievementRow> {
    let mut sorted: Vec<&AchievementRow> = rows.iter().collect();
    sorted.sort_by(|left, right| {
        match (left.rarity, right.rarity) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| right.unlocked.cmp(&left.unlocked))
        .then_with(|| left.name.cmp(&right.name))
    });
    sorted
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
                    rarity: None,
                    icon_url: None,
                    icon_path: None,
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

/// Télécharge les images de succès dans le cache local et remplit `icon_path`.
///
/// La lecture se fait dans le fil d'arrière-plan des succès : l'interface
/// n'attend jamais le réseau. Une image déjà présente n'est pas retéléchargée,
/// et un échec laisse simplement la pastille colorée à la place.
pub fn cache_icons(info: &mut AchievementInfo, provider: Provider, product_id: &str) {
    let folder = crate::config::artwork_root()
        .join("achievement-icons")
        .join(format!(
            "{}-{}",
            match provider {
                Provider::Steam => "steam",
                Provider::Epic => "epic",
                Provider::Gog => "gog",
            },
            safe_segment(product_id)
        ));
    for row in &mut info.rows {
        let Some(url) = row.icon_url.as_deref() else {
            continue;
        };
        let key = if row.key.trim().is_empty() {
            row.name.as_str()
        } else {
            row.key.as_str()
        };
        if let Some(path) = cached_icon(&folder, key).or_else(|| download_icon(url, &folder, key)) {
            row.icon_path = Some(path);
        }
    }
}

fn safe_segment(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn cached_icon(folder: &Path, key: &str) -> Option<PathBuf> {
    let path = folder.join(format!("{}.png", safe_segment(key)));
    std::fs::metadata(&path)
        .map(|metadata| metadata.len() > 128)
        .unwrap_or(false)
        .then_some(path)
}

fn download_icon(url: &str, folder: &Path, key: &str) -> Option<PathBuf> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let response = crate::artwork::http()
        .get(url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = crate::net::bytes(response, 4 * 1024 * 1024, "icône de succès").ok()?;
    let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (width, height) = (image.width().max(1), image.height().max(1));
    let scale = (96.0 / width.max(height) as f32).min(1.0);
    let (width, height) = (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    );
    let small = if image.width() != width || image.height() != height {
        image::imageops::thumbnail(&image, width, height)
    } else {
        image
    };
    std::fs::create_dir_all(folder).ok()?;
    let path = folder.join(format!("{}.png", safe_segment(key)));
    small.save(&path).ok()?;
    Some(path)
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
            rarity: None,
            icon_url: None,
            icon_path: None,
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

    #[test]
    fn le_pourcentage_est_lu_dans_les_formes_publiees_par_les_boutiques() {
        assert_eq!(rarity_percent(Some(&serde_json::json!(4.7))), Some(4.7));
        assert_eq!(
            rarity_percent(Some(&serde_json::json!("12,5 %"))),
            Some(12.5)
        );
        assert_eq!(
            rarity_percent(Some(&serde_json::json!({"percent": 42}))),
            Some(42.0)
        );
        assert_eq!(rarity_percent(Some(&serde_json::json!(120))), None);
        assert_eq!(rarity_percent(None), None);
    }

    #[test]
    fn les_succes_sont_tries_du_plus_rare_au_plus_courant() {
        let mut rare_locked = row("rare_locked", "Rare verrouillé", false);
        rare_locked.rarity = Some(2.5);
        let mut rare_unlocked = row("rare_unlocked", "Rare débloqué", true);
        rare_unlocked.rarity = Some(2.5);
        let mut common = row("common", "Courant", false);
        common.rarity = Some(70.0);
        let unknown = row("unknown", "Sans pourcentage", true);
        let rows = [unknown, common, rare_locked, rare_unlocked];
        let names: Vec<&str> = sorted_rows(&rows)
            .iter()
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "Rare débloqué",
                "Rare verrouillé",
                "Courant",
                "Sans pourcentage"
            ]
        );
    }
}
