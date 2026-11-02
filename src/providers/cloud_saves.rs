//! Synchronisation des sauvegardes cloud Epic et GOG.
//!
//! Epic passe par `legendary sync-saves`, qui compare les dates locales et
//! distantes : la sauvegarde la plus récente gagne. `-y` et `--accept-path`
//! rendent la commande non interactive ; le dossier calculé par Legendary est
//! accepté une seule fois puis mémorisé par l'outil.
//!
//! GOG passe par `gogdl save-sync`, qui exige un dossier et un horodatage. Le
//! dossier vient du réglage de sauvegarde de l'installation (ou de la fiche de
//! boutique) ; l'horodatage renvoyé par gogdl est conservé dans l'installation
//! pour que la comparaison suivante reste correcte.

use super::model::{Provider, ProviderGame};
use super::runner;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CLOUD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub message: String,
    pub timestamp: Option<i64>,
}

/// Synchronise une installation. `save_path` est obligatoire pour GOG ; Epic
/// peut laisser Legendary calculer son chemin.
pub fn sync(
    record: &ProviderGame,
    save_path: Option<&Path>,
    timestamp: Option<i64>,
) -> Result<SyncReport, String> {
    match record.key.provider {
        Provider::Epic => sync_epic(record, save_path),
        Provider::Gog => sync_gog(record, save_path, timestamp),
        Provider::Steam => {
            Err("Steam ne publie pas ses sauvegardes cloud sans le client Steam".to_string())
        }
    }
}

fn sync_epic(record: &ProviderGame, save_path: Option<&Path>) -> Result<SyncReport, String> {
    let adapter = super::legendary::LegendaryAdapter::new();
    if !adapter.executable.is_file() {
        return Err("Legendary n'est pas installé".to_string());
    }
    let mut args = vec![
        "-y".to_string(),
        "sync-saves".to_string(),
        record.key.product_id.clone(),
        "--accept-path".to_string(),
    ];
    if let Some(path) = save_path {
        args.push("--save-path".to_string());
        args.push(path.display().to_string());
    }
    let output = runner::run(adapter.command(args), CLOUD_TIMEOUT, None)?;
    if output.exit_code != Some(0) {
        return Err(format!(
            "Legendary n'a pas pu synchroniser les sauvegardes ({})",
            runner::failure_hint(&output)
        ));
    }
    Ok(SyncReport {
        message: summarize(&output.stdout, &output.stderr),
        timestamp: Some(crate::config::now_ts()),
    })
}

fn sync_gog(
    record: &ProviderGame,
    save_path: Option<&Path>,
    timestamp: Option<i64>,
) -> Result<SyncReport, String> {
    let path = save_path
        .ok_or_else(|| "Choisis d'abord un dossier de sauvegarde pour ce jeu GOG".to_string())?;
    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|_| "Le dossier de sauvegarde GOG ne peut pas être créé".to_string())?;
    }
    let adapter = super::gog::GogAdapter::new(super::gog::auth_config_path());
    if !adapter.executable.is_file() {
        return Err("gogdl n'est pas installé".to_string());
    }
    if !super::gog::GogAdapter::is_logged_in() {
        return Err("Session GOG absente : connecte la boutique dans Paramètres".to_string());
    }
    let args = super::gog::GogAdapter::save_sync_args(
        path,
        &record.key.product_id,
        timestamp.unwrap_or(0),
        "windows",
    );
    let output = runner::run(adapter.command(args), CLOUD_TIMEOUT, None)?;
    if output.exit_code != Some(0) {
        return Err(format!(
            "gogdl n'a pas pu synchroniser les sauvegardes ({})",
            runner::failure_hint(&output)
        ));
    }
    let new_timestamp = parse_gog_timestamp(&output.stdout).or_else(|| {
        // gogdl peut ne rien imprimer quand rien n'a changé : on garde alors
        // l'ancien horodatage plutôt que de le remettre à zéro.
        timestamp
    });
    Ok(SyncReport {
        message: summarize(&output.stdout, &output.stderr),
        timestamp: new_timestamp,
    })
}

/// gogdl termine par le nouvel horodatage (secondes décimales) sur stdout.
/// Les lignes de journal peuvent contenir d'autres nombres : on ne retient
/// qu'une ligne entièrement numérique.
pub fn parse_gog_timestamp(stdout: &str) -> Option<i64> {
    stdout.lines().rev().find_map(|line| {
        let line = line.trim();
        let seconds = line.parse::<f64>().ok()?;
        (seconds.is_finite() && seconds >= 0.0).then_some(seconds as i64)
    })
}

fn summarize(stdout: &str, stderr: &str) -> String {
    let text = format!("{stdout}\n{stderr}").to_lowercase();
    if text.contains("downloading remote save") || text.contains("downloading files") {
        "sauvegarde cloud téléchargée".to_string()
    } else if text.contains("uploading local save") || text.contains("uploading files") {
        "sauvegarde locale envoyée dans le cloud".to_string()
    } else if text.contains("up to date") || text.contains("same age") {
        "sauvegarde déjà à jour".to_string()
    } else if text.contains("no cloud or local") || text.contains("no files") {
        "aucune sauvegarde cloud ou locale".to_string()
    } else {
        "synchronisation terminée".to_string()
    }
}

/// Dossier de sauvegarde effectif d'une installation : le chemin explicite de
/// l'installation, sinon le premier dossier personnalisé de la fiche boutique.
pub fn save_path_for(game: Option<&crate::config::Game>, record: &ProviderGame) -> Option<PathBuf> {
    if let Some(game) = game
        && let Some(installation) = game.installations.iter().find(|installation| {
            installation.provider == Some(record.key.provider)
                && installation.product_id.as_deref() == Some(record.key.product_id.as_str())
        })
        && let Some(path) = installation
            .save_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
    {
        return Some(PathBuf::from(crate::config::expand_tilde(path)));
    }
    record.custom_save_paths.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_horodatage_gog_est_lu_sur_une_ligne_entiere() {
        let output = "[INFO] Uploading files\n1700000000.5\n";
        assert_eq!(parse_gog_timestamp(output), Some(1_700_000_000));
        assert_eq!(parse_gog_timestamp("aucun nombre"), None);
        // Une ligne de journal contenant un nombre ne doit pas être prise.
        assert_eq!(parse_gog_timestamp("[INFO] 12 files uploaded"), None);
    }

    #[test]
    fn les_messages_de_synchronisation_sont_lisibles() {
        assert_eq!(
            summarize("Downloading remote savegame...", ""),
            "sauvegarde cloud téléchargée"
        );
        assert_eq!(
            summarize("[INFO] No files in directory", ""),
            "aucune sauvegarde cloud ou locale"
        );
    }
}
