//! Envoi de courriel par webhook (n8n ou équivalent).
//!
//! **La composition reste ici, le transport part ailleurs.** Le sujet, le
//! corps traduit et le lien de vérification sont construits par ce dépôt —
//! ce sont des décisions de produit, pas de plomberie. Le webhook ne reçoit
//! qu'un message déjà écrit et le remet à un relais. Déporter la composition
//! dans le flux n8n mettrait les traductions et le ton des messages hors du
//! dépôt, hors des tests, et hors de portée d'une relecture.
//!
//! **Pourquoi un webhook plutôt qu'un client SMTP.** Un relais réel demande SPF,
//! DKIM et DMARC correctement posés, et un domaine dont la réputation
//! d'expédition se construit. Passer par un automate déjà en place déplace ce
//! travail là où il est fait, et le service n'a plus à connaître d'identifiants
//! de messagerie.
//!
//! **Ce que l'adaptateur ne fait pas** : réessayer. Un échec est journalisé et
//! l'appelant continue — l'inscription aboutit, le jeton reste valable une
//! heure, et le renvoi (Story 1.2) rattrape le cas. Réessayer ici tiendrait la
//! requête HTTP de quelqu'un pendant qu'on insiste auprès d'un tiers, et c'est
//! au flux n8n de porter la reprise s'il en faut une.

use awc::Client;
use serde::Serialize;
use std::time::Duration;

use klaar_application::ports::courriel::{
    CourrielInscription, CourrielSecurite, EnvoiCourriel, ErreurEnvoi,
};
use klaar_shared_kernel::{Email, Locale};

use crate::messages::{corps_inscription, corps_securite, sujet_inscription, sujet_securite};

/// Délai au-delà duquel on renonce à joindre le webhook.
///
/// Cinq secondes. L'envoi a lieu pendant la requête de quelqu'un qui attend son
/// écran : un automate lent ne doit pas faire patienter une inscription. Au-delà
/// on renonce, on journalise, et le renvoi rattrape.
pub const DELAI_SECONDES: u64 = 5;

/// Ce que le webhook reçoit.
///
/// **Aucun secret n'y figure.** Le lien de vérification en est un, et il est
/// dans le corps — c'est inévitable, puisque c'est le message. Mais le jeton
/// n'apparaît nulle part ailleurs, et rien de ce qui est envoyé ne permet de
/// remonter à autre chose que ce message-ci.
#[derive(Serialize)]
struct Charge<'a> {
    destinataire: &'a str,
    /// `fr`, `nl` ou `en` — le flux peut en avoir besoin pour choisir un
    /// expéditeur ou un gabarit d'enveloppe.
    locale: &'a str,
    sujet: &'a str,
    corps: &'a str,
    /// `inscription`, `verification` ou `securite`. Permet au flux de router
    /// sans avoir à interpréter le sujet, qui est traduit et changera.
    genre: &'a str,
}

pub struct CourrielWebhook {
    url: String,
    /// Jeton d'authentification du webhook, s'il en attend un.
    ///
    /// **Jamais journalisé.** Un webhook n8n sans authentification est une
    /// route publique qui envoie des courriels en votre nom ; avec, le jeton
    /// est la seule chose qui l'en empêche.
    jeton: Option<String>,
    url_publique: String,
}

