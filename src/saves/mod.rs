//! Safe local save snapshots through the verified Ludusavi CLI.

use crate::providers::model::ProviderGameKey;
use crate::providers::runner::{CommandOutput, ProviderCommand};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

const MAX_TOOL_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_FULL_BACKUPS: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupPhase {
    BeforeLaunch,
    AfterExit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveIdentity {
    pub provider_key: ProviderGameKey,
    pub install_path: Option<PathBuf>,
    pub ludusavi_title: String,
    pub custom_paths: Vec<PathBuf>,
}

const CONFIG_UNRECOGNIZED: &str = "configuration Ludusavi isolée non reconnue; fichier conservé";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewFile {
    /// Display-only data from Ludusavi. Quark never writes to this path.
    pub path: String,
    pub change: Option<String>,
    pub bytes: Option<u64>,
    pub failed: bool,
    pub ignored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPreview {
    pub title: String,
    pub files: Vec<PreviewFile>,
    pub errors: Vec<String>,
    pub changed_games: Option<usize>,
    pub processed_games: Option<usize>,
    pub processed_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    /// Opaque ID returned by `backups --api` and consumed by `restore --backup`.
    pub id: String,
    pub when: String,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupRecord {
    pub title: String,
    pub phase: BackupPhase,
    pub changed_games: usize,
    pub processed_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct SaveManager {
    pub executable: PathBuf,
    pub root: PathBuf,
    pub config: PathBuf,
    pub timeout: Duration,
}

impl SaveManager {
    pub fn new() -> Self {
        Self {
            executable: crate::providers::tools::ManagedToolId::Ludusavi.installed_path(),
            root: crate::config::data_dir().join("save-backups"),
            config: crate::config::data_dir().join("ludusavi"),
            timeout: MAX_TOOL_TIMEOUT,
        }
    }

    pub fn bucket_for(&self, identity: &SaveIdentity) -> PathBuf {
        let hash = ring::digest::digest(
            &ring::digest::SHA256,
            format!(
                "{:?}:{}:{}",
                identity.provider_key.provider,
                identity.provider_key.product_id,
                identity
                    .install_path
                    .as_deref()
                    .map(Path::display)
                    .map(|path| path.to_string())
                    .unwrap_or_default()
            )
            .as_bytes(),
        );
        let suffix: String = hash
            .as_ref()
            .iter()
            .take(16)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.root.join(suffix)
    }

    pub fn prepare_bucket(&self, identity: &SaveIdentity) -> Result<PathBuf, String> {
        if let Some(parent) = self.root.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "coffre local des sauvegardes inaccessible".to_string())?;
        }
        match std::fs::symlink_metadata(&self.root) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("racine des sauvegardes non sûre".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&self.root)
                    .map_err(|_| "impossible de créer le coffre des sauvegardes".to_string())?;
            }
            Err(_) => return Err("racine des sauvegardes inaccessible".into()),
        }
        let root_metadata = std::fs::symlink_metadata(&self.root)
            .map_err(|_| "racine des sauvegardes inaccessible".to_string())?;
        if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
            return Err("racine des sauvegardes non sûre".into());
        }
        if let Some(parent) = self.config.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "configuration Ludusavi isolée inaccessible".to_string())?;
        }
        match std::fs::symlink_metadata(&self.config) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("configuration Ludusavi isolée non sûre".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&self.config).map_err(|_| {
                    "impossible de créer la configuration Ludusavi isolée".to_string()
                })?;
            }
            Err(_) => return Err("configuration Ludusavi isolée inaccessible".into()),
        }
        self.update_custom_game_config(identity)?;
        let bucket = self.bucket_for(identity);
        match std::fs::symlink_metadata(&bucket) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("dossier de sauvegarde non sûr".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&bucket)
                    .map_err(|_| "impossible de créer le dossier de sauvegarde".to_string())?;
            }
            Err(_) => return Err("dossier de sauvegarde inaccessible".into()),
        }
        let canonical_root = self
            .root
            .canonicalize()
            .map_err(|_| "racine des sauvegardes inaccessible".to_string())?;
        let canonical_bucket = bucket
            .canonicalize()
            .map_err(|_| "dossier de sauvegarde inaccessible".to_string())?;
        if !canonical_bucket.starts_with(canonical_root) {
            return Err("dossier de sauvegarde hors du coffre Quark".into());
        }
        Ok(canonical_bucket)
    }

    fn update_custom_game_config(&self, identity: &SaveIdentity) -> Result<(), String> {
        if identity.custom_paths.is_empty() {
            return Ok(());
        }
        let files: Vec<String> = identity
            .custom_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        if files.iter().any(|path| !Path::new(path).is_absolute()) {
            return Err("dossier de sauvegarde personnalisé non absolu".into());
        }
        let config_path = self.config.join("config.yaml");
        // Un `config.yaml` existant appartient à l'utilisateur (Ludusavi y écrit
        // ses réglages, souvent en YAML commenté) : on ne le remplace jamais par
        // une structure qui ne garderait que nos propres champs.
        let mut config = match std::fs::symlink_metadata(&config_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("configuration Ludusavi isolée non sûre".into());
            }
            Ok(_) => read_ludusavi_config(&config_path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                serde_yaml_ng::Value::Mapping(serde_yaml_ng::Mapping::new())
            }
            Err(_) => return Err("configuration Ludusavi isolée inaccessible".into()),
        };
        upsert_custom_game(&mut config, &identity.ludusavi_title, &files)?;
        let yaml = serde_yaml_ng::to_string(&config)
            .map_err(|_| "configuration Ludusavi impossible à sérialiser".to_string())?;
        write_config_atomic(&config_path, yaml.as_bytes())
    }

    pub fn preview_command(&self, identity: &SaveIdentity) -> Result<ProviderCommand, String> {
        let bucket = self.prepare_bucket(identity)?;
        Ok(ludusavi_command(
            &self.executable,
            [
                "--config".to_string(),
                self.config.display().to_string(),
                "backup".into(),
                "--preview".into(),
                "--api".into(),
                "--no-cloud-sync".into(),
                "--path".into(),
                bucket.display().to_string(),
                "--full-limit".into(),
                MAX_FULL_BACKUPS.to_string(),
                "--differential-limit".into(),
                "0".into(),
                identity.ludusavi_title.clone(),
            ],
        ))
    }

    pub fn restore_preview_command(
        &self,
        identity: &SaveIdentity,
        backup_id: &str,
    ) -> Result<ProviderCommand, String> {
        validate_backup_id(backup_id)?;
        let bucket = self.prepare_bucket(identity)?;
        Ok(ludusavi_command(
            &self.executable,
            [
                "--config".to_string(),
                self.config.display().to_string(),
                "restore".into(),
                "--preview".into(),
                "--api".into(),
                "--no-cloud-sync".into(),
                "--path".into(),
                bucket.display().to_string(),
                "--backup".into(),
                backup_id.to_string(),
                identity.ludusavi_title.clone(),
            ],
        ))
    }

    pub fn restore_command(
        &self,
        _identity: &SaveIdentity,
        _backup_id: &str,
        confirmed: bool,
    ) -> Result<ProviderCommand, String> {
        if !confirmed {
            return Err("restauration non exécutée sans confirmation explicite".into());
        }
        Err("une restauration doit passer par l'inventaire et l'aperçu validé".into())
    }

    fn confirmed_restore_command(
        &self,
        identity: &SaveIdentity,
        backup_id: &str,
    ) -> Result<ProviderCommand, String> {
        validate_backup_id(backup_id)?;
        let bucket = self.prepare_bucket(identity)?;
        Ok(ludusavi_command(
            &self.executable,
            [
                "--config".to_string(),
                self.config.display().to_string(),
                "restore".into(),
                "--api".into(),
                "--no-cloud-sync".into(),
                "--force".into(),
                "--path".into(),
                bucket.display().to_string(),
                "--backup".into(),
                backup_id.to_string(),
                identity.ludusavi_title.clone(),
            ],
        ))
    }

    pub fn backup_command(
        &self,
        identity: &SaveIdentity,
        confirmed: bool,
    ) -> Result<ProviderCommand, String> {
        if !confirmed {
            return Err("création de sauvegarde non exécutée sans confirmation explicite".into());
        }
        let bucket = self.prepare_bucket(identity)?;
        Ok(ludusavi_command(
            &self.executable,
            [
                "--config".to_string(),
                self.config.display().to_string(),
                "backup".into(),
                "--api".into(),
                "--no-cloud-sync".into(),
                "--force".into(),
                "--path".into(),
                bucket.display().to_string(),
                "--full-limit".into(),
                MAX_FULL_BACKUPS.to_string(),
                "--differential-limit".into(),
                "0".into(),
                identity.ludusavi_title.clone(),
            ],
        ))
    }

    pub fn run(
        &self,
        command: ProviderCommand,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> Result<CommandOutput, String> {
        crate::providers::runner::run(command, self.timeout, cancellation)
    }

    pub fn preview(&self, identity: &SaveIdentity) -> Result<BackupPreview, String> {
        let output = self.run(self.preview_command(identity)?, None)?;
        ensure_success(&output)?;
        parse_preview(&output.stdout, &identity.ludusavi_title)
    }

    pub fn backup(
        &self,
        identity: &SaveIdentity,
        phase: BackupPhase,
        confirmed: bool,
    ) -> Result<BackupRecord, String> {
        let output = self.run(self.backup_command(identity, confirmed)?, None)?;
        ensure_success(&output)?;
        let preview = parse_preview(&output.stdout, &identity.ludusavi_title)?;
        if !preview.errors.is_empty() || preview.files.iter().any(|file| file.failed) {
            return Err(
                "Ludusavi signale un échec; la sauvegarde n'est pas déclarée réussie".into(),
            );
        }
        let changed_games = preview
            .changed_games
            .ok_or_else(|| "statistiques globales Ludusavi manquantes".to_string())?;
        let processed_bytes = preview
            .processed_bytes
            .ok_or_else(|| "compteur d'octets Ludusavi manquant".to_string())?;
        if preview.processed_games.is_none() {
            return Err("compteur de jeux traités Ludusavi manquant".into());
        }
        Ok(BackupRecord {
            title: identity.ludusavi_title.clone(),
            phase,
            changed_games,
            processed_bytes,
        })
    }

    pub fn backups_command(&self, identity: &SaveIdentity) -> Result<ProviderCommand, String> {
        let bucket = self.prepare_bucket(identity)?;
        Ok(ludusavi_command(
            &self.executable,
            [
                "--config".to_string(),
                self.config.display().to_string(),
                "backups".into(),
                "--api".into(),
                "--path".into(),
                bucket.display().to_string(),
                identity.ludusavi_title.clone(),
            ],
        ))
    }

    pub fn backups(&self, identity: &SaveIdentity) -> Result<Vec<BackupInfo>, String> {
        let output = self.run(self.backups_command(identity)?, None)?;
        ensure_success(&output)?;
        parse_backups(&output.stdout, &identity.ludusavi_title)
    }

    pub fn restore_preview(
        &self,
        identity: &SaveIdentity,
        backup_id: &str,
    ) -> Result<BackupPreview, String> {
        let backups = self.backups(identity)?;
        validate_backup_id_in_inventory(&backups, backup_id)?;
        let command = self.restore_preview_command(identity, backup_id)?;
        let output = self.run(command, None)?;
        ensure_success(&output)?;
        let preview = parse_preview(&output.stdout, &identity.ludusavi_title)?;
        validate_restore_preview_paths(identity, &preview, &crate::config::home_dir())?;
        Ok(preview)
    }

    pub fn restore(
        &self,
        identity: &SaveIdentity,
        backup_id: &str,
        confirmed: bool,
        expected_paths: &[String],
    ) -> Result<(), String> {
        if !confirmed {
            return Err("restauration non exécutée sans confirmation explicite".into());
        }
        let backups = self.backups(identity)?;
        validate_backup_id_in_inventory(&backups, backup_id)?;
        let preview = self.restore_preview(identity, backup_id)?;
        validate_restore_preview_paths(identity, &preview, &crate::config::home_dir())?;
        let mut current_paths: Vec<String> =
            preview.files.iter().map(|file| file.path.clone()).collect();
        let mut approved_paths = expected_paths.to_vec();
        current_paths.sort();
        approved_paths.sort();
        if current_paths != approved_paths {
            return Err(
                "les destinations ont changé depuis l'aperçu; relancez l'aperçu avant de restaurer"
                    .into(),
            );
        }
        let command = self.confirmed_restore_command(identity, backup_id)?;
        let output = self.run(command, None)?;
        ensure_success(&output)?;
        let result = parse_preview(&output.stdout, &identity.ludusavi_title)?;
        if !result.errors.is_empty() || result.files.iter().any(|file| file.failed) {
            return Err("Ludusavi signale un échec pendant la restauration".into());
        }
        Ok(())
    }
}

