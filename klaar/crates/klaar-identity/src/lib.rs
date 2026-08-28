//! Bounded context Identity & Access : User, Provider (FR-001 à FR-007,
//! FR-038 à FR-041).
//!
//! Domaine pur : aucune IO, aucun transport, aucun SQL. Les invariants sont
//! vérifiés à la construction et ne peuvent pas être contournés en passant par
//! un autre chemin, parce qu'il n'y en a pas d'autre.

mod jeton_verification;
mod mot_de_passe;
mod numero_bce;
mod ops;
mod provider;
mod totp;
mod utilisateur;
mod verrouillage;

pub use jeton_verification::{EmpreinteJeton, JetonVerification};
pub use mot_de_passe::{EmpreinteMotDePasse, MotDePasse, MotDePasseError, ParametresArgon2};
pub use numero_bce::{NumeroBce, NumeroBceError};
pub use ops::{CompteOps, OpsError, Permission, RoleOps, INACTIVITE_MAX_JOURS};
pub use provider::{
    OrigineKyc, PreuveKyc, Provider, ProviderError, StatutProvider, RAISON_SOCIALE_MAX,
    RAYON_INTERVENTION_DEFAUT, RAYON_INTERVENTION_MAX, RAYON_INTERVENTION_MIN,
};
pub use totp::{
    base32 as base32_totp, calculer as calculer_totp, verifier as verifier_totp, VerificationTotp,
    CHIFFRES as TOTP_CHIFFRES, PAS_SECONDES as TOTP_PAS_SECONDES,
    SECRET_OCTETS as TOTP_SECRET_OCTETS,
};
pub use utilisateur::{JetonEmis, StatutUtilisateur, Utilisateur, DELAI_EFFACEMENT_JOURS};
pub use verrouillage::{Verrouillage, DUREE_VERROU_MINUTES, FENETRE_ECHECS_MINUTES, MAX_ECHECS};
