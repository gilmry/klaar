//! Adapter IdentityProvider : itsme OIDC (FR-002).
//!
//! **Le contrat itsme manque ; les garanties, non.** Ce qui demande un contrat
//! est l'échange réseau : les identifiants client, le document de découverte,
//! le jeu de clés publiques. Ce qui protège cet échange — le jeton
//! anti-falsification, le nombre à usage unique contre le rejeu, le
//! vérificateur PKCE, et le contrôle des revendications du jeton d'identité —
//! ne dépend d'aucun contrat. C'est à nous, et c'est précisément ce qu'on ne
//! voudrait pas écrire dans l'urgence le jour où les identifiants arrivent.

mod echange;
mod jeton_identite;

pub use echange::{Echange, EchangeError, OCTETS_ALEA, VALIDITE_SECONDES};
pub use jeton_identite::{
    code_erreur_itsme, numero_belge, verifier_revendications, Attendu, JetonError, Revendications,
    ACR_ATTENDU, DERIVE_TOLEREE_SECONDES, DUREE_MAX_SECONDES,
};