impl Default for SaveManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lit un `config.yaml` Ludusavi tel quel : YAML comme JSON, puisque le second
/// est un sous-ensemble du premier. Un fichier vide part d'une table vide ;
/// tout le reste doit être une table, sinon on refuse sans rien écrire.
fn read_ludusavi_config(path: &Path) -> Result<serde_yaml_ng::Value, String> {
    let contents =
        std::fs::read(path).map_err(|_| "configuration Ludusavi isolée illisible".to_string())?;
    if contents.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Ok(serde_yaml_ng::Value::Mapping(serde_yaml_ng::Mapping::new()));
    }
    let config: serde_yaml_ng::Value =
        serde_yaml_ng::from_slice(&contents).map_err(|_| CONFIG_UNRECOGNIZED.to_string())?;
    if !config.is_mapping() {
        return Err(CONFIG_UNRECOGNIZED.to_string());
    }
    Ok(config)
}

/// Ajoute les dossiers personnalisés Quark à l'entrée `customGames` du titre,
/// sans toucher au reste du fichier ni aux autres clés de cette entrée.
fn upsert_custom_game(
    config: &mut serde_yaml_ng::Value,
    title: &str,
    files: &[String],
) -> Result<(), String> {
    let mapping = config
        .as_mapping_mut()
        .ok_or_else(|| CONFIG_UNRECOGNIZED.to_string())?;
    let games_key = serde_yaml_ng::Value::from("customGames");
    if !mapping.contains_key(&games_key) {
        mapping.insert(
            games_key.clone(),
            serde_yaml_ng::Value::Sequence(Vec::new()),
        );
    }
    let games = mapping
        .get_mut(&games_key)
        .and_then(serde_yaml_ng::Value::as_sequence_mut)
        .ok_or_else(|| {
            "configuration Ludusavi : « customGames » doit être une liste; fichier conservé"
                .to_string()
        })?;
    let name_key = serde_yaml_ng::Value::from("name");
    let index = match games
        .iter()
        .position(|entry| entry.get("name").and_then(serde_yaml_ng::Value::as_str) == Some(title))
    {
        Some(index) => index,
        None => {
            let mut entry = serde_yaml_ng::Mapping::new();
            entry.insert(name_key, serde_yaml_ng::Value::from(title));
            games.push(serde_yaml_ng::Value::Mapping(entry));
            games.len() - 1
        }
    };
    let entry = games[index].as_mapping_mut().ok_or_else(|| {
        "configuration Ludusavi : entrée « customGames » invalide; fichier conservé".to_string()
    })?;
    let files_key = serde_yaml_ng::Value::from("files");
    // Fusion sans doublon : un chemin déjà présent dans le fichier (ajouté à la
    // main ou lors d'une exécution précédente) n'est jamais effacé, car rien ne
    // permet de distinguer un chemin Quark d'un chemin écrit par l'utilisateur.
    let mut merged: Vec<serde_yaml_ng::Value> = match entry.get(&files_key) {
        Some(existing) => existing
            .as_sequence()
            .ok_or_else(|| {
                "configuration Ludusavi : « files » doit être une liste; fichier conservé"
                    .to_string()
            })?
            .clone(),
        None => Vec::new(),
    };
    for path in files {
        let value = serde_yaml_ng::Value::from(path.as_str());
        if !merged.contains(&value) {
            merged.push(value);
        }
    }
    entry.insert(files_key, serde_yaml_ng::Value::Sequence(merged));
    entry.insert(
        serde_yaml_ng::Value::from("integration"),
        serde_yaml_ng::Value::from("extend"),
    );
    Ok(())
}

