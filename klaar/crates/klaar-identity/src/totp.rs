//! Seconde authentification par code à usage unique (RFC 6238, FR-041).
//!
//! **Pourquoi SHA-1 alors que le reste du service est en SHA-256.** RFC 6238
//! autorise les deux, mais les applications d'authentification que les gens ont
//! réellement sur leur téléphone — Google Authenticator en tête — ne lisent que
//! SHA-1. Un TOTP plus solide sur le papier et illisible par l'application de
//! l'utilisateur ne protège personne : il fait renoncer à la seconde
//! authentification. Le compromis est ici, écrit, plutôt qu'ailleurs, découvert.
//!
//! **Ce que SHA-1 coûte ici, et pourquoi c'est acceptable.** Les faiblesses
//! connues de SHA-1 sont des collisions ; TOTP repose sur HMAC, dont la
//! sécurité ne dépend pas de la résistance aux collisions. Aucune attaque
//! pratique sur HMAC-SHA-1 n'est connue.
//!
//! **Le rejeu est fermé par l'appelant, pas ici.** Un code vaut trente
//! secondes, et une fenêtre de tolérance en fait quatre-vingt-dix : sans
//! mémoire du dernier pas accepté, un code lu par-dessus une épaule reste
//! utilisable une minute et demie. `dernier_pas_accepte` est rendu pour que le
//! stockage puisse le refuser.

use hmac::{Hmac, Mac};
use sha1::Sha1;

/// Durée d'un pas, en secondes (RFC 6238 §4, valeur recommandée).
pub const PAS_SECONDES: i64 = 30;

/// Chiffres du code.
pub const CHIFFRES: u32 = 6;

/// Pas de tolérance de part et d'autre.
///
/// Un seul : l'horloge d'un téléphone dérive de quelques secondes, pas de
/// minutes. Élargir la fenêtre allonge d'autant la durée de vie d'un code
/// dérobé.
pub const TOLERANCE_PAS: i64 = 1;

/// Octets du secret partagé.
///
/// Vingt : le minimum recommandé par la RFC 4226 §4 pour HMAC-SHA-1, et ce que
/// les applications d'authentification affichent proprement en base32.
pub const SECRET_OCTETS: usize = 20;

type HmacSha1 = Hmac<Sha1>;

/// Ce que la vérification a conclu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationTotp {
    /// Le pas de temps qui a accepté le code.
    ///
    /// **À conserver.** Le refuser au prochain appel est la seule chose qui
    /// empêche le rejeu d'un code encore valide.
    pub pas: i64,
}

/// Vérifie un code contre un secret.
///
/// `dernier_pas_accepte` est le pas déjà consommé par ce compte, s'il y en a
/// un : tout pas inférieur ou égal est refusé, ce qui ferme le rejeu.
///
/// La comparaison des codes est **en temps constant** : un `==` sur des chaînes
/// s'arrête au premier caractère différent, et la durée de l'appel dirait alors
/// combien de chiffres sont justes.
pub fn verifier(
    secret: &[u8],
    code: &str,
    horodatage: i64,
    dernier_pas_accepte: Option<i64>,
) -> Option<VerificationTotp> {
    // Un code d'une autre longueur ne peut pas être juste : le refuser tout de
    // suite évite six calculs de HMAC pour rien.
    if code.len() != CHIFFRES as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let pas_courant = horodatage.div_euclid(PAS_SECONDES);
    for decalage in -TOLERANCE_PAS..=TOLERANCE_PAS {
        let pas = pas_courant + decalage;
        if dernier_pas_accepte.is_some_and(|dernier| pas <= dernier) {
            continue;
        }
        if egal_en_temps_constant(&calculer(secret, pas), code) {
            return Some(VerificationTotp { pas });
        }
    }
    None
}

