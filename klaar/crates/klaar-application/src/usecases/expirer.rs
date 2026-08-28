//! Cas d'usage « éteindre les tours de diffusion écoulés » (FR-015, Story 3.6).
//!
//! **Trente secondes sans réponse valent une réponse.** Quelqu'un devant une
//! fuite a besoin de savoir que personne ne vient, pour élargir ou appeler
//! ailleurs. Laisser sa Demande diffusée indéfiniment le laisserait attendre
//! sans rien lui dire, ce qui est la pire des trois issues.
//!
//! **La sélection et l'extinction sont une seule opération.** Le dépôt rend les
//! Demandes qu'il vient d'éteindre, et elles seules : deux passages du balayage
//! ne peuvent donc pas notifier deux fois le même demandeur.
//!
//! **La notification n'est pas la bascule.** Une panne du service de push ne
//! doit pas laisser une Demande diffusée pour toujours : le statut est écrit
//! d'abord, l'avis suit, et son échec est journalisé sans rien annuler. Le
//! demandeur verra l'état réel en ouvrant l'application.

use chrono::{DateTime, Utc};
use klaar_shared_kernel::Locale;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::push::PushNotifier;
use crate::ports::push_repository::PushSubscriptionRepository;
use crate::usecases::notifier::notifier_sans_reponse;

/// Demandes traitées en un passage.
///
/// Borne un rattrapage après une longue interruption : sans elle, un balayage
/// qui reprend après une panne tenterait de tout traiter d'un coup, et
/// tiendrait la table sous verrou pendant ce temps.
pub const PAR_PASSAGE_MAX: i64 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BilanExpiration {
    /// Demandes passées en `NO_MATCH`.
    pub eteintes: usize,
    /// Demandeurs joints sur au moins un appareil.
    pub notifies: usize,
}

/// Éteint les tours écoulés et prévient les demandeurs.
pub async fn expirer_les_tours<D, A, N, H>(
    demandes: &D,
    abonnements: &A,
    notifieur: Option<&N>,
    horloge: &H,
    locale: Locale,
) -> Result<BilanExpiration, RepositoryError>
where
    D: DemandeRepository,
    A: PushSubscriptionRepository,
    N: PushNotifier,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let echeance = echeance(maintenant);

    let eteintes = demandes.expirer_echues(echeance, PAR_PASSAGE_MAX).await?;
    let mut bilan = BilanExpiration {
        eteintes: eteintes.len(),
        notifies: 0,
    };

    // Sans clé VAPID configurée, le service tourne sans notifications : c'est un
    // mode de fonctionnement légitime, pas une panne, et les Demandes doivent
    // s'éteindre quand même.
    let Some(notifieur) = notifieur else {
        return Ok(bilan);
    };

    for demande in &eteintes {
        match notifier_sans_reponse(abonnements, notifieur, demande, locale).await {
            Ok(envoi) if envoi.notifies > 0 => bilan.notifies += 1,
            Ok(_) => {}
            Err(e) => {
                // Le statut est déjà écrit. Interrompre ici laisserait les
                // Demandes suivantes éteintes et leurs auteurs sans avis, pour
                // une panne qui ne les concerne pas.
                tracing::error!(erreur = %e, "avis de fin de tour impossible");
            }
        }
    }

    Ok(bilan)
}

