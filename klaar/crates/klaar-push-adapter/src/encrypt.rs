//! Chiffrement du contenu d'un message push : RFC 8291 (Web Push Message
//! Encryption) au-dessus de RFC 8188 (`aes128gcm`).
//!
//! Le protocole est assemblé ici plutôt que délégué à une bibliothèque
//! dédiée. Ce choix ne serait pas défendable sans preuve : chaque valeur
//! intermédiaire de la dérivation est comparée à celles publiées en annexe A
//! du RFC 8291, ce qu'une bibliothèque qui ne les expose pas ne permettrait
//! pas de vérifier. Voir les tests en fin de fichier.
//!
//! Enchaînement, pour qui relit sans le RFC sous les yeux :
//!
//! 1. ECDH entre la clé éphémère de l'expéditeur et la clé publique du
//!    navigateur → `ecdh_secret` ;
//! 2. HKDF avec le secret d'authentification de l'abonnement pour lier la
//!    clé à cet abonnement précis → `IKM` ;
//! 3. HKDF avec le sel aléatoire → clé de chiffrement et nonce ;
//! 4. AES-128-GCM sur le contenu suffixé du délimiteur de remplissage `0x02` ;
//! 5. en-tête RFC 8188 (sel, taille d'enregistrement, clé publique) concaténé
//!    au chiffré.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Nonce};
use hkdf::Hkdf;
use p256::ecdh::diffie_hellman;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use sha2::Sha256;

use klaar_application::ports::push::PushError;

/// Taille d'enregistrement annoncée dans l'en-tête. 4096 est la valeur de
/// l'exemple du RFC et couvre largement une notification.
const TAILLE_ENREGISTREMENT: u32 = 4096;

/// Longueur du secret d'authentification, imposée par le RFC 8291 §3.2.
const LONGUEUR_AUTH: usize = 16;

/// Longueur d'une clé publique P-256 sous forme non compressée : `0x04`
/// suivi des deux coordonnées de 32 octets.
const LONGUEUR_CLE_PUBLIQUE: usize = 65;

fn crypto(detail: impl Into<String>) -> PushError {
    PushError::Cryptographie(detail.into())
}

/// Dérive `IKM` à partir du secret ECDH et du contexte de l'abonnement.
///
/// C'est l'étape propre au Web Push : sans elle, la clé ne dépendrait que du
/// couple de clés, et un abonnement volé sur un autre appareil déchiffrerait
/// les messages. `key_info` lie la dérivation aux deux clés publiques.
fn deriver_ikm(
    ecdh_secret: &[u8],
    auth_secret: &[u8],
    ua_public: &[u8],
    as_public: &[u8],
) -> [u8; 32] {
    let mut key_info = Vec::with_capacity(14 + 1 + LONGUEUR_CLE_PUBLIQUE * 2);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(ua_public);
    key_info.extend_from_slice(as_public);

    let hk = Hkdf::<Sha256>::new(Some(auth_secret), ecdh_secret);
    let mut ikm = [0u8; 32];
    hk.expand(&key_info, &mut ikm)
        .expect("32 octets est une longueur valide pour HKDF-SHA256");
    ikm
}

/// Dérive la clé de contenu et le nonce à partir de `IKM` et du sel.
fn deriver_cle_et_nonce(ikm: &[u8], salt: &[u8]) -> ([u8; 16], [u8; 12]) {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut cek = [0u8; 16];
    let mut nonce = [0u8; 12];
    hk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .expect("16 octets est une longueur valide");
    hk.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .expect("12 octets est une longueur valide");
    (cek, nonce)
}

/// En-tête RFC 8188 : sel (16), taille d'enregistrement (4, gros-boutiste),
/// longueur de la clé (1), clé publique de l'expéditeur (65).
fn entete(salt: &[u8], as_public: &[u8]) -> Vec<u8> {
    let mut h = Vec::with_capacity(16 + 4 + 1 + LONGUEUR_CLE_PUBLIQUE);
    h.extend_from_slice(salt);
    h.extend_from_slice(&TAILLE_ENREGISTREMENT.to_be_bytes());
    h.push(as_public.len() as u8);
    h.extend_from_slice(as_public);
    h
}

