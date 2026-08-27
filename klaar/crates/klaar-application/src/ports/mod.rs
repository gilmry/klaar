//! Ports de la couche Application : les interfaces que l'Infrastructure
//! implémente et que les use cases consomment. Aucun détail de transport,
//! de protocole ni de fournisseur ne doit apparaître ici.

pub mod push;
pub mod push_repository;
