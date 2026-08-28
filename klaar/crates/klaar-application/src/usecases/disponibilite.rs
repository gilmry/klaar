//! Cas d'usage « gérer sa disponibilité » (Story 3.7).
//!
//! **Trois notions distinctes, et c'est le point.** Un prestataire peut être
//! écarté du matching pour trois raisons qui n'ont rien à voir : son statut
//! (suspendu, en attente de contrôle), sa disponibilité (« je suis en congé »),
//! et son occupation (une Mission en cours). Les confondre ferait d'une pause
//! une sanction, ou laisserait notifier quelqu'un qui ne peut pas répondre.
//!
//! Ce cas d'usage ne touche que la deuxième. Le statut relève du contrôle
//! d'entreprise, et l'occupation ne se règle pas : elle se lit dans les
//! Missions en cours, et disparaît quand elles se terminent.

use klaar_identity::{Provider, ProviderError};
use std::fmt;
use uuid::Uuid;

use crate::ports::erreurs::RepositoryError;
use crate::ports::mission_repository::MissionRepository;
use crate::ports::provider_repository::ProviderRepository;

/// Ce qu'un prestataire voit de son propre état.
#[derive(Debug, Clone, PartialEq)]
pub struct EtatDisponibilite {
    pub provider_id: Uuid,
    /// Statut du prestataire : `PENDING_KYC`, `ACTIVE` ou `SUSPENDED`.
    pub statut: &'static str,
    pub disponible: bool,
    pub rayon_intervention_metres: f64,
    /// Vrai si une Mission en cours l'empêche d'en prendre une autre.
    ///
    /// Ne se règle pas : c'est un fait, pas un réglage. L'exposer évite qu'un
    /// prestataire en service et pourtant jamais sollicité en conclue que le
    /// service est cassé.
    pub occupe: bool,
    /// Vrai s'il reçoit effectivement des Demandes en ce moment.
    ///
    /// La conjonction des trois : c'est la seule réponse à la question qu'il se
    /// pose réellement.
    pub sollicitable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurDisponibilite {
    /// Le compte n'a pas de fiche prestataire.
    PasPrestataire,
    /// Rayon hors des bornes utiles.
    Reglage(ProviderError),
    Indisponible(String),
}

impl ErreurDisponibilite {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PasPrestataire => "NOT_A_PROVIDER",
            Self::Reglage(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurDisponibilite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasPrestataire => write!(f, "ce compte n'est pas un prestataire"),
            Self::Reglage(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurDisponibilite {}

impl From<RepositoryError> for ErreurDisponibilite {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Lit l'état de disponibilité du prestataire attaché à un compte.
pub async fn consulter<P, M>(
    prestataires: &P,
    missions: &M,
    utilisateur_id: Uuid,
) -> Result<EtatDisponibilite, ErreurDisponibilite>
where
    P: ProviderRepository,
    M: MissionRepository,
{
    let provider = prestataires
        .par_utilisateur_id(utilisateur_id)
        .await?
        .ok_or(ErreurDisponibilite::PasPrestataire)?;
    etat(&provider, missions).await
}

/// Se met en service ou en pause, et règle son rayon d'intervention.
///
/// Les deux réglages sont facultatifs et indépendants : reprendre le service
/// ne doit pas réinitialiser un rayon, et changer de rayon ne doit pas sortir
/// quelqu'un de sa pause.
pub async fn regler<P, M>(
    prestataires: &P,
    missions: &M,
    utilisateur_id: Uuid,
    disponible: Option<bool>,
    rayon_metres: Option<f64>,
) -> Result<EtatDisponibilite, ErreurDisponibilite>
where
    P: ProviderRepository,
    M: MissionRepository,
{
    let mut provider = prestataires
        .par_utilisateur_id(utilisateur_id)
        .await?
        .ok_or(ErreurDisponibilite::PasPrestataire)?;

    // Le rayon d'abord, parce qu'il peut échouer : valider avant d'écrire quoi
    // que ce soit évite de laisser le prestataire en service avec un rayon
    // refusé, c'est-à-dire dans un état qu'il n'a pas demandé.
    if let Some(metres) = rayon_metres {
        provider
            .definir_rayon_intervention(metres)
            .map_err(ErreurDisponibilite::Reglage)?;
        prestataires
            .definir_rayon_intervention(provider.id, metres)
            .await?;
    }

    if let Some(en_service) = disponible {
        provider.definir_disponibilite(en_service);
        prestataires
            .definir_disponibilite(provider.id, en_service)
            .await?;
    }

    etat(&provider, missions).await
}

async fn etat<M>(
    provider: &Provider,
    missions: &M,
) -> Result<EtatDisponibilite, ErreurDisponibilite>
where
    M: MissionRepository,
{
    let occupe = missions.en_cours_pour(provider.id).await?.is_some();
    Ok(EtatDisponibilite {
        provider_id: provider.id,
        statut: provider.statut.as_str(),
        disponible: provider.disponible,
        rayon_intervention_metres: provider.rayon_intervention_metres,
        occupe,
        // L'occupation entre ici, alors qu'elle n'est pas dans
        // `peut_etre_sollicite` : le domaine ne connaît pas les Missions, mais
        // le prestataire, lui, veut savoir s'il reçoit des Demandes ou non.
        sollicitable: provider.peut_etre_sollicite() && !occupe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::provider_repository::ProviderProche;
    use chrono::{DateTime, TimeZone, Utc};
    use klaar_catalog::CodeCatalogue;
    use klaar_identity::{
        NumeroBce, OrigineKyc, StatutProvider, RAYON_INTERVENTION_DEFAUT, RAYON_INTERVENTION_MAX,
        RAYON_INTERVENTION_MIN,
    };
    use klaar_intervention::Mission;
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;

    use crate::ports::mission_repository::ResultatAttribution;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn provider(statut: StatutProvider, disponible: bool) -> Provider {
        let corps = 1_234_567u64;
        Provider {
            id: Uuid::new_v4(),
            utilisateur_id: Uuid::new_v4(),
            numero_bce: NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).unwrap(),
            raison_sociale: "Prestataire".to_string(),
            base: Geo::new(50.8467, 4.3525).unwrap(),
            statut,
            origine_kyc: Some(OrigineKyc::Demonstration),
            kyc_verifie_le: Some(instant()),
            competences: vec![CodeCatalogue::parse("plomberie").unwrap()],
            disponible,
            rayon_intervention_metres: RAYON_INTERVENTION_DEFAUT,
            cree_le: instant(),
        }
    }

    #[derive(Default)]
    struct PrestatairesMemoire {
        fiche: Option<Provider>,
        disponibilites: RefCell<Vec<bool>>,
        rayons: RefCell<Vec<f64>>,
    }

    impl ProviderRepository for PrestatairesMemoire {
        async fn creer(&self, _: &Provider) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn par_id(&self, _: Uuid) -> Result<Option<Provider>, RepositoryError> {
            unreachable!()
        }
        async fn par_numero_bce(&self, _: &NumeroBce) -> Result<Option<Provider>, RepositoryError> {
            unreachable!()
        }
        async fn par_utilisateur_id(&self, _: Uuid) -> Result<Option<Provider>, RepositoryError> {
            Ok(self.fiche.clone())
        }
        async fn mettre_a_jour_etat(&self, _: &Provider) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn definir_disponibilite(
            &self,
            _: Uuid,
            disponible: bool,
        ) -> Result<(), RepositoryError> {
            self.disponibilites.borrow_mut().push(disponible);
            Ok(())
        }
        async fn definir_rayon_intervention(
            &self,
            _: Uuid,
            metres: f64,
        ) -> Result<(), RepositoryError> {
            self.rayons.borrow_mut().push(metres);
            Ok(())
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
    struct MissionsMemoire {
        en_cours: bool,
    }

    impl MissionRepository for MissionsMemoire {
        async fn attribuer(
            &self,
            _: Uuid,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> Result<ResultatAttribution, RepositoryError> {
            unreachable!()
        }
        async fn par_demande(&self, _: Uuid) -> Result<Option<Mission>, RepositoryError> {
            unreachable!()
        }
        async fn par_id(&self, _: Uuid) -> Result<Option<Mission>, RepositoryError> {
            unreachable!()
        }
        async fn transiter(
            &self,
            _: Uuid,
            _: klaar_intervention::StatutMission,
            _: &klaar_intervention::TransitionMission,
        ) -> Result<bool, RepositoryError> {
            unreachable!()
        }
        async fn en_cours_pour(
            &self,
            provider_id: Uuid,
        ) -> Result<Option<Mission>, RepositoryError> {
            Ok(self
                .en_cours
                .then(|| Mission::attribuer(Uuid::new_v4(), provider_id, instant())))
        }
    }

    #[tokio::test]
    async fn happy_un_prestataire_actif_en_service_et_libre_est_sollicitable() {
        let p = provider(StatutProvider::Actif, true);
        let compte = p.utilisateur_id;
        let etat = consulter(
            &PrestatairesMemoire {
                fiche: Some(p),
                ..Default::default()
            },
            &MissionsMemoire::default(),
            compte,
        )
        .await
        .unwrap();
        assert!(etat.sollicitable);
        assert!(!etat.occupe);
        assert_eq!(etat.statut, "ACTIVE");
    }

    #[tokio::test]
    async fn happy_la_mise_en_pause_est_ecrite() {
        let p = provider(StatutProvider::Actif, true);
        let compte = p.utilisateur_id;
        let depot = PrestatairesMemoire {
            fiche: Some(p),
            ..Default::default()
        };

        let etat = regler(
            &depot,
            &MissionsMemoire::default(),
            compte,
            Some(false),
            None,
        )
        .await
        .unwrap();
        assert!(!etat.disponible);
        assert!(!etat.sollicitable);
        assert_eq!(*depot.disponibilites.borrow(), vec![false]);
        assert!(
            depot.rayons.borrow().is_empty(),
            "le rayon ne doit pas bouger"
        );
    }

    #[tokio::test]
    async fn happy_le_rayon_se_regle_sans_toucher_a_la_disponibilite() {
        // Changer de rayon ne doit pas sortir quelqu'un de sa pause.
        let p = provider(StatutProvider::Actif, false);
        let compte = p.utilisateur_id;
        let depot = PrestatairesMemoire {
            fiche: Some(p),
            ..Default::default()
        };

        let etat = regler(
            &depot,
            &MissionsMemoire::default(),
            compte,
            None,
            Some(3_000.0),
        )
        .await
        .unwrap();
        assert_eq!(etat.rayon_intervention_metres, 3_000.0);
        assert!(!etat.disponible);
        assert!(depot.disponibilites.borrow().is_empty());
        assert_eq!(*depot.rayons.borrow(), vec![3_000.0]);
    }

    #[tokio::test]
    async fn happy_les_deux_reglages_passent_ensemble() {
        let p = provider(StatutProvider::Actif, false);
        let compte = p.utilisateur_id;
        let depot = PrestatairesMemoire {
            fiche: Some(p),
            ..Default::default()
        };

        let etat = regler(
            &depot,
            &MissionsMemoire::default(),
            compte,
            Some(true),
            Some(RAYON_INTERVENTION_MIN),
        )
        .await
        .unwrap();
        assert!(etat.disponible);
        assert_eq!(etat.rayon_intervention_metres, RAYON_INTERVENTION_MIN);
    }

    #[tokio::test]
    async fn negative_un_compte_sans_fiche_prestataire_est_refuse() {
        let e = consulter(
            &PrestatairesMemoire::default(),
            &MissionsMemoire::default(),
            Uuid::new_v4(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "NOT_A_PROVIDER");
    }

    #[tokio::test]
    async fn negative_un_rayon_hors_bornes_est_refuse_sans_rien_ecrire() {
        // Valider avant d'écrire évite de laisser le prestataire en service
        // avec un rayon refusé, c'est-à-dire dans un état qu'il n'a pas demandé.
        let p = provider(StatutProvider::Actif, false);
        let compte = p.utilisateur_id;
        let depot = PrestatairesMemoire {
            fiche: Some(p),
            ..Default::default()
        };

        let e = regler(
            &depot,
            &MissionsMemoire::default(),
            compte,
            Some(true),
            Some(RAYON_INTERVENTION_MAX + 1.0),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "SERVICE_RADIUS_OUT_OF_RANGE");
        assert!(depot.disponibilites.borrow().is_empty());
        assert!(depot.rayons.borrow().is_empty());
    }

    #[tokio::test]
    async fn edge_un_prestataire_occupe_n_est_pas_sollicitable_meme_en_service() {
        // Le notifier lui ferait ouvrir l'application pour se voir refuser, et
        // volerait sa place à quelqu'un de libre.
        let p = provider(StatutProvider::Actif, true);
        let compte = p.utilisateur_id;
        let etat = consulter(
            &PrestatairesMemoire {
                fiche: Some(p),
                ..Default::default()
            },
            &MissionsMemoire { en_cours: true },
            compte,
        )
        .await
        .unwrap();
        assert!(etat.occupe);
        assert!(etat.disponible, "il n'est pas en pause pour autant");
        assert!(!etat.sollicitable);
    }

    #[tokio::test]
    async fn edge_l_occupation_ne_se_regle_pas() {
        // C'est un fait, pas un réglage : se déclarer en service ne libère pas
        // une Mission en cours.
        let p = provider(StatutProvider::Actif, true);
        let compte = p.utilisateur_id;
        let depot = PrestatairesMemoire {
            fiche: Some(p),
            ..Default::default()
        };

        let etat = regler(
            &depot,
            &MissionsMemoire { en_cours: true },
            compte,
            Some(true),
            None,
        )
        .await
        .unwrap();
        assert!(etat.occupe);
        assert!(!etat.sollicitable);
    }

    #[tokio::test]
    async fn security_un_suspendu_qui_se_met_en_service_ne_redevient_pas_sollicitable() {
        // Une pause n'est pas une radiation, et lever une pause ne lève pas une
        // suspension : ce sont deux décisions de deux personnes différentes.
        let p = provider(StatutProvider::Suspendu, false);
        let compte = p.utilisateur_id;
        let depot = PrestatairesMemoire {
            fiche: Some(p),
            ..Default::default()
        };

        let etat = regler(
            &depot,
            &MissionsMemoire::default(),
            compte,
            Some(true),
            None,
        )
        .await
        .unwrap();
        assert!(etat.disponible);
        assert!(!etat.sollicitable);
        assert_eq!(etat.statut, "SUSPENDED");
    }

    #[tokio::test]
    async fn security_un_prestataire_en_attente_de_controle_reste_ecarte() {
        let p = provider(StatutProvider::EnAttenteKyc, true);
        let compte = p.utilisateur_id;
        let etat = consulter(
            &PrestatairesMemoire {
                fiche: Some(p),
                ..Default::default()
            },
            &MissionsMemoire::default(),
            compte,
        )
        .await
        .unwrap();
        assert!(!etat.sollicitable);
        assert_eq!(etat.statut, "PENDING_KYC");
    }

    #[tokio::test]
    async fn security_le_reglage_ne_porte_que_sur_sa_propre_fiche() {
        // La fiche est retrouvée par le compte du jeton, jamais par un
        // identifiant reçu : il n'existe aucun chemin pour régler celle d'un
        // autre.
        let p = provider(StatutProvider::Actif, true);
        let sien = p.id;
        let compte = p.utilisateur_id;
        let depot = PrestatairesMemoire {
            fiche: Some(p),
            ..Default::default()
        };

        let etat = regler(
            &depot,
            &MissionsMemoire::default(),
            compte,
            Some(false),
            None,
        )
        .await
        .unwrap();
        assert_eq!(etat.provider_id, sien);
    }
}
