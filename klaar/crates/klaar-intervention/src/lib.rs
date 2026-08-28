//! Bounded context Intervention : Mission et sa machine à états (FR-013,
//! FR-018 à FR-023).
//!
//! La Mission naît de l'acceptation d'une Demande (FR-013) et progresse par
//! transitions explicites (FR-018). Ce qui n'est pas encore ici : le devis
//! (FR-016), les preuves photo (FR-020), la validation par le demandeur
//! (FR-021), les pénalités d'annulation (FR-022) et la reprogrammation
//! (FR-023).

mod mission;

pub use mission::{
    Mission, MissionError, StatutMission, TransitionMission, DERIVE_HORODATAGE_MAX_MINUTES,
};
