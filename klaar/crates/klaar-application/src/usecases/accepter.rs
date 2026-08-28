//! Cas d'usage « accepter une Demande » (FR-013, Story 3.4).
//!
//! **Le premier arrivé gagne, et un seul arrive.** Cinq prestataires reçoivent
//! la même notification et peuvent toucher « accepter » dans la même seconde.
//! La garantie n'est pas ici : elle est dans l'opération atomique du dépôt, que
//! la base sérialise. Ce cas d'usage décide *qui a le droit d'essayer*, puis
//! traduit l'issue.
//!
//! **L'éligibilité se vérifie au moment d'accepter, pas au matching.** Un
//! prestataire suspendu entre les deux ne doit pas passer : la notification
//! qu'il a reçue il y a trois minutes ne dit rien de son état présent. C'est
//! FR-013 `@security`, et c'est aussi la raison pour laquelle ce contrôle vient
//! **avant** toute lecture de la Demande — sinon les codes d'erreur rendus à un
//! prestataire suspendu lui diraient quelles Demandes existent et lesquelles
//! sont déjà prises.

use klaar_identity::StatutProvider;
use klaar_intervention::Mission;
use klaar_matching::StatutDemande;
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::mission_repository::{MissionRepository, ResultatAttribution};
use crate::ports::provider_repository::ProviderRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurAcceptation {
    /// Le compte n'a pas de fiche prestataire, ou elle n'est pas active.
    ///
    /// Un seul code pour les deux : distinguer « vous n'êtes pas prestataire »
    /// de « votre compte est suspendu » ne change rien pour l'appelant
    /// légitime, qui le sait déjà, et renseignerait qui essaie au hasard.
    NonEligible,
    Introuvable,
    DejaAttribuee,
    /// Le tour de diffusion est écoulé, ou la Demande est déjà `NO_MATCH`.
    Expiree,
    /// Le demandeur l'a retirée (FR-014, FR-015 `@security`).
    ///
    /// Rendue en 410 et non en 409 : la Demande a existé et n'existe plus,
    /// c'est ce que FR-014 `@edge` demande, et cela distingue « c'est fini »
    /// de « quelqu'un d'autre l'a ».
    Annulee,
    /// Une Mission en cours occupe déjà le prestataire (FR-013 `@edge`).
    Occupe,
    Indisponible(String),
}

