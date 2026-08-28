//! Vérification de la signature d'un webhook Stripe (FR-028, Story 5.5).
//!
//! **L'endpoint de webhook est public, et c'est la signature qui tient lieu
//! d'authentification.** Il n'y a pas de jeton à présenter : Stripe appelle
//! depuis ses propres adresses, qui changent. Ce module est donc le seul
//! rempart entre un inconnu et une écriture sur l'argent de quelqu'un.
//!
//! **Rien ici n'a besoin d'un compte Stripe.** Le format de l'en-tête, le calcul
//! HMAC, la fenêtre anti-rejeu et la comparaison en temps constant sont du code
//! local, entièrement vérifiable — et c'est précisément la partie qu'on ne
//! voudrait pas écrire dans l'urgence le jour où les clés arrivent.
//!
//! Le format de `Stripe-Signature` est documenté par Stripe :
//!
//! ```text
//! t=1614556800,v1=5257a869e7...,v1=<autre>,v0=<ancien schéma>
//! ```
//!
//! Plusieurs `v1` peuvent coexister pendant une rotation de secret : c'est le
//! mécanisme même qui permet de changer de clé sans interruption, et n'en
//! accepter qu'un rendrait la rotation impossible sans perdre des événements.

use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

/// Tolérance d'horodatage, en secondes.
///
/// Cinq minutes, la valeur que Stripe recommande. Elle borne le rejeu : une
/// requête interceptée et renvoyée plus tard est refusée même si sa signature
/// est authentique. Plus large, la fenêtre de rejeu s'ouvre ; plus étroite, un
/// simple décalage d'horloge entre Stripe et le serveur ferait perdre des
/// événements réels.
pub const TOLERANCE_SECONDES: i64 = 300;

/// Version de schéma acceptée. `v0` est l'ancien, et n'est pas accepté :
/// tolérer un schéma déprécié laisse ouvert le chemin qu'un attaquant
/// choisira.
const SCHEMA: &str = "v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureError {
    /// En-tête absent, vide, ou dont la forme n'est pas celle attendue.
    EnteteIllisible,
    /// Aucune signature du schéma accepté dans l'en-tête.
    SchemaAbsent,
    /// L'horodatage sort de la fenêtre de tolérance.
    ///
    /// **Distinct d'une signature fausse**, pour le journal d'exploitation
    /// seulement : la réponse HTTP est la même, sans quoi elle dirait à qui
    /// essaie si sa signature était bonne.
    HorodatageHorsFenetre,
    /// Aucune des signatures présentées ne correspond.
    SignatureFausse,
}

impl SignatureError {
    pub fn code(&self) -> &'static str {
        // **Un seul code pour les quatre.** FR-028 `@negative` demande
        // `INVALID_SIGNATURE` ; distinguer « horodatage périmé » de « signature
        // fausse » dans la réponse apprendrait à qui essaie qu'il a trouvé le
        // secret mais raté la fenêtre.
        "INVALID_SIGNATURE"
    }
}

impl fmt::Display for SignatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnteteIllisible => write!(f, "en-tête Stripe-Signature illisible"),
            Self::SchemaAbsent => write!(f, "aucune signature au schéma {SCHEMA}"),
            Self::HorodatageHorsFenetre => write!(
                f,
                "horodatage hors de la fenêtre de {TOLERANCE_SECONDES} secondes"
            ),
            Self::SignatureFausse => write!(f, "signature invalide"),
        }
    }
}

impl std::error::Error for SignatureError {}

/// L'en-tête décomposé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnteteSignature {
    /// Horodatage annoncé par Stripe.
    pub horodatage: DateTime<Utc>,
    /// Les signatures du schéma accepté. Plusieurs pendant une rotation.
    pub signatures: Vec<String>,
}

