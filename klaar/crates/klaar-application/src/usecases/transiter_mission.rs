//! Cas d'usage « faire avancer une Mission » (FR-018, Story 4.3).
//!
//! **Seul le prestataire attribué peut faire avancer sa Mission.** Le contrôle
//! passe par la fiche prestataire attachée au compte du jeton, jamais par un
//! identifiant reçu : accepter un `provider_id` en entrée laisserait déclarer
//! « je suis sur place » au nom d'un autre.
//!
//! **La machine à états vit dans le domaine, pas ici.** Ce cas d'usage lit,
//! délègue la décision à `Mission::transiter`, écrit, puis prévient. Il ne
//! connaît aucune transition permise, ce qui est exactement ce qu'il faut pour
//! que FR-021 et FR-022 en ajoutent sans le toucher.

use chrono::{DateTime, Utc};
use klaar_intervention::{Mission, MissionError, StatutMission, TransitionMission};
use klaar_shared_kernel::Geo;
use std::fmt;
use uuid::Uuid;

use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::mission_repository::MissionRepository;
use crate::ports::provider_repository::ProviderRepository;

#[derive(Debug, Clone, PartialEq)]
pub enum ErreurTransition {
    /// Le compte n'a pas de fiche prestataire.
    PasPrestataire,
    /// Mission inconnue, ou attribuée à quelqu'un d'autre.
    ///
    /// Un seul cas pour les deux : distinguer « elle n'existe pas » de « elle
    /// n'est pas à vous » laisserait apprendre quelles Missions existent en
    /// essayant des identifiants. FR-018 `@negative` demande un 403 pour le
    /// prestataire non attribué ; c'est un 404 qui est rendu, par la même
    /// précédence anti-énumération que les routes de Demande.
    Introuvable,
    /// Statut cible inconnu du vocabulaire.
    StatutInconnu,
    /// Transition refusée par la machine à états, ou horodatage invraisemblable.
    Domaine(MissionError),
    /// La Mission a changé d'état entre la lecture et l'écriture.
    Concurrence,
    Indisponible(String),
}

