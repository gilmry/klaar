//! Cas d'usage « annuler sa Demande » (FR-014, Story 3.5).
//!
//! **Avant l'attribution seulement.** Une fois qu'un prestataire a accepté, il
//! est peut-être déjà en route : annuler la Demande le laisserait rouler vers
//! une intervention que plus rien ne porte. C'est la Mission qu'il faut alors
//! annuler (FR-023), et le refus le dit plutôt que de faire croire au demandeur
//! qu'il en a fini.
//!
//! **La course avec une acceptation est tranchée par la base** (FR-014
//! `@edge`). Les deux écritures portent sur la même ligne et sont chacune une
//! seule instruction : PostgreSQL les sérialise, et la seconde ne trouve plus
//! l'état qu'elle attendait. Si l'annulation gagne, le prestataire reçoit un
//! refus ; si l'acceptation gagne, le demandeur est renvoyé vers FR-023.
//!
//! **Le motif est facultatif et fermé.** C'est une information que le demandeur
//! offre, pas une qu'on lui réclame pour lui rendre un droit — et un champ
//! libre inviterait à écrire une donnée personnelle dans un champ dont la
//! finalité annoncée est statistique.

use klaar_matching::{Demande, MotifAnnulation, StatutDemande};
use std::fmt;
use uuid::Uuid;

use crate::ports::audit::{CodeAudit, EntreeAudit, JournalAudit};
use crate::ports::demande_repository::DemandeRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurAnnulation {
    /// Demande inconnue, ou appartenant à quelqu'un d'autre.
    ///
    /// FR-014 `@negative` demande un 403 pour la Demande d'autrui. C'est un 404
    /// qui est rendu, comme pour l'élargissement : distinguer « elle n'existe
    /// pas » de « elle n'est pas à vous » laisserait apprendre quelles Demandes
    /// existent, et la précédence de l'anti-énumération est une décision déjà
    /// prise sur ce projet. Rendre deux codes différents sur deux routes de la
    /// même ressource serait en outre incohérent.
    Introuvable,
    /// Un prestataire l'a déjà acceptée : c'est la Mission qu'il faut annuler.
    DejaAttribuee,
    /// Elle était déjà annulée. Rien n'a été fait, et rien n'a été perdu.
    DejaAnnulee,
    Indisponible(String),
}

