//! Bounded context Payment : Quote, Escrow, Payout, Invoice (FR-016, FR-017, FR-024 à FR-029).
//!
//! Livré à ce jour : le `Devis` (FR-016). L'Escrow, le Payout et la facture
//! dépendent de Stripe, dont le compte n'est pas ouvert ; ils suivront epic par
//! epic (voir docs/bmad-livrables/04-Epics-Stories.md).

mod devis;
mod liberation;

pub use liberation::{
    echeance_validation, repartir, Liberation, LiberationError, OrigineValidation, Repartition,
    StatutLiberation, DELAI_VALIDATION_HEURES, SEUIL_QUATRE_YEUX_CENTS, TAUX_COMMISSION_BP,
};

pub use devis::{
    Devis, DevisError, MotifRefus, Proposition, StatutDevis, DELAI_MAX_MINUTES, DELAI_MIN_MINUTES,
    DEVIS_MAX_PAR_MISSION, MONTANT_MAX_CENTS, MONTANT_MIN_CENTS, NOTE_MAX_CARACTERES,
    PREUVE_MAX_CARACTERES, TAUX_ADMIS, VALIDITE_MINUTES,
};