impl ErreurTransition {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PasPrestataire => "NOT_A_PROVIDER",
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::StatutInconnu => "UNKNOWN_STATUS",
            Self::Domaine(e) => e.code(),
            // Le même code qu'une transition interdite, et c'est juste : dans
            // les deux cas, la Mission n'était pas dans l'état d'où le
            // prestataire croyait partir.
            Self::Concurrence => "INVALID_TRANSITION",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasPrestataire => write!(f, "ce compte n'est pas un prestataire"),
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::StatutInconnu => write!(f, "statut inconnu"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Concurrence => write!(f, "la Mission a changé d'état entre-temps"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurTransition {}

impl From<RepositoryError> for ErreurTransition {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que le prestataire déclare.
///
/// Groupé plutôt qu'égrené en paramètres : les trois champs viennent du même
/// corps de requête et voyagent toujours ensemble, et une liste de huit
/// arguments finit par se remplir dans le mauvais ordre.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration<'a> {
    /// Statut visé, dans le vocabulaire de FR-018.
    pub statut_cible: &'a str,
    /// Instant déclaré par le client, pour une transition faite hors connexion.
    pub horodate_le: Option<DateTime<Utc>>,
    /// Position au moment de la transition. Facultative.
    pub position: Option<Geo>,
}

/// Ce que la transition produit.
#[derive(Debug, Clone, PartialEq)]
pub struct Avancement {
    pub mission: Mission,
    pub entree: TransitionMission,
}

/// Fait avancer une Mission d'un statut au suivant.
pub async fn transiter<P, M, H>(
    prestataires: &P,
    missions: &M,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    declaration: Declaration<'_>,
) -> Result<Avancement, ErreurTransition>
where
    P: ProviderRepository,
    M: MissionRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();

    // Le statut cible est validé avant toute lecture : un vocabulaire inconnu
    // est une erreur de client, et le dire tout de suite évite de renseigner
    // sur l'existence d'une Mission.
    let vers =
        StatutMission::parse(declaration.statut_cible).ok_or(ErreurTransition::StatutInconnu)?;

    let provider = prestataires
        .par_utilisateur_id(utilisateur_id)
        .await?
        .ok_or(ErreurTransition::PasPrestataire)?;

    let mut mission = missions
        .par_id(mission_id)
        .await?
        // Filtré et non comparé après coup : la Mission d'autrui est
        // introuvable, pas interdite.
        .filter(|m| m.appartient_a(provider.id))
        .ok_or(ErreurTransition::Introuvable)?;

    let depuis = mission.statut;
    let entree = mission
        .transiter(
            vers,
            declaration.horodate_le,
            declaration.position,
            maintenant,
        )
        .map_err(ErreurTransition::Domaine)?;

    // La garde du dépôt tranche la course : deux transitions concurrentes
    // depuis le même état ne doivent pas toutes deux aboutir, sinon
    // l'historique porterait deux entrées pour un seul changement.
    if !missions.transiter(mission.id, depuis, &entree).await? {
        return Err(ErreurTransition::Concurrence);
    }

    if entree.hors_zone {
        // FR-018 `@edge`. Une alerte d'exploitation, pas un refus : la Mission
        // continue, et c'est à quelqu'un d'y regarder. Ni la position ni la
        // Mission ne figurent dans le message — le journal applicatif n'a pas à
        // dire où se trouve un prestataire.
        tracing::warn!(
            code = "OUT_OF_ZONE",
            statut = entree.statut.as_str(),
            "transition déclarée hors de la Région"
        );
    }

    Ok(Avancement { mission, entree })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::mission_repository::ResultatAttribution;
    use crate::ports::provider_repository::ProviderProche;
    use chrono::{Duration, TimeZone};
    use klaar_catalog::CodeCatalogue;
    use klaar_identity::{
        NumeroBce, OrigineKyc, Provider, StatutProvider, RAYON_INTERVENTION_DEFAUT,
    };
    use std::cell::RefCell;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn bruxelles() -> Geo {
        Geo::new(50.8467, 4.3525).unwrap()
    }

    fn provider() -> Provider {
        let corps = 1_234_567u64;
        Provider {
            id: Uuid::new_v4(),
            utilisateur_id: Uuid::new_v4(),
            numero_bce: NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).unwrap(),
            raison_sociale: "Prestataire".to_string(),
            base: bruxelles(),
            statut: StatutProvider::Actif,
            origine_kyc: Some(OrigineKyc::Demonstration),
            kyc_verifie_le: Some(instant()),
            competences: vec![CodeCatalogue::parse("plomberie").unwrap()],
            disponible: true,
            rayon_intervention_metres: RAYON_INTERVENTION_DEFAUT,
            cree_le: instant(),
        }
    }

    #[derive(Default)]
    struct PrestatairesMemoire {
        fiche: Option<Provider>,
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

    struct MissionsMemoire {
        mission: Option<Mission>,
        /// `false` simule une transition concurrente qui a gagné la course.
        accorde: bool,
        consignees: RefCell<Vec<TransitionMission>>,
    }

    impl MissionsMemoire {
        fn avec(mission: Mission) -> Self {
            Self {
                mission: Some(mission),
                accorde: true,
                consignees: RefCell::default(),
            }
        }
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
        async fn en_cours_pour(&self, _: Uuid) -> Result<Option<Mission>, RepositoryError> {
            unreachable!()
        }
        async fn par_demande(&self, _: Uuid) -> Result<Option<Mission>, RepositoryError> {
            unreachable!()
        }
        async fn par_id(&self, _: Uuid) -> Result<Option<Mission>, RepositoryError> {
            Ok(self.mission.clone())
        }
        async fn transiter(
            &self,
            _: Uuid,
            _: StatutMission,
            entree: &TransitionMission,
        ) -> Result<bool, RepositoryError> {
            if self.accorde {
                self.consignees.borrow_mut().push(entree.clone());
            }
            Ok(self.accorde)
        }
    }

    async fn tenter(
        fiche: Option<Provider>,
        depot: &MissionsMemoire,
        compte: Uuid,
        mission_id: Uuid,
        cible: &str,
        horodate_le: Option<DateTime<Utc>>,
        position: Option<Geo>,
    ) -> Result<Avancement, ErreurTransition> {
        transiter(
            &PrestatairesMemoire { fiche },
            depot,
            &HorlogeFigee(instant()),
            compte,
            mission_id,
            Declaration {
                statut_cible: cible,
                horodate_le,
                position,
            },
        )
        .await
    }

    #[tokio::test]
    async fn happy_le_prestataire_attribue_fait_avancer_sa_mission() {
        let p = provider();
        let m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        let (compte, id) = (p.utilisateur_id, m.id);
        let depot = MissionsMemoire::avec(m);

        let r = tenter(
            Some(p),
            &depot,
            compte,
            id,
            "PROVIDER_EN_ROUTE",
            None,
            Some(bruxelles()),
        )
        .await
        .unwrap();
        assert_eq!(r.mission.statut, StatutMission::EnRoute);
        assert_eq!(r.entree.statut, StatutMission::EnRoute);
        assert_eq!(depot.consignees.borrow().len(), 1);
    }

    #[tokio::test]
    async fn happy_l_entree_consignee_porte_le_prestataire_et_l_instant() {
        // FR-018 `@security` : status, ts, geo, provider_id.
        let p = provider();
        let m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        let (compte, id, pid) = (p.utilisateur_id, m.id, p.id);
        let depot = MissionsMemoire::avec(m);

        tenter(
            Some(p),
            &depot,
            compte,
            id,
            "PROVIDER_EN_ROUTE",
            None,
            Some(bruxelles()),
        )
        .await
        .unwrap();
        let entree = depot.consignees.borrow()[0].clone();
        assert_eq!(entree.provider_id, pid);
        assert_eq!(entree.enregistre_le, instant());
        assert_eq!(entree.position, Some(bruxelles()));
    }

    #[tokio::test]
    async fn negative_une_transition_interdite_est_refusee() {
        let p = provider();
        let mut m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        m.statut = StatutMission::Terminee;
        let (compte, id) = (p.utilisateur_id, m.id);
        let depot = MissionsMemoire::avec(m);

        let e = tenter(Some(p), &depot, compte, id, "PROVIDER_EN_ROUTE", None, None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "INVALID_TRANSITION");
        assert!(depot.consignees.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_un_statut_inconnu_est_refuse_avant_toute_lecture() {
        // Dire tout de suite qu'un vocabulaire est inconnu évite de renseigner
        // sur l'existence d'une Mission.
        let depot = MissionsMemoire {
            mission: None,
            accorde: true,
            consignees: RefCell::default(),
        };
        let e = tenter(
            None,
            &depot,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "EN_ROUTE",
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "UNKNOWN_STATUS");
    }

    #[tokio::test]
    async fn negative_un_compte_sans_fiche_prestataire_est_refuse() {
        let depot = MissionsMemoire {
            mission: None,
            accorde: true,
            consignees: RefCell::default(),
        };
        let e = tenter(
            None,
            &depot,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "ON_SITE",
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "NOT_A_PROVIDER");
    }

    #[tokio::test]
    async fn negative_un_horodatage_invraisemblable_est_refuse() {
        let p = provider();
        let m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        let (compte, id) = (p.utilisateur_id, m.id);
        let depot = MissionsMemoire::avec(m);

        let e = tenter(
            Some(p),
            &depot,
            compte,
            id,
            "PROVIDER_EN_ROUTE",
            Some(instant() - Duration::hours(2)),
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "TIMESTAMP_IMPLAUSIBLE");
        assert!(depot.consignees.borrow().is_empty());
    }

    #[tokio::test]
    async fn edge_une_transition_concurrente_perd_la_course() {
        let p = provider();
        let m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        let (compte, id) = (p.utilisateur_id, m.id);
        let depot = MissionsMemoire {
            mission: Some(m),
            accorde: false,
            consignees: RefCell::default(),
        };

        let e = tenter(Some(p), &depot, compte, id, "PROVIDER_EN_ROUTE", None, None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "INVALID_TRANSITION");
    }

    #[tokio::test]
    async fn edge_une_position_hors_region_passe_mais_est_marquee() {
        let anvers = Geo::new(51.2194, 4.4025).unwrap();
        let p = provider();
        let m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        let (compte, id) = (p.utilisateur_id, m.id);
        let depot = MissionsMemoire::avec(m);

        let r = tenter(
            Some(p),
            &depot,
            compte,
            id,
            "PROVIDER_EN_ROUTE",
            None,
            Some(anvers),
        )
        .await
        .unwrap();
        assert!(r.entree.hors_zone);
        assert_eq!(r.mission.statut, StatutMission::EnRoute);
    }

    #[tokio::test]
    async fn edge_sans_position_la_transition_passe_quand_meme() {
        // Exiger la position rendrait l'autorisation de géolocalisation de fait
        // obligatoire, alors que quelqu'un sans GPS doit pouvoir dire qu'il est
        // arrivé.
        let p = provider();
        let mut m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        m.statut = StatutMission::EnRoute;
        let (compte, id) = (p.utilisateur_id, m.id);
        let depot = MissionsMemoire::avec(m);

        let r = tenter(Some(p), &depot, compte, id, "ON_SITE", None, None)
            .await
            .unwrap();
        assert_eq!(r.entree.position, None);
        assert!(!r.entree.hors_zone);
    }

    #[tokio::test]
    async fn security_la_mission_d_un_autre_est_introuvable() {
        // FR-018 `@negative` demande un 403 ; c'est un « introuvable » qui est
        // rendu, par la même précédence anti-énumération que les Demandes.
        let attribue = provider();
        let autre = provider();
        let m = Mission::attribuer(Uuid::new_v4(), attribue.id, instant());
        let (compte, id) = (autre.utilisateur_id, m.id);
        let depot = MissionsMemoire::avec(m);

        let e = tenter(
            Some(autre),
            &depot,
            compte,
            id,
            "PROVIDER_EN_ROUTE",
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "MISSION_NOT_FOUND");
        assert!(depot.consignees.borrow().is_empty());
    }

    #[tokio::test]
    async fn security_une_transition_refusee_ne_consigne_rien() {
        // L'historique doit raconter ce qui s'est passé, pas ce qui a été
        // tenté.
        let p = provider();
        let mut m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        m.statut = StatutMission::Annulee;
        let (compte, id) = (p.utilisateur_id, m.id);
        let depot = MissionsMemoire::avec(m);

        let _ = tenter(Some(p), &depot, compte, id, "ON_SITE", None, None).await;
        assert!(depot.consignees.borrow().is_empty());
    }

    #[tokio::test]
    async fn security_le_parcours_complet_consigne_une_entree_par_etape() {
        let p = provider();
        let mut m = Mission::attribuer(Uuid::new_v4(), p.id, instant());
        let (compte, id) = (p.utilisateur_id, m.id);

        for cible in ["PROVIDER_EN_ROUTE", "ON_SITE", "COMPLETED"] {
            let depot = MissionsMemoire::avec(m.clone());
            let r = tenter(Some(p.clone()), &depot, compte, id, cible, None, None)
                .await
                .unwrap();
            assert_eq!(depot.consignees.borrow().len(), 1, "étape {cible}");
            m = r.mission;
        }
        assert_eq!(m.statut, StatutMission::Terminee);
    }
}
