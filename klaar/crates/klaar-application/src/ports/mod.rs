//! Ports de la couche Application : les interfaces que l'Infrastructure
//! implémente et que les use cases consomment. Aucun détail de transport,
//! de protocole ni de fournisseur ne doit apparaître ici.

pub mod audit;
pub mod catalogue_repository;
pub mod courriel;
pub mod demande_repository;
pub mod devis_repository;
pub mod erreurs;
pub mod horloge;
pub mod jeton_acces;
pub mod mission_repository;
pub mod provider_repository;
pub mod push;
pub mod push_repository;
pub mod session_repository;
pub mod trace_repository;
pub mod utilisateur_repository;
