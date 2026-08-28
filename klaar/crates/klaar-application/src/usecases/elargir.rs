//! Cas d'usage « élargir le rayon d'une Demande » (FR-015, Story 3.6).
//!
//! Trente secondes ont passé, personne n'a répondu, et le demandeur est toujours
//! devant sa fuite. FR-015 lui offre deux issues : élargir, ou annuler. Celle-ci
//! est la première.
//!
//! **La limite de trois élargissements se termine par une annulation.** FR-015
//! `@security` le demande, et c'est la bonne réponse : laisser la Demande en
//! `NO_MATCH` après le dernier refus entretiendrait l'idée que quelque chose
//! peut encore arriver, alors que non. Mieux vaut le dire et rendre au
//! demandeur sa liberté d'appeler ailleurs.
//!
//! **Seul l'auteur élargit.** L'identifiant vient du jeton ; la Demande d'un
//! autre est refusée comme si elle n'existait pas, sans quoi il suffirait
//! d'essayer des identifiants pour apprendre lesquels existent.

use klaar_matching::{Demande, DemandeError, StatutDemande};
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurElargissement {
    /// Demande inconnue, ou appartenant à quelqu'un d'autre.
    ///
    /// Un seul cas pour les deux : distinguer « elle n'existe pas » de « elle
    /// n'est pas à vous » laisserait apprendre quelles Demandes existent en
    /// essayant des identifiants.
    Introuvable,
    /// Elle est encore diffusée : rien à élargir pour l'instant.
    PasEnAttente,
    /// Elle est attribuée ou annulée.
    Close,
    /// Quatrième élargissement : la Demande vient d'être annulée (FR-015).
    RayonMaximalAtteint,
    Indisponible(String),
}

