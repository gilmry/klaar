//! Bounded context Trust & Moderation : notation, litiges, sanctions (FR-033 à FR-037).
//!
//! Livré à ce jour : la réputation en borne basse de Wilson (FR-037) et la
//! notation double sens (FR-033). Les litiges (FR-034), les sanctions (FR-035)
//! et la médiation (FR-036) suivront epic par epic.

mod notation;
mod reputation;

pub use notation::{
    echeance_notation, publiables, Cible, Notation, NotationError, COMMENTAIRE_MAX_CARACTERES,
    FENETRE_NOTATION_JOURS, NOTE_MAX, NOTE_MIN,
};
pub use reputation::{note_de_matching, wilson, PRIOR_SANS_NOTE, Z_95};
