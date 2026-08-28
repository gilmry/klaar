//! Bounded context Intervention : Mission et sa machine à états (FR-013,
//! FR-018 à FR-023).
//!
//! La Mission naît de l'acceptation d'une Demande (FR-013) et progresse par
//! transitions explicites (FR-018). La validation par le demandeur (FR-021) et
//! l'annulation avec ses conséquences (FR-022) y sont. Ce qui n'est pas encore
//! ici : les preuves photo (FR-020) et la reprogrammation (FR-023).

mod annulation;
mod mission;
mod preuve;
mod reprogrammation;
mod suivi;

pub use annulation::{
    AnnulationError, AnnulationMission, AuteurAnnulation, ConsequenceAnnulation,
    MotifAnnulationMission, ANNULATIONS_AVANT_SIGNALEMENT, DESISTEMENTS_AVANT_SUSPENSION,
    FENETRE_ANNULATIONS_JOURS, FENETRE_DESISTEMENTS_JOURS, FORFAIT_DEPLACEMENT_CENTS,
    SUSPENSION_JOURS,
};
pub use mission::{
    Mission, MissionError, StatutMission, TransitionMission, DERIVE_HORODATAGE_MAX_MINUTES,
};
pub use preuve::{
    paire_complete, valider as valider_preuve, FormatImage, MetadonneesExif, PhasePreuve, Preuve,
    PreuveError, PREUVES_MAX_PAR_PHASE, TAILLE_MAX_OCTETS,
};
pub use reprogrammation::{
    echeance_reprogrammation, Reprogrammation, ReprogrammationError, StatutReprogrammation,
    FENETRE_REPROGRAMMATION_JOURS,
};
pub use suivi::{
    degrader, echeance_purge, etat as etat_suivi, relever, EtatSuivi, PositionSuivie, SuiviError,
    PAS_LATITUDE, PAS_LONGITUDE, PERTE_POSITION_SECONDES, PRECISION_METRES, PURGE_HEURES,
};
