//! Bounded context Matching & Dispatch : Demande, Match (FR-011 à FR-015).
//!
//! Domaine pur, sans IO. Le périmètre géographique du service y vit aussi :
//! c'est une règle métier — qui est servi, et qui ne l'est pas — et non un
//! détail d'infrastructure.

mod demande;
mod score;

pub use demande::{
    Demande, DemandeError, MotifAnnulation, StatutDemande, Urgence, DESCRIPTION_MAX,
    DUREE_DIFFUSION_SECONDES, ELARGISSEMENTS_MAX, FENETRE_DOUBLON_MINUTES, RAYONS_METRES,
};
// Le périmètre a déménagé dans le shared kernel : la Story 4.3 en a eu besoin
// depuis le bounded context Intervention, et le dupliquer aurait fait diverger
// deux définitions de la même frontière. Ré-exporté ici pour ne pas casser les
// chemins d'import existants.
pub use klaar_shared_kernel::{dans_le_perimetre, LAT_MAX, LAT_MIN, LON_MAX, LON_MIN};
pub use score::{calculer as calculer_score, Contribution, Score, CANDIDATS_MAX, RAYON_METRES};