/// Décompose l'en-tête `Stripe-Signature`.
///
/// **Aucune vérification cryptographique ici**, seulement la forme : séparer la
/// lecture du contrôle permet de tester chacune, et évite qu'un en-tête
/// malformé passe par le chemin qui compare des octets.
pub fn lire_entete(entete: &str) -> Result<EnteteSignature, SignatureError> {
    let mut horodatage = None;
    let mut signatures = Vec::new();

    for element in entete.split(',') {
        let Some((cle, valeur)) = element.trim().split_once('=') else {
            // Un élément sans `=` : en-tête malformé. L'ignorer laisserait
            // passer un en-tête tronqué dont la partie lue paraîtrait valide.
            return Err(SignatureError::EnteteIllisible);
        };
        match cle.trim() {
            "t" => {
                let secondes: i64 = valeur
                    .trim()
                    .parse()
                    .map_err(|_| SignatureError::EnteteIllisible)?;
                horodatage = DateTime::from_timestamp(secondes, 0);
                if horodatage.is_none() {
                    return Err(SignatureError::EnteteIllisible);
                }
            }
            SCHEMA => signatures.push(valeur.trim().to_string()),
            // Les autres schémas — `v0` notamment — sont ignorés sans erreur :
            // Stripe les envoie encore, et refuser l'en-tête entier pour leur
            // présence rejetterait des appels parfaitement valides.
            _ => {}
        }
    }

    let horodatage = horodatage.ok_or(SignatureError::EnteteIllisible)?;
    if signatures.is_empty() {
        return Err(SignatureError::SchemaAbsent);
    }
    Ok(EnteteSignature {
        horodatage,
        signatures,
    })
}

/// Vérifie qu'un corps de webhook porte bien la signature du secret donné.
///
/// `corps` est le corps **brut**, tel qu'il est arrivé : le re-sérialiser après
/// l'avoir désérialisé changerait un espace ou l'ordre d'une clé, et la
/// signature ne correspondrait plus. C'est l'erreur classique de cette
/// vérification, et la raison pour laquelle cette fonction prend des octets.
pub fn verifier(
    corps: &[u8],
    entete: &str,
    secret: &[u8],
    maintenant: DateTime<Utc>,
) -> Result<(), SignatureError> {
    let lu = lire_entete(entete)?;

    // La fenêtre est contrôlée **avant** le calcul HMAC : inutile de faire le
    // travail cryptographique pour un événement qu'on refusera de toute façon,
    // et cela borne ce qu'un envoi massif coûte au service.
    let ecart = maintenant - lu.horodatage;
    if ecart.abs() > Duration::seconds(TOLERANCE_SECONDES) {
        return Err(SignatureError::HorodatageHorsFenetre);
    }

    // La charge signée est « horodatage.corps », comme Stripe le spécifie.
    // Signer le seul corps laisserait rejouer indéfiniment une capture
    // authentique.
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepte toute longueur de clé");
    mac.update(lu.horodatage.timestamp().to_string().as_bytes());
    mac.update(b".");
    mac.update(corps);
    let attendue = mac.finalize().into_bytes();

    // **Comparaison en temps constant, et sur tous les candidats.** Sortir à la
    // première correspondance serait correct ; sortir à la première différence
    // d'octet ne le serait pas, car le temps de réponse révélerait le préfixe
    // commun. `egal_en_temps_constant` compare la totalité dans tous les cas.
    let mut acceptee = false;
    for signature in &lu.signatures {
        if let Some(octets) = hex_vers_octets(signature) {
            acceptee |= egal_en_temps_constant(&octets, &attendue);
        }
    }

    if acceptee {
        Ok(())
    } else {
        Err(SignatureError::SignatureFausse)
    }
}

/// Décode une chaîne hexadécimale. `None` si elle n'en est pas une.
///
/// Écrit à la main plutôt qu'ajouté en dépendance : trente lignes pour un
/// décodage hexadécimal ne justifient pas une caisse de plus dans un chemin qui
/// traite des entrées non authentifiées.
fn hex_vers_octets(chaine: &str) -> Option<Vec<u8>> {
    if !chaine.len().is_multiple_of(2) {
        return None;
    }
    let octets = chaine.as_bytes();
    let mut sortie = Vec::with_capacity(chaine.len() / 2);
    for paire in octets.chunks(2) {
        let haut = chiffre_hex(paire[0])?;
        let bas = chiffre_hex(paire[1])?;
        sortie.push(haut << 4 | bas);
    }
    Some(sortie)
}

fn chiffre_hex(octet: u8) -> Option<u8> {
    match octet {
        b'0'..=b'9' => Some(octet - b'0'),
        b'a'..=b'f' => Some(octet - b'a' + 10),
        // Stripe écrit en minuscules ; accepter les majuscules ne coûte rien et
        // évite un refus incompréhensible si cela changeait un jour.
        b'A'..=b'F' => Some(octet - b'A' + 10),
        _ => None,
    }
}

