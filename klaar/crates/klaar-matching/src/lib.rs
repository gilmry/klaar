//! Bounded context Matching & Dispatch : Demande, Match (FR-011 à FR-015).
//!
//! Domaine pur, sans IO. Le périmètre géographique du service y vit aussi :
//! c'est une règle métier — qui est servi, et qui ne l'est pas — et non un
//! détail d'infrastructure.

mod demande;
mod perimetre;
mod score;

pub use demande::{
    Demande, DemandeError, StatutDemande, Urgence, DESCRIPTION_MAX, DUREE_DIFFUSION_SECONDES,
    ELARGISSEMENTS_MAX, FENETRE_DOUBLON_MINUTES, RAYONS_METRES,
};
pub use perimetre::{dans_le_perimetre, LAT_MAX, LAT_MIN, LON_MAX, LON_MIN};
pub use score::{calculer as calculer_score, Contribution, Score, CANDIDATS_MAX, RAYON_METRES};
