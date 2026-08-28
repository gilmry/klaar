//! Cas d'usage « notifier les candidats » (Story 3.3, FR-012).
//!
//! **Ce qu'une notification a le droit de dire.** Elle s'affiche sur un écran
//! verrouillé, lisible par quiconque passe à côté du téléphone. Elle ne porte
//! donc **ni la description du problème, ni l'adresse, ni rien du demandeur** :
//! seulement le secteur, la distance arrondie et l'urgence. C'est assez pour
//! décider d'ouvrir l'application, et c'est tout ce qu'il faut.
//!
//! La charge est chiffrée en transit (RFC 8291), mais le chiffrement ne protège
//! pas de ce que l'écran affiche. Les deux problèmes sont distincts, et seul le
//! second se règle en choisissant ce qu'on écrit.
//!
//! **Un abonnement mort est supprimé, pas réessayé.** Quand le service de push
//! répond 410, l'appareil a désinstallé ou révoqué : garder la ligne, c'est
//! conserver une donnée personnelle devenue sans finalité, et réessayer sans
//! fin.

use klaar_matching::{Demande, Urgence};
use klaar_shared_kernel::Locale;

use crate::ports::erreurs::RepositoryError;
use crate::ports::push::{PushError, PushMessage, PushNotifier};
use crate::ports::push_repository::PushSubscriptionRepository;
use crate::usecases::matcher::Candidat;

/// Résultat d'un tour de notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BilanNotification {
    /// Appareils qui ont reçu le message.
    pub notifies: usize,
    /// Candidats sans aucun abonnement : ils verront la Demande en ouvrant
    /// l'application, ce qui n'est pas un échec.
    pub sans_abonnement: usize,
    /// Abonnements supprimés parce que le service de push les a déclarés
    /// disparus.
    pub purges: usize,
}

/// Arrondit la distance pour l'affichage.
///
/// À la centaine de mètres sous le kilomètre, au dixième de kilomètre au-delà.
/// Une distance au mètre près sur un écran verrouillé situerait le demandeur
/// bien plus précisément que nécessaire, et la précision n'apporte rien à qui
/// décide s'il peut y aller.
fn distance_lisible(metres: f64) -> String {
    if metres < 1_000.0 {
        format!("{} m", ((metres / 100.0).round() * 100.0) as i64)
    } else {
        format!("{:.1} km", metres / 1_000.0)
    }
}

fn libelle_urgence(urgence: Urgence, locale: Locale) -> &'static str {
    match (urgence, locale) {
        (Urgence::Haute, Locale::Fr) => "tout de suite",
        (Urgence::Haute, Locale::Nl) => "onmiddellijk",
        (Urgence::Haute, Locale::En) => "right away",
        (Urgence::Normale, Locale::Fr) => "dans la journée",
        (Urgence::Normale, Locale::Nl) => "vandaag",
        (Urgence::Normale, Locale::En) => "today",
        (Urgence::Basse, Locale::Fr) => "peut attendre",
        (Urgence::Basse, Locale::Nl) => "kan wachten",
        (Urgence::Basse, Locale::En) => "can wait",
    }
}

/// Compose le message affiché.
///
/// Le secteur est donné par son **code**, pas par son libellé : le cas d'usage
/// ne connaît pas le catalogue, et un code lisible comme `plomberie` reste
/// compréhensible. Le libellé traduit viendra quand la notification passera par
/// une vue qui a le catalogue sous la main.
pub fn composer(demande: &Demande, distance_metres: f64, locale: Locale) -> PushMessage {
    let titre = match locale {
        Locale::Fr => format!("Demande — {}", demande.secteur),
        Locale::Nl => format!("Aanvraag — {}", demande.secteur),
        Locale::En => format!("Request — {}", demande.secteur),
    };
    PushMessage {
        titre,
        corps: format!(
            "{} · {}",
            distance_lisible(distance_metres),
            libelle_urgence(demande.urgence, locale)
        ),
        url: format!("/prestataire/demande?id={}", demande.id),
        // Un `tag` par Demande : deux notifications pour la même Demande se
        // remplacent au lieu de s'empiler.
        tag: Some(format!("demande-{}", demande.id)),
    }
}

