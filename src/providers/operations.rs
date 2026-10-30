//! Provider title matching and installation lifecycle state.

use super::model::{InstallState, ManualGameGroup, ProviderGame, ProviderGameKey};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum OperationState {
    Queued,
    Running {
        progress: Option<f32>,
        message: String,
    },
    NeedsConfirmation(String),
    Succeeded {
        install_path: PathBuf,
    },
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstallOperation {
    pub key: ProviderGameKey,
    pub destination: PathBuf,
    pub state: OperationState,
}

impl InstallOperation {
    pub fn new(key: ProviderGameKey, destination: PathBuf) -> Self {
        Self {
            key,
            destination,
            state: OperationState::Queued,
        }
    }

    pub fn start(&mut self) {
        self.state = OperationState::Running {
            progress: None,
            message: "Installation en cours".into(),
        };
    }

    pub fn request_confirmation(&mut self, message: impl Into<String>) {
        self.state = OperationState::NeedsConfirmation(message.into());
    }

    pub fn complete_verified(&mut self, install_path: PathBuf) -> Result<(), String> {
        if !matches!(self.state, OperationState::Running { .. }) {
            return Err("installation non confirmée ou non démarrée".into());
        }
        if !install_path.is_absolute() || !install_path.is_dir() {
            return Err("dossier d'installation absent ou invalide".into());
        }
        let destination = self
            .destination
            .canonicalize()
            .map_err(|_| "destination d'installation absente ou invalide".to_string())?;
        let install_path = install_path
            .canonicalize()
            .map_err(|_| "dossier d'installation absent ou invalide".to_string())?;
        if !install_path.starts_with(&destination) {
            return Err("installation détectée hors du dossier de destination choisi".into());
        }
        self.state = OperationState::Succeeded { install_path };
        Ok(())
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = OperationState::Failed(message.into());
    }
    pub fn cancel(&mut self) {
        self.state = OperationState::Cancelled;
    }

    pub fn apply_to(&self, game: &mut ProviderGame) {
        if game.key != self.key {
            return;
        }
        match &self.state {
            OperationState::Queued => game.install_state = InstallState::Queued,
            OperationState::Running { .. } => game.install_state = InstallState::Installing,
            OperationState::NeedsConfirmation(_) => {
                // A URI handoff or provider prompt is not proof the game exists locally.
                game.install_state = InstallState::Unknown;
            }
            OperationState::Succeeded { install_path } => {
                game.install_state = InstallState::Installed;
                game.install_path = Some(install_path.clone());
            }
            OperationState::Failed(_) => game.install_state = InstallState::Failed,
            OperationState::Cancelled => game.install_state = InstallState::NotInstalled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchCandidate {
    pub normalized_title: String,
    pub members: Vec<ProviderGameKey>,
}

pub fn match_candidates(games: &[ProviderGame]) -> Vec<MatchCandidate> {
    let mut grouped: std::collections::BTreeMap<String, Vec<ProviderGameKey>> =
        std::collections::BTreeMap::new();
    for game in games {
        let title = normalize_title(&game.title);
        if !title.is_empty() {
            grouped.entry(title).or_default().push(game.key.clone());
        }
    }
    grouped
        .into_iter()
        .filter_map(|(normalized_title, members)| {
            (members.len() > 1).then_some(MatchCandidate {
                normalized_title,
                members,
            })
        })
        .collect()
}

pub fn normalize_title(title: &str) -> String {
    title
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

pub fn merge_group(members: &[ProviderGameKey]) -> Result<ManualGameGroup, String> {
    if members.len() < 2 {
        return Err("une fusion manuelle nécessite au moins deux jeux".into());
    }
    let unique: std::collections::HashSet<_> = members.iter().collect();
    if unique.len() != members.len() {
        return Err("un même jeu ne peut pas apparaître deux fois dans un groupe".into());
    }
    Ok(ManualGameGroup {
        members: members.to_vec(),
    })
}

pub fn split_member(group: &mut ManualGameGroup, member: &ProviderGameKey) {
    group.members.retain(|candidate| candidate != member);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::model::{Provider, ProviderGame};

    fn record(provider: Provider, id: &str, title: &str) -> ProviderGame {
        ProviderGame {
            key: ProviderGameKey {
                provider,
                product_id: id.into(),
            },
            title: title.into(),
            owned: true,
            install_path: None,
            custom_save_paths: Vec::new(),
            install_state: InstallState::NotInstalled,
            quark_game_id: None,
            capabilities: Default::default(),
            last_synced: None,
        }
    }

    #[test]
    fn exact_normalized_titles_match_but_near_matches_remain_separate() {
        let games = vec![
            record(Provider::Steam, "1", "The Witcher 3: Wild Hunt"),
            record(Provider::Epic, "witcher", "the witcher 3 wild hunt"),
            record(Provider::Gog, "witcher-gog", "The Witcher 3"),
            record(Provider::Epic, "portal", "Portal 2"),
        ];
        let groups = match_candidates(&games);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 2);
        assert_eq!(groups[0].members[0].product_id, "1");
        assert_eq!(groups[0].members[1].product_id, "witcher");
    }

    #[test]
    fn manual_merge_rejects_duplicate_keys_and_split_removes_only_selected_member() {
        let first = ProviderGameKey {
            provider: Provider::Steam,
            product_id: "1".into(),
        };
        let second = ProviderGameKey {
            provider: Provider::Epic,
            product_id: "2".into(),
        };
        assert!(merge_group(&[first.clone(), first.clone()]).is_err());
        let mut group = merge_group(&[first.clone(), second.clone()]).unwrap();
        split_member(&mut group, &first);
        assert_eq!(group.members, vec![second]);
        split_member(
            &mut group,
            &ProviderGameKey {
                provider: Provider::Gog,
                product_id: "missing".into(),
            },
        );
        assert_eq!(group.members.len(), 1);
    }

    #[test]
    fn success_requires_running_operation_and_an_existing_install_directory() {
        let key = ProviderGameKey {
            provider: Provider::Epic,
            product_id: "app".into(),
        };
        let mut operation = InstallOperation::new(key, std::env::temp_dir());
        let directory =
            std::env::temp_dir().join(format!("quark-install-ok-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        assert!(operation.complete_verified(directory.clone()).is_err());
        operation.start();
        operation.complete_verified(directory.clone()).unwrap();
        assert_eq!(
            operation.state,
            OperationState::Succeeded {
                install_path: directory.clone().canonicalize().unwrap()
            }
        );
        let missing = directory.join("not-installed");
        assert!(operation.complete_verified(missing).is_err());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn installation_is_rejected_when_verified_path_is_outside_selected_destination() {
        let root = std::env::temp_dir().join(format!("quark-install-root-{}", std::process::id()));
        let outside =
            std::env::temp_dir().join(format!("quark-install-outside-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let key = ProviderGameKey {
            provider: Provider::Epic,
            product_id: "app".into(),
        };
        let mut operation = InstallOperation::new(key, root.clone());
        operation.start();
        assert!(operation.complete_verified(outside.clone()).is_err());
        assert!(matches!(operation.state, OperationState::Running { .. }));
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn operation_changes_only_the_provider_record_with_the_same_key() {
        let key = ProviderGameKey {
            provider: Provider::Steam,
            product_id: "620".into(),
        };
        let mut operation = InstallOperation::new(key, PathBuf::from("/tmp"));
        operation.start();
        let mut other = record(Provider::Epic, "620", "Portal 2");
        operation.apply_to(&mut other);
        assert_eq!(other.install_state, InstallState::NotInstalled);
    }

    #[test]
    fn client_handoff_needs_confirmation_and_never_marks_install_complete() {
        let key = ProviderGameKey {
            provider: Provider::Steam,
            product_id: "620".into(),
        };
        let mut operation = InstallOperation::new(key, PathBuf::from("/games"));
        operation.start();
        operation.request_confirmation("Confirme la détection du manifeste Steam");
        assert!(matches!(
            operation.state,
            OperationState::NeedsConfirmation(_)
        ));
        let mut game = record(Provider::Steam, "620", "Portal 2");
        operation.apply_to(&mut game);
        assert_eq!(game.install_state, InstallState::Unknown);
        assert!(game.install_path.is_none());
    }
}