/// Écrit à côté puis renomme : une coupure en cours d'écriture ne laisse pas un
/// `config.yaml` tronqué à la place des réglages de l'utilisateur.
fn write_config_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let temp = path.with_file_name(format!(".config.yaml.{}-{unique}.tmp", std::process::id()));
    std::fs::write(&temp, contents)
        .map_err(|_| "configuration Ludusavi impossible à écrire".to_string())?;
    if std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err("configuration Ludusavi impossible à remplacer".into());
    }
    Ok(())
}

fn ludusavi_command(executable: &Path, args: impl IntoIterator<Item = String>) -> ProviderCommand {
    ProviderCommand {
        program: executable.to_path_buf(),
        args: args.into_iter().map(Into::into).collect(),
        env: vec![],
        cwd: None,
        secrets: vec![],
        discard_output: false,
    }
}

fn validate_backup_id(backup_id: &str) -> Result<(), String> {
    if backup_id.is_empty()
        || backup_id.len() > 128
        || matches!(backup_id, "." | "..")
        || backup_id
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err("identifiant de sauvegarde invalide".into());
    }
    Ok(())
}

fn validate_backup_id_in_inventory(backups: &[BackupInfo], backup_id: &str) -> Result<(), String> {
    if backups.iter().any(|backup| backup.id == backup_id) {
        Ok(())
    } else {
        Err("snapshot absent de l'inventaire de cette installation".into())
    }
}

