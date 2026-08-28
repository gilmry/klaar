//! Signature chaînée de la trace de matching (AI Act art. 12, Story 3.8).
//!
//! **Ce que la signature garantit, et ce qu'elle ne garantit pas.** Chaque
//! ligne de trace porte un HMAC-SHA256 calculé sur son contenu **et sur la
//! signature de la ligne précédente**. Modifier une ligne casse sa propre
//! signature ; en supprimer une, en insérer une au milieu ou en changer
//! l'ordre casse la chaîne à partir de là. C'est cette dernière propriété qui
//! motive le chaînage : un HMAC par ligne, indépendant, ne dirait rien d'une
//! suppression, et supprimer est exactement ce que ferait quelqu'un qui veut
//! effacer un matching discriminatoire.
//!
//! **La clé vit sur le même serveur que la base.** Quelqu'un qui obtient les
//! deux peut donc réécrire la trace **et** la resigner : la signature détecte
//! une altération faite depuis la base seule, pas une compromission complète
//! du serveur. Un stockage WORM avec verrou de rétention chez un tiers lèverait
//! cette limite ; il demande un compte d'hébergement, hors du périmètre
//! vitrine, et c'est écrit dans `COMPLIANCE.md`.
//!
//! **La vérification compare en temps constant.** Un `==` sur des octets sort
//! au premier écart, et le temps de sortie renseigne sur le préfixe correct ;
//! `verify_slice` de la caisse `hmac` ne le fait pas.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

/// Longueur minimale de la clé, en octets.
///
/// Trente-deux, comme le secret de signature des jetons : la RFC 2104 §3
/// recommande une clé au moins aussi longue que la sortie de la fonction de
/// hachage, soit trente-deux octets pour SHA-256. Plus court n'ajoute pas de
/// sécurité au-delà de sa propre longueur.
pub const CLE_MIN_OCTETS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    /// Clé trop courte pour la garantie annoncée.
    CleTropCourte { octets: usize },
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CleTropCourte { octets } => write!(
                f,
                "clé de {octets} octets, {CLE_MIN_OCTETS} au minimum (RFC 2104 §3)"
            ),
        }
    }
}

impl std::error::Error for SignatureError {}

/// Signataire de la chaîne de trace.
///
/// Ne dérive ni `Debug` ni `Clone` : la clé ne doit apparaître ni dans un
/// journal ni dans un message d'erreur, et une copie de plus est une copie de
/// plus à oublier.
pub struct SignataireTrace {
    cle: Vec<u8>,
}

impl SignataireTrace {
    pub fn new(cle: &[u8]) -> Result<Self, SignatureError> {
        if cle.len() < CLE_MIN_OCTETS {
            return Err(SignatureError::CleTropCourte { octets: cle.len() });
        }
        Ok(Self { cle: cle.to_vec() })
    }

    /// Signature d'une ligne, chaînée sur la précédente.
    ///
    /// `precedente` vaut `None` pour le premier maillon. Le préfixe de domaine
    /// « klaar-trace-v1 » empêche qu'une signature calculée ici puisse valoir
    /// pour un autre usage de la même clé, et le numéro de version permettra
    /// d'en changer le contenu sans rendre l'ancienne chaîne invérifiable.
    pub fn signer(&self, precedente: Option<&[u8]>, contenu: &[u8]) -> Vec<u8> {
        let mut mac =
            HmacSha256::new_from_slice(&self.cle).expect("HMAC accepte toute longueur de clé");
        mac.update(b"klaar-trace-v1");
        // Séparateurs de longueur explicite plutôt que concaténation nue : sans
        // eux, deux découpages différents des mêmes octets produiraient la même
        // signature, et une ligne pourrait être maquillée en une autre.
        ecrire_champ(&mut mac, precedente.unwrap_or(&[]));
        ecrire_champ(&mut mac, contenu);
        mac.finalize().into_bytes().to_vec()
    }

    /// Vrai si la signature correspond, en temps constant.
    pub fn verifier(&self, precedente: Option<&[u8]>, contenu: &[u8], signature: &[u8]) -> bool {
        let mut mac =
            HmacSha256::new_from_slice(&self.cle).expect("HMAC accepte toute longueur de clé");
        mac.update(b"klaar-trace-v1");
        ecrire_champ(&mut mac, precedente.unwrap_or(&[]));
        ecrire_champ(&mut mac, contenu);
        mac.verify_slice(signature).is_ok()
    }
}

