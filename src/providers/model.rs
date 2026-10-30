//! Modèle sérialisable commun aux bibliothèques des boutiques.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Steam,
    Epic,
    Gog,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderGameKey {
    pub provider: Provider,
    pub product_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "reason")]
pub enum CapabilityState {
    Supported,
    Unsupported(String),
    Unknown,
    RequiresSetup(String),
}

impl Default for CapabilityState {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GameCapabilities {
    pub achievements: CapabilityState,
    pub cloud_saves: CapabilityState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    #[default]
    Unknown,
    NotInstalled,
    Queued,
    Installing,
    Installed,
    Updating,
    Repairing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderGame {
    pub key: ProviderGameKey,
    pub title: String,
    pub owned: bool,
    pub install_path: Option<PathBuf>,
    #[serde(default)]
    pub custom_save_paths: Vec<PathBuf>,
    pub install_state: InstallState,
    /// Identifiant de la fiche Quark associée, si elle existe.
    pub quark_game_id: Option<String>,
    pub capabilities: GameCapabilities,
    pub last_synced: Option<i64>,
}

impl Default for ProviderGame {
    fn default() -> Self {
        Self {
            key: ProviderGameKey {
                provider: Provider::Steam,
                product_id: String::new(),
            },
            title: String::new(),
            owned: false,
            install_path: None,
            custom_save_paths: Vec::new(),
            install_state: InstallState::Unknown,
            quark_game_id: None,
            capabilities: GameCapabilities::default(),
            last_synced: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ManualGameGroup {
    /// Les identifiants de fournisseurs réunis volontairement par l'utilisateur.
    pub members: Vec<ProviderGameKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderLibrary {
    pub games: Vec<ProviderGame>,
    pub manual_groups: Vec<ManualGameGroup>,
}