/// Validate les destinations calculées par Ludusavi avant une restauration.
/// Les chemins sont des données non fiables, même lorsqu'ils proviennent d'un aperçu.
pub fn validate_restore_preview_paths(
    identity: &SaveIdentity,
    preview: &BackupPreview,
    home: &Path,
) -> Result<(), String> {
    let mut allowed_roots = vec![home.to_path_buf()];
    if let Some(install_path) = &identity.install_path {
        allowed_roots.push(install_path.clone());
    }
    allowed_roots.extend(identity.custom_paths.iter().cloned());
    let canonical_roots: Vec<PathBuf> = allowed_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .filter(|root| root.is_dir())
        .collect();
    if canonical_roots.is_empty() {
        return Err("aucune racine sûre disponible pour la restauration".into());
    }
    for file in &preview.files {
        let path = Path::new(&file.path);
        if !path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err("destination de restauration relative ou avec traversal refusée".into());
        }
        let mut ancestor = path;
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| "destination de restauration invalide".to_string())?;
        }
        let canonical_ancestor = ancestor
            .canonicalize()
            .map_err(|_| "destination de restauration inaccessible".to_string())?;
        let lexical_allowed = allowed_roots.iter().any(|root| path.starts_with(root));
        let resolved_allowed = canonical_roots
            .iter()
            .any(|root| canonical_ancestor.starts_with(root));
        if !lexical_allowed || !resolved_allowed {
            return Err("destination de restauration hors des dossiers autorisés (ou lien symbolique sortant)".into());
        }
    }
    Ok(())
}

fn ensure_success(output: &CommandOutput) -> Result<(), String> {
    if output.exit_code == Some(0) {
        Ok(())
    } else {
        Err("commande Ludusavi échouée".into())
    }
}

pub fn parse_backups(json: &str, title: &str) -> Result<Vec<BackupInfo>, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "sortie JSON Ludusavi invalide".to_string())?;
    let games = root
        .get("games")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "sortie Ludusavi sans objet games".to_string())?;
    let Some(game) = games.get(title) else {
        return Ok(Vec::new());
    };
    let records = game
        .get("backups")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "liste de backups Ludusavi invalide".to_string())?;
    let mut backups = Vec::with_capacity(records.len());
    for record in records {
        let id = record
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "identifiant de snapshot manquant".to_string())?;
        validate_backup_id(id)?;
        backups.push(BackupInfo {
            id: id.to_string(),
            when: record
                .get("when")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "date de snapshot manquante".to_string())?
                .to_string(),
            locked: record
                .get("locked")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(backups)
}