/// Résultat d'un chiffrement : le corps à poster tel quel.
#[derive(Debug)]
pub struct ContenuChiffre {
    pub corps: Vec<u8>,
}

/// Chiffre `contenu` pour un abonnement, avec un sel et une clé éphémère
/// fournis. Séparé de [`chiffrer`] pour que les tests puissent rejouer les
/// vecteurs du RFC, qui fixent ces deux valeurs aléatoires.
pub fn chiffrer_avec(
    contenu: &[u8],
    ua_public_octets: &[u8],
    auth_secret: &[u8],
    as_secret: &SecretKey,
    salt: &[u8; 16],
) -> Result<ContenuChiffre, PushError> {
    if auth_secret.len() != LONGUEUR_AUTH {
        return Err(PushError::AbonnementInvalide(format!(
            "secret d'authentification de {} octets, {LONGUEUR_AUTH} attendus",
            auth_secret.len()
        )));
    }
    if ua_public_octets.len() != LONGUEUR_CLE_PUBLIQUE || ua_public_octets[0] != 0x04 {
        return Err(PushError::AbonnementInvalide(
            "clé p256dh : forme non compressée de 65 octets attendue".to_string(),
        ));
    }

    let ua_public = PublicKey::from_sec1_bytes(ua_public_octets)
        .map_err(|e| PushError::AbonnementInvalide(format!("clé p256dh invalide : {e}")))?;
    let as_public_point = as_secret.public_key().to_encoded_point(false);
    let as_public_octets = as_public_point.as_bytes();

    let partage = diffie_hellman(as_secret.to_nonzero_scalar(), ua_public.as_affine());
    let ikm = deriver_ikm(
        &partage.raw_secret_bytes()[..],
        auth_secret,
        ua_public_octets,
        as_public_octets,
    );
    let (cek, nonce) = deriver_cle_et_nonce(&ikm, salt);

    // Délimiteur de remplissage du dernier (et unique) enregistrement.
    let mut clair = contenu.to_vec();
    clair.push(0x02);

    let en_tete = entete(salt, as_public_octets);
    let chiffreur = Aes128Gcm::new_from_slice(&cek).map_err(|e| crypto(e.to_string()))?;
    let chiffre = chiffreur
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: &clair,
                aad: b"",
            },
        )
        .map_err(|e| crypto(e.to_string()))?;

    let mut corps = en_tete;
    corps.extend_from_slice(&chiffre);
    Ok(ContenuChiffre { corps })
}

