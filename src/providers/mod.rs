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