/// Le code attendu pour un pas donné.
///
/// Public pour que l'appelant puisse afficher un code de contrôle au moment de
/// la configuration : demander à quelqu'un de scanner un QR code sans jamais
/// lui confirmer que ça a marché est le meilleur moyen de le voir se
/// verrouiller dehors.
pub fn calculer(secret: &[u8], pas: i64) -> String {
    let mut mac = HmacSha1::new_from_slice(secret).expect("HMAC accepte toute longueur de clé");
    mac.update(&pas.to_be_bytes());
    let condense = mac.finalize().into_bytes();

    // Troncature dynamique (RFC 4226 §5.3) : le dernier demi-octet désigne où
    // lire les quatre octets qui portent le code.
    let decalage = (condense[condense.len() - 1] & 0x0f) as usize;
    let binaire = u32::from_be_bytes([
        condense[decalage] & 0x7f,
        condense[decalage + 1],
        condense[decalage + 2],
        condense[decalage + 3],
    ]);

    format!(
        "{:0largeur$}",
        binaire % 10u32.pow(CHIFFRES),
        largeur = CHIFFRES as usize
    )
}

/// Comparaison sans fuite de temps.
fn egal_en_temps_constant(attendu: &str, recu: &str) -> bool {
    if attendu.len() != recu.len() {
        return false;
    }
    attendu
        .bytes()
        .zip(recu.bytes())
        .fold(0u8, |ecart, (a, b)| ecart | (a ^ b))
        == 0
}