/// Chiffre `contenu` en tirant le sel et la clé éphémère au hasard.
///
/// Une clé éphémère par message est ce que demande le RFC : la réutiliser
/// rendrait deux messages corrélables et, avec un sel constant, réutiliserait
/// un nonce AES-GCM — ce qui casse la confidentialité, pas seulement
/// l'élégance.
pub fn chiffrer(
    contenu: &[u8],
    ua_public_octets: &[u8],
    auth_secret: &[u8],
) -> Result<ContenuChiffre, PushError> {
    use rand_core::RngCore;
    let mut rng = rand_core::OsRng;
    let as_secret = SecretKey::random(&mut rng);
    let mut salt = [0u8; 16];
    rng.fill_bytes(&mut salt);
    chiffrer_avec(contenu, ua_public_octets, auth_secret, &as_secret, &salt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base64url;

    // Vecteurs de la section 5 et de l'annexe A du RFC 8291. Les reproduire
    // octet pour octet est ce qui distingue une implémentation vérifiée d'une
    // implémentation qui « a l'air de marcher » : une erreur de dérivation
    // produit un chiffré parfaitement bien formé, que seul le navigateur
    // destinataire rejetterait, silencieusement.
    const AUTH_SECRET: &str = "BTBZMqHH6r4Tts7J_aSIgg";
    const UA_PUBLIC: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const AS_PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
    const AS_PUBLIC: &str =
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    const SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
    const CLAIR: &[u8] = b"When I grow up, I want to be a watermelon";

    const ECDH_SECRET: &str = "kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs";
    const IKM: &str = "S4lYMb_L0FxCeq0WhDx813KgSYqU26kOyzWUdsXYyrg";
    const CEK: &str = "oIhVW04MRdy2XN9CiKLxTg";
    const NONCE: &str = "4h_95klXJ5E_qnoN";
    const CORPS_ATTENDU: &str = concat!(
        "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27ml",
        "mlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPT",
        "pK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
    );

    fn as_secret() -> SecretKey {
        SecretKey::from_slice(&base64url::decode(AS_PRIVATE).unwrap()).unwrap()
    }

    #[test]
    fn ecdh_reproduit_le_secret_partage_du_rfc() {
        let ua = PublicKey::from_sec1_bytes(&base64url::decode(UA_PUBLIC).unwrap()).unwrap();
        let partage = diffie_hellman(as_secret().to_nonzero_scalar(), ua.as_affine());
        assert_eq!(
            base64url::encode(&partage.raw_secret_bytes()[..]),
            ECDH_SECRET
        );
    }

    #[test]
    fn derive_l_ikm_du_rfc() {
        let ua = base64url::decode(UA_PUBLIC).unwrap();
        let a_s = base64url::decode(AS_PUBLIC).unwrap();
        let ua_pk = PublicKey::from_sec1_bytes(&ua).unwrap();
        let partage = diffie_hellman(as_secret().to_nonzero_scalar(), ua_pk.as_affine());
        let ikm = deriver_ikm(
            &partage.raw_secret_bytes()[..],
            &base64url::decode(AUTH_SECRET).unwrap(),
            &ua,
            &a_s,
        );
        assert_eq!(base64url::encode(&ikm), IKM);
    }

    #[test]
    fn derive_la_cle_et_le_nonce_du_rfc() {
        let (cek, nonce) = deriver_cle_et_nonce(
            &base64url::decode(IKM).unwrap(),
            &base64url::decode(SALT).unwrap(),
        );
        assert_eq!(base64url::encode(&cek), CEK);
        assert_eq!(base64url::encode(&nonce), NONCE);
    }

    #[test]
    fn chiffre_le_message_exemple_du_rfc_octet_pour_octet() {
        let salt: [u8; 16] = base64url::decode(SALT).unwrap().try_into().unwrap();
        let resultat = chiffrer_avec(
            CLAIR,
            &base64url::decode(UA_PUBLIC).unwrap(),
            &base64url::decode(AUTH_SECRET).unwrap(),
            &as_secret(),
            &salt,
        )
        .unwrap();
        assert_eq!(base64url::encode(&resultat.corps), CORPS_ATTENDU);
    }

    #[test]
    fn refuse_un_secret_d_authentification_de_mauvaise_longueur() {
        let erreur = chiffrer(CLAIR, &base64url::decode(UA_PUBLIC).unwrap(), b"trop court")
            .expect_err("un secret de 10 octets doit être refusé");
        assert!(matches!(erreur, PushError::AbonnementInvalide(_)));
    }

    #[test]
    fn refuse_une_cle_publique_qui_n_est_pas_sur_la_courbe() {
        // 65 octets bien formés en apparence, mais dont le point n'appartient
        // pas à P-256. L'accepter exposerait à une attaque par courbe
        // invalide, qui permet de retrouver la clé privée.
        let mut faux = vec![0x04u8; 65];
        faux[64] = 0x01;
        let erreur = chiffrer(CLAIR, &faux, &base64url::decode(AUTH_SECRET).unwrap())
            .expect_err("un point hors courbe doit être refusé");
        assert!(matches!(erreur, PushError::AbonnementInvalide(_)));
    }

    #[test]
    fn tire_un_sel_et_une_cle_ephemere_differents_a_chaque_appel() {
        // Réutiliser le couple (clé, sel) réutiliserait le nonce AES-GCM, ce
        // qui casse la confidentialité, pas seulement la forme.
        let ua = base64url::decode(UA_PUBLIC).unwrap();
        let auth = base64url::decode(AUTH_SECRET).unwrap();
        let a = chiffrer(CLAIR, &ua, &auth).unwrap().corps;
        let b = chiffrer(CLAIR, &ua, &auth).unwrap().corps;
        assert_ne!(a[..16], b[..16], "le sel doit changer");
        assert_ne!(a[21..86], b[21..86], "la clé éphémère doit changer");
    }
}
