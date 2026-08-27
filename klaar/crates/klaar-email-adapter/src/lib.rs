//! Adaptateur d'envoi de courriel.
//!
//! Aucun relais réel n'est branché à ce stade : il en faudrait un provisionné,
//! avec SPF, DKIM et DMARC, ce que le périmètre vitrine exclut. L'adaptateur
//! fourni compose le message pour de bon — sujet et corps traduits, lien de
//! vérification complet — puis le journalise au lieu de l'expédier. Le port est
//! respecté, si bien que brancher un relais SMTP revient à écrire un second
//! `impl EnvoiCourriel` sans toucher au cas d'usage.

use klaar_application::ports::courriel::{
    CourrielInscription, CourrielSecurite, EnvoiCourriel, ErreurEnvoi,
};
use klaar_shared_kernel::{Email, Locale};

mod messages;

pub use messages::{
    corps_inscription, corps_securite, sujet_inscription, sujet_securite, MessageCourriel,
};

/// Adaptateur de développement : compose puis journalise.
pub struct CourrielJournalise {
    /// Racine publique de la PWA, pour construire le lien de vérification.
    url_publique: String,
    /// Journaliser le lien complet expose un jeton d'activation dans les
    /// journaux, où il survit à sa validité et se lit sans authentification.
    /// Utile en développement, inacceptable ailleurs : d'où l'activation
    /// explicite plutôt que la valeur par défaut.
    afficher_le_lien: bool,
}

impl CourrielJournalise {
    pub fn new(url_publique: impl Into<String>, afficher_le_lien: bool) -> Self {
        Self {
            url_publique: url_publique.into(),
            afficher_le_lien,
        }
    }

    /// Construit l'adaptateur depuis l'environnement.
    pub fn depuis_environnement() -> Self {
        Self::new(
            std::env::var("KLAAR_URL_PUBLIQUE")
                .unwrap_or_else(|_| "http://localhost:4321".to_string()),
            std::env::var("KLAAR_COURRIEL_AFFICHER_LIEN").as_deref() == Ok("1"),
        )
    }

    pub fn lien_verification(&self, jeton: &str) -> String {
        format!(
            "{}/verifier-email?jeton={}",
            self.url_publique.trim_end_matches('/'),
            jeton
        )
    }
}

impl EnvoiCourriel for CourrielJournalise {
    async fn envoyer_securite(
        &self,
        _destinataire: &Email,
        locale: Locale,
        contenu: CourrielSecurite,
    ) -> Result<(), ErreurEnvoi> {
        let message = MessageCourriel {
            sujet: sujet_securite(locale, &contenu),
            corps: corps_securite(locale, &contenu),
        };
        // Ni destinataire ni détail du verrou : le couple « adresse + alerte »
        // dirait à qui lit les journaux quels comptes sont attaqués.
        tracing::warn!(
            genre = "securite",
            locale = locale.as_str(),
            octets_corps = message.corps.len(),
            "alerte de sécurité composée (adaptateur de journalisation, non expédiée)"
        );
        Ok(())
    }

    async fn envoyer_inscription(
        &self,
        _destinataire: &Email,
        locale: Locale,
        contenu: CourrielInscription,
    ) -> Result<(), ErreurEnvoi> {
        let (genre, lien) = match &contenu {
            CourrielInscription::Verification { jeton } => {
                ("verification", Some(self.lien_verification(jeton.expose())))
            }
            CourrielInscription::CompteDejaExistant => ("compte-deja-existant", None),
        };

        let message = MessageCourriel {
            sujet: sujet_inscription(locale, &contenu),
            corps: corps_inscription(locale, &contenu, lien.as_deref()),
        };

        // Le destinataire n'apparaît pas : c'est une donnée personnelle, et le
        // couple « adresse + genre du message » dirait à qui lit les journaux
        // quelles adresses ont déjà un compte, soit exactement l'énumération
        // que le cas d'usage empêche côté HTTP.
        tracing::info!(
            genre,
            locale = locale.as_str(),
            octets_corps = message.corps.len(),
            "courriel d'inscription composé (adaptateur de journalisation, non expédié)"
        );

        if self.afficher_le_lien {
            if let Some(lien) = &lien {
                tracing::debug!(lien, "lien de vérification (développement uniquement)");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use klaar_identity::JetonVerification;

    fn adaptateur() -> CourrielJournalise {
        CourrielJournalise::new("https://klaar.be/", false)
    }

    #[tokio::test]
    async fn happy_compose_et_accepte_un_courriel_de_verification() {
        let a = adaptateur();
        let r = a
            .envoyer_inscription(
                &Email::parse("marie@example.eu").unwrap(),
                Locale::Fr,
                CourrielInscription::Verification {
                    jeton: JetonVerification::tirer(),
                },
            )
            .await;
        assert!(r.is_ok());
    }

    #[test]
    fn happy_le_lien_porte_le_jeton_et_la_route_de_verification() {
        let lien = adaptateur().lien_verification("abc123");
        assert_eq!(lien, "https://klaar.be/verifier-email?jeton=abc123");
    }

    #[test]
    fn negative_une_racine_sans_barre_finale_ne_colle_pas_les_segments() {
        let lien = CourrielJournalise::new("https://klaar.be", false).lien_verification("x");
        assert_eq!(lien, "https://klaar.be/verifier-email?jeton=x");
    }

    #[test]
    fn edge_les_trois_locales_donnent_trois_sujets_distincts() {
        let contenu = CourrielInscription::CompteDejaExistant;
        let sujets: Vec<_> = [Locale::Fr, Locale::Nl, Locale::En]
            .into_iter()
            .map(|l| sujet_inscription(l, &contenu))
            .collect();
        assert_eq!(sujets.len(), 3);
        assert_ne!(sujets[0], sujets[1]);
        assert_ne!(sujets[1], sujets[2]);
        assert_ne!(sujets[0], sujets[2]);
    }

    #[test]
    fn edge_aucun_message_ne_reste_vide_dans_aucune_locale() {
        let jeton = JetonVerification::tirer();
        for locale in [Locale::Fr, Locale::Nl, Locale::En] {
            for contenu in [
                CourrielInscription::Verification {
                    jeton: JetonVerification::depuis_chaine(jeton.expose()),
                },
                CourrielInscription::CompteDejaExistant,
            ] {
                assert!(!sujet_inscription(locale, &contenu).is_empty());
                assert!(!corps_inscription(locale, &contenu, Some("https://x/y")).is_empty());
            }
        }
    }

    #[test]
    fn security_le_message_de_compte_existant_ne_porte_aucun_lien() {
        // Ce courriel part à quelqu'un qui n'a rien demandé. S'il contenait un
        // lien d'activation ou de réinitialisation, l'inscription sur
        // l'adresse d'autrui deviendrait un moyen de lui envoyer un jeton.
        for locale in [Locale::Fr, Locale::Nl, Locale::En] {
            let corps = corps_inscription(
                locale,
                &CourrielInscription::CompteDejaExistant,
                Some("https://klaar.be/verifier-email?jeton=secret"),
            );
            assert!(!corps.contains("jeton="));
            assert!(!corps.contains("secret"));
        }
    }

    #[test]
    fn security_le_lien_de_verification_est_absent_du_corps_sans_jeton_fourni() {
        let corps = corps_inscription(
            Locale::Fr,
            &CourrielInscription::Verification {
                jeton: JetonVerification::tirer(),
            },
            None,
        );
        assert!(!corps.contains("http"));
    }
}