fn ecrire_champ(mac: &mut HmacSha256, champ: &[u8]) {
    mac.update(&(champ.len() as u64).to_be_bytes());
    mac.update(champ);
}

/// Sérialise une ligne de trace en octets, de façon reproductible.
///
/// **L'ordre et la mise en forme sont figés ici**, et non délégués à `serde` :
/// un changement de sérialiseur, ou un `HashMap` dont l'ordre varie, rendrait
/// toute la chaîne antérieure invérifiable sans que rien ne le signale.
///
/// Les nombres à virgule passent par leur représentation binaire (`to_bits`) et
/// non par leur écriture décimale : `format!("{}", 0.1 + 0.2)` dépend de la
/// bibliothèque standard, alors que les octets d'un `f64` ne dépendent de rien.
#[allow(clippy::too_many_arguments)]
pub fn contenu_canonique(
    demande_id: &uuid::Uuid,
    provider_id: &uuid::Uuid,
    score_total: f64,
    distance_metres: f64,
    retenu: bool,
    motif_ecart: Option<&str>,
    tracee_le: i64,
) -> Vec<u8> {
    let mut octets = Vec::with_capacity(96);
    octets.extend_from_slice(demande_id.as_bytes());
    octets.extend_from_slice(provider_id.as_bytes());
    octets.extend_from_slice(&score_total.to_bits().to_be_bytes());
    octets.extend_from_slice(&distance_metres.to_bits().to_be_bytes());
    octets.push(u8::from(retenu));
    let motif = motif_ecart.unwrap_or("");
    octets.extend_from_slice(&(motif.len() as u64).to_be_bytes());
    octets.extend_from_slice(motif.as_bytes());
    octets.extend_from_slice(&tracee_le.to_be_bytes());
    octets
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    const CLE: &[u8] = b"une-cle-de-test-de-trente-deux-o";

    fn signataire() -> SignataireTrace {
        SignataireTrace::new(CLE).unwrap()
    }

    fn contenu(retenu: bool) -> Vec<u8> {
        contenu_canonique(
            &Uuid::from_u128(1),
            &Uuid::from_u128(2),
            0.75,
            1_200.0,
            retenu,
            if retenu { None } else { Some("HORS_TOP") },
            1_780_000_000,
        )
    }

    #[test]
    fn happy_une_signature_se_verifie() {
        let s = signataire();
        let sig = s.signer(None, &contenu(true));
        assert!(s.verifier(None, &contenu(true), &sig));
    }

    #[test]
    fn happy_la_chaine_se_verifie_maillon_par_maillon() {
        let s = signataire();
        let a = s.signer(None, &contenu(true));
        let b = s.signer(Some(&a), &contenu(false));
        assert!(s.verifier(None, &contenu(true), &a));
        assert!(s.verifier(Some(&a), &contenu(false), &b));
    }

    #[test]
    fn happy_la_signature_est_deterministe() {
        // Sans quoi une vérification ultérieure échouerait sur une trace
        // pourtant intacte, et l'audit crierait au loup.
        let s = signataire();
        assert_eq!(
            s.signer(None, &contenu(true)),
            s.signer(None, &contenu(true))
        );
    }

    /// `unwrap_err` exigerait `T: Debug` sur `SignataireTrace`, qui ne le
    /// dérive pas — c'est précisément la garantie qu'on veut. Le `match` fait
    /// le même travail sans forcer à l'affaiblir.
    fn erreur(cle: &[u8]) -> SignatureError {
        match SignataireTrace::new(cle) {
            Err(e) => e,
            Ok(_) => panic!("cette clé aurait dû être refusée"),
        }
    }

    #[test]
    fn negative_une_cle_trop_courte_est_refusee() {
        // RFC 2104 §3 : au moins la longueur de la sortie de SHA-256.
        assert!(matches!(
            erreur(b"trop-court"),
            SignatureError::CleTropCourte { .. }
        ));
        assert!(SignataireTrace::new(&[0u8; CLE_MIN_OCTETS]).is_ok());
    }

    #[test]
    fn security_une_ligne_modifiee_casse_sa_signature() {
        // C'est la garantie de base : altérer le contenu se voit.
        let s = signataire();
        let sig = s.signer(None, &contenu(true));
        assert!(!s.verifier(None, &contenu(false), &sig));
    }

    #[test]
    fn security_une_ligne_supprimee_casse_la_chaine() {
        // C'est ce qu'un HMAC par ligne indépendant ne dirait pas, et c'est
        // exactement ce que ferait quelqu'un qui veut effacer un matching
        // discriminatoire.
        let s = signataire();
        let a = s.signer(None, &contenu(true));
        let b = s.signer(Some(&a), &contenu(false));
        // On retire `a` : `b` prétend alors être le premier maillon.
        assert!(!s.verifier(None, &contenu(false), &b));
    }

    #[test]
    fn security_deux_lignes_interverties_cassent_la_chaine() {
        let s = signataire();
        let a = s.signer(None, &contenu(true));
        let b = s.signer(Some(&a), &contenu(false));
        // Ordre inversé : `a` devrait suivre `b`, sa signature ne le dit pas.
        assert!(!s.verifier(Some(&b), &contenu(true), &a));
    }

    #[test]
    fn security_une_autre_cle_ne_verifie_rien() {
        let s = signataire();
        let sig = s.signer(None, &contenu(true));
        let autre = SignataireTrace::new(b"une-AUTRE-cle-de-trente-deux-oct").unwrap();
        assert!(!autre.verifier(None, &contenu(true), &sig));
    }

    #[test]
    fn security_le_decoupage_des_champs_ne_se_maquille_pas() {
        // Sans séparateur de longueur, `("ab", "c")` et `("a", "bc")`
        // produiraient les mêmes octets, donc la même signature : une ligne
        // pourrait être maquillée en une autre.
        let s = signataire();
        let a = s.signer(Some(b"ab"), b"c");
        let b = s.signer(Some(b"a"), b"bc");
        assert_ne!(a, b);
    }

    #[test]
    fn security_le_contenu_canonique_distingue_ce_qui_doit_l_etre() {
        // Chaque champ compte : deux lignes qui ne diffèrent que par l'un
        // d'eux ne doivent pas se signer pareil.
        let base = contenu_canonique(
            &Uuid::from_u128(1),
            &Uuid::from_u128(2),
            0.75,
            1_200.0,
            true,
            None,
            1_780_000_000,
        );
        let variantes = [
            contenu_canonique(
                &Uuid::from_u128(9),
                &Uuid::from_u128(2),
                0.75,
                1_200.0,
                true,
                None,
                1_780_000_000,
            ),
            contenu_canonique(
                &Uuid::from_u128(1),
                &Uuid::from_u128(9),
                0.75,
                1_200.0,
                true,
                None,
                1_780_000_000,
            ),
            contenu_canonique(
                &Uuid::from_u128(1),
                &Uuid::from_u128(2),
                0.76,
                1_200.0,
                true,
                None,
                1_780_000_000,
            ),
            contenu_canonique(
                &Uuid::from_u128(1),
                &Uuid::from_u128(2),
                0.75,
                1_201.0,
                true,
                None,
                1_780_000_000,
            ),
            contenu_canonique(
                &Uuid::from_u128(1),
                &Uuid::from_u128(2),
                0.75,
                1_200.0,
                false,
                Some("HORS_TOP"),
                1_780_000_000,
            ),
            contenu_canonique(
                &Uuid::from_u128(1),
                &Uuid::from_u128(2),
                0.75,
                1_200.0,
                true,
                None,
                1_780_000_001,
            ),
        ];
        for (i, v) in variantes.iter().enumerate() {
            assert_ne!(&base, v, "variante {i}");
        }
    }

    #[test]
    fn security_la_cle_ne_fuit_par_aucun_affichage() {
        // `SignataireTrace` ne dérive pas `Debug` : `{:?}` dessus ne compile
        // pas, et `unwrap_err` non plus — c'est ce qui a obligé à écrire
        // l'assistant `erreur` plus haut. L'erreur de compilation est le vrai
        // garde-fou ; ce test vérifie l'autre moitié, que le message de refus
        // ne recopie pas la clé qu'on vient de lui donner.
        assert!(!format!("{}", erreur(b"court")).contains("court"));
    }
}
