//! Envoi effectif vers le service de push du navigateur.

use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::SecretKey;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

use klaar_application::ports::push::{PushError, PushMessage, PushSubscription};

use crate::{base64url, encrypt, vapid};

/// Durée de rétention demandée au service de push, en secondes. Une alerte de
/// dépannage n'a plus d'intérêt une heure plus tard : la livrer à retardement
/// ferait sonner le téléphone d'un prestataire pour une Demande déjà prise.
const TTL_SECONDES: u32 = 3600;

/// Couple de clés VAPID de l'application.
pub struct ClesVapid {
    privee: SecretKey,
    /// Moyen de joindre le responsable des envois (`mailto:` ou `https:`),
    /// exigé par le RFC 8292.
    sujet: String,
}

impl ClesVapid {
    /// Charge la clé privée depuis sa forme base64url (32 octets).
    pub fn depuis_base64url(cle_privee: &str, sujet: impl Into<String>) -> Result<Self, PushError> {
        let octets = base64url::decode(cle_privee)
            .map_err(|e| PushError::Cryptographie(format!("clé VAPID illisible : {e}")))?;
        let privee = SecretKey::from_slice(&octets)
            .map_err(|e| PushError::Cryptographie(format!("clé VAPID invalide : {e}")))?;
        let sujet = sujet.into();
        if !sujet.starts_with("mailto:") && !sujet.starts_with("https://") {
            return Err(PushError::Cryptographie(
                "le sujet VAPID doit être un mailto: ou un https:".to_string(),
            ));
        }
        Ok(Self { privee, sujet })
    }

    /// Génère un couple neuf. Destiné à l'amorçage d'un environnement, pas à
    /// être appelé au démarrage : changer de clé invalide **tous** les
    /// abonnements existants, que les navigateurs ont liés à la clé publique
    /// qu'on leur a donnée.
    pub fn generer(sujet: impl Into<String>) -> Result<(Self, String, String), PushError> {
        let privee = SecretKey::random(&mut rand_core::OsRng);
        let privee_b64 = base64url::encode(&privee.to_bytes());
        let publique_b64 =
            base64url::encode(privee.public_key().to_encoded_point(false).as_bytes());
        let cles = Self::depuis_base64url(&privee_b64, sujet)?;
        Ok((cles, privee_b64, publique_b64))
    }

    /// Clé publique à transmettre au navigateur (`applicationServerKey`).
    pub fn cle_publique_base64url(&self) -> String {
        base64url::encode(self.privee.public_key().to_encoded_point(false).as_bytes())
    }
}

/// Corps et en-têtes prêts à poster.
pub type RequetePush = (Vec<u8>, Vec<(&'static str, String)>);

#[derive(Serialize)]
struct Charge<'a> {
    titre: &'a str,
    corps: &'a str,
    url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<&'a str>,
}

pub struct WebPushSender {
    cles: ClesVapid,
}

impl WebPushSender {
    pub fn new(cles: ClesVapid) -> Self {
        Self { cles }
    }

    pub fn cle_publique_base64url(&self) -> String {
        self.cles.cle_publique_base64url()
    }

    /// Prépare la requête sans l'émettre : corps chiffré et en-têtes.
    ///
    /// Séparé de l'envoi pour être testable sans réseau — c'est là que vit
    /// tout ce qui peut être faux.
    pub fn preparer(
        &self,
        abonnement: &PushSubscription,
        message: &PushMessage,
        maintenant: u64,
    ) -> Result<RequetePush, PushError> {
        let p256dh = base64url::decode(&abonnement.p256dh)
            .map_err(|e| PushError::AbonnementInvalide(format!("p256dh illisible : {e}")))?;
        let auth = base64url::decode(&abonnement.auth)
            .map_err(|e| PushError::AbonnementInvalide(format!("auth illisible : {e}")))?;

        let charge = serde_json::to_vec(&Charge {
            titre: &message.titre,
            corps: &message.corps,
            url: &message.url,
            tag: message.tag.as_deref(),
        })
        .map_err(|e| PushError::Cryptographie(e.to_string()))?;

        let chiffre = encrypt::chiffrer(&charge, &p256dh, &auth)?;
        let autorisation = vapid::entete_authorization(
            &self.cles.privee,
            &abonnement.endpoint,
            &self.cles.sujet,
            maintenant,
        )?;

        let entetes = vec![
            ("Authorization", autorisation),
            ("Content-Encoding", "aes128gcm".to_string()),
            ("Content-Type", "application/octet-stream".to_string()),
            ("TTL", TTL_SECONDES.to_string()),
            // Sans « Urgency », certains services de push retardent la
            // livraison pour économiser la batterie. Une Demande de dépannage
            // ne supporte pas ce délai.
            ("Urgency", "high".to_string()),
        ];
        Ok((chiffre.corps, entetes))
    }

    /// Envoie le message. Un 404 ou un 410 remonte en
    /// [`PushError::AbonnementExpire`] : l'appelant doit alors supprimer
    /// l'abonnement, faute de quoi il conserve une donnée personnelle inutile
    /// et réessaie sans fin.
    pub async fn envoyer(
        &self,
        abonnement: &PushSubscription,
        message: &PushMessage,
    ) -> Result<(), PushError> {
        let maintenant = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| PushError::Cryptographie(e.to_string()))?
            .as_secs();
        let (corps, entetes) = self.preparer(abonnement, message, maintenant)?;

        let client = awc::Client::default();
        let mut requete = client.post(&abonnement.endpoint);
        for (nom, valeur) in entetes {
            requete = requete.insert_header((nom, valeur));
        }

        let mut reponse = requete
            .send_body(corps)
            .await
            .map_err(|e| PushError::Transport(e.to_string()))?;

        let status = reponse.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        if status == 404 || status == 410 {
            return Err(PushError::AbonnementExpire);
        }
        let corps_reponse = reponse
            .body()
            .await
            .map(|b| String::from_utf8_lossy(&b).chars().take(500).collect())
            .unwrap_or_default();
        Err(PushError::ServiceDePush {
            status,
            corps: corps_reponse,
        })
    }
}

