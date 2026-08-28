//! Bounded context Audit : trace des décisions algorithmiques (AI Act art. 12).
//!
//! Ce que cette caisse contient : la **signature chaînée** de la trace de
//! matching, et rien d'autre. Le journal d'audit des actions de compte
//! (FR-001) vit dans `klaar-sqlx-repos`, parce qu'il n'a pas d'autre logique
//! que sa persistance ; celle-ci en a une, et elle est cryptographique.

mod signature;

pub use signature::{contenu_canonique, SignataireTrace, SignatureError, CLE_MIN_OCTETS};
