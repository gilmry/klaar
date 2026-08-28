//! Cas d'usage « éteindre les devis sans réponse » (FR-016 `@edge`, Story 4.1).
//!
//! **Une heure sans réponse vaut une réponse.** Un prestataire qui attend ne
//! peut ni relancer — un seul devis en attente à la fois — ni passer à autre
//! chose. Laisser le devis en suspens indéfiniment le bloquerait sur une
//! affaire close depuis longtemps.
//!
//! **La sélection et l'extinction sont une seule opération.** Le dépôt rend les
//! devis qu'il vient d'éteindre, et eux seuls : deux passages du balayage ne
//! peuvent donc pas prévenir deux fois le même prestataire.
//!
//! **Ce que ce balayage ne fait pas.** FR-016 `@edge` mentionne une remise en
//! diffusion de la Demande après un second échec. Elle n'est pas ici : défaire
//! une attribution suppose de rendre le prestataire disponible, de rouvrir la
//! Demande et de recommencer un tour, c'est-à-dire une décision sur l'argent
//! déjà engagé que FR-017 n'a pas encore tranchée. Le devis expire, la Mission
//! reste attribuée, et le prestataire peut en envoyer un autre. La limite est
//! écrite plutôt que découverte.

use klaar_shared_kernel::Locale;

use crate::ports::devis_repository::DevisRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::provider_repository::ProviderRepository;
use crate::ports::push::{PushMessage, PushNotifier};
use crate::ports::push_repository::PushSubscriptionRepository;

/// Devis traités en un passage.
///
/// Borne un rattrapage après une longue interruption, comme pour les tours de
/// diffusion : sans elle, un balayage qui reprend après une panne tenterait de
/// tout traiter d'un coup.
pub const PAR_PASSAGE_MAX: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BilanExpirationDevis {
    /// Devis passés en `EXPIRED`.
    pub eteints: usize,
    /// Prestataires joints sur au moins un appareil.
    pub notifies: usize,
}

/// Compose l'avis « votre devis a expiré », pour le prestataire.
///
/// **Sans le montant**, comme l'avis de réception envoyé au demandeur : il
/// s'affiche sur un écran verrouillé, et ce que quelqu'un a chiffré pour une
/// intervention n'a pas à se lire par-dessus une épaule.
pub fn composer_devis_expire(devis_id: uuid::Uuid, locale: Locale) -> PushMessage {
    let corps = match locale {
        Locale::Fr => "Votre devis a expiré sans réponse.",
        Locale::Nl => "Uw offerte is vervallen zonder antwoord.",
        Locale::En => "Your quote expired without an answer.",
    };
    PushMessage {
        titre: "Klaar".to_string(),
        corps: corps.to_string(),
        tag: Some(format!("devis-{devis_id}")),
        url: "/prestataire".to_string(),
    }
}