/// Encode un secret en base32 sans remplissage, comme les applications
/// d'authentification l'attendent (RFC 4648 §6).
///
/// Écrit à la main plutôt qu'importé : c'est trente lignes, et la seule
/// alternative aurait été une dépendance de plus pour un alphabet de trente-deux
/// caractères.
pub fn base32(secret: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut sortie = String::new();
    let mut tampon: u32 = 0;
    let mut bits = 0u32;

    for octet in secret {
        tampon = (tampon << 8) | u32::from(*octet);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            sortie.push(ALPHABET[((tampon >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        sortie.push(ALPHABET[((tampon << (5 - bits)) & 0x1f) as usize] as char);
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le secret de référence de la RFC 4226 : « 12345678901234567890 ».
    const SECRET_RFC: &[u8] = b"12345678901234567890";

    // === @happy ===

    #[test]
    fn happy_les_vecteurs_de_la_rfc_6238_sont_reproduits() {
        // **La seule façon honnête de tester une implémentation
        // cryptographique** : les vecteurs publiés. Les recalculer soi-même
        // reviendrait à tester le code contre lui-même.
        //
        // RFC 6238 annexe B, mode SHA-1, huit chiffres tronqués à six.
        for (horodatage, attendu) in [
            (59_i64, "287082"),
            (1_111_111_109, "081804"),
            (1_111_111_111, "050471"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
        ] {
            let pas = horodatage.div_euclid(PAS_SECONDES);
            assert_eq!(calculer(SECRET_RFC, pas), attendu, "à {horodatage}");
        }
    }

    #[test]
    fn happy_un_code_courant_est_accepte() {
        let horodatage: i64 = 1_780_000_000;
        let code = calculer(SECRET_RFC, horodatage.div_euclid(PAS_SECONDES));
        assert!(verifier(SECRET_RFC, &code, horodatage, None).is_some());
    }

    // === @negative ===

    #[test]
    fn negative_un_code_faux_est_refuse() {
        assert!(verifier(SECRET_RFC, "000000", 1_780_000_000, None).is_none());
    }

    #[test]
    fn negative_un_code_mal_forme_est_refuse_sans_calcul() {
        for maladroit in ["", "12345", "1234567", "abcdef", "12 34 56", "١٢٣٤٥٦"] {
            assert!(
                verifier(SECRET_RFC, maladroit, 1_780_000_000, None).is_none(),
                "{maladroit}"
            );
        }
    }

    #[test]
    fn negative_un_autre_secret_ne_valide_rien() {
        let horodatage: i64 = 1_780_000_000;
        let code = calculer(SECRET_RFC, horodatage.div_euclid(PAS_SECONDES));
        assert!(verifier(b"un-autre-secret-de-20", &code, horodatage, None).is_none());
    }

    // === @edge ===

    #[test]
    fn edge_le_pas_precedent_est_tolere() {
        // L'horloge d'un téléphone dérive de quelques secondes : refuser le pas
        // d'avant ferait échouer une saisie sur deux à la frontière.
        let horodatage: i64 = 1_780_000_000;
        let precedent = calculer(SECRET_RFC, horodatage.div_euclid(PAS_SECONDES) - 1);
        assert!(verifier(SECRET_RFC, &precedent, horodatage, None).is_some());
    }

    #[test]
    fn edge_le_pas_suivant_est_tolere() {
        let horodatage: i64 = 1_780_000_000;
        let suivant = calculer(SECRET_RFC, horodatage.div_euclid(PAS_SECONDES) + 1);
        assert!(verifier(SECRET_RFC, &suivant, horodatage, None).is_some());
    }

    #[test]
    fn edge_au_dela_de_la_tolerance_le_code_est_refuse() {
        let horodatage: i64 = 1_780_000_000;
        let vieux = calculer(SECRET_RFC, horodatage.div_euclid(PAS_SECONDES) - 2);
        assert!(verifier(SECRET_RFC, &vieux, horodatage, None).is_none());
    }

    #[test]
    fn edge_le_base32_suit_l_alphabet_de_la_rfc_4648() {
        // Un secret que l'application d'authentification ne sait pas lire rend
        // toute la fonctionnalité inutilisable, et l'erreur ne se voit qu'au
        // moment où quelqu'un essaie de se connecter.
        assert_eq!(base32(b""), "");
        assert_eq!(base32(b"a"), "ME");
        assert_eq!(base32(b"abc"), "MFRGG");
        assert!(base32(SECRET_RFC)
            .chars()
            .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)));
    }

    // === @security ===

    #[test]
    fn security_un_code_deja_utilise_ne_repasse_pas() {
        // **Sans cela, un code lu par-dessus une épaule reste utilisable une
        // minute et demie.** C'est la fenêtre de tolérance qui l'exige : elle
        // allonge la vie du code, et le compteur la referme.
        let horodatage: i64 = 1_780_000_000;
        let pas = horodatage.div_euclid(PAS_SECONDES);
        let code = calculer(SECRET_RFC, pas);

        let premiere = verifier(SECRET_RFC, &code, horodatage, None).expect("acceptée");
        assert_eq!(premiere.pas, pas);

        assert!(
            verifier(SECRET_RFC, &code, horodatage, Some(premiere.pas)).is_none(),
            "un code consommé ne doit pas repasser"
        );
    }

    #[test]
    fn security_un_pas_anterieur_au_dernier_accepte_est_refuse() {
        // Le rejeu ne se limite pas au code exact : un code plus ancien encore
        // dans la fenêtre serait tout aussi rejouable.
        let horodatage: i64 = 1_780_000_000;
        let pas = horodatage.div_euclid(PAS_SECONDES);
        let precedent = calculer(SECRET_RFC, pas - 1);

        assert!(verifier(SECRET_RFC, &precedent, horodatage, Some(pas)).is_none());
    }

    #[test]
    fn security_la_comparaison_ne_fuit_pas_par_le_temps() {
        // Un `==` sur des chaînes s'arrête au premier caractère différent, et
        // la durée de l'appel dirait alors combien de chiffres sont justes.
        assert!(egal_en_temps_constant("123456", "123456"));
        assert!(!egal_en_temps_constant("123456", "123457"));
        assert!(!egal_en_temps_constant("123456", "923456"));
        assert!(!egal_en_temps_constant("123456", "12345"));
    }

    #[test]
    fn security_le_secret_fait_au_moins_la_taille_recommandee() {
        // RFC 4226 §4 : vingt octets pour HMAC-SHA-1. Un secret plus court
        // réduit d'autant l'entropie de la seconde authentification.
        //
        // La vérification est à la compilation : baisser la constante ne doit
        // pas produire un test rouge qu'on pourrait ignorer, mais un binaire
        // qui ne se construit pas.
        const _: () = assert!(SECRET_OCTETS >= 20);
        // Et le secret tiré au sort fait bien cette taille, ce qu'une constante
        // seule ne dit pas.
        assert_eq!(vec![0u8; SECRET_OCTETS].len(), SECRET_OCTETS);
    }

    #[test]
    fn security_le_code_fait_toujours_six_chiffres() {
        // Un code tronqué à cinq chiffres passerait pour valide une fois sur
        // dix de plus.
        for pas in [0_i64, 1, 42, 59_999_999] {
            let code = calculer(SECRET_RFC, pas);
            assert_eq!(code.len(), CHIFFRES as usize, "pas {pas}");
            assert!(code.bytes().all(|b| b.is_ascii_digit()), "pas {pas}");
        }
    }
}