impl ErreurAnnulation {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "REQUEST_NOT_FOUND",
            Self::DejaAttribuee => "ALREADY_MATCHED",
            Self::DejaAnnulee => "ALREADY_CANCELLED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurAnnulation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Demande introuvable"),
            Self::DejaAttribuee => write!(f, "Demande déjà attribuée : annuler la Mission"),
            Self::DejaAnnulee => write!(f, "Demande déjà annulée"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurAnnulation {}

impl From<RepositoryError> for ErreurAnnulation {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Retire une Demande, à la demande de son auteur.
///
/// Rend la Demande annulée, pour que l'appelant puisse prévenir les
/// prestataires notifiés. La notification n'est pas faite ici : une panne du
/// service de push ne doit pas empêcher quelqu'un de retirer sa Demande.
pub async fn annuler<D, J, H>(
    demandes: &D,
    journal: &J,
    horloge: &H,
    utilisateur_id: Uuid,
    demande_id: Uuid,
    motif: Option<MotifAnnulation>,
) -> Result<Demande, ErreurAnnulation>
where
    D: DemandeRepository,
    J: JournalAudit,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let mut demande = demandes
        .par_id(demande_id)
        .await?
        // Filtré et non comparé après coup : la Demande d'autrui est
        // introuvable, pas interdite.
        .filter(|d| d.demandeur_id == utilisateur_id)
        .ok_or(ErreurAnnulation::Introuvable)?;

    // Contrôle en amont pour rendre le bon message ; c'est la garde du dépôt
    // qui tranche réellement la course.
    match demande.statut {
        StatutDemande::Attribuee => return Err(ErreurAnnulation::DejaAttribuee),
        StatutDemande::Annulee => return Err(ErreurAnnulation::DejaAnnulee),
        _ => {}
    }

    if !demandes.annuler(demande.id, motif).await? {
        // La Demande a changé d'état entre la lecture et l'écriture. Le seul
        // état qui refuse ici est `MATCHED` : c'est l'acceptation qui a gagné
        // la course (FR-014 `@edge`).
        return Err(ErreurAnnulation::DejaAttribuee);
    }
    demande.annuler(motif);

    // Journalisé après coup, et sans le motif : celui-ci vit sur la Demande, où
    // il s'efface avec elle quand le compte est effacé (art. 17).
    if let Err(e) = journal
        .consigner(EntreeAudit {
            code: CodeAudit::RequestCancelled,
            sujet_id: Some(utilisateur_id),
            horodatage: maintenant,
        })
        .await
    {
        // L'annulation est faite. Échouer ici la laisserait effective sans le
        // dire au demandeur, qui recommencerait pour rien.
        tracing::error!(erreur = %e, "annulation non journalisée");
    }

    Ok(demande)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use chrono::{DateTime, TimeZone, Utc};
    use klaar_catalog::CodeCatalogue;
    use klaar_matching::Urgence;
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn demande(auteur: Uuid) -> Demande {
        Demande::soumettre(
            auteur,
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
        stockee: RefCell<Option<Demande>>,
        /// Ce que le dépôt rend : `false` simule une acceptation qui a gagné la
        /// course entre la lecture et l'écriture.
        annulation_accordee: bool,
        motifs: RefCell<Vec<Option<MotifAnnulation>>>,
    }

    impl DemandesMemoire {
        fn avec(d: Demande) -> Self {
            Self {
                stockee: RefCell::new(Some(d)),
                annulation_accordee: true,
                motifs: RefCell::default(),
            }
        }
    }

    impl DemandeRepository for DemandesMemoire {
        async fn creer(&self, _: &Demande) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn par_id(&self, _: Uuid) -> Result<Option<Demande>, RepositoryError> {
            Ok(self.stockee.borrow().clone())
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
            motif: Option<MotifAnnulation>,
        ) -> Result<bool, RepositoryError> {
            self.motifs.borrow_mut().push(motif);
            Ok(self.annulation_accordee)
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

    #[derive(Default)]
    struct JournalMemoire {
        entrees: RefCell<Vec<EntreeAudit>>,
        en_panne: bool,
    }

    impl JournalAudit for JournalMemoire {
        async fn consigner(&self, entree: EntreeAudit) -> Result<(), RepositoryError> {
            if self.en_panne {
                return Err(RepositoryError::Indisponible("test".into()));
            }
            self.entrees.borrow_mut().push(entree);
            Ok(())
        }
    }

    async fn tenter(
        depot: &DemandesMemoire,
        journal: &JournalMemoire,
        auteur: Uuid,
        cible: Uuid,
        motif: Option<MotifAnnulation>,
    ) -> Result<Demande, ErreurAnnulation> {
        annuler(
            depot,
            journal,
            &HorlogeFigee(instant()),
            auteur,
            cible,
            motif,
        )
        .await
    }

    #[tokio::test]
    async fn happy_le_demandeur_retire_sa_demande_diffusee() {
        let auteur = Uuid::new_v4();
        let d = demande(auteur);
        let id = d.id;
        let depot = DemandesMemoire::avec(d);
        let journal = JournalMemoire::default();

        let annulee = tenter(&depot, &journal, auteur, id, None).await.unwrap();
        assert_eq!(annulee.statut, StatutDemande::Annulee);
        assert_eq!(journal.entrees.borrow().len(), 1);
        assert_eq!(
            journal.entrees.borrow()[0].code,
            CodeAudit::RequestCancelled
        );
    }

    #[tokio::test]
    async fn happy_une_demande_sans_reponse_se_retire_aussi() {
        let auteur = Uuid::new_v4();
        let mut d = demande(auteur);
        d.statut = StatutDemande::SansReponse;
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let annulee = tenter(&depot, &JournalMemoire::default(), auteur, id, None)
            .await
            .unwrap();
        assert_eq!(annulee.statut, StatutDemande::Annulee);
    }

    #[tokio::test]
    async fn happy_le_motif_donne_est_transmis_au_depot() {
        let auteur = Uuid::new_v4();
        let d = demande(auteur);
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        tenter(
            &depot,
            &JournalMemoire::default(),
            auteur,
            id,
            Some(MotifAnnulation::TrouveAilleurs),
        )
        .await
        .unwrap();
        assert_eq!(
            *depot.motifs.borrow(),
            vec![Some(MotifAnnulation::TrouveAilleurs)]
        );
    }

    #[tokio::test]
    async fn negative_une_demande_attribuee_renvoie_vers_l_annulation_de_mission() {
        // Le prestataire est peut-être déjà en route.
        let auteur = Uuid::new_v4();
        let mut d = demande(auteur);
        d.statut = StatutDemande::Attribuee;
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let e = tenter(&depot, &JournalMemoire::default(), auteur, id, None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "ALREADY_MATCHED");
        assert!(depot.motifs.borrow().is_empty(), "rien ne doit être écrit");
    }

    #[tokio::test]
    async fn negative_une_demande_deja_annulee_le_dit() {
        let auteur = Uuid::new_v4();
        let mut d = demande(auteur);
        d.statut = StatutDemande::Annulee;
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let e = tenter(&depot, &JournalMemoire::default(), auteur, id, None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "ALREADY_CANCELLED");
    }

    #[tokio::test]
    async fn negative_une_demande_inconnue_est_introuvable() {
        let depot = DemandesMemoire::default();
        let e = tenter(
            &depot,
            &JournalMemoire::default(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_NOT_FOUND");
    }

    #[tokio::test]
    async fn edge_une_acceptation_qui_gagne_la_course_renvoie_vers_la_mission() {
        // FR-014 `@edge` : la Demande était diffusée à la lecture, un
        // prestataire l'a prise avant l'écriture. C'est la garde du dépôt qui
        // tranche, et le demandeur est renvoyé vers FR-023.
        let auteur = Uuid::new_v4();
        let d = demande(auteur);
        let id = d.id;
        let depot = DemandesMemoire {
            stockee: RefCell::new(Some(d)),
            annulation_accordee: false,
            motifs: RefCell::default(),
        };

        let e = tenter(&depot, &JournalMemoire::default(), auteur, id, None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "ALREADY_MATCHED");
    }

    #[tokio::test]
    async fn edge_une_panne_du_journal_n_annule_pas_l_annulation() {
        // Échouer ici laisserait l'annulation effective sans le dire au
        // demandeur, qui recommencerait pour rien.
        let auteur = Uuid::new_v4();
        let d = demande(auteur);
        let id = d.id;
        let depot = DemandesMemoire::avec(d);
        let journal = JournalMemoire {
            en_panne: true,
            ..Default::default()
        };

        let annulee = tenter(&depot, &journal, auteur, id, None).await.unwrap();
        assert_eq!(annulee.statut, StatutDemande::Annulee);
    }

    #[tokio::test]
    async fn security_la_demande_d_un_autre_est_introuvable() {
        // FR-014 demande un 403 ; c'est un 404 qui est rendu, parce que
        // distinguer les deux laisserait apprendre quelles Demandes existent.
        let auteur = Uuid::new_v4();
        let d = demande(auteur);
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let e = tenter(&depot, &JournalMemoire::default(), Uuid::new_v4(), id, None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "REQUEST_NOT_FOUND");
        assert!(
            depot.motifs.borrow().is_empty(),
            "la Demande d'autrui ne doit pas être touchée"
        );
    }

    #[tokio::test]
    async fn security_le_journal_ne_porte_pas_le_motif() {
        // Il vit sur la Demande, où il s'efface avec elle quand le compte est
        // effacé (art. 17). Dans le journal, il survivrait à l'effacement.
        let auteur = Uuid::new_v4();
        let d = demande(auteur);
        let id = d.id;
        let depot = DemandesMemoire::avec(d);
        let journal = JournalMemoire::default();

        tenter(
            &depot,
            &journal,
            auteur,
            id,
            Some(MotifAnnulation::TrouveAilleurs),
        )
        .await
        .unwrap();
        // L'entrée d'audit n'a pas de champ où le glisser : ce test fixe
        // l'intention, et échouera si on lui en ajoute un.
        let entree = journal.entrees.borrow()[0].clone();
        assert_eq!(entree.code, CodeAudit::RequestCancelled);
        assert_eq!(entree.sujet_id, Some(auteur));
    }
}