/// L'adaptateur satisfait le port de la couche Application.
///
/// L'implémentation ne fait que déléguer : la méthode inhérente existait avant
/// le port, et les deux signatures coïncident maintenant que le port est
/// asynchrone.
impl klaar_application::ports::push::PushNotifier for WebPushSender {
    async fn envoyer(
        &self,
        abonnement: &klaar_application::ports::push::PushSubscription,
        message: &klaar_application::ports::push::PushMessage,
    ) -> Result<(), klaar_application::ports::push::PushError> {
        WebPushSender::envoyer(self, abonnement, message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UA_PUBLIC: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";

    fn abonnement() -> PushSubscription {
        PushSubscription {
            endpoint: "https://push.example.net/envoi/abc".to_string(),
            p256dh: UA_PUBLIC.to_string(),
            auth: AUTH.to_string(),
        }
    }

    fn message() -> PushMessage {
        PushMessage {
            titre: "Nouvelle Demande".to_string(),
            corps: "Plomberie, Saint-Gilles, à 1,2 km".to_string(),
            url: "/missions/M-1234".to_string(),
            tag: Some("demande-M-1234".to_string()),
        }
    }

    fn expediteur() -> WebPushSender {
        let (cles, _, _) = ClesVapid::generer("mailto:ops@klaar.be").unwrap();
        WebPushSender::new(cles)
    }

    #[test]
    fn prepare_un_corps_chiffre_et_les_entetes_attendus() {
        let (corps, entetes) = expediteur()
            .preparer(&abonnement(), &message(), 1_700_000_000)
            .unwrap();

        // 16 (sel) + 4 (taille) + 1 (longueur) + 65 (clé) = 86 octets d'en-tête.
        assert!(corps.len() > 86);
        let noms: Vec<&str> = entetes.iter().map(|(n, _)| *n).collect();
        assert!(noms.contains(&"Authorization"));
        assert!(noms.contains(&"TTL"));
        let encodage = entetes
            .iter()
            .find(|(n, _)| *n == "Content-Encoding")
            .unwrap();
        assert_eq!(encodage.1, "aes128gcm");
    }

    #[test]
    fn le_corps_en_clair_n_apparait_jamais_dans_la_requete() {
        // La charge utile contient l'adresse d'intervention : elle ne doit
        // exister en clair ni dans le corps ni dans un en-tête, le service de
        // push étant un tiers non contractant.
        let (corps, entetes) = expediteur()
            .preparer(&abonnement(), &message(), 1_700_000_000)
            .unwrap();
        let motif = b"Saint-Gilles";
        assert!(!corps.windows(motif.len()).any(|f| f == motif));
        for (_, valeur) in &entetes {
            assert!(!valeur.contains("Saint-Gilles"));
        }
    }

    #[test]
    fn deux_envois_du_meme_message_produisent_des_corps_differents() {
        let e = expediteur();
        let a = e
            .preparer(&abonnement(), &message(), 1_700_000_000)
            .unwrap()
            .0;
        let b = e
            .preparer(&abonnement(), &message(), 1_700_000_000)
            .unwrap()
            .0;
        assert_ne!(
            a, b,
            "chaque envoi doit tirer un sel et une clé éphémère neufs"
        );
    }

    #[test]
    fn refuse_un_abonnement_dont_les_cles_sont_illisibles() {
        let mut mauvais = abonnement();
        mauvais.p256dh = "pas du base64 !!".to_string();
        let erreur = expediteur()
            .preparer(&mauvais, &message(), 0)
            .expect_err("une clé illisible doit être refusée");
        assert!(matches!(erreur, PushError::AbonnementInvalide(_)));
    }

    #[test]
    fn refuse_un_sujet_vapid_qui_n_est_pas_joignable() {
        // Le RFC 8292 veut un moyen de contact. « klaar » n'en est pas un, et
        // un service de push qui ne peut joindre personne finit par bloquer
        // l'expéditeur.
        assert!(ClesVapid::generer("klaar").is_err());
        assert!(ClesVapid::generer("mailto:ops@klaar.be").is_ok());
        assert!(ClesVapid::generer("https://klaar.be/contact").is_ok());
    }

    #[test]
    fn la_cle_publique_exposee_correspond_a_la_cle_privee() {
        let (cles, privee_b64, publique_b64) = ClesVapid::generer("mailto:ops@klaar.be").unwrap();
        assert_eq!(cles.cle_publique_base64url(), publique_b64);
        let rechargee = ClesVapid::depuis_base64url(&privee_b64, "mailto:ops@klaar.be").unwrap();
        assert_eq!(rechargee.cle_publique_base64url(), publique_b64);
    }
}