pub fn parse_preview(json: &str, title: &str) -> Result<BackupPreview, String> {
    let root: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "sortie JSON Ludusavi invalide".to_string())?;
    let games = root
        .get("games")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "sortie Ludusavi sans objet games".to_string())?;
    let game = games
        .get(title)
        .ok_or_else(|| format!("jeu non reconnu par Ludusavi : {title}"))?;
    let mut files = Vec::new();
    {
        if let Some(by_path) = game.get("files").and_then(serde_json::Value::as_object) {
            for (path, details) in by_path {
                files.push(PreviewFile {
                    path: path.clone(),
                    change: details
                        .get("change")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    bytes: details.get("bytes").and_then(serde_json::Value::as_u64),
                    failed: details
                        .get("failed")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    ignored: details
                        .get("ignored")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                });
            }
        }
    }
    let error_object = root.get("errors").and_then(serde_json::Value::as_object);
    let mut errors = Vec::new();
    if let Some(value) = error_object.and_then(|entries| entries.get("someGamesFailed")) {
        if value.as_bool() == Some(true) {
            errors.push("someGamesFailed".to_string());
        }
    }
    if let Some(unknown) = error_object
        .and_then(|entries| entries.get("unknownGames"))
        .and_then(serde_json::Value::as_array)
    {
        errors.extend(
            unknown
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(|title| format!("jeu inconnu : {title}")),
        );
    }
    if let Some(overall) = root.get("overall").filter(|value| !value.is_null()) {
        if let Some(failed) = overall
            .get("gamesFailed")
            .and_then(serde_json::Value::as_array)
        {
            errors.extend(
                failed
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(|title| format!("échec : {title}")),
            );
        }
    }
    let changed_games = root
        .get("overall")
        .and_then(|overall| overall.get("changedGames"))
        .and_then(serde_json::Value::as_object)
        .map(|counts| {
            ["new", "different"]
                .into_iter()
                .filter_map(|key| counts.get(key).and_then(serde_json::Value::as_u64))
                .sum::<u64>() as usize
        });
    let processed_games = root
        .get("overall")
        .and_then(|overall| overall.get("processedGames"))
        .and_then(serde_json::Value::as_u64)
        .map(|value| value as usize);
    let processed_bytes = root
        .get("overall")
        .and_then(|overall| overall.get("processedBytes"))
        .and_then(serde_json::Value::as_u64);
    Ok(BackupPreview {
        title: title.to_owned(),
        files,
        errors,
        changed_games,
        processed_games,
        processed_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::model::Provider;

    fn identity(provider: Provider, id: &str, title: &str) -> SaveIdentity {
        SaveIdentity {
            provider_key: ProviderGameKey {
                provider,
                product_id: id.into(),
            },
            install_path: None,
            ludusavi_title: title.into(),
            custom_paths: vec![],
        }
    }
    struct TestManager {
        manager: SaveManager,
        root: PathBuf,
    }

    impl TestManager {
        fn new() -> Self {
            static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("quark-save-test-{}-{unique}", std::process::id()));
            std::fs::create_dir_all(&root).expect("répertoire de test temporaire");
            let manager = SaveManager {
                executable: PathBuf::from("/bin/false"),
                root: root.join("backups"),
                config: root.join("ludusavi"),
                timeout: Duration::from_secs(1),
            };
            Self { manager, root }
        }
    }

    impl Drop for TestManager {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn ludusavi_preview_parses_changes_but_keeps_reported_paths_as_display_data() {
        let json = r#"{"games":{"Example Game":{"change":"New","decision":"Processed","dump":{},"files":{"../../outside/save.dat":{"bytes":42,"change":"New","duplicatedBy":[],"failed":false,"ignored":false}},"registry":{}}},"overall":{"changedGames":{"new":1,"same":0,"different":0},"processedBytes":42,"processedGames":1,"totalBytes":42,"totalGames":1},"errors":null}"#;
        let preview = parse_preview(json, "Example Game").unwrap();
        assert_eq!(preview.files.len(), 1);
        assert_eq!(preview.files[0].path, "../../outside/save.dat");
        assert_eq!(preview.files[0].bytes, Some(42));
        assert!(!preview.files[0].failed);
        assert_eq!(preview.changed_games, Some(1));
        assert_eq!(preview.processed_games, Some(1));
        assert_eq!(preview.processed_bytes, Some(42));
    }

    #[test]
    fn ludusavi_false_failure_flag_does_not_become_an_error_but_true_does() {
        let clear = r#"{"games":{"Title":{"files":{}}},"errors":{"someGamesFailed":false},"overall":{"changedGames":{"new":0,"same":0,"different":0},"processedBytes":0,"processedGames":1,"totalBytes":0,"totalGames":1}}"#;
        assert!(parse_preview(clear, "Title").unwrap().errors.is_empty());
        let failed = clear.replace("false", "true");
        assert!(!parse_preview(&failed, "Title").unwrap().errors.is_empty());
    }

    #[test]
    fn an_unknown_title_is_reported_instead_of_a_successful_empty_preview() {
        let preview = parse_preview(r#"{"games":{},"errors":null}"#, "Missing Game");
        assert!(preview.is_err());
    }

    #[test]
    fn backup_retention_limits_full_and_differential_snapshots() {
        let manager = TestManager::new();
        let game = identity(Provider::Steam, "620", "Portal 2");
        let args: Vec<String> = manager
            .manager
            .backup_command(&game, true)
            .unwrap()
            .args
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["--full-limit", "10"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--differential-limit", "0"])
        );
    }

    #[test]
    fn malformed_or_wrong_ludusavi_schema_is_rejected() {
        assert!(parse_preview("not json", "Title").is_err());
        assert!(parse_preview("{}", "Title").is_err());
        assert!(parse_preview(r#"{"games":[]}"#, "Title").is_err());
    }

    #[test]
    fn each_provider_installation_gets_an_isolated_backup_bucket() {
        let manager = TestManager::new();
        let steam_identity = identity(Provider::Steam, "620", "Portal 2");
        let steam = manager.manager.bucket_for(&steam_identity);
        let epic = manager
            .manager
            .bucket_for(&identity(Provider::Epic, "620", "Portal 2"));
        let other = manager
            .manager
            .bucket_for(&identity(Provider::Steam, "400", "Portal"));
        let mut second_install = steam_identity;
        second_install.install_path = Some(PathBuf::from("/games/portal-2-copy"));
        let second_install_bucket = manager.manager.bucket_for(&second_install);
        assert_ne!(steam, epic);
        assert_ne!(steam, other);
        assert_ne!(steam, second_install_bucket);
        assert_eq!(
            steam.components().count(),
            manager.manager.root.components().count() + 1
        );
    }

    #[cfg(unix)]
    #[test]
    fn backup_bucket_refuses_a_symlink_that_escapes_the_quark_root() {
        use std::os::unix::fs::symlink;
        let temp = std::env::temp_dir().join(format!("quark-save-symlink-{}", std::process::id()));
        let manager = SaveManager {
            executable: PathBuf::from("/bin/false"),
            root: temp.join("root"),
            config: temp.join("config"),
            timeout: Duration::from_secs(1),
        };
        let outside = temp.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&manager.root).unwrap();
        std::fs::create_dir_all(&manager.config).unwrap();
        let game = identity(Provider::Steam, "620", "Portal 2");
        symlink(&outside, manager.bucket_for(&game)).unwrap();
        assert!(manager.prepare_bucket(&game).is_err());
        let _ = std::fs::remove_file(manager.bucket_for(&game));
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn ludusavi_commands_keep_title_as_a_single_argument_and_limit_full_backups() {
        let manager = TestManager::new();
        let game = identity(Provider::Gog, "id; $(touch x)", "Game; $(touch x)");
        let command = manager.manager.preview_command(&game).unwrap();
        let args: Vec<String> = command
            .args
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect();
        assert_eq!(args.last().unwrap(), "Game; $(touch x)");
        assert!(args.windows(2).any(|pair| pair == ["--full-limit", "10"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--differential-limit", "0"])
        );
        assert!(args.contains(&"--preview".to_string()));
        assert!(args.contains(&"--config".to_string()));
        assert!(args.contains(&manager.manager.config.display().to_string()));
        assert!(args.contains(&"--no-cloud-sync".to_string()));
        assert!(!args.contains(&"--force".to_string()));
    }

    #[test]
    fn creating_a_backup_requires_explicit_confirmation_before_force_is_passed() {
        let manager = TestManager::new();
        let game = identity(Provider::Steam, "620", "Portal 2");
        assert!(manager.manager.backup_command(&game, false).is_err());
        let command = manager.manager.backup_command(&game, true).unwrap();
        assert!(command.args.iter().any(|argument| argument == "--force"));
    }

    #[test]
    fn parses_backups_ids_names_times_and_locks_from_the_real_api_shape() {
        let json = r#"{"games":{"Title":{"backupPath":"/private/local/Title","backups":[{"locked":true,"name":"2026-10-30T12:00:00Z","os":"linux","when":"2026-10-30T12:00:00Z"},{"locked":false,"name":"manual snapshot","os":"linux","when":"2026-10-30T12:30:00Z"}]}}}"#;
        let backups = parse_backups(json, "Title").unwrap();
        assert_eq!(backups.len(), 2);
        assert_eq!(backups[0].id, "2026-10-30T12:00:00Z");
        assert!(backups[0].locked);
        assert_eq!(backups[1].id, "manual snapshot");
        assert_eq!(backups[1].when, "2026-10-30T12:30:00Z");
    }

    #[test]
    fn rejects_backup_ids_with_path_separators() {
        let json = r#"{"games":{"Title":{"backups":[{"locked":false,"name":"../escape","os":"linux","when":"2026-10-30T12:00:00Z"}]}}}"#;
        assert!(parse_backups(json, "Title").is_err());
    }

    #[test]
    fn restoring_requires_explicit_confirmation_and_a_safe_backup_id() {
        let manager = TestManager::new();
        let game = identity(Provider::Steam, "620", "Portal 2");
        assert!(
            manager
                .manager
                .restore_command(&game, "backup-1", false)
                .is_err()
        );
        assert!(
            manager
                .manager
                .restore_command(&game, "../../live", true)
                .is_err()
        );
        assert!(
            manager
                .manager
                .restore_command(&game, "backup 1", true)
                .is_err()
        );
    }

    #[test]
    fn restore_preview_paths_must_be_absolute_and_inside_an_allowed_root() {
        let manager = TestManager::new();
        let game_root = manager.root.join("game");
        std::fs::create_dir_all(game_root.join("saves")).unwrap();
        let game = SaveIdentity {
            install_path: Some(game_root.clone()),
            ..identity(Provider::Steam, "620", "Portal 2")
        };
        let preview = BackupPreview {
            title: "Portal 2".into(),
            files: vec![PreviewFile {
                path: game_root.join("saves/slot.dat").display().to_string(),
                change: Some("Different".into()),
                bytes: Some(42),
                failed: false,
                ignored: false,
            }],
            errors: vec![],
            changed_games: Some(1),
            processed_games: Some(1),
            processed_bytes: Some(42),
        };
        let home = manager.root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        assert!(validate_restore_preview_paths(&game, &preview, &home).is_ok());

        let mut outside = preview.clone();
        outside.files[0].path = manager.root.join("outside.dat").display().to_string();
        assert!(validate_restore_preview_paths(&game, &outside, &home).is_err());

        let mut relative = preview;
        relative.files[0].path = "../../outside.dat".into();
        assert!(validate_restore_preview_paths(&game, &relative, &home).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn restore_preview_rejects_a_symlink_ancestor_that_escapes_allowed_roots() {
        use std::os::unix::fs::symlink;
        let manager = TestManager::new();
        let game_root = manager.root.join("game");
        let outside_root = manager.root.join("outside");
        std::fs::create_dir_all(&game_root).unwrap();
        std::fs::create_dir_all(&outside_root).unwrap();
        symlink(&outside_root, game_root.join("saves")).unwrap();
        let game = SaveIdentity {
            install_path: Some(game_root.clone()),
            ..identity(Provider::Steam, "620", "Portal 2")
        };
        let preview = BackupPreview {
            title: "Portal 2".into(),
            files: vec![PreviewFile {
                path: game_root.join("saves/slot.dat").display().to_string(),
                change: None,
                bytes: Some(1),
                failed: false,
                ignored: false,
            }],
            errors: vec![],
            changed_games: Some(1),
            processed_games: Some(1),
            processed_bytes: Some(1),
        };
        let home = manager.root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        assert!(validate_restore_preview_paths(&game, &preview, &home).is_err());
    }

    #[test]
    fn backup_snapshot_id_must_belong_to_the_selected_installation_inventory() {
        let known = BackupInfo {
            id: "snapshot-1".into(),
            when: "2026-10-30T12:00:00Z".into(),
            locked: false,
        };
        assert!(
            validate_backup_id_in_inventory(std::slice::from_ref(&known), "snapshot-1").is_ok()
        );
        assert!(validate_backup_id_in_inventory(&[known], "other-install-snapshot").is_err());
    }

    #[test]
    fn custom_save_folders_are_added_to_isolated_config_without_dropping_other_entries() {
        let manager = TestManager::new();
        std::fs::create_dir_all(&manager.manager.config).unwrap();
        let config_path = manager.manager.config.join("config.yaml");
        std::fs::write(&config_path, r#"{"customGames":[{"name":"Earlier","files":["/home/demo/earlier"],"integration":"extend"}]}"#).unwrap();
        let custom = manager.root.join("custom-save");
        std::fs::create_dir_all(&custom).unwrap();
        let game = SaveIdentity {
            custom_paths: vec![custom.clone()],
            ..identity(Provider::Steam, "620", "Portal 2")
        };
        manager.manager.update_custom_game_config(&game).unwrap();
        let config: serde_json::Value =
            serde_yaml_ng::from_slice(&std::fs::read(config_path).unwrap()).unwrap();
        let games = config["customGames"].as_array().unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0]["name"], "Earlier");
        assert_eq!(games[1]["name"], "Portal 2");
        assert_eq!(games[1]["files"][0], custom.display().to_string());
    }

    #[test]
    fn a_yaml_config_keeps_its_settings_when_a_custom_game_is_added() {
        let manager = TestManager::new();
        std::fs::create_dir_all(&manager.manager.config).unwrap();
        let config_path = manager.manager.config.join("config.yaml");
        std::fs::write(
            &config_path,
            r#"# Réglages écrits par Ludusavi, jamais par Quark
manifest:
  enable: false
  url: "https://example.invalid/manifest.yaml"
restore:
  path: /home/demo/ludusavi-backup
  ignoredGames:
    - Deprecated Game
customGames:
  - name: Earlier
    files:
      - /home/demo/earlier
    registry:
      - HKEY_CURRENT_USER/Software/Earlier
    integration: extend
"#,
        )
        .unwrap();
        let custom = manager.root.join("custom-save");
        std::fs::create_dir_all(&custom).unwrap();
        let game = SaveIdentity {
            custom_paths: vec![custom.clone()],
            ..identity(Provider::Steam, "620", "Portal 2")
        };
        manager.manager.update_custom_game_config(&game).unwrap();
        let raw = std::fs::read(&config_path).unwrap();
        let config: serde_json::Value = serde_yaml_ng::from_slice(&raw).unwrap();
        assert_eq!(config["manifest"]["enable"], false);
        assert_eq!(
            config["manifest"]["url"],
            "https://example.invalid/manifest.yaml"
        );
        assert_eq!(config["restore"]["path"], "/home/demo/ludusavi-backup");
        assert_eq!(config["restore"]["ignoredGames"][0], "Deprecated Game");
        let games = config["customGames"].as_array().unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0]["name"], "Earlier");
        assert_eq!(
            games[0]["registry"][0],
            "HKEY_CURRENT_USER/Software/Earlier"
        );
        assert_eq!(games[1]["name"], "Portal 2");
        assert_eq!(games[1]["files"][0], custom.display().to_string());
        assert_eq!(games[1]["integration"], "extend");
    }

    #[test]
    fn an_existing_custom_game_keeps_its_other_keys_and_its_own_files() {
        let manager = TestManager::new();
        std::fs::create_dir_all(&manager.manager.config).unwrap();
        let config_path = manager.manager.config.join("config.yaml");
        std::fs::write(
            &config_path,
            r#"customGames:
  - name: Portal 2
    files:
      - /home/demo/hand-written
    alias: Portal
    ignore: true
    integration: override
"#,
        )
        .unwrap();
        let custom = manager.root.join("custom-save");
        std::fs::create_dir_all(&custom).unwrap();
        let game = SaveIdentity {
            custom_paths: vec![custom.clone()],
            ..identity(Provider::Steam, "620", "Portal 2")
        };
        manager.manager.update_custom_game_config(&game).unwrap();
        let config: serde_json::Value =
            serde_yaml_ng::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
        let games = config["customGames"].as_array().unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(
            games[0]["files"],
            serde_json::json!(["/home/demo/hand-written", custom.display().to_string()])
        );
        assert_eq!(games[0]["alias"], "Portal");
        assert_eq!(games[0]["ignore"], true);
        assert_eq!(games[0]["integration"], "extend");
    }

    #[test]
    fn an_unreadable_config_is_refused_and_left_byte_for_byte() {
        let manager = TestManager::new();
        std::fs::create_dir_all(&manager.manager.config).unwrap();
        let config_path = manager.manager.config.join("config.yaml");
        let game = SaveIdentity {
            custom_paths: vec![manager.root.join("custom-save")],
            ..identity(Provider::Steam, "620", "Portal 2")
        };
        for original in [
            "customGames: [\n  still open\n",
            "- not\n- a\n- mapping\n",
            "customGames:\n  name: Portal 2\n",
        ] {
            std::fs::write(&config_path, original).unwrap();
            assert!(manager.manager.update_custom_game_config(&game).is_err());
            assert_eq!(std::fs::read_to_string(&config_path).unwrap(), original);
        }
        std::fs::create_dir_all(manager.root.join("custom-save")).unwrap();
        std::fs::write(&config_path, "customGames: 3\n").unwrap();
        assert!(manager.manager.update_custom_game_config(&game).is_err());
        assert_eq!(
            std::fs::read_to_string(&config_path).unwrap(),
            "customGames: 3\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_config_is_refused_rather_than_followed() {
        use std::os::unix::fs::symlink;
        let manager = TestManager::new();
        std::fs::create_dir_all(&manager.manager.config).unwrap();
        let outside = manager.root.join("outside.yaml");
        std::fs::write(&outside, "manifest:\n  enable: false\n").unwrap();
        symlink(&outside, manager.manager.config.join("config.yaml")).unwrap();
        std::fs::create_dir_all(manager.root.join("custom-save")).unwrap();
        let game = SaveIdentity {
            custom_paths: vec![manager.root.join("custom-save")],
            ..identity(Provider::Steam, "620", "Portal 2")
        };
        assert!(manager.manager.update_custom_game_config(&game).is_err());
        assert_eq!(
            std::fs::read_to_string(&outside).unwrap(),
            "manifest:\n  enable: false\n"
        );
    }

    #[test]
    fn ludusavi_failures_never_report_a_successful_backup() {
        let output = CommandOutput {
            stdout: r#"{"games":{},"errors":{"someGamesFailed":true}}"#.into(),
            stderr: String::new(),
            exit_code: Some(1),
        };
        assert!(ensure_success(&output).is_err());
        let preview = parse_preview(r#"{"games":{"Title":{"files":{"/save":{"bytes":1,"failed":true,"ignored":false}}}},"errors":null}"#, "Title").unwrap();
        assert!(preview.files[0].failed);
    }
}
