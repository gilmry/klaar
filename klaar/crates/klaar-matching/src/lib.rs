//! Bounded context Matching & Dispatch : Demande, Match (FR-011 à FR-015).
//!
//! Domaine pur, sans IO. Le périmètre géographique du service y vit aussi :
//! c'est une règle métier — qui est servi, et qui ne l'est pas — et non un
//! détail d'infrastructure.

mod demande;
mod perimetre;

pub use demande::{
    Demande, DemandeError, StatutDemande, Urgence, DESCRIPTION_MAX, FENETRE_DOUBLON_MINUTES,
};
pub use perimetre::{dans_le_perimetre, LAT_MAX, LAT_MIN, LON_MAX, LON_MIN};
