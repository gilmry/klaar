//! Ports de la couche Application : les interfaces que l'Infrastructure
//! implémente et que les use cases consomment. Aucun détail de transport,
//! de protocole ni de fournisseur ne doit apparaître ici.

pub mod annulation_repository;
pub mod audit;
pub mod catalogue_admin_repository;
pub mod catalogue_repository;
pub mod courriel;
pub mod demande_repository;
pub mod devis_repository;
pub mod erreurs;
pub mod evenement_stripe_repository;
pub mod evenements;
pub mod export_repository;
pub mod horloge;
pub mod jeton_acces;
pub mod langue;
pub mod liberation_repository;
pub mod litige_repository;
pub mod message_repository;
pub mod mission_repository;
pub mod notation_repository;
pub mod ops_repository;
pub mod provider_repository;
pub mod push;
pub mod push_repository;
pub mod reprogrammation_repository;
pub mod revue_kyc_repository;
pub mod session_repository;
pub mod suivi_repository;
pub mod tableau_bord_repository;
pub mod trace_repository;
pub mod utilisateur_repository;