pub async fn notifier<A, N>(
    abonnements: &A,
    notifieur: &N,
    demande: &Demande,
    candidats: &[Candidat],
    locale: Locale,
) -> Result<BilanNotification, RepositoryError>
where
    A: PushSubscriptionRepository,
    N: PushNotifier,
{
    let mut bilan = BilanNotification::default();

    for candidat in candidats {
        let appareils = abonnements
            .lister_par_sujet(candidat.utilisateur_id)
            .await?;
        if appareils.is_empty() {
            bilan.sans_abonnement += 1;
            continue;
        }

        let message = composer(demande, candidat.distance_metres, locale);
        for appareil in appareils {
            match notifieur.envoyer(&appareil.abonnement, &message).await {
                Ok(()) => bilan.notifies += 1,
                Err(PushError::AbonnementExpire) => {
                    // 410 : l'appareil a désinstallé ou révoqué. Garder la
                    // ligne conserverait une donnée personnelle sans finalité,
                    // et ferait réessayer sans fin.
                    abonnements
                        .supprimer_par_endpoint(&appareil.abonnement.endpoint)
                        .await?;
                    bilan.purges += 1;
                }
                Err(e) => {
                    // Une panne du service de push ne doit pas interrompre le
                    // tour : les autres candidats n'y sont pour rien, et la
                    // Demande reste diffusée.
                    tracing::warn!(erreur = %e, "notification non délivrée");
                }
            }
        }
    }

    Ok(bilan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::push::PushSubscription;
    use crate::ports::push_repository::AbonnementEnregistre;
    use chrono::{DateTime, TimeZone, Utc};
    use klaar_catalog::CodeCatalogue;
    use klaar_matching::Score;
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;
    use uuid::Uuid;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn demande(urgence: Urgence) -> Demande {
        Demande::soumettre(
            Uuid::new_v4(),
            CodeCatalogue::parse("plomberie").unwrap(),
            "Fuite très reconnaissable sous l'évier de la cuisine",
            Geo::new(50.8467, 4.3525).unwrap(),
            urgence,
            instant(),
        )
        .unwrap()
    }

    fn candidat(utilisateur_id: Uuid, distance: f64) -> Candidat {
        Candidat {
            provider_id: Uuid::new_v4(),
            utilisateur_id,
            distance_metres: distance,
            score: klaar_matching::calculer_score(distance, 0.0, None),
        }
    }

    fn abonnement(endpoint: &str) -> PushSubscription {
        PushSubscription {
            endpoint: endpoint.to_string(),
            p256dh: "p".to_string(),
            auth: "a".to_string(),
        }
    }

    #[derive(Default)]
    struct AbonnementsMemoire {
        par_sujet: RefCell<Vec<(Uuid, PushSubscription)>>,
        supprimes: RefCell<Vec<String>>,
    }

    impl PushSubscriptionRepository for AbonnementsMemoire {
        async fn enregistrer(
            &self,
            _: &PushSubscription,
            _: Option<Uuid>,
        ) -> Result<AbonnementEnregistre, RepositoryError> {
            unreachable!()
        }

        async fn lister_par_sujet(
            &self,
            sujet_id: Uuid,
        ) -> Result<Vec<AbonnementEnregistre>, RepositoryError> {
            Ok(self
                .par_sujet
                .borrow()
                .iter()
                .filter(|(s, _)| *s == sujet_id)
                .map(|(s, a)| AbonnementEnregistre {
                    id: Uuid::new_v4(),
                    abonnement: a.clone(),
                    sujet_id: Some(*s),
                })
                .collect())
        }

        async fn supprimer_par_endpoint(&self, endpoint: &str) -> Result<bool, RepositoryError> {
            self.supprimes.borrow_mut().push(endpoint.to_string());
            self.par_sujet
                .borrow_mut()
                .retain(|(_, a)| a.endpoint != endpoint);
            Ok(true)
        }

        async fn compter(&self) -> Result<i64, RepositoryError> {
            unreachable!()
        }
    }

    #[derive(Default)]
    struct NotifieurFactice {
        envoyes: RefCell<Vec<(String, PushMessage)>>,
        disparus: Vec<String>,
        en_panne: Vec<String>,
    }

    impl PushNotifier for NotifieurFactice {
        async fn envoyer(
            &self,
            abonnement: &PushSubscription,
            message: &PushMessage,
        ) -> Result<(), PushError> {
            if self.disparus.contains(&abonnement.endpoint) {
                return Err(PushError::AbonnementExpire);
            }
            if self.en_panne.contains(&abonnement.endpoint) {
                return Err(PushError::Transport("test".into()));
            }
            self.envoyes
                .borrow_mut()
                .push((abonnement.endpoint.clone(), message.clone()));
            Ok(())
        }
    }

    #[tokio::test]
    async fn happy_chaque_candidat_abonne_recoit_le_message() {
        let sujet = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((sujet, abonnement("https://push.example.net/a")));
        let notifieur = NotifieurFactice::default();

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &demande(Urgence::Haute),
            &[candidat(sujet, 1_200.0)],
            Locale::Fr,
        )
        .await
        .unwrap();

        assert_eq!(bilan.notifies, 1);
        assert_eq!(notifieur.envoyes.borrow().len(), 1);
    }

    #[tokio::test]
    async fn happy_un_candidat_a_plusieurs_appareils_les_recoit_tous() {
        let sujet = Uuid::new_v4();
        let abonnements = AbonnementsMemoire::default();
        for e in ["https://push.example.net/a", "https://push.example.net/b"] {
            abonnements
                .par_sujet
                .borrow_mut()
                .push((sujet, abonnement(e)));
        }
        let notifieur = NotifieurFactice::default();

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &demande(Urgence::Normale),
            &[candidat(sujet, 500.0)],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 2);
    }

    #[tokio::test]
    async fn negative_un_candidat_sans_abonnement_n_est_pas_un_echec() {
        // Il verra la Demande en ouvrant l'application.
        let abonnements = AbonnementsMemoire::default();
        let notifieur = NotifieurFactice::default();
        let bilan = notifier(
            &abonnements,
            &notifieur,
            &demande(Urgence::Haute),
            &[candidat(Uuid::new_v4(), 100.0)],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.sans_abonnement, 1);
        assert_eq!(bilan.notifies, 0);
    }

    #[tokio::test]
    async fn negative_un_abonnement_disparu_est_purge() {
        let sujet = Uuid::new_v4();
        let mort = "https://push.example.net/mort";
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((sujet, abonnement(mort)));
        let notifieur = NotifieurFactice {
            disparus: vec![mort.to_string()],
            ..Default::default()
        };

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &demande(Urgence::Haute),
            &[candidat(sujet, 100.0)],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.purges, 1);
        assert_eq!(
            abonnements.supprimes.borrow().as_slice(),
            &[mort.to_string()]
        );
    }

    #[tokio::test]
    async fn negative_une_panne_de_transport_n_interrompt_pas_le_tour() {
        // Les autres candidats n'y sont pour rien.
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let casse = "https://push.example.net/casse";
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((a, abonnement(casse)));
        abonnements
            .par_sujet
            .borrow_mut()
            .push((b, abonnement("https://push.example.net/ok")));
        let notifieur = NotifieurFactice {
            en_panne: vec![casse.to_string()],
            ..Default::default()
        };

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &demande(Urgence::Haute),
            &[candidat(a, 100.0), candidat(b, 200.0)],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 1);
        assert_eq!(bilan.purges, 0, "une panne n'est pas une disparition");
    }

    #[tokio::test]
    async fn edge_la_distance_est_arrondie_pour_l_affichage() {
        assert_eq!(distance_lisible(0.0), "0 m");
        assert_eq!(distance_lisible(137.0), "100 m");
        assert_eq!(distance_lisible(180.0), "200 m");
        assert_eq!(distance_lisible(1_234.0), "1.2 km");
        assert_eq!(distance_lisible(4_999.0), "5.0 km");
    }

    #[tokio::test]
    async fn edge_les_trois_langues_produisent_un_message_non_vide() {
        for locale in [Locale::Fr, Locale::Nl, Locale::En] {
            for urgence in [Urgence::Basse, Urgence::Normale, Urgence::Haute] {
                let m = composer(&demande(urgence), 1_000.0, locale);
                assert!(!m.titre.is_empty(), "{locale:?}/{urgence:?}");
                assert!(!m.corps.is_empty());
            }
        }
    }

    #[tokio::test]
    async fn edge_deux_notifications_de_la_meme_demande_se_remplacent() {
        // Un `tag` par Demande : sinon dix alertes s'empilent pour une seule
        // intervention.
        let d = demande(Urgence::Haute);
        let premier = composer(&d, 100.0, Locale::Fr);
        let second = composer(&d, 200.0, Locale::Fr);
        assert_eq!(premier.tag, second.tag);
        assert_eq!(premier.tag, Some(format!("demande-{}", d.id)));
    }

    #[tokio::test]
    async fn security_le_message_ne_porte_ni_description_ni_demandeur() {
        // Une notification s'affiche sur un écran verrouillé, lisible par
        // quiconque passe à côté du téléphone. Le chiffrement de la charge
        // (RFC 8291) n'y change rien : les deux problèmes sont distincts.
        let d = demande(Urgence::Haute);
        let m = composer(&d, 1_200.0, Locale::Fr);
        let tout = format!("{} {} {}", m.titre, m.corps, m.url);

        assert!(!tout.contains("Fuite très reconnaissable"), "{tout}");
        assert!(!tout.contains("évier"));
        assert!(!tout.contains(&d.demandeur_id.to_string()));
    }

    #[tokio::test]
    async fn security_le_message_ne_porte_aucune_coordonnee() {
        // Une latitude sur un écran verrouillé situe le demandeur chez lui.
        let d = demande(Urgence::Haute);
        let m = composer(&d, 1_200.0, Locale::Fr);
        let tout = format!("{} {} {}", m.titre, m.corps, m.url);
        assert!(!tout.contains("50.8"), "{tout}");
        assert!(!tout.contains("4.35"));
    }

    #[tokio::test]
    async fn security_la_distance_affichee_ne_situe_pas_au_metre_pres() {
        // Une distance au mètre près, croisée avec la position du prestataire,
        // situerait le demandeur bien plus précisément que nécessaire.
        for metres in [137.0, 342.0, 981.0] {
            let affichee = distance_lisible(metres);
            assert!(affichee.ends_with(" m"));
            let valeur: f64 = affichee.trim_end_matches(" m").parse().unwrap();
            assert_eq!(valeur % 100.0, 0.0, "{metres} → {affichee}");
        }
    }

    #[tokio::test]
    async fn security_un_candidat_ne_recoit_pas_l_abonnement_d_un_autre() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((b, abonnement("https://push.example.net/b")));
        let notifieur = NotifieurFactice::default();

        let bilan = notifier(
            &abonnements,
            &notifieur,
            &demande(Urgence::Haute),
            &[candidat(a, 100.0)],
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.notifies, 0);
        assert_eq!(bilan.sans_abonnement, 1);
        assert!(notifieur.envoyes.borrow().is_empty());
    }

    #[tokio::test]
    async fn security_le_score_n_est_pas_communique_au_prestataire() {
        // Le score sert au classement, pas à l'affichage : le publier
        // inviterait à l'optimiser plutôt qu'à bien travailler.
        let d = demande(Urgence::Haute);
        let m = composer(&d, 1_200.0, Locale::Fr);
        let _: Score = klaar_matching::calculer_score(1_200.0, 0.0, None);
        assert!(!format!("{} {}", m.titre, m.corps).contains("score"));
    }
}