impl ErreurElargissement {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "REQUEST_NOT_FOUND",
            Self::PasEnAttente => "REQUEST_NOT_EXPIRED",
            Self::Close => "REQUEST_CLOSED",
            Self::RayonMaximalAtteint => "MAX_RADIUS_REACHED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurElargissement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Demande introuvable"),
            Self::PasEnAttente => write!(f, "la Demande est encore diffusée"),
            Self::Close => write!(f, "la Demande est attribuée ou annulée"),
            Self::RayonMaximalAtteint => write!(f, "rayon maximal atteint, Demande annulée"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurElargissement {}

impl From<RepositoryError> for ErreurElargissement {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Relance la diffusion d'une Demande sans réponse, sur un rayon plus large.
///
/// Rend la Demande relancée, prête à être rematchée par l'appelant : le
/// matching n'est pas fait ici pour la même raison qu'il ne l'est pas à la
/// soumission — c'est une opération distincte, dont l'échec ne doit pas défaire
/// l'élargissement.
pub async fn elargir<D, H>(
    demandes: &D,
    horloge: &H,
    utilisateur_id: Uuid,
    demande_id: Uuid,
) -> Result<Demande, ErreurElargissement>
where
    D: DemandeRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let mut demande = demandes
        .par_id(demande_id)
        .await?
        .filter(|d| d.demandeur_id == utilisateur_id)
        .ok_or(ErreurElargissement::Introuvable)?;

    // Une Demande diffusée mais dont le tour est écoulé n'a pas encore été
    // balayée. La traiter comme sans réponse plutôt que de renvoyer le
    // demandeur attendre le prochain passage : c'est le même état de fait, et
    // il n'a pas à connaître la cadence de nos tâches de fond.
    if demande.statut == StatutDemande::Diffusion && demande.est_expiree(maintenant) {
        demande.expirer(maintenant);
    }

    match demande.elargir(maintenant) {
        Ok(()) => {}
        Err(DemandeError::ElargissementsEpuises) => {
            // FR-015 `@security` : la Demande est auto-annulée. Laisser un
            // `NO_MATCH` entretiendrait l'idée que quelque chose peut encore
            // arriver.
            // Aucun motif : ce n'est pas le demandeur qui renonce, c'est la règle
            // des trois élargissements qui s'arrête. Lui prêter un motif
            // fausserait l'analyse des annulations volontaires.
            demande.annuler(None);
            // `annuler` et non `changer_statut` : la Demande est en `NO_MATCH`,
            // et `changer_statut` ne quitte que `BROADCASTING`. Les confondre
            // laissait la Demande en attente après le refus, ce qu'un test
            // d'intégration a attrapé.
            demandes.annuler(demande.id, None).await?;
            return Err(ErreurElargissement::RayonMaximalAtteint);
        }
        Err(_) => {
            return Err(match demande.statut {
                StatutDemande::Diffusion => ErreurElargissement::PasEnAttente,
                _ => ErreurElargissement::Close,
            })
        }
    }

    // Le dépôt garde `NO_MATCH` en condition : deux clics sur « élargir » ne
    // doivent pas consommer deux élargissements, et une Demande acceptée entre
    // la lecture et l'écriture ne doit pas repartir en diffusion.
    if !demandes.relancer(&demande).await? {
        return Err(ErreurElargissement::Close);
    }

    Ok(demande)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use klaar_catalog::CodeCatalogue;
    use klaar_matching::{Urgence, DUREE_DIFFUSION_SECONDES, ELARGISSEMENTS_MAX, RAYONS_METRES};
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
        relances: RefCell<usize>,
        statuts_ecrits: RefCell<Vec<StatutDemande>>,
    }

    impl DemandesMemoire {
        fn avec(d: Demande) -> Self {
            Self {
                stockee: RefCell::new(Some(d)),
                ..Default::default()
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
            statut: StatutDemande,
            _: DateTime<Utc>,
        ) -> Result<(), RepositoryError> {
            self.statuts_ecrits.borrow_mut().push(statut);
            Ok(())
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
            self.statuts_ecrits
                .borrow_mut()
                .push(StatutDemande::Annulee);
            Ok(true)
        }
        async fn relancer(&self, demande: &Demande) -> Result<bool, RepositoryError> {
            *self.relances.borrow_mut() += 1;
            *self.stockee.borrow_mut() = Some(demande.clone());
            Ok(true)
        }
        async fn compter_depuis_une_heure(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> Result<i64, RepositoryError> {
            unreachable!()
        }
    }

    async fn tenter(
        depot: &DemandesMemoire,
        auteur: Uuid,
        cible: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<Demande, ErreurElargissement> {
        elargir(depot, &HorlogeFigee(maintenant), auteur, cible).await
    }

    #[tokio::test]
    async fn happy_une_demande_sans_reponse_repart_sur_un_rayon_plus_large() {
        let auteur = Uuid::new_v4();
        let mut d = demande(auteur);
        d.statut = StatutDemande::SansReponse;
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let apres = instant() + Duration::seconds(45);
        let relancee = tenter(&depot, auteur, id, apres).await.unwrap();
        assert_eq!(relancee.statut, StatutDemande::Diffusion);
        assert_eq!(relancee.rayon_metres, RAYONS_METRES[1]);
        assert_eq!(relancee.elargissements, 1);
        // Une fenêtre entière, pas le reliquat de la précédente.
        assert_eq!(relancee.diffuse_depuis, apres);
        assert!(relancee.est_acceptable(apres));
    }

    #[tokio::test]
    async fn happy_une_demande_echue_mais_pas_encore_balayee_s_elargit_aussi() {
        // Le demandeur n'a pas à connaître la cadence de nos tâches de fond :
        // pour lui, trente secondes ont passé et personne n'est venu.
        let auteur = Uuid::new_v4();
        let d = demande(auteur);
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let apres = instant() + Duration::seconds(DUREE_DIFFUSION_SECONDES);
        let relancee = tenter(&depot, auteur, id, apres).await.unwrap();
        assert_eq!(relancee.elargissements, 1);
    }

    #[tokio::test]
    async fn negative_une_demande_encore_dans_sa_fenetre_ne_s_elargit_pas() {
        // Cela couperait le tour en cours, alors qu'un prestataire est
        // peut-être en train de répondre.
        let auteur = Uuid::new_v4();
        let d = demande(auteur);
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let e = tenter(&depot, auteur, id, instant() + Duration::seconds(5))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "REQUEST_NOT_EXPIRED");
        assert_eq!(*depot.relances.borrow(), 0);
    }

    #[tokio::test]
    async fn negative_une_demande_attribuee_ou_annulee_est_close() {
        for statut in [StatutDemande::Attribuee, StatutDemande::Annulee] {
            let auteur = Uuid::new_v4();
            let mut d = demande(auteur);
            d.statut = statut;
            let id = d.id;
            let depot = DemandesMemoire::avec(d);

            let e = tenter(&depot, auteur, id, instant() + Duration::seconds(60))
                .await
                .unwrap_err();
            assert_eq!(e.code(), "REQUEST_CLOSED", "statut {statut:?}");
        }
    }

    #[tokio::test]
    async fn negative_une_demande_inconnue_est_introuvable() {
        let depot = DemandesMemoire::default();
        let e = tenter(&depot, Uuid::new_v4(), Uuid::new_v4(), instant())
            .await
            .unwrap_err();
        assert_eq!(e.code(), "REQUEST_NOT_FOUND");
    }

    #[tokio::test]
    async fn edge_le_quatrieme_elargissement_annule_la_demande() {
        // FR-015 `@security`. Laisser un `NO_MATCH` entretiendrait l'idée que
        // quelque chose peut encore arriver.
        let auteur = Uuid::new_v4();
        let mut d = demande(auteur);
        d.statut = StatutDemande::SansReponse;
        d.elargissements = ELARGISSEMENTS_MAX;
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let e = tenter(&depot, auteur, id, instant() + Duration::seconds(60))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "MAX_RADIUS_REACHED");
        assert_eq!(
            *depot.statuts_ecrits.borrow(),
            vec![StatutDemande::Annulee],
            "la Demande doit être annulée, pas laissée en attente"
        );
        assert_eq!(*depot.relances.borrow(), 0);
    }

    #[tokio::test]
    async fn edge_les_trois_elargissements_menent_au_dernier_rayon() {
        let auteur = Uuid::new_v4();
        let mut d = demande(auteur);
        d.statut = StatutDemande::SansReponse;
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let mut t = instant();
        for (tour, attendu) in RAYONS_METRES.iter().enumerate().skip(1) {
            t += Duration::seconds(45);
            let r = tenter(&depot, auteur, id, t).await.unwrap();
            assert_eq!(r.rayon_metres, *attendu, "tour {tour}");
            // Remise en attente pour le tour suivant, comme le ferait le
            // balayage.
            depot.stockee.borrow_mut().as_mut().unwrap().statut = StatutDemande::SansReponse;
        }
        t += Duration::seconds(45);
        assert_eq!(
            tenter(&depot, auteur, id, t).await.unwrap_err().code(),
            "MAX_RADIUS_REACHED"
        );
    }

    #[tokio::test]
    async fn security_la_demande_d_un_autre_est_introuvable() {
        // Et non « interdite » : distinguer les deux laisserait apprendre
        // quelles Demandes existent en essayant des identifiants.
        let auteur = Uuid::new_v4();
        let mut d = demande(auteur);
        d.statut = StatutDemande::SansReponse;
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let e = tenter(
            &depot,
            Uuid::new_v4(),
            id,
            instant() + Duration::seconds(60),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "REQUEST_NOT_FOUND");
        assert_eq!(*depot.relances.borrow(), 0);
    }

    #[tokio::test]
    async fn security_le_compteur_ne_se_remet_pas_a_zero_entre_deux_appels() {
        let auteur = Uuid::new_v4();
        let mut d = demande(auteur);
        d.statut = StatutDemande::SansReponse;
        d.elargissements = 2;
        let id = d.id;
        let depot = DemandesMemoire::avec(d);

        let r = tenter(&depot, auteur, id, instant() + Duration::seconds(60))
            .await
            .unwrap();
        assert_eq!(r.elargissements, 3);
    }
}
