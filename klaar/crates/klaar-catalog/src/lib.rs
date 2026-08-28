//! Bounded context Catalogue : Secteurs et Skills (FR-008 à FR-010).
//!
//! Domaine pur, sans IO. Le catalogue est la seule liste que l'utilisateur voit
//! avant de décrire son problème : ses codes finissent dans des URL, des
//! statistiques et des exports, et ne se renomment donc pas.

mod administration;
mod fourchette;
mod libelles;
mod secteur;

pub use administration::{
    valider_creation, valider_desactivation, valider_publication, AdministrationError,
    SecteurACreer, StatutSecteur,
};
pub use fourchette::{calculer as calculer_fourchette, FourchettePrix, MINIMUM_MISSIONS};
pub use libelles::Libelles;
pub use secteur::{CodeCatalogue, CodeError, Secteur, Skill};