impl CourrielWebhook {
    pub fn new(
        url: impl Into<String>,
        jeton: Option<String>,
        url_publique: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            jeton,
            url_publique: url_publique.into(),
        }
    }

    fn lien_verification(&self, jeton: &str) -> String {
        format!(
            "{}/verifier-email?jeton={}",
            self.url_publique.trim_end_matches('/'),
            jeton
        )
    }

    async fn poster(&self, charge: Charge<'_>) -> Result<(), ErreurEnvoi> {
        let client = Client::builder()
            .timeout(Duration::from_secs(DELAI_SECONDES))
            .finish();

        let mut requete = client.post(&self.url);
        if let Some(jeton) = &self.jeton {
            requete = requete.insert_header(("Authorization", format!("Bearer {jeton}")));
        }

        let reponse = requete
            .send_json(&charge)
            .await
            // **L'erreur de transport n'est pas recopiée telle quelle.** Elle
            // contient l'URL, donc le jeton s'il est dans la chaîne de requête,
            // et cette erreur finit dans les journaux.
            .map_err(|_| ErreurEnvoi("webhook injoignable".to_string()))?;

        let statut = reponse.status();
        if statut.is_success() {
            Ok(())
        } else {
            // Le corps de la réponse n'est pas repris non plus : un automate mal
            // configuré y renvoie volontiers la charge reçue, adresse comprise.
            Err(ErreurEnvoi(format!("webhook a répondu {statut}")))
        }
    }
}

impl EnvoiCourriel for CourrielWebhook {
    async fn envoyer_securite(
        &self,
        destinataire: &Email,
        locale: Locale,
        contenu: CourrielSecurite,
    ) -> Result<(), ErreurEnvoi> {
        let sujet = sujet_securite(locale, &contenu);
        let corps = corps_securite(locale, &contenu);

        // Journalisé **sans l'adresse** : le journal n'a pas à dire qui a vu son
        // compte verrouillé.
        tracing::info!(
            genre = "securite",
            locale = locale.as_str(),
            octets_corps = corps.len(),
            "alerte de sécurité remise au webhook"
        );

        self.poster(Charge {
            destinataire: destinataire.as_str(),
            locale: locale.as_str(),
            sujet: &sujet,
            corps: &corps,
            genre: "securite",
        })
        .await
    }

    async fn envoyer_inscription(
        &self,
        destinataire: &Email,
        locale: Locale,
        contenu: CourrielInscription,
    ) -> Result<(), ErreurEnvoi> {
        let genre = match contenu {
            CourrielInscription::Verification { .. } => "verification",
            CourrielInscription::CompteDejaExistant => "compte_existant",
        };
        // Le lien n'est construit que pour la variante qui en porte un. La
        // variante « compte déjà existant » n'en a pas, et lui en donner un
        // ferait d'une tentative d'inscription sur l'adresse d'autrui un moyen
        // de lui expédier une action à cliquer.
        let lien = match &contenu {
            CourrielInscription::Verification { jeton } => {
                Some(self.lien_verification(jeton.expose()))
            }
            CourrielInscription::CompteDejaExistant => None,
        };
        let sujet = sujet_inscription(locale, &contenu);
        let corps = corps_inscription(locale, &contenu, lien.as_deref());

        tracing::info!(
            genre,
            locale = locale.as_str(),
            octets_corps = corps.len(),
            "courriel d'inscription remis au webhook"
        );

        self.poster(Charge {
            destinataire: destinataire.as_str(),
            locale: locale.as_str(),
            sujet: &sujet,
            corps: &corps,
            genre,
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_le_jeton_n_apparait_pas_dans_le_debug_de_la_charge() {
        // La charge est sérialisée vers un tiers : elle ne doit porter que le
        // message, jamais de quoi rejouer un appel au webhook.
        let charge = Charge {
            destinataire: "a@example.eu",
            locale: "fr",
            sujet: "Sujet",
            corps: "Corps",
            genre: "verification",
        };
        let json = serde_json::to_string(&charge).unwrap();
        assert!(!json.contains("Bearer"));
        assert!(!json.contains("jeton"));
        // Et elle porte bien ce dont le flux a besoin pour router.
        assert!(json.contains("\"genre\":\"verification\""));
        assert!(json.contains("\"locale\":\"fr\""));
    }

    // L'envoi a lieu pendant la requête de quelqu'un : un automate lent ne doit
    // pas faire patienter une inscription au-delà de quelques secondes. Vérifié
    // à la compilation — relever la constante ne compile plus, ce qui oblige à
    // venir lire cette phrase.
    const _: () = assert!(DELAI_SECONDES <= 10);
}
