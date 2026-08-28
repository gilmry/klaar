//! Cas d'usage « envoyer un devis » (FR-016, Story 4.1).
//!
//! **Le prix vient du prestataire.** Ce cas d'usage ne le regarde pas, ne le
//! compare pas à un historique et ne le corrige pas : il vérifie *qui a le
//! droit d'en proposer un*, laisse le domaine dire si la proposition tient
//! debout, et écrit. C'est l'invariant §10.2, dont dépend la qualification du
//! travail de plateforme (loi belge du 26 avril 2024).
//!
//! **Seul le prestataire attribué peut chiffrer sa Mission**, et l'identité
//! vient de la fiche attachée au jeton, jamais d'un identifiant reçu. La
//! signature de cette fonction ne comporte aucun `provider_id` : c'est ce qui
//! rend l'envoi d'un devis au nom d'un autre non pas interdit, mais impossible
//! à écrire.
//!
//! **Les deux comptages sont dans la base, pas ici.** « Un seul devis en
//! attente » et « trois au maximum » portent sur des lignes que d'autres
//! transactions écrivent au même moment ; les lire puis décider laisserait deux
//! envois simultanés passer.

use klaar_identity::StatutProvider;
use klaar_intervention::StatutMission;
use klaar_payment::{Devis, DevisError, Proposition, DEVIS_MAX_PAR_MISSION};
use std::fmt;
use uuid::Uuid;

use crate::ports::devis_repository::{DevisRepository, ResultatEmission};
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::mission_repository::MissionRepository;
use crate::ports::provider_repository::ProviderRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurEmissionDevis {
    /// Le compte n'a pas de fiche prestataire, ou elle n'est plus active.
    ///
    /// Un seul code pour les deux, comme à l'acceptation : distinguer
    /// « vous n'êtes pas prestataire » de « votre compte est suspendu » ne
    /// change rien pour l'appelant légitime, qui le sait déjà.
    NonEligible,
    /// Mission inconnue, ou attribuée à quelqu'un d'autre.
    ///
    /// Un seul cas pour les deux. FR-016 `@negative` demande un 403
    /// `NOT_ASSIGNED` ; c'est un 404 qui est rendu, par la même précédence
    /// anti-énumération que les autres routes de Mission — un 403 apprendrait
    /// à qui essaie des identifiants lesquels correspondent à une Mission
    /// existante.
    Introuvable,
    /// La Mission est terminée ou annulée : il n'y a plus rien à chiffrer.
    MissionClose,
    /// Un devis attend déjà une réponse pour cette Mission.
    DevisEnCours,
    /// Trois devis ont déjà été envoyés (FR-016 `@edge`).
    ///
    /// Porte l'issue de l'annulation de la Mission : le demandeur doit être
    /// prévenu que l'affaire est close, et l'annulation peut avoir été perdue
    /// par une course avec un changement d'état concurrent.
    PlafondAtteint {
        mission_annulee: bool,
    },
    /// La proposition elle-même est refusée par le domaine.
    Domaine(DevisError),
    Indisponible(String),
}

