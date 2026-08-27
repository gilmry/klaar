//! Authentification VAPID (RFC 8292) : l'expéditeur se déclare auprès du
//! service de push en signant un JWT avec une clé qu'il contrôle.
//!
//! Ce que ça apporte concrètement : le service de push peut identifier
//! l'origine d'un envoi et joindre son responsable en cas d'abus. Ce que ça
//! n'apporte pas : aucune confidentialité — c'est le rôle du chiffrement de
//! contenu (voir `encrypt`), qui est indépendant.

use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::SecretKey;
use serde::Serialize;

use klaar_application::ports::push::PushError;

use crate::base64url;

/// Durée de validité du jeton. Le RFC 8292 §2 plafonne à 24 h ; on reste
/// nettement en dessous, un jeton n'ayant pas à survivre à l'envoi qu'il
/// autorise.
const VALIDITE_SECONDES: u64 = 12 * 3600;

/// Le RFC 8292 §2 plafonne la validité d'un jeton à 24 h : au-delà, le service
/// de push rejette l'envoi. Vérifié à la compilation plutôt que par un test,
/// pour que baisser la garde exige de toucher cette ligne.
const _: () = assert!(VALIDITE_SECONDES <= 24 * 3600);

#[derive(Serialize)]
struct Entete {
    typ: &'static str,
    alg: &'static str,
}

#[derive(Serialize)]
struct Revendications<'a> {
    /// Origine du service de push, schéma et hôte seulement.
    aud: &'a str,
    /// Expiration, en secondes depuis l'époque Unix.
    exp: u64,
    /// Moyen de joindre le responsable de l'envoi : `mailto:` ou `https:`.
    sub: &'a str,
}

/// Extrait l'origine (`schéma://hôte`) d'une URL d'abonnement.
///
/// Le RFC 8292 impose que `aud` soit l'origine, pas l'URL complète. Y laisser
/// le chemin fait rejeter le jeton par certains services de push, avec un 401
/// dont le message n'explique rien.
pub fn origine(endpoint: &str) -> Result<String, PushError> {
    let (schema, reste) = endpoint.split_once("://").ok_or_else(|| {
        PushError::AbonnementInvalide(format!("endpoint sans schéma : {endpoint}"))
    })?;
    let hote = reste.split('/').next().unwrap_or_default();
    if hote.is_empty() {
        return Err(PushError::AbonnementInvalide(format!(
            "endpoint sans hôte : {endpoint}"
        )));
    }
    Ok(format!("{schema}://{hote}"))
}

/// Construit l'en-tête `Authorization` d'un envoi Web Push.
///
/// `maintenant` est passé plutôt que lu de l'horloge : c'est ce qui rend
/// l'expiration testable.
pub fn entete_authorization(
    cle_privee: &SecretKey,
    endpoint: &str,
    sujet: &str,
    maintenant: u64,
) -> Result<String, PushError> {
    let aud = origine(endpoint)?;
    let jeton = signer_jwt(cle_privee, &aud, sujet, maintenant + VALIDITE_SECONDES)?;
    let cle_publique = cle_privee.public_key().to_encoded_point(false);
    Ok(format!(
        "vapid t={jeton}, k={}",
        base64url::encode(cle_publique.as_bytes())
    ))
}