/// Instant avant lequel un tour commencé est écoulé.
///
/// Extrait pour être testable : c'est le seul calcul de ce cas d'usage, et
/// c'est celui où une erreur de signe éteindrait toutes les Demandes au lieu
/// des seules échues.
fn echeance(maintenant: DateTime<Utc>) -> DateTime<Utc> {
    maintenant - chrono::Duration::seconds(klaar_matching::DUREE_DIFFUSION_SECONDES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::push::{PushError, PushMessage, PushSubscription};
    use crate::ports::push_repository::AbonnementEnregistre;
    use chrono::{Duration, TimeZone};
    use klaar_catalog::CodeCatalogue;
    use klaar_matching::{Demande, StatutDemande, Urgence, DUREE_DIFFUSION_SECONDES};
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;
    use uuid::Uuid;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn demande(demandeur_id: Uuid) -> Demande {
        Demande::soumettre(
            demandeur_id,
            CodeCatalogue::parse("plomberie").unwrap(),
            "Fuite",
            Geo::new(50.8467, 4.3525).unwrap(),
            Urgence::Haute,
            instant(),
        )
        .unwrap()
    }

    #[derive(Default)]
    struct DemandesMemoire {
        vivantes: RefCell<Vec<Demande>>,
        appels: RefCell<usize>,
    }

    impl DemandeRepository for DemandesMemoire {
        async fn creer(&self, _: &Demande) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn par_id(&self, _: Uuid) -> Result<Option<Demande>, RepositoryError> {
            unreachable!()
        }
        async fn doublon_recent(
            &self,
            _: Uuid,
            _: &CodeCatalogue,
            _: Geo,
            _: DateTime<Utc>,
        ) -> Result<Option<Demande>, RepositoryError> {
            unreachable!()
        }
        async fn changer_statut(
            &self,
            _: Uuid,
            _: StatutDemande,
            _: DateTime<Utc>,
        ) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn expirer_echues(
            &self,
            avant: DateTime<Utc>,
            limite: i64,
        ) -> Result<Vec<Demande>, RepositoryError> {
            *self.appels.borrow_mut() += 1;
            let mut eteintes = Vec::new();
            // Le double rend ce que le SQL rend : les lignes qu'il vient
            // d'écrire, retirées de la population diffusée.
            self.vivantes.borrow_mut().retain(|d| {
                let echue = d.statut == StatutDemande::Diffusion
                    && d.diffuse_depuis <= avant
                    && (eteintes.len() as i64) < limite;
                if echue {
                    let mut eteinte = d.clone();
                    eteinte.statut = StatutDemande::SansReponse;
                    eteintes.push(eteinte);
                }
                !echue
            });
            Ok(eteintes)
        }
        async fn annuler(
            &self,
            _: Uuid,
            _: Option<klaar_matching::MotifAnnulation>,
        ) -> Result<bool, RepositoryError> {
            unreachable!()
        }
        async fn relancer(&self, _: &Demande) -> Result<bool, RepositoryError> {
            unreachable!()
        }
        async fn proposees_a(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> Result<Vec<crate::ports::demande_repository::DemandeProposee>, RepositoryError>
        {
            unreachable!()
        }
        async fn compter_depuis_une_heure(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> Result<i64, RepositoryError> {
            unreachable!()
        }
    }

    #[derive(Default)]
    struct AbonnementsMemoire {
        par_sujet: RefCell<Vec<(Uuid, String)>>,
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
        async fn supprimer_par_endpoint(&self, _: &str) -> Result<bool, RepositoryError> {
            Ok(true)
        }
        async fn compter(&self) -> Result<i64, RepositoryError> {
            unreachable!()
        }
    }

    #[derive(Default)]
    struct NotifieurFactice {
        envoyes: RefCell<Vec<PushMessage>>,
        en_panne: bool,
    }

    impl PushNotifier for NotifieurFactice {
        async fn envoyer(
            &self,
            _: &PushSubscription,
            message: &PushMessage,
        ) -> Result<(), PushError> {
            if self.en_panne {
                return Err(PushError::Transport("test".into()));
            }
            self.envoyes.borrow_mut().push(message.clone());
            Ok(())
        }
    }

    async fn balayer(
        demandes: &DemandesMemoire,
        abonnements: &AbonnementsMemoire,
        notifieur: Option<&NotifieurFactice>,
        maintenant: DateTime<Utc>,
    ) -> BilanExpiration {
        expirer_les_tours(
            demandes,
            abonnements,
            notifieur,
            &HorlogeFigee(maintenant),
            Locale::Fr,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn happy_un_tour_ecoule_est_eteint_et_son_auteur_prevenu() {
        let auteur = Uuid::new_v4();
        let demandes = DemandesMemoire::default();
        demandes.vivantes.borrow_mut().push(demande(auteur));
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((auteur, "https://push.example.net/a".into()));
        let notifieur = NotifieurFactice::default();

        let bilan = balayer(
            &demandes,
            &abonnements,
            Some(&notifieur),
            instant() + Duration::seconds(DUREE_DIFFUSION_SECONDES),
        )
        .await;
        assert_eq!(bilan.eteintes, 1);
        assert_eq!(bilan.notifies, 1);
        // Le message dit quoi faire ensuite : « personne n'a répondu » sans
        // suite laisserait quelqu'un devant une fuite sans savoir quoi faire.
        assert!(!notifieur.envoyes.borrow()[0].corps.is_empty());
    }

    #[tokio::test]
    async fn happy_une_demande_encore_dans_sa_fenetre_est_laissee_tranquille() {
        let demandes = DemandesMemoire::default();
        demandes.vivantes.borrow_mut().push(demande(Uuid::new_v4()));

        let bilan = balayer(
            &demandes,
            &AbonnementsMemoire::default(),
            Some(&NotifieurFactice::default()),
            instant() + Duration::seconds(DUREE_DIFFUSION_SECONDES - 1),
        )
        .await;
        assert_eq!(bilan.eteintes, 0);
        assert_eq!(demandes.vivantes.borrow().len(), 1);
    }

    #[tokio::test]
    async fn edge_un_second_passage_ne_renotifie_personne() {
        // Le dépôt ne rend que ce qu'il vient d'éteindre : c'est ce qui évite
        // de réveiller deux fois quelqu'un pour la même mauvaise nouvelle.
        let auteur = Uuid::new_v4();
        let demandes = DemandesMemoire::default();
        demandes.vivantes.borrow_mut().push(demande(auteur));
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((auteur, "https://push.example.net/a".into()));
        let notifieur = NotifieurFactice::default();
        let apres = instant() + Duration::seconds(60);

        balayer(&demandes, &abonnements, Some(&notifieur), apres).await;
        let second = balayer(&demandes, &abonnements, Some(&notifieur), apres).await;
        assert_eq!(second.eteintes, 0);
        assert_eq!(notifieur.envoyes.borrow().len(), 1);
    }

    #[tokio::test]
    async fn edge_un_demandeur_sans_abonnement_n_empeche_pas_l_extinction() {
        let demandes = DemandesMemoire::default();
        demandes.vivantes.borrow_mut().push(demande(Uuid::new_v4()));

        let bilan = balayer(
            &demandes,
            &AbonnementsMemoire::default(),
            Some(&NotifieurFactice::default()),
            instant() + Duration::seconds(60),
        )
        .await;
        assert_eq!(bilan.eteintes, 1);
        assert_eq!(bilan.notifies, 0);
    }

    #[tokio::test]
    async fn edge_sans_service_de_push_les_demandes_s_eteignent_quand_meme() {
        let demandes = DemandesMemoire::default();
        demandes.vivantes.borrow_mut().push(demande(Uuid::new_v4()));

        let bilan = expirer_les_tours(
            &demandes,
            &AbonnementsMemoire::default(),
            None::<&NotifieurFactice>,
            &HorlogeFigee(instant() + Duration::seconds(60)),
            Locale::Fr,
        )
        .await
        .unwrap();
        assert_eq!(bilan.eteintes, 1);
    }

    #[tokio::test]
    async fn security_une_panne_de_push_n_empeche_pas_l_extinction() {
        // Sinon une panne du service de push laisserait des Demandes diffusées
        // pour toujours, et leurs auteurs à attendre.
        let auteur = Uuid::new_v4();
        let demandes = DemandesMemoire::default();
        demandes.vivantes.borrow_mut().push(demande(auteur));
        let abonnements = AbonnementsMemoire::default();
        abonnements
            .par_sujet
            .borrow_mut()
            .push((auteur, "https://push.example.net/a".into()));
        let notifieur = NotifieurFactice {
            en_panne: true,
            ..Default::default()
        };

        let bilan = balayer(
            &demandes,
            &abonnements,
            Some(&notifieur),
            instant() + Duration::seconds(60),
        )
        .await;
        assert_eq!(bilan.eteintes, 1);
        assert_eq!(bilan.notifies, 0);
    }

    #[tokio::test]
    async fn security_l_echeance_regarde_en_arriere_et_non_en_avant() {
        // Une erreur de signe ici éteindrait toutes les Demandes, y compris
        // celles qui viennent d'être soumises.
        assert!(echeance(instant()) < instant());
        assert_eq!(
            instant() - echeance(instant()),
            Duration::seconds(DUREE_DIFFUSION_SECONDES)
        );
    }

    #[tokio::test]
    async fn edge_un_passage_est_borne() {
        let demandes = DemandesMemoire::default();
        for _ in 0..(PAR_PASSAGE_MAX + 5) {
            demandes.vivantes.borrow_mut().push(demande(Uuid::new_v4()));
        }

        let bilan = balayer(
            &demandes,
            &AbonnementsMemoire::default(),
            None,
            instant() + Duration::seconds(60),
        )
        .await;
        assert_eq!(bilan.eteintes, PAR_PASSAGE_MAX as usize);
        assert_eq!(demandes.vivantes.borrow().len(), 5);
    }
}