impl ErreurEmissionDevis {
    /// Codes de FR-016, repris tels quels ; les deux derniers sont à nous.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NonEligible => "PROVIDER_NOT_ELIGIBLE",
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::MissionClose => "MISSION_CLOSED",
            Self::DevisEnCours => "QUOTE_ALREADY_PENDING",
            Self::PlafondAtteint { .. } => "MAX_QUOTES_REACHED",
            Self::Domaine(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurEmissionDevis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonEligible => write!(f, "ce compte ne peut pas émettre de devis"),
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::MissionClose => write!(f, "la Mission est close"),
            Self::DevisEnCours => write!(f, "un devis attend déjà une réponse"),
            Self::PlafondAtteint { .. } => write!(
                f,
                "{DEVIS_MAX_PAR_MISSION} devis déjà envoyés pour cette Mission"
            ),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurEmissionDevis {}

impl From<RepositoryError> for ErreurEmissionDevis {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que l'émission produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevisEmis {
    pub devis: Devis,
    /// La Demande dont la Mission est née. Rendue pour que l'appelant puisse
    /// prévenir le demandeur sans relire la Mission.
    pub demande_id: Uuid,
}

/// Émet un devis pour une Mission.
pub async fn emettre_devis<P, M, Q, H>(
    prestataires: &P,
    missions: &M,
    devis_repo: &Q,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    proposition: Proposition,
) -> Result<DevisEmis, ErreurEmissionDevis>
where
    P: ProviderRepository,
    M: MissionRepository,
    Q: DevisRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();

    let provider = prestataires
        .par_utilisateur_id(utilisateur_id)
        .await?
        .ok_or(ErreurEmissionDevis::NonEligible)?;
    // Un prestataire suspendu peut encore faire avancer une intervention
    // commencée — quelqu'un l'attend chez lui — mais ne peut plus engager
    // d'argent. Chiffrer, c'est engager : le contrôle est ici et pas dans
    // `transiter`, et c'est délibéré.
    if provider.statut != StatutProvider::Actif {
        return Err(ErreurEmissionDevis::NonEligible);
    }

    let mut mission = missions
        .par_id(mission_id)
        .await?
        .ok_or(ErreurEmissionDevis::Introuvable)?;
    if !mission.appartient_a(provider.id) {
        return Err(ErreurEmissionDevis::Introuvable);
    }
    // Terminée ou annulée : plus rien à chiffrer. Les états intermédiaires
    // passent tous, et c'est voulu — FR-016 décrit le devis à l'attribution,
    // mais un plombier chiffre après avoir vu la fuite, pas avant d'avoir
    // ouvert le placard. Refuser depuis `ON_SITE` obligerait à deviner.
    if mission.statut.est_terminal() {
        return Err(ErreurEmissionDevis::MissionClose);
    }

    let devis = Devis::emettre(mission.id, provider.id, proposition, maintenant)
        .map_err(ErreurEmissionDevis::Domaine)?;

    match devis_repo.emettre(&devis, DEVIS_MAX_PAR_MISSION).await? {
        ResultatEmission::Emis(ecrit) => Ok(DevisEmis {
            devis: ecrit,
            demande_id: mission.demande_id,
        }),
        ResultatEmission::DejaEnCours => Err(ErreurEmissionDevis::DevisEnCours),
        ResultatEmission::PlafondAtteint => {
            // FR-016 `@edge` : « la Mission est annulée, le User doit
            // relancer ». Sans cela, le demandeur resterait attaché à un
            // prestataire qui a épuisé ses tentatives, sans moyen d'en trouver
            // un autre — le pire des trois états possibles.
            //
            // L'échec de l'annulation ne change pas la réponse au prestataire :
            // son devis n'est pas passé, et c'est cela qu'il doit savoir. Le
            // demandeur, lui, verra l'état réel en ouvrant l'application.
            // Le statut **d'avant** la transition sert de garde. `transiter`
            // le remplace sur l'agrégat, donc il se relève avant l'appel :
            // passer `Acceptee` en dur ferait échouer l'annulation d'une
            // Mission déjà en route, c'est-à-dire exactement le cas où trois
            // devis ont eu le temps d'être refusés.
            let etat_connu = mission.statut;
            let annulee = match mission.transiter(StatutMission::Annulee, None, None, maintenant) {
                Ok(entree) => missions
                    .transiter(mission.id, etat_connu, &entree)
                    .await
                    .unwrap_or(false),
                Err(_) => false,
            };
            Err(ErreurEmissionDevis::PlafondAtteint {
                mission_annulee: annulee,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::mission_repository::ResultatAttribution;
    use crate::ports::provider_repository::ProviderProche;
    use chrono::{DateTime, TimeZone, Utc};
    use klaar_catalog::CodeCatalogue;
    use klaar_identity::{
        NumeroBce, OrigineKyc, Provider, StatutProvider, RAYON_INTERVENTION_DEFAUT,
    };
    use klaar_intervention::{Mission, TransitionMission};
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;

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

    fn proposition() -> Proposition {
        Proposition {
            montant_htva_cents: 18_000,
            taux_tva_bp: 2100,
            delai_minutes: 45,
            note: Some("remplacement joint".to_string()),
            preuve_tva_reduite: None,
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
        annulations: RefCell<Vec<TransitionMission>>,
    }

    impl MissionsMemoire {
        fn avec(mission: Mission) -> Self {
            Self {
                mission: Some(mission),
                annulations: RefCell::default(),
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
            self.annulations.borrow_mut().push(entree.clone());
            Ok(true)
        }
    }

    struct DevisMemoire {
        issue: RefCell<Vec<ResultatEmission>>,
        recus: RefCell<Vec<Devis>>,
    }

    impl DevisMemoire {
        /// Le dépôt écrit ce qu'on lui donne : c'est le cas nominal.
        fn accueillant() -> Self {
            Self {
                issue: RefCell::default(),
                recus: RefCell::default(),
            }
        }

        fn refusant(issue: ResultatEmission) -> Self {
            Self {
                issue: RefCell::new(vec![issue]),
                recus: RefCell::default(),
            }
        }
    }

    impl DevisRepository for DevisMemoire {
        async fn emettre(
            &self,
            devis: &Devis,
            _: usize,
        ) -> Result<ResultatEmission, RepositoryError> {
            self.recus.borrow_mut().push(devis.clone());
            match self.issue.borrow_mut().pop() {
                Some(prevue) => Ok(prevue),
                None => Ok(ResultatEmission::Emis(devis.clone())),
            }
        }
        async fn en_cours_pour_mission(&self, _: Uuid) -> Result<Option<Devis>, RepositoryError> {
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
            _: DateTime<Utc>,
            _: i64,
        ) -> Result<Vec<Devis>, RepositoryError> {
            unreachable!()
        }
    }

    async fn envoyer(
        prestataires: &PrestatairesMemoire,
        missions: &MissionsMemoire,
        devis: &DevisMemoire,
        utilisateur_id: Uuid,
        mission_id: Uuid,
    ) -> Result<DevisEmis, ErreurEmissionDevis> {
        emettre_devis(
            prestataires,
            missions,
            devis,
            &HorlogeFigee(instant()),
            utilisateur_id,
            mission_id,
            proposition(),
        )
        .await
    }

    // === @happy ===

    #[tokio::test]
    async fn happy_le_prestataire_attribue_envoie_son_devis() {
        let fiche = provider();
        let mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
        let prestataires = PrestatairesMemoire {
            fiche: Some(fiche.clone()),
        };
        let missions = MissionsMemoire::avec(mission.clone());
        let devis = DevisMemoire::accueillant();

        let emis = envoyer(
            &prestataires,
            &missions,
            &devis,
            fiche.utilisateur_id,
            mission.id,
        )
        .await
        .unwrap();

        assert_eq!(emis.devis.montant_htva.cents(), 18_000);
        assert_eq!(emis.devis.total_ttc.cents(), 21_780);
        assert_eq!(emis.demande_id, mission.demande_id);
        assert_eq!(emis.devis.mission_id, mission.id);
    }

    #[tokio::test]
    async fn happy_un_devis_est_possible_depuis_un_etat_intermediaire() {
        // Un plombier chiffre après avoir vu la fuite, donc depuis `ON_SITE`.
        let fiche = provider();
        let mut mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
        mission.statut = StatutMission::SurPlace;
        let prestataires = PrestatairesMemoire {
            fiche: Some(fiche.clone()),
        };
        let missions = MissionsMemoire::avec(mission.clone());

        assert!(envoyer(
            &prestataires,
            &missions,
            &DevisMemoire::accueillant(),
            fiche.utilisateur_id,
            mission.id,
        )
        .await
        .is_ok());
    }

    // === @negative ===

    #[tokio::test]
    async fn negative_un_compte_sans_fiche_prestataire_est_refuse() {
        let missions = MissionsMemoire::avec(Mission::attribuer(
            Uuid::new_v4(),
            Uuid::new_v4(),
            instant(),
        ));
        let refus = envoyer(
            &PrestatairesMemoire::default(),
            &missions,
            &DevisMemoire::accueillant(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;
        assert_eq!(refus, Err(ErreurEmissionDevis::NonEligible));
    }

    #[tokio::test]
    async fn negative_une_mission_inconnue_est_introuvable() {
        let fiche = provider();
        let missions = MissionsMemoire {
            mission: None,
            annulations: RefCell::default(),
        };
        let refus = envoyer(
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &missions,
            &DevisMemoire::accueillant(),
            fiche.utilisateur_id,
            Uuid::new_v4(),
        )
        .await;
        assert_eq!(refus, Err(ErreurEmissionDevis::Introuvable));
    }

    #[tokio::test]
    async fn negative_une_proposition_absurde_est_refusee_par_le_domaine() {
        let fiche = provider();
        let mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
        let refus = emettre_devis(
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &MissionsMemoire::avec(mission.clone()),
            &DevisMemoire::accueillant(),
            &HorlogeFigee(instant()),
            fiche.utilisateur_id,
            mission.id,
            Proposition {
                montant_htva_cents: 0,
                ..proposition()
            },
        )
        .await;
        assert_eq!(
            refus,
            Err(ErreurEmissionDevis::Domaine(DevisError::MontantNul))
        );
        assert_eq!(refus.unwrap_err().code(), "AMOUNT_ZERO");
    }

    // === @edge ===

    #[tokio::test]
    async fn edge_une_mission_close_ne_se_chiffre_plus() {
        let fiche = provider();
        for terminal in [StatutMission::Terminee, StatutMission::Annulee] {
            let mut mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
            mission.statut = terminal;
            let refus = envoyer(
                &PrestatairesMemoire {
                    fiche: Some(fiche.clone()),
                },
                &MissionsMemoire::avec(mission.clone()),
                &DevisMemoire::accueillant(),
                fiche.utilisateur_id,
                mission.id,
            )
            .await;
            assert_eq!(
                refus,
                Err(ErreurEmissionDevis::MissionClose),
                "{}",
                terminal.as_str()
            );
        }
    }

    #[tokio::test]
    async fn edge_un_devis_deja_en_attente_bloque_le_suivant() {
        let fiche = provider();
        let mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
        let refus = envoyer(
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &MissionsMemoire::avec(mission.clone()),
            &DevisMemoire::refusant(ResultatEmission::DejaEnCours),
            fiche.utilisateur_id,
            mission.id,
        )
        .await;
        assert_eq!(refus, Err(ErreurEmissionDevis::DevisEnCours));
    }

    #[tokio::test]
    async fn edge_le_quatrieme_devis_annule_la_mission() {
        let fiche = provider();
        let mut mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
        // Trois devis refusés supposent une intervention déjà en cours : la
        // garde d'annulation doit partir de l'état réel, pas de `ACCEPTED`.
        mission.statut = StatutMission::SurPlace;
        let missions = MissionsMemoire::avec(mission.clone());

        let refus = envoyer(
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &missions,
            &DevisMemoire::refusant(ResultatEmission::PlafondAtteint),
            fiche.utilisateur_id,
            mission.id,
        )
        .await;

        assert_eq!(
            refus,
            Err(ErreurEmissionDevis::PlafondAtteint {
                mission_annulee: true
            })
        );
        let annulations = missions.annulations.borrow();
        assert_eq!(annulations.len(), 1);
        assert_eq!(annulations[0].statut, StatutMission::Annulee);
    }

    // === @security ===

    #[tokio::test]
    async fn security_la_mission_d_un_autre_est_rendue_introuvable() {
        // 404 et non 403 : un 403 apprendrait à qui essaie des identifiants
        // lesquels correspondent à une Mission qui existe.
        let fiche = provider();
        let mission = Mission::attribuer(Uuid::new_v4(), Uuid::new_v4(), instant());
        let refus = envoyer(
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &MissionsMemoire::avec(mission.clone()),
            &DevisMemoire::accueillant(),
            fiche.utilisateur_id,
            mission.id,
        )
        .await;
        assert_eq!(refus, Err(ErreurEmissionDevis::Introuvable));
        assert_eq!(refus.unwrap_err().code(), "MISSION_NOT_FOUND");
    }

    #[tokio::test]
    async fn security_un_prestataire_suspendu_n_engage_plus_d_argent() {
        let mut fiche = provider();
        fiche.statut = StatutProvider::Suspendu;
        let mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
        let refus = envoyer(
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &MissionsMemoire::avec(mission.clone()),
            &DevisMemoire::accueillant(),
            fiche.utilisateur_id,
            mission.id,
        )
        .await;
        assert_eq!(refus, Err(ErreurEmissionDevis::NonEligible));
    }

    #[tokio::test]
    async fn security_l_emetteur_est_celui_du_jeton_et_non_un_champ_recu() {
        // La signature ne comporte aucun `provider_id` : c'est structurel. Ce
        // test fixe la conséquence, pour qu'un ajout de paramètre le casse.
        let fiche = provider();
        let mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
        let devis = DevisMemoire::accueillant();
        let emis = envoyer(
            &PrestatairesMemoire {
                fiche: Some(fiche.clone()),
            },
            &MissionsMemoire::avec(mission.clone()),
            &devis,
            fiche.utilisateur_id,
            mission.id,
        )
        .await
        .unwrap();

        assert_eq!(emis.devis.provider_id, fiche.id);
        assert_eq!(devis.recus.borrow()[0].provider_id, fiche.id);
    }

    #[tokio::test]
    async fn security_le_montant_traverse_le_cas_d_usage_sans_etre_touche() {
        // L'invariant §10.2 ne vaut que si personne ne « corrige » le prix en
        // chemin. Ce test regarde ce que le dépôt reçoit, pas ce que le domaine
        // rend : c'est l'écriture qui compte.
        let fiche = provider();
        let mission = Mission::attribuer(Uuid::new_v4(), fiche.id, instant());
        for cents in [1, 4_999, 18_000, 99_999] {
            let devis = DevisMemoire::accueillant();
            emettre_devis(
                &PrestatairesMemoire {
                    fiche: Some(fiche.clone()),
                },
                &MissionsMemoire::avec(mission.clone()),
                &devis,
                &HorlogeFigee(instant()),
                fiche.utilisateur_id,
                mission.id,
                Proposition {
                    montant_htva_cents: cents,
                    ..proposition()
                },
            )
            .await
            .unwrap();
            assert_eq!(devis.recus.borrow()[0].montant_htva.cents(), cents);
        }
    }
}
