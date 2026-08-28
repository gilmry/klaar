//! Bounded context Trust & Moderation : notation, litiges, sanctions (FR-033 à FR-037).
//!
//! Livré à ce jour : la réputation en borne basse de Wilson (FR-037), la
//! notation double sens (FR-033) et l'ouverture de litige (FR-034) avec ses
//! seuils de sanction (FR-035). La médiation (FR-036) demande une console
//! d'exploitation et suivra avec l'Epic 8.

mod litige;
mod notation;
mod reputation;

pub use litige::{
    echeance_litige, examen_merite, suspension_meritee, Litige, LitigeError, MotifLitige,
    PartieLitige, StatutLitige, DESCRIPTION_MAX_CARACTERES, DESCRIPTION_MIN_CARACTERES,
    FENETRE_DEMANDEUR_JOURS, FENETRE_LITIGES_JOURS, FENETRE_LITIGE_JOURS, LITIGES_AVANT_EXAMEN,
    LITIGES_AVANT_SUSPENSION,
};
pub use notation::{
    echeance_notation, publiables, Cible, Notation, NotationError, COMMENTAIRE_MAX_CARACTERES,
    FENETRE_NOTATION_JOURS, NOTE_MAX, NOTE_MIN,
};
pub use reputation::{note_de_matching, wilson, PRIOR_SANS_NOTE, Z_95};