/// Compare deux suites d'octets sans court-circuit.
///
/// La différence de longueur sort tout de suite : elle est publique — un
/// attaquant la connaît en comptant les caractères de sa propre signature — et
/// la masquer n'apporterait rien.
fn egal_en_temps_constant(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        difference |= x ^ y;
    }
    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const SECRET: &[u8] = b"whsec_exemple_de_test_jamais_en_production";
    const CORPS: &[u8] = br#"{"id":"evt_1","type":"payment_intent.succeeded"}"#;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap()
    }

    /// Fabrique un en-tête authentique pour un instant donné.
    fn entete_valide(quand: DateTime<Utc>, corps: &[u8], secret: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(quand.timestamp().to_string().as_bytes());
        mac.update(b".");
        mac.update(corps);
        let signature: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|o| format!("{o:02x}"))
            .collect();
        format!("t={},v1={}", quand.timestamp(), signature)
    }

    #[test]
    fn happy_une_signature_authentique_est_acceptee() {
        let entete = entete_valide(t0(), CORPS, SECRET);
        assert_eq!(verifier(CORPS, &entete, SECRET, t0()), Ok(()));
    }

    #[test]
    fn happy_la_rotation_de_secret_accepte_deux_signatures() {
        // Pendant une rotation, Stripe envoie une `v1` par secret actif. N'en
        // accepter qu'une rendrait toute rotation impossible sans perdre des
        // événements.
        let ancienne = entete_valide(t0(), CORPS, b"ancien_secret");
        let nouvelle = entete_valide(t0(), CORPS, SECRET);
        let combinee = format!("{},{}", ancienne, nouvelle.split_once(',').unwrap().1);
        assert_eq!(verifier(CORPS, &combinee, SECRET, t0()), Ok(()));
    }

    #[test]
    fn happy_un_schema_inconnu_dans_l_entete_est_ignore() {
        // Stripe envoie encore `v0`. Refuser l'en-tête entier pour sa présence
        // rejetterait des appels parfaitement valides.
        let entete = format!("{},v0=abcdef", entete_valide(t0(), CORPS, SECRET));
        assert_eq!(verifier(CORPS, &entete, SECRET, t0()), Ok(()));
    }

    #[test]
    fn security_une_signature_fausse_est_refusee() {
        let entete = entete_valide(t0(), CORPS, b"un_autre_secret");
        assert_eq!(
            verifier(CORPS, &entete, SECRET, t0()),
            Err(SignatureError::SignatureFausse)
        );
    }

    #[test]
    fn security_un_corps_modifie_invalide_la_signature() {
        // Le cœur du dispositif : la signature porte sur le corps, donc changer
        // un centime dans la charge la casse.
        let entete = entete_valide(t0(), CORPS, SECRET);
        let falsifie = br#"{"id":"evt_1","type":"payment_intent.succeeded","x":1}"#;
        assert_eq!(
            verifier(falsifie, &entete, SECRET, t0()),
            Err(SignatureError::SignatureFausse)
        );
    }

    #[test]
    fn security_l_horodatage_est_dans_la_charge_signee() {
        // Réutiliser une signature authentique sous un autre horodatage doit
        // échouer : sinon un appel intercepté se rejouerait indéfiniment en
        // rafraîchissant simplement le `t=`.
        let authentique = entete_valide(t0(), CORPS, SECRET);
        let signature = authentique.split_once("v1=").unwrap().1;
        let plus_tard = t0() + Duration::seconds(60);
        let rejoue = format!("t={},v1={}", plus_tard.timestamp(), signature);
        assert_eq!(
            verifier(CORPS, &rejoue, SECRET, plus_tard),
            Err(SignatureError::SignatureFausse)
        );
    }

    #[test]
    fn security_un_horodatage_trop_ancien_est_refuse() {
        // Le rejeu d'un appel authentique intercepté. La signature est bonne ;
        // c'est la fenêtre qui le refuse.
        let vieux = t0() - Duration::seconds(TOLERANCE_SECONDES + 1);
        let entete = entete_valide(vieux, CORPS, SECRET);
        assert_eq!(
            verifier(CORPS, &entete, SECRET, t0()),
            Err(SignatureError::HorodatageHorsFenetre)
        );
    }

    #[test]
    fn security_un_horodatage_dans_le_futur_est_refuse_aussi() {
        // Symétrique, et ce n'est pas de la symétrie gratuite : sans borne
        // haute, une signature fabriquée pour l'an prochain resterait valable
        // un an.
        let futur = t0() + Duration::seconds(TOLERANCE_SECONDES + 1);
        let entete = entete_valide(futur, CORPS, SECRET);
        assert_eq!(
            verifier(CORPS, &entete, SECRET, t0()),
            Err(SignatureError::HorodatageHorsFenetre)
        );
    }

    #[test]
    fn edge_la_fenetre_accepte_la_borne_exacte() {
        let limite = t0() - Duration::seconds(TOLERANCE_SECONDES);
        let entete = entete_valide(limite, CORPS, SECRET);
        assert_eq!(verifier(CORPS, &entete, SECRET, t0()), Ok(()));
    }

    #[test]
    fn security_tous_les_codes_de_refus_sont_indistinguables() {
        // FR-028 `@negative` : la réponse est `INVALID_SIGNATURE`, quelle que
        // soit la cause. Distinguer « horodatage périmé » de « signature
        // fausse » dirait à qui essaie qu'il a trouvé le secret.
        for erreur in [
            SignatureError::EnteteIllisible,
            SignatureError::SchemaAbsent,
            SignatureError::HorodatageHorsFenetre,
            SignatureError::SignatureFausse,
        ] {
            assert_eq!(erreur.code(), "INVALID_SIGNATURE");
        }
    }

    #[test]
    fn negative_un_entete_malforme_est_refuse() {
        for entete in [
            "",
            "n'importe quoi",
            "t=pasunnombre,v1=abcd",
            // Sans horodatage : la charge signée ne peut pas être reconstruite.
            "v1=abcdef",
            // Élément sans `=` : en-tête tronqué. L'ignorer laisserait passer
            // un en-tête dont la partie lue paraîtrait valide.
            "t=1,v1=ab,tronque",
        ] {
            assert!(
                verifier(CORPS, entete, SECRET, t0()).is_err(),
                "en-tête accepté à tort : {entete:?}"
            );
        }
    }

    #[test]
    fn negative_sans_signature_au_schema_accepte_le_refus_est_explicite() {
        // `v0` seul : l'ancien schéma ne suffit pas. Le tolérer laisserait
        // ouvert le chemin qu'un attaquant choisirait.
        let entete = format!("t={},v0=abcdef", t0().timestamp());
        assert_eq!(
            verifier(CORPS, &entete, SECRET, t0()),
            Err(SignatureError::SchemaAbsent)
        );
    }

    #[test]
    fn negative_une_signature_non_hexadecimale_ne_fait_pas_paniquer() {
        // Entrée non authentifiée : elle doit être refusée, jamais faire
        // tomber le service.
        for signature in ["zz", "abc", "v1", "0x1234"] {
            let entete = format!("t={},v1={}", t0().timestamp(), signature);
            assert_eq!(
                verifier(CORPS, &entete, SECRET, t0()),
                Err(SignatureError::SignatureFausse)
            );
        }
    }

    #[test]
    fn security_la_comparaison_ne_court_circuite_pas() {
        // On ne mesure pas le temps ici — un test de temporisation serait
        // instable en intégration continue. Ce qui est vérifié est la
        // propriété fonctionnelle dont dépend la constance : deux suites qui ne
        // diffèrent qu'au dernier octet sont refusées comme celles qui
        // diffèrent au premier.
        assert!(!egal_en_temps_constant(&[1, 2, 3], &[9, 2, 3]));
        assert!(!egal_en_temps_constant(&[1, 2, 3], &[1, 2, 9]));
        assert!(!egal_en_temps_constant(&[1, 2, 3], &[1, 2]));
        assert!(egal_en_temps_constant(&[1, 2, 3], &[1, 2, 3]));
        assert!(egal_en_temps_constant(&[], &[]));
    }

    #[test]
    fn edge_l_entete_tolere_les_espaces() {
        let entete = entete_valide(t0(), CORPS, SECRET).replace(',', " , ");
        assert_eq!(verifier(CORPS, &entete, SECRET, t0()), Ok(()));
    }
}