/// Éteint les devis échus et prévient ceux qui les ont émis.
pub async fn expirer_les_devis<Q, P, A, N, H>(
    devis_repo: &Q,
    prestataires: &P,
    abonnements: &A,
    notifieur: Option<&N>,
    horloge: &H,
    locale: Locale,
) -> Result<BilanExpirationDevis, RepositoryError>
where
    Q: DevisRepository,
    P: ProviderRepository,
    A: PushSubscriptionRepository,
    N: PushNotifier,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let eteints = devis_repo
        .expirer_les_echus(maintenant, PAR_PASSAGE_MAX)
        .await?;

    let mut bilan = BilanExpirationDevis {
        eteints: eteints.len(),
        notifies: 0,
    };

    // L'extinction est écrite avant tout avis, et son échec n'annule rien : une
    // panne du service de push ne doit pas laisser un devis en attente pour
    // toujours. Le prestataire verra l'état réel en ouvrant l'application.
    let Some(notifieur) = notifieur else {
        return Ok(bilan);
    };

    for devis in eteints {
        // Le compte à prévenir est celui du prestataire, pas la fiche : les
        // abonnements push vivent sur le compte.
        let compte = match prestataires.par_id(devis.provider_id).await? {
            Some(p) => p.utilisateur_id,
            None => continue,
        };
        let message = composer_devis_expire(devis.id, locale);
        let mut joint = false;
        for appareil in abonnements.lister_par_sujet(compte).await? {
            match notifieur.envoyer(&appareil.abonnement, &message).await {
                Ok(()) => joint = true,
                Err(crate::ports::push::PushError::AbonnementExpire) => {
                    abonnements
                        .supprimer_par_endpoint(&appareil.abonnement.endpoint)
                        .await?;
                }
                Err(e) => {
                    // Les autres destinataires n'y sont pour rien.
                    tracing::warn!(erreur = %e, "avis d'expiration de devis non délivré");
                }
            }
        }
        if joint {
            bilan.notifies += 1;
        }
    }

    Ok(bilan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::provider_repository::ProviderProche;
    use crate::ports::push::{PushError, PushSubscription};
    use crate::ports::push_repository::{AbonnementEnregistre, PushSubscriptionRepository};
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use klaar_catalog::CodeCatalogue;
    use klaar_identity::{
        NumeroBce, OrigineKyc, Provider, StatutProvider, RAYON_INTERVENTION_DEFAUT,
    };
    use klaar_payment::{Devis, Proposition, StatutDevis};
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;
    use uuid::Uuid;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn provider() -> Provider {
        let corps = 1_234_567u64;
        Provider {
            id: Uuid::new_v4(),
            utilisateur_id: Uuid::new_v4(),
            numero_bce: NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).unwrap(),
            raison_sociale: "Prestataire".to_string(),
            base: Geo::new(50.8467, 4.3525).unwrap(),
            statut: StatutProvider::Actif,
            origine_kyc: Some(OrigineKyc::Demonstration),
            kyc_verifie_le: Some(instant()),
            competences: vec![CodeCatalogue::parse("plomberie").unwrap()],
            disponible: true,
            rayon_intervention_metres: RAYON_INTERVENTION_DEFAUT,
            cree_le: instant(),
        }
    }

    fn devis(provider_id: Uuid) -> Devis {
        Devis::emettre(
            Uuid::new_v4(),
            provider_id,
            Proposition {
                montant_htva_cents: 18_000,
                taux_tva_bp: 2100,
                delai_minutes: 45,
                note: None,
                preuve_tva_reduite: None,
            },
            instant() - Duration::hours(2),
        )
        .expect("devis valide")
    }

    /// Rend, comme le SQL, les devis qu'il vient d'éteindre — et eux seuls.
    #[derive(Default)]
    struct DevisMemoire {
        vivants: RefCell<Vec<Devis>>,
        passages: RefCell<usize>,
    }

    impl DevisRepository for DevisMemoire {
        async fn emettre(
            &self,
            _: &Devis,
            _: usize,
        ) -> Result<crate::ports::devis_repository::ResultatEmission, RepositoryError> {
            unreachable!()
        }
        async fn en_cours_pour_mission(&self, _: Uuid) -> Result<Option<Devis>, RepositoryError> {
            unreachable!()
        }
        async fn repondre(
            &self,
            _: Uuid,
            _: klaar_payment::StatutDevis,
            _: Option<&str>,
        ) -> Result<bool, RepositoryError> {
            unreachable!()
        }
        async fn par_id(&self, _: Uuid) -> Result<Option<Devis>, RepositoryError> {
            unreachable!()
        }
        async fn dernier_pour_mission(&self, _: Uuid) -> Result<Option<Devis>, RepositoryError> {
            unreachable!()
        }
        async fn compter_pour_mission(&self, _: Uuid) -> Result<usize, RepositoryError> {
            unreachable!()
        }
        async fn expirer_les_echus(
            &self,
            maintenant: DateTime<Utc>,
            limite: i64,
        ) -> Result<Vec<Devis>, RepositoryError> {
            *self.passages.borrow_mut() += 1;
            let mut eteints = Vec::new();
            self.vivants.borrow_mut().retain(|d| {
                let echu = d.est_expire(maintenant) && (eteints.len() as i64) < limite;
                if echu {
                    let mut eteint = d.clone();
                    eteint.statut = StatutDevis::Expire;
                    eteints.push(eteint);
                }
                !echu
            });
            Ok(eteints)
        }
    }

    struct PrestatairesMemoire {
        fiche: Option<Provider>,
    }

    impl ProviderRepository for PrestatairesMemoire {
        async fn creer(&self, _: &Provider) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn par_id(&self, _: Uuid) -> Result<Option<Provider>, RepositoryError> {
            Ok(self.fiche.clone())
        }
        async fn par_numero_bce(&self, _: &NumeroBce) -> Result<Option<Provider>, RepositoryError> {
            unreachable!()
        }
        async fn par_utilisateur_id(&self, _: Uuid) -> Result<Option<Provider>, RepositoryError> {
            unreachable!()
        }
        async fn mettre_a_jour_etat(&self, _: &Provider) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn definir_disponibilite(&self, _: Uuid, _: bool) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn definir_rayon_intervention(&self, _: Uuid, _: f64) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn proches(
            &self,
            _: &CodeCatalogue,
            _: Geo,
            _: f64,
            _: i64,
        ) -> Result<Vec<ProviderProche>, RepositoryError> {
            unreachable!()
        }
    }

    #[derive(Default)]
    struct AbonnementsMemoire {
        par_sujet: RefCell<Vec<(Uuid, String)>>,
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
                .map(|(s, e)| AbonnementEnregistre {
                    id: Uuid::new_v4(),
                    abonnement: PushSubscription {
                        endpoint: e.clone(),
                        p256dh: "p".into(),
                        auth: "a".into(),
                    },
                    sujet_id: Some(*s),
                })
                .collect())
        }
        async fn supprimer_par_endpoint(&self, endpoint: &str) -> Result<bool, RepositoryError> {
            self.supprimes.borrow_mut().push(endpoint.to_string());
            Ok(true)
        }
        async fn compter(&self) -> Result<i64, RepositoryError> {
            unreachable!()
        }
    }

    #[derive(Default)]
    struct NotifieurFactice {
        envoyes: RefCell<Vec<PushMessage>>,
        expire: bool,
    }

    impl PushNotifier for NotifieurFactice {
        async fn envoyer(
            &self,
            _: &PushSubscription,
            message: &PushMessage,
        ) -> Result<(), PushError> {
            if self.expire {
                return Err(PushError::AbonnementExpire);
            }
            self.envoyes.borrow_mut().push(message.clone());
            Ok(())
        }
    }

    async fn balayer(
        devis_repo: &DevisMemoire,
        prestataires: &PrestatairesMemoire,
        abonnements: &AbonnementsMemoire,
        notifieur: Option<&NotifieurFactice>,
    ) -> BilanExpirationDevis {
        expirer_les_devis(
            devis_repo,
            prestataires,
            abonnements,
            notifieur,
            &HorlogeFigee(instant()),
            Locale::Fr,
        )
        .await
        .expect("balayage")
    }

    // === @happy ===

    #[tokio::test]
    async fn happy_un_devis_echu_est_eteint_et_son_emetteur_prevenu() {
        let fiche = provider();
        let d = devis(fiche.id);
        let devis_repo = DevisMemoire {
            vivants: RefCell::new(vec![d]),
            ..Default::default()
        };
        let abonnements = AbonnementsMemoire {
            par_sujet: RefCell::new(vec![(fiche.utilisateur_id, "https://push/1".into())]),
            ..Default::default()
        };
        let notifieur = NotifieurFactice::default();

        let bilan = balayer(
            &devis_repo,
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &abonnements,
            Some(&notifieur),
        )
        .await;

        assert_eq!(bilan.eteints, 1);
        assert_eq!(bilan.notifies, 1);
        assert_eq!(notifieur.envoyes.borrow().len(), 1);
    }

    // === @negative ===

    #[tokio::test]
    async fn negative_sans_notifieur_le_devis_s_eteint_quand_meme() {
        // L'extinction est le fait ; l'avis est un service en plus. Un
        // déploiement sans clé VAPID ne doit pas laisser des devis en attente
        // pour toujours.
        let fiche = provider();
        let devis_repo = DevisMemoire {
            vivants: RefCell::new(vec![devis(fiche.id)]),
            ..Default::default()
        };

        let bilan = expirer_les_devis(
            &devis_repo,
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &AbonnementsMemoire::default(),
            None::<&NotifieurFactice>,
            &HorlogeFigee(instant()),
            Locale::Fr,
        )
        .await
        .expect("balayage");

        assert_eq!(bilan.eteints, 1);
        assert_eq!(bilan.notifies, 0);
        assert!(devis_repo.vivants.borrow().is_empty());
    }

    // === @edge ===

    #[tokio::test]
    async fn edge_un_second_passage_ne_retrouve_rien() {
        // Idempotence : deux balayages ne doivent pas prévenir deux fois le
        // même prestataire.
        let fiche = provider();
        let devis_repo = DevisMemoire {
            vivants: RefCell::new(vec![devis(fiche.id)]),
            ..Default::default()
        };
        let abonnements = AbonnementsMemoire {
            par_sujet: RefCell::new(vec![(fiche.utilisateur_id, "https://push/1".into())]),
            ..Default::default()
        };
        let notifieur = NotifieurFactice::default();
        let prestataires = PrestatairesMemoire {
            fiche: Some(fiche.clone()),
        };

        balayer(&devis_repo, &prestataires, &abonnements, Some(&notifieur)).await;
        let second = balayer(&devis_repo, &prestataires, &abonnements, Some(&notifieur)).await;

        assert_eq!(second.eteints, 0);
        assert_eq!(notifieur.envoyes.borrow().len(), 1);
        assert_eq!(*devis_repo.passages.borrow(), 2);
    }

    #[tokio::test]
    async fn edge_un_prestataire_sans_appareil_est_compte_mais_pas_notifie() {
        let fiche = provider();
        let devis_repo = DevisMemoire {
            vivants: RefCell::new(vec![devis(fiche.id)]),
            ..Default::default()
        };
        let notifieur = NotifieurFactice::default();

        let bilan = balayer(
            &devis_repo,
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &AbonnementsMemoire::default(),
            Some(&notifieur),
        )
        .await;

        assert_eq!(bilan.eteints, 1);
        assert_eq!(bilan.notifies, 0);
    }

    #[tokio::test]
    async fn edge_un_abonnement_expire_est_purge() {
        // 410 : garder la ligne conserverait une donnée personnelle sans
        // finalité, et ferait réessayer sans fin.
        let fiche = provider();
        let devis_repo = DevisMemoire {
            vivants: RefCell::new(vec![devis(fiche.id)]),
            ..Default::default()
        };
        let abonnements = AbonnementsMemoire {
            par_sujet: RefCell::new(vec![(fiche.utilisateur_id, "https://push/mort".into())]),
            ..Default::default()
        };
        let notifieur = NotifieurFactice {
            expire: true,
            ..Default::default()
        };

        let bilan = balayer(
            &devis_repo,
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &abonnements,
            Some(&notifieur),
        )
        .await;

        assert_eq!(bilan.eteints, 1);
        assert_eq!(bilan.notifies, 0);
        assert_eq!(
            abonnements.supprimes.borrow().as_slice(),
            ["https://push/mort"]
        );
    }

    // === @security ===

    #[tokio::test]
    async fn security_l_avis_d_expiration_ne_porte_pas_le_montant() {
        // Il s'affiche sur un écran verrouillé. Ce que quelqu'un a chiffré pour
        // une intervention n'a pas à se lire par-dessus une épaule.
        let fiche = provider();
        let d = devis(fiche.id);
        let devis_repo = DevisMemoire {
            vivants: RefCell::new(vec![d.clone()]),
            ..Default::default()
        };
        let abonnements = AbonnementsMemoire {
            par_sujet: RefCell::new(vec![(fiche.utilisateur_id, "https://push/1".into())]),
            ..Default::default()
        };
        let notifieur = NotifieurFactice::default();

        balayer(
            &devis_repo,
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &abonnements,
            Some(&notifieur),
        )
        .await;

        let envoyes = notifieur.envoyes.borrow();
        let message = envoyes.first().expect("un avis");
        for interdit in ["180", "217", "18000", "21780"] {
            assert!(
                !message.corps.contains(interdit) && !message.titre.contains(interdit),
                "le montant ne doit pas figurer dans l'avis : {interdit}"
            );
        }
    }

    #[tokio::test]
    async fn security_une_fiche_prestataire_disparue_n_interrompt_pas_le_balayage() {
        // Le devis reste éteint : son extinction ne dépend pas de la
        // possibilité de prévenir.
        let devis_repo = DevisMemoire {
            vivants: RefCell::new(vec![devis(Uuid::new_v4())]),
            ..Default::default()
        };
        let notifieur = NotifieurFactice::default();

        let bilan = balayer(
            &devis_repo,
            &PrestatairesMemoire { fiche: None },
            &AbonnementsMemoire::default(),
            Some(&notifieur),
        )
        .await;

        assert_eq!(bilan.eteints, 1);
        assert_eq!(bilan.notifies, 0);
    }
}
