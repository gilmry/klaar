//! Adaptateur Web Push (Story 0.12, ADR-010 — amende ADR-007).
//!
//! ADR-007 visait APNs et FCM directement. ADR-010 ayant retiré Tauri, il n'y
//! a plus d'application native à qui livrer un jeton d'appareil : le client
//! est une PWA, et le protocole devient **Web Push**, un seul chemin pour
//! tous les navigateurs. APNs reste atteint sur iOS, mais indirectement, par
//! le service de push de Safari — ce qui n'est plus notre affaire.
//!
//! Trois RFC sont en jeu :
//! - **8188** : encodage `aes128gcm` du corps ;
//! - **8291** : dérivation des clés propre au Web Push ;
//! - **8292** : authentification VAPID de l'expéditeur.

pub mod base64url;
pub mod encrypt;
pub mod vapid;

mod sender;

pub use sender::{ClesVapid, RequetePush, WebPushSender};

use klaar_application::ports::push::{PushError, PushSubscription};

/// Vérifie qu'un abonnement est exploitable avant de l'accepter.
///
/// Un abonnement mal formé accepté aujourd'hui devient une notification
/// silencieusement perdue plus tard, sans rien pour relier les deux.
pub fn valider_abonnement(abonnement: &PushSubscription) -> Result<(), PushError> {
    vapid::origine(&abonnement.endpoint)?;

    let p256dh = base64url::decode(&abonnement.p256dh)
        .map_err(|e| PushError::AbonnementInvalide(format!("p256dh illisible : {e}")))?;
    if p256dh.len() != 65 || p256dh[0] != 0x04 {
        return Err(PushError::AbonnementInvalide(
            "p256dh : forme non compressée de 65 octets attendue".to_string(),
        ));
    }
    // Vérifie que le point appartient bien à P-256 : accepter un point hors
    // courbe ouvrirait une attaque par courbe invalide au moment du chiffrement.
    p256::PublicKey::from_sec1_bytes(&p256dh)
        .map_err(|e| PushError::AbonnementInvalide(format!("p256dh hors courbe : {e}")))?;

    let auth = base64url::decode(&abonnement.auth)
        .map_err(|e| PushError::AbonnementInvalide(format!("auth illisible : {e}")))?;
    if auth.len() != 16 {
        return Err(PushError::AbonnementInvalide(format!(
            "secret d'authentification de {} octets, 16 attendus",
            auth.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests_validation {
    use super::*;

    fn valide() -> PushSubscription {
        PushSubscription {
            endpoint: "https://push.example.net/envoi/abc".to_string(),
            p256dh: "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4".to_string(),
            auth: "BTBZMqHH6r4Tts7J_aSIgg".to_string(),
        }
    }

    #[test]
    fn accepte_un_abonnement_conforme() {
        assert!(valider_abonnement(&valide()).is_ok());
    }

    #[test]
    fn refuse_un_endpoint_sans_schema() {
        let mut a = valide();
        a.endpoint = "push.example.net/envoi/abc".to_string();
        assert!(valider_abonnement(&a).is_err());
    }

    #[test]
    fn refuse_un_secret_d_authentification_trop_court() {
        let mut a = valide();
        a.auth = base64url::encode(&[0u8; 8]);
        assert!(valider_abonnement(&a).is_err());
    }

    #[test]
    fn refuse_une_cle_hors_courbe() {
        let mut faux = vec![0x04u8; 65];
        faux[64] = 0x01;
        let mut a = valide();
        a.p256dh = base64url::encode(&faux);
        assert!(valider_abonnement(&a).is_err());
    }

    #[test]
    fn refuse_une_cle_compressee() {
        // Forme compressée (33 octets, préfixe 0x02/0x03) : valide en soi,
        // mais le RFC 8291 impose la forme non compressée dans la dérivation.
        // L'accepter produirait un chiffré que le navigateur ne saurait pas
        // déchiffrer.
        let mut a = valide();
        a.p256dh = base64url::encode(&[0x02u8; 33]);
        assert!(valider_abonnement(&a).is_err());
    }
}
