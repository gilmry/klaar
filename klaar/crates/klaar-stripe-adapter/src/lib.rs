//! Adapter PaymentGateway : Stripe Connect (ADR pending, devis §payment).
//!
//! Scaffolding Sprint 0 (Story 0.1) : le bounded context existe et compile,
//! son contenu métier sera implémenté epic par epic (voir
//! docs/bmad-livrables/04-Epics-Stories.md).

mod evenement;
mod signature;

pub use evenement::{
    decider, ordonner, valider_id, Evenement, EvenementError, Suite, TypeEvenement,
    ID_MAX_CARACTERES,
};
pub use signature::{
    lire_entete, verifier as verifier_signature, EnteteSignature, SignatureError,
    TOLERANCE_SECONDES,
};
