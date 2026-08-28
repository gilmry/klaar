//! Bounded context Payment : Quote, Escrow, Payout, Invoice (FR-016, FR-017, FR-024 à FR-029).
//!
//! Livré à ce jour : le `Devis` (FR-016), la répartition de commission
//! (FR-021, FR-025) et le **cycle de vie du séquestre** (FR-024 à FR-027).
//!
//! **Ce que « dépend de Stripe » recouvre, et ce qu'il ne recouvre pas.** Le
//! mouvement d'argent passe par une passerelle qui n'est pas provisionnée. Mais
//! les règles qui l'encadrent — ce qu'on a le droit de capturer, ce qu'on a le
//! droit de rembourser, l'égalité comptable à chaque instant — ne sont tenues
//! par aucune passerelle : elles sont ici, écrites et vérifiées. Ce qui manque
//! au jour où les clés arriveront est du câblage réseau, pas ces décisions-là.
//!
//! Ce qui reste effectivement bloqué : la facture (FR-026), qui doit naître
//! avec le paiement — en émettre pour de l'argent qui n'a jamais bougé
//! créerait des pièces comptables sans contrepartie.

mod devis;
mod liberation;
mod sequestre;

pub use liberation::{
    echeance_validation, repartir, Liberation, LiberationError, OrigineValidation, Repartition,
    StatutLiberation, DELAI_VALIDATION_HEURES, SEUIL_QUATRE_YEUX_CENTS, TAUX_COMMISSION_BP,
};

pub use sequestre::{Sequestre, SequestreError, StatutSequestre, AUTORISATION_JOURS};

pub use devis::{
    Devis, DevisError, MotifRefus, Proposition, StatutDevis, DELAI_MAX_MINUTES, DELAI_MIN_MINUTES,
    DEVIS_MAX_PAR_MISSION, MONTANT_MAX_CENTS, MONTANT_MIN_CENTS, NOTE_MAX_CARACTERES,
    PREUVE_MAX_CARACTERES, TAUX_ADMIS, VALIDITE_MINUTES,
};