/// Signe un JWT ES256. La signature est la concaténation brute `r || s` de
/// 64 octets, et non un DER : c'est ce que demande JWS, et le confondre
/// produit un jeton rejeté sans explication utile.
pub fn signer_jwt(
    cle_privee: &SecretKey,
    aud: &str,
    sub: &str,
    exp: u64,
) -> Result<String, PushError> {
    let entete = serde_json::to_vec(&Entete {
        typ: "JWT",
        alg: "ES256",
    })
    .map_err(|e| PushError::Cryptographie(e.to_string()))?;
    let revendications = serde_json::to_vec(&Revendications { aud, exp, sub })
        .map_err(|e| PushError::Cryptographie(e.to_string()))?;

    let corps = format!(
        "{}.{}",
        base64url::encode(&entete),
        base64url::encode(&revendications)
    );

    let signataire = SigningKey::from(cle_privee);
    let signature: Signature = signataire.sign(corps.as_bytes());
    Ok(format!(
        "{corps}.{}",
        base64url::encode(&signature.to_bytes())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::VerifyingKey;

    fn cle() -> SecretKey {
        SecretKey::from_slice(
            &base64url::decode("yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw").unwrap(),
        )
        .unwrap()
    }

    fn parties(jwt: &str) -> (serde_json::Value, serde_json::Value, Vec<u8>) {
        let p: Vec<&str> = jwt.split('.').collect();
        assert_eq!(p.len(), 3, "un JWT a trois parties");
        (
            serde_json::from_slice(&base64url::decode(p[0]).unwrap()).unwrap(),
            serde_json::from_slice(&base64url::decode(p[1]).unwrap()).unwrap(),
            base64url::decode(p[2]).unwrap(),
        )
    }

    #[test]
    fn produit_un_jwt_es256_dont_la_signature_se_verifie() {
        let jwt = signer_jwt(
            &cle(),
            "https://push.example.net",
            "mailto:ops@klaar.be",
            1_800_000_000,
        )
        .unwrap();
        let (entete, revendications, signature) = parties(&jwt);

        assert_eq!(entete["alg"], "ES256");
        assert_eq!(entete["typ"], "JWT");
        assert_eq!(revendications["aud"], "https://push.example.net");
        assert_eq!(revendications["sub"], "mailto:ops@klaar.be");
        assert_eq!(revendications["exp"], 1_800_000_000u64);

        // JWS exige r || s brut, 64 octets. Un DER en ferait 70 à 72 et serait
        // rejeté par le service de push.
        assert_eq!(signature.len(), 64, "la signature doit être au format brut");

        let verificateur = VerifyingKey::from(cle().public_key());
        let corps = jwt.rsplit_once('.').unwrap().0;
        verificateur
            .verify(
                corps.as_bytes(),
                &Signature::from_slice(&signature).unwrap(),
            )
            .expect("la signature doit se vérifier avec la clé publique correspondante");
    }

    #[test]
    fn une_signature_ne_se_verifie_pas_avec_une_autre_cle() {
        let jwt = signer_jwt(&cle(), "https://push.example.net", "mailto:ops@klaar.be", 1).unwrap();
        let (_, _, signature) = parties(&jwt);
        let autre = SecretKey::random(&mut rand_core::OsRng);
        let verificateur = VerifyingKey::from(autre.public_key());
        let corps = jwt.rsplit_once('.').unwrap().0;
        assert!(verificateur
            .verify(
                corps.as_bytes(),
                &Signature::from_slice(&signature).unwrap()
            )
            .is_err());
    }

    #[test]
    fn l_audience_est_l_origine_et_non_l_url_complete() {
        assert_eq!(
            origine("https://fcm.googleapis.com/fcm/send/abc123?x=1").unwrap(),
            "https://fcm.googleapis.com"
        );
        assert_eq!(
            origine("https://web.push.apple.com/QDEF456").unwrap(),
            "https://web.push.apple.com"
        );
    }

    #[test]
    fn refuse_un_endpoint_mal_forme() {
        assert!(origine("pas-une-url").is_err());
        assert!(origine("https:///chemin-sans-hote").is_err());
    }

    #[test]
    fn l_entete_authorization_porte_le_jeton_et_la_cle_publique() {
        let entete = entete_authorization(
            &cle(),
            "https://push.example.net/x/y",
            "mailto:ops@klaar.be",
            1_700_000_000,
        )
        .unwrap();
        assert!(entete.starts_with("vapid t="));
        let k = entete
            .split(", k=")
            .nth(1)
            .expect("la clé publique doit être jointe");
        // Le service de push s'en sert pour vérifier la signature : elle doit
        // être la forme non compressée de 65 octets.
        assert_eq!(base64url::decode(k).unwrap().len(), 65);
    }

    #[test]
    fn l_expiration_est_bornee_a_douze_heures() {
        let entete = entete_authorization(
            &cle(),
            "https://push.example.net",
            "mailto:ops@klaar.be",
            1_000,
        )
        .unwrap();
        let jwt = entete
            .trim_start_matches("vapid t=")
            .split(", k=")
            .next()
            .unwrap();
        let (_, revendications, _) = parties(jwt);
        assert_eq!(revendications["exp"], 1_000 + VALIDITE_SECONDES);
    }
}