impl ErreurAcceptation {
    /// Codes de FR-013, repris tels quels.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NonEligible => "PROVIDER_NOT_ELIGIBLE",
            Self::Introuvable => "REQUEST_NOT_FOUND",
            Self::DejaAttribuee => "REQUEST_ALREADY_MATCHED",
            Self::Expiree => "REQUEST_EXPIRED",
            Self::Annulee => "REQUEST_CANCELLED",
            Self::Occupe => "PROVIDER_BUSY",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurAcceptation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonEligible => write!(f, "prestataire non éligible"),
            Self::Introuvable => write!(f, "Demande introuvable"),
            Self::DejaAttribuee => write!(f, "Demande déjà attribuée"),
            Self::Expiree => write!(f, "Demande expirée"),
            Self::Annulee => write!(f, "Demande annulée par son auteur"),
            Self::Occupe => write!(f, "une Mission est déjà en cours"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurAcceptation {}

impl From<RepositoryError> for ErreurAcceptation {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que l'acceptation produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    pub mission: Mission,
    /// Prestataire retenu, pour prévenir les autres candidats ensuite.
    pub provider_id: Uuid,
}

/// Attribue une Demande au prestataire qui accepte.
///
/// L'ordre des contrôles est délibéré :
/// 1. l'éligibilité du prestataire, avant toute lecture de la Demande ;
/// 2. l'existence de la Demande ;
/// 3. la compétence : un serrurier ne prend pas une fuite d'eau ;
/// 4. la fenêtre de diffusion, que le statut stocké ne dit pas ;
/// 5. l'attribution atomique, qui tranche la course.
pub async fn accepter<P, D, M, H>(
    prestataires: &P,
    demandes: &D,
    missions: &M,
    horloge: &H,
    utilisateur_id: Uuid,
    demande_id: Uuid,
) -> Result<Attribution, ErreurAcceptation>
where
    P: ProviderRepository,
    D: DemandeRepository,
    M: MissionRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();

    let provider = prestataires
        .par_utilisateur_id(utilisateur_id)
        .await?
        .ok_or(ErreurAcceptation::NonEligible)?;
    // L'état est relu maintenant, pas celui qui valait au matching : un
    // prestataire suspendu entre-temps ne doit pas passer (FR-013 `@security`).
    // La disponibilité n'entre pas dans le contrôle : elle règle qui **reçoit**
    // les Demandes, alors qu'accepter est un acte délibéré. Refuser ici ferait
    // rater une intervention à quelqu'un qui a rouvert son application.
    if provider.statut != StatutProvider::Actif {
        return Err(ErreurAcceptation::NonEligible);
    }

    let demande = demandes
        .par_id(demande_id)
        .await?
        .ok_or(ErreurAcceptation::Introuvable)?;

    // La compétence est revérifiée ici, alors que le matching ne notifie déjà
    // que des prestataires qui couvrent le secteur. La route est ouverte à tout
    // compte prestataire actif : sans ce contrôle, un serrurier qui connaît
    // l'identifiant d'une Demande pourrait rafler une fuite d'eau, et le
    // demandeur verrait arriver quelqu'un qui ne sait pas la réparer.
    //
    // Le refus arrive **après** la lecture de la Demande, contrairement au
    // contrôle de statut : il a besoin de connaître le secteur. Un prestataire
    // actif apprend donc qu'une Demande existe, ce qui ne dit rien de son état
    // et ne s'exploite pas — les identifiants sont des UUID v4. Un prestataire
    // **non éligible**, lui, ne distingue toujours rien.
    if !provider.couvre(&demande.secteur) {
        return Err(ErreurAcceptation::NonEligible);
    }

    // Avant l'attribution atomique, et séparément d'elle. L'ordre des cas
    // compte : une Demande dont le tour est écoulé n'est pas « déjà prise », et
    // dire l'un pour l'autre enverrait le prestataire chercher un concurrent
    // qui n'existe pas.
    //
    // `NO_MATCH` répond `REQUEST_EXPIRED` et non `REQUEST_ALREADY_MATCHED` :
    // c'est ce que demande FR-015 `@edge`, et c'est aussi la vérité — le tour
    // s'est terminé sans personne.
    match demande.statut {
        StatutDemande::SansReponse => return Err(ErreurAcceptation::Expiree),
        StatutDemande::Annulee => return Err(ErreurAcceptation::Annulee),
        StatutDemande::Attribuee => return Err(ErreurAcceptation::DejaAttribuee),
        StatutDemande::Diffusion if demande.est_expiree(maintenant) => {
            return Err(ErreurAcceptation::Expiree)
        }
        StatutDemande::Diffusion => {}
    }

    match missions
        .attribuer(demande.id, provider.id, maintenant)
        .await?
    {
        ResultatAttribution::Attribuee(mission) => Ok(Attribution {
            mission,
            provider_id: provider.id,
        }),
        // La Demande a changé d'état entre la lecture et l'écriture, ou elle
        // n'était déjà plus diffusée. Les deux se répondent pareil : elle n'est
        // plus à prendre.
        ResultatAttribution::DemandeNonDiffusee => Err(ErreurAcceptation::DejaAttribuee),
        ResultatAttribution::ProviderOccupe => Err(ErreurAcceptation::Occupe),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::provider_repository::ProviderProche;
    use chrono::{DateTime, Utc};
    use chrono::{Duration, TimeZone};
    use klaar_catalog::CodeCatalogue;
    use klaar_identity::{NumeroBce, OrigineKyc, Provider};
    use klaar_matching::{Demande, Urgence, DUREE_DIFFUSION_SECONDES};
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn bruxelles() -> Geo {
        Geo::new(50.8467, 4.3525).unwrap()
    }

    fn demande() -> Demande {
        Demande::soumettre(
            Uuid::new_v4(),
            CodeCatalogue::parse("plomberie").unwrap(),
            "Fuite",
            bruxelles(),
            Urgence::Haute,
            instant(),
        )
        .unwrap()
    }

    fn provider_du_secteur(statut: StatutProvider, secteur: &str) -> Provider {
        let corps = 1_234_567u64;
        Provider {
            id: Uuid::new_v4(),
            utilisateur_id: Uuid::new_v4(),
            numero_bce: NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).unwrap(),
            raison_sociale: "Prestataire".to_string(),
            base: bruxelles(),
            statut,
            origine_kyc: (statut != StatutProvider::EnAttenteKyc)
                .then_some(OrigineKyc::Demonstration),
            kyc_verifie_le: (statut != StatutProvider::EnAttenteKyc).then(instant),
            competences: vec![CodeCatalogue::parse(secteur).unwrap()],
            disponible: true,
            rayon_intervention_metres: klaar_identity::RAYON_INTERVENTION_DEFAUT,
            cree_le: instant(),
        }
    }

    fn provider(statut: StatutProvider) -> Provider {
        provider_du_secteur(statut, "plomberie")
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

    #[derive(Default)]
    struct DemandesMemoire {
        demande: Option<Demande>,
    }

    impl DemandeRepository for DemandesMemoire {
        async fn creer(&self, _: &Demande) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn par_id(&self, _: Uuid) -> Result<Option<Demande>, RepositoryError> {
            Ok(self.demande.clone())
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
            _: DateTime<Utc>,
            _: i64,
        ) -> Result<Vec<Demande>, RepositoryError> {
            unreachable!()
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
        async fn compter_depuis_une_heure(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> Result<i64, RepositoryError> {
            unreachable!()
        }
    }

    /// Double qui rejoue une issue décidée d'avance, et compte les appels.
    ///
    /// L'atomicité ne se teste pas ici : elle appartient à la base, et le test
    /// qui la vérifie lance deux acceptations concurrentes contre PostgreSQL
    /// (`crates/klaar-sqlx-repos/tests/mission.rs`). Ce double sert à vérifier
    /// que le cas d'usage traduit chaque issue et n'essaie qu'une fois.
    struct MissionsMemoire {
        issue: RefCell<Option<ResultatAttribution>>,
        appels: RefCell<usize>,
    }

    impl MissionsMemoire {
        fn rendant(issue: ResultatAttribution) -> Self {
            Self {
                issue: RefCell::new(Some(issue)),
                appels: RefCell::new(0),
            }
        }
    }

    impl MissionRepository for MissionsMemoire {
        async fn attribuer(
            &self,
            demande_id: Uuid,
            provider_id: Uuid,
            maintenant: DateTime<Utc>,
        ) -> Result<ResultatAttribution, RepositoryError> {
            *self.appels.borrow_mut() += 1;
            Ok(match self.issue.borrow_mut().take() {
                Some(ResultatAttribution::Attribuee(_)) => ResultatAttribution::Attribuee(
                    Mission::attribuer(demande_id, provider_id, maintenant),
                ),
                Some(autre) => autre,
                None => ResultatAttribution::DemandeNonDiffusee,
            })
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
        async fn en_cours_pour(&self, _: Uuid) -> Result<Option<Mission>, RepositoryError> {
            unreachable!()
        }
    }

    fn attribuee() -> ResultatAttribution {
        ResultatAttribution::Attribuee(Mission::attribuer(
            Uuid::new_v4(),
            Uuid::new_v4(),
            instant(),
        ))
    }

    async fn tenter(
        provider: Option<Provider>,
        demande: Option<Demande>,
        issue: ResultatAttribution,
        maintenant: DateTime<Utc>,
    ) -> Result<Attribution, ErreurAcceptation> {
        let compte = provider
            .as_ref()
            .map(|p| p.utilisateur_id)
            .unwrap_or_else(Uuid::new_v4);
        let cible = demande.as_ref().map(|d| d.id).unwrap_or_else(Uuid::new_v4);
        accepter(
            &PrestatairesMemoire { fiche: provider },
            &DemandesMemoire { demande },
            &MissionsMemoire::rendant(issue),
            &HorlogeFigee(maintenant),
            compte,
            cible,
        )
        .await
    }

    #[tokio::test]
    async fn happy_un_prestataire_actif_obtient_la_mission() {
        let p = provider(StatutProvider::Actif);
        let d = demande();
        let r = tenter(Some(p.clone()), Some(d.clone()), attribuee(), instant())
            .await
            .unwrap();
        assert_eq!(r.provider_id, p.id);
        assert_eq!(r.mission.demande_id, d.id);
        assert_eq!(r.mission.provider_id, p.id);
        assert_eq!(r.mission.statut.as_str(), "ACCEPTED");
    }

    #[tokio::test]
    async fn happy_la_mission_porte_l_instant_de_l_acceptation() {
        // Et non celui de la Demande : c'est l'acceptation qui engage.
        let plus_tard = instant() + Duration::seconds(10);
        let r = tenter(
            Some(provider(StatutProvider::Actif)),
            Some(demande()),
            attribuee(),
            plus_tard,
        )
        .await
        .unwrap();
        assert_eq!(r.mission.cree_le, plus_tard);
    }

    #[tokio::test]
    async fn negative_un_compte_sans_fiche_prestataire_est_refuse() {
        let e = tenter(None, Some(demande()), attribuee(), instant())
            .await
            .unwrap_err();
        assert_eq!(e.code(), "PROVIDER_NOT_ELIGIBLE");
    }

    #[tokio::test]
    async fn negative_un_prestataire_suspendu_ou_en_attente_est_refuse() {
        for statut in [StatutProvider::Suspendu, StatutProvider::EnAttenteKyc] {
            let e = tenter(
                Some(provider(statut)),
                Some(demande()),
                attribuee(),
                instant(),
            )
            .await
            .unwrap_err();
            assert_eq!(e.code(), "PROVIDER_NOT_ELIGIBLE", "statut {statut:?}");
        }
    }

    #[tokio::test]
    async fn negative_un_prestataire_d_un_autre_secteur_est_refuse() {
        // Un serrurier ne prend pas une fuite d'eau : le demandeur verrait
        // arriver quelqu'un qui ne sait pas la réparer.
        let e = tenter(
            Some(provider_du_secteur(StatutProvider::Actif, "serrurerie")),
            Some(demande()),
            attribuee(),
            instant(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "PROVIDER_NOT_ELIGIBLE");
    }

    #[tokio::test]
    async fn security_un_prestataire_hors_secteur_ne_touche_jamais_la_demande() {
        let missions = MissionsMemoire::rendant(attribuee());
        let p = provider_du_secteur(StatutProvider::Actif, "serrurerie");
        let d = demande();
        let compte = p.utilisateur_id;
        let cible = d.id;
        let _ = accepter(
            &PrestatairesMemoire { fiche: Some(p) },
            &DemandesMemoire { demande: Some(d) },
            &missions,
            &HorlogeFigee(instant()),
            compte,
            cible,
        )
        .await;
        assert_eq!(*missions.appels.borrow(), 0);
    }

    #[tokio::test]
    async fn negative_une_demande_inconnue_rend_introuvable() {
        let e = tenter(
            Some(provider(StatutProvider::Actif)),
            None,
            attribuee(),
            instant(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_NOT_FOUND");
    }

    #[tokio::test]
    async fn negative_une_demande_deja_prise_rend_un_conflit() {
        let e = tenter(
            Some(provider(StatutProvider::Actif)),
            Some(demande()),
            ResultatAttribution::DemandeNonDiffusee,
            instant(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_ALREADY_MATCHED");
    }

    #[tokio::test]
    async fn negative_un_prestataire_deja_en_mission_est_refuse() {
        let e = tenter(
            Some(provider(StatutProvider::Actif)),
            Some(demande()),
            ResultatAttribution::ProviderOccupe,
            instant(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "PROVIDER_BUSY");
    }

    #[tokio::test]
    async fn edge_un_tour_ecoule_rend_expiree() {
        let e = tenter(
            Some(provider(StatutProvider::Actif)),
            Some(demande()),
            attribuee(),
            instant() + Duration::seconds(DUREE_DIFFUSION_SECONDES),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_EXPIRED");
    }

    #[tokio::test]
    async fn negative_une_demande_sans_reponse_rend_expiree() {
        // FR-015 `@edge` : l'accept tardif est rejeté en 410, pas en 409. Le
        // tour s'est terminé sans personne, il n'y a pas de concurrent à aller
        // chercher.
        let mut d = demande();
        d.statut = StatutDemande::SansReponse;
        let e = tenter(
            Some(provider(StatutProvider::Actif)),
            Some(d),
            attribuee(),
            instant(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_EXPIRED");
    }

    #[tokio::test]
    async fn negative_une_demande_annulee_le_dit() {
        let mut d = demande();
        d.statut = StatutDemande::Annulee;
        let e = tenter(
            Some(provider(StatutProvider::Actif)),
            Some(d),
            attribuee(),
            instant(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_CANCELLED");
    }

    #[tokio::test]
    async fn negative_une_demande_deja_attribuee_le_dit_sans_toucher_la_base() {
        let missions = MissionsMemoire::rendant(attribuee());
        let p = provider(StatutProvider::Actif);
        let mut d = demande();
        d.statut = StatutDemande::Attribuee;
        let compte = p.utilisateur_id;
        let cible = d.id;
        let e = accepter(
            &PrestatairesMemoire { fiche: Some(p) },
            &DemandesMemoire { demande: Some(d) },
            &missions,
            &HorlogeFigee(instant()),
            compte,
            cible,
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_ALREADY_MATCHED");
        assert_eq!(*missions.appels.borrow(), 0);
    }

    #[tokio::test]
    async fn edge_expiree_et_deja_prise_repond_expiree() {
        // L'ordre compte : la fenêtre se vérifie avant l'attribution, sinon une
        // Demande d'hier serait attribuable tant que personne ne l'a prise.
        let mut d = demande();
        d.statut = StatutDemande::Diffusion;
        let e = tenter(
            Some(provider(StatutProvider::Actif)),
            Some(d),
            ResultatAttribution::DemandeNonDiffusee,
            instant() + Duration::hours(1),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_EXPIRED");
    }

    #[tokio::test]
    async fn security_un_prestataire_non_eligible_n_apprend_rien_sur_la_demande() {
        // Même code, même refus, que la Demande existe, soit déjà prise ou
        // n'ait jamais existé : sinon les erreurs rendues à quelqu'un qui
        // essaie au hasard lui diraient quelles Demandes sont en cours.
        let suspendu = || Some(provider(StatutProvider::Suspendu));
        let a = tenter(suspendu(), Some(demande()), attribuee(), instant())
            .await
            .unwrap_err();
        let b = tenter(suspendu(), None, attribuee(), instant())
            .await
            .unwrap_err();
        let c = tenter(
            suspendu(),
            Some(demande()),
            ResultatAttribution::DemandeNonDiffusee,
            instant(),
        )
        .await
        .unwrap_err();
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a.code(), "PROVIDER_NOT_ELIGIBLE");
    }

    #[tokio::test]
    async fn security_un_prestataire_refuse_ne_touche_jamais_la_demande() {
        // Le contrôle d'éligibilité vient avant l'attribution : un suspendu ne
        // doit pas pouvoir faire basculer une Demande, même une fois.
        let missions = MissionsMemoire::rendant(attribuee());
        let p = provider(StatutProvider::Suspendu);
        let d = demande();
        let compte = p.utilisateur_id;
        let cible = d.id;
        let _ = accepter(
            &PrestatairesMemoire { fiche: Some(p) },
            &DemandesMemoire { demande: Some(d) },
            &missions,
            &HorlogeFigee(instant()),
            compte,
            cible,
        )
        .await;
        assert_eq!(*missions.appels.borrow(), 0);
    }

    #[tokio::test]
    async fn security_l_attribution_n_est_tentee_qu_une_fois() {
        // Aucun réessai en cas de conflit : reprendre après un échec ferait de
        // la course un concours de patience, et le perdant finirait par
        // arracher une Demande déjà attribuée.
        let missions = MissionsMemoire::rendant(ResultatAttribution::DemandeNonDiffusee);
        let p = provider(StatutProvider::Actif);
        let d = demande();
        let compte = p.utilisateur_id;
        let cible = d.id;
        let _ = accepter(
            &PrestatairesMemoire { fiche: Some(p) },
            &DemandesMemoire { demande: Some(d) },
            &missions,
            &HorlogeFigee(instant()),
            compte,
            cible,
        )
        .await;
        assert_eq!(*missions.appels.borrow(), 1);
    }
}
