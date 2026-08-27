//! Jeton de vérification d'adresse email (FR-001, garantie post « token courte
//! durée, marqué utilisé, non rejouable »).
//!
//! **Écart tracé avec le PRD.** FR-001 dit « token JWT courte durée (1 h) ». Un
//! JWT est vérifiable sans état côté serveur, ce qui est précisément ce qui
//! empêche de le marquer comme utilisé : le même jeton reste valable jusqu'à
//! son expiration, autant de fois qu'on le présente. Or le même FR exige
//! qu'il ne soit pas rejouable. Les deux ne tiennent pas ensemble sans une
//! table de jetons consommés — c'est-à-dire sans l'état que le JWT prétendait
//! éviter.
//!
//! Le choix retenu est donc un jeton opaque de 32 octets tirés du générateur
//! du système, conservé **haché** et à usage unique. Il coûte la même écriture
//! en base que la liste de révocation qu'aurait imposée le JWT, sans en avoir
//! la cryptographie ni la surface d'attaque (`alg: none`, confusion de clés).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Durée de validité, alignée sur FR-001.
pub const VALIDITE_HEURES: i64 = 1;

/// Jeton en clair. Il n'existe qu'entre son tirage et son envoi par email ;
/// rien ne le conserve.
#[derive(Clone, PartialEq, Eq)]
pub struct JetonVerification(String);

impl JetonVerification {
    /// Tire un jeton neuf. 32 octets du générateur du système, soit 256 bits :
    /// hors de portée d'une énumération, y compris hors ligne.
    pub fn tirer() -> Self {
        let mut octets = [0u8; 32];
        OsRng.fill_bytes(&mut octets);
        Self(URL_SAFE_NO_PAD.encode(octets))
    }

    /// Reconstruit un jeton reçu du client, pour le comparer à l'empreinte
    /// conservée. Ne valide rien par elle-même.
    pub fn depuis_chaine(valeur: &str) -> Self {
        Self(valeur.to_string())
    }

    /// Valeur à placer dans le lien de vérification.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn empreinte(&self) -> EmpreinteJeton {
        EmpreinteJeton::calculer(self)
    }
}

impl fmt::Debug for JetonVerification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JetonVerification(***)")
    }
}

/// Empreinte SHA-256 du jeton, en hexadécimal. C'est elle, et elle seule, qui
/// est conservée.
///
/// Un jeton de vérification stocké en clair est un mot de passe à usage
/// unique stocké en clair : quiconque lit la table peut activer les comptes en
/// attente. Le hachage n'a pas besoin d'être lent ici, contrairement à celui
/// d'un mot de passe — le jeton fait 256 bits d'entropie tirés au sort, il n'y
/// a pas de dictionnaire à lui opposer.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmpreinteJeton(String);

impl EmpreinteJeton {
    pub fn calculer(jeton: &JetonVerification) -> Self {
        let condense = Sha256::digest(jeton.0.as_bytes());
        Self(condense.iter().map(|o| format!("{o:02x}")).collect())
    }

    pub fn depuis_hex(hex: &str) -> Self {
        Self(hex.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EmpreinteJeton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmpreinteJeton({})", &self.0[..8.min(self.0.len())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_un_jeton_correspond_a_sa_propre_empreinte() {
        let jeton = JetonVerification::tirer();
        let conserve = jeton.empreinte();
        let presente = JetonVerification::depuis_chaine(jeton.expose());
        assert_eq!(presente.empreinte(), conserve);
    }

    #[test]
    fn negative_un_autre_jeton_ne_correspond_pas() {
        let a = JetonVerification::tirer();
        let b = JetonVerification::tirer();
        assert_ne!(a.empreinte(), b.empreinte());
    }

    #[test]
    fn negative_une_chaine_arbitraire_ne_correspond_a_rien() {
        let jeton = JetonVerification::tirer();
        let bidon = JetonVerification::depuis_chaine("abc123");
        assert_ne!(bidon.empreinte(), jeton.empreinte());
    }

    #[test]
    fn edge_le_jeton_est_utilisable_tel_quel_dans_une_url() {
        // base64url sans remplissage : ni `+`, ni `/`, ni `=`, donc aucun
        // ré-encodage entre le lien envoyé et la valeur reçue.
        let jeton = JetonVerification::tirer();
        let v = jeton.expose();
        assert!(!v.contains('+') && !v.contains('/') && !v.contains('='));
        assert!(v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn edge_l_empreinte_fait_toujours_64_hexadecimaux() {
        for _ in 0..16 {
            let e = JetonVerification::tirer().empreinte();
            assert_eq!(e.as_str().len(), 64);
            assert!(e.as_str().chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn security_deux_tirages_ne_se_repetent_jamais() {
        let mut vus = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(vus.insert(JetonVerification::tirer().expose().to_string()));
        }
    }

    #[test]
    fn security_le_jeton_ne_s_imprime_pas_et_l_empreinte_reste_tronquee() {
        let jeton = JetonVerification::tirer();
        let trace = format!("{jeton:?}");
        assert_eq!(trace, "JetonVerification(***)");
        assert!(!trace.contains(jeton.expose()));

        let empreinte = jeton.empreinte();
        assert!(!format!("{empreinte:?}").contains(empreinte.as_str()));
    }

    #[test]
    fn security_l_empreinte_ne_permet_pas_de_remonter_au_jeton() {
        // Contrôle grossier mais utile : l'empreinte ne doit pas contenir le
        // jeton, ce qu'un « hachage » réduit à une copie ferait.
        let jeton = JetonVerification::tirer();
        assert!(!jeton.empreinte().as_str().contains(jeton.expose()));
    }
}
