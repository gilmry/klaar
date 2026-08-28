//! Cas d'usage « chercher les prestataires d'une Demande » (FR-012, Story 3.2).
//!
//! **Ce que cette story livre, et ce qu'elle ne livre pas.** Elle trouve les
//! candidats, les classe, retient les dix meilleurs et écrit la trace que l'AI
//! Act réclame. Elle **ne les notifie pas** : l'envoi push appartient à la
//! Story 3.3. Une Demande sans candidat passe en `NO_MATCH`, ce qui est déjà
//! une réponse utile pour celui qui attend.
//!
//! **La trace conserve aussi les écartés.** Ne garder que les retenus rendrait
//! la trace inutile pour celui qui demande des comptes — c'est-à-dire pour la
//! seule personne à qui elle est destinée : le prestataire qui n'a pas été
//! notifié et veut savoir pourquoi.

use chrono::{DateTime, Utc};
use klaar_matching::{calculer_score, Demande, Score, StatutDemande, CANDIDATS_MAX};
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::provider_repository::ProviderRepository;
use crate::ports::trace_repository::{LigneTrace, MotifEcart, TraceRepository};

/// Candidats examinés au-delà des dix retenus.
///
/// La trace doit dire pourquoi les autres n'ont pas été notifiés (FR-012
/// `@edge`), ce qui suppose de les avoir vus. Cent bornent le travail sans
/// rendre la trace creuse : au-delà, l'explication « il y avait plus de cent
/// candidats plus proches » se suffit à elle-même.
pub const CANDIDATS_EXAMINES_MAX: i64 = 100;

/// Un prestataire retenu pour notification.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidat {
    pub provider_id: Uuid,
    /// Compte du prestataire : c'est lui qui porte les abonnements push, pas
    /// la fiche d'entreprise.
    pub utilisateur_id: Uuid,
    pub distance_metres: f64,
    pub score: Score,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResultatMatching {
    /// Des candidats ont été retenus, dans l'ordre de notification.
    Candidats(Vec<Candidat>),
    /// Personne dans le rayon. La Demande est passée en `NO_MATCH`.
    Aucun,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurMatching {
    /// La Demande n'est plus en diffusion : annulée, ou déjà traitée.
    DemandeNonDiffusee,
    Indisponible(String),
}

impl fmt::Display for ErreurMatching {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DemandeNonDiffusee => write!(f, "la Demande n'est plus en diffusion"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurMatching {}

impl From<RepositoryError> for ErreurMatching {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ancienneté du contrôle, en jours.
///
/// Un prestataire sans date de contrôle est traité comme périmé plutôt que
/// comme frais : il ne devrait pas être actif, et lui accorder le bénéfice du
/// doute reviendrait à récompenser une donnée manquante.
fn anciennete_jours(verifie_le: Option<DateTime<Utc>>, maintenant: DateTime<Utc>) -> f64 {
    match verifie_le {
        Some(date) => (maintenant - date).num_days().max(0) as f64,
        None => f64::MAX,
    }
}

pub async fn chercher_candidats<P, D, T, H>(
    providers: &P,
    demandes: &D,
    traces: &T,
    horloge: &H,
    demande: &Demande,
) -> Result<ResultatMatching, ErreurMatching>
where
    P: ProviderRepository,
    D: DemandeRepository,
    T: TraceRepository,
    H: Horloge,
{
    // Une Demande annulée ne doit réveiller personne. Le contrôle est ici et
    // non chez l'appelant : ce cas d'usage sera déclenché par une tâche de
    // fond, qui peut s'exécuter longtemps après la soumission.
    if demande.statut != StatutDemande::Diffusion {
        return Err(ErreurMatching::DemandeNonDiffusee);
    }

    let maintenant = horloge.maintenant();
    let proches = providers
        .proches(
            &demande.secteur,
            demande.position,
            // Le rayon du tour en cours, et non une constante : après un
            // élargissement (FR-015), chercher dans cinq kilomètres
            // rendrait l'élargissement sans effet.
            demande.rayon_metres,
            CANDIDATS_EXAMINES_MAX,
        )
        .await?;

    if proches.is_empty() {
        // `NO_MATCH` plutôt qu'un silence : celui qui attend a besoin de savoir
        // que personne ne viendra, pour appeler ailleurs.
        demandes
            .changer_statut(demande.id, StatutDemande::SansReponse, maintenant)
            .await?;
        return Ok(ResultatMatching::Aucun);
    }

    let mut classes: Vec<Candidat> = proches
        .iter()
        .map(|p| Candidat {
            provider_id: p.provider.id,
            utilisateur_id: p.provider.utilisateur_id,
            distance_metres: p.distance_metres,
            score: calculer_score(
                p.distance_metres,
                demande.rayon_metres,
                anciennete_jours(p.provider.kyc_verifie_le, maintenant),
                // La note n'existe pas : le bounded context Trust arrive plus
                // tard. `None` et non zéro — voir `klaar_matching::score`.
                None,
            ),
        })
        .collect();

    // Tri décroissant sur le score, départagé par l'identifiant. Sans ce
    // second critère, deux candidats au score identique s'ordonneraient au
    // gré de la base, et la trace cesserait d'expliquer le classement.
    classes.sort_by(|a, b| {
        b.score
            .total
            .partial_cmp(&a.score.total)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.provider_id.cmp(&b.provider_id))
    });

    let retenus: Vec<Candidat> = classes.iter().take(CANDIDATS_MAX).cloned().collect();

    let lignes: Vec<LigneTrace> = classes
        .iter()
        .enumerate()
        .map(|(rang, c)| LigneTrace {
            demande_id: demande.id,
            provider_id: c.provider_id,
            score: c.score,
            distance_metres: c.distance_metres,
            retenu: rang < CANDIDATS_MAX,
            motif_ecart: (rang >= CANDIDATS_MAX).then_some(MotifEcart::HorsTop),
            tracee_le: maintenant,
        })
        .collect();

    // La trace est écrite **avant** que les candidats ne soient rendus. Si
    // l'écriture échoue, personne n'est notifié : une notification qu'aucune
    // trace n'explique est précisément ce que l'AI Act interdit.
    traces.consigner(&lignes).await?;

    Ok(ResultatMatching::Candidats(retenus))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::provider_repository::ProviderProche;
    use chrono::{Duration, TimeZone};
    use klaar_catalog::CodeCatalogue;
    use klaar_identity::{NumeroBce, OrigineKyc, Provider, StatutProvider};
    use klaar_matching::{Urgence, RAYONS_METRES};
    use klaar_shared_kernel::Geo;
    use std::cell::RefCell;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn secteur() -> CodeCatalogue {
        CodeCatalogue::parse("plomberie").unwrap()
    }

    fn bruxelles() -> Geo {
        Geo::new(50.8467, 4.3525).unwrap()
    }

    fn demande() -> Demande {
        Demande::soumettre(
            Uuid::new_v4(),
            secteur(),
            "Fuite",
            bruxelles(),
            Urgence::Haute,
            instant(),
        )
        .unwrap()
    }

    fn provider(kyc_il_y_a_jours: i64) -> Provider {
        let corps = 1_234_567u64;
        Provider {
            id: Uuid::new_v4(),
            utilisateur_id: Uuid::new_v4(),
            numero_bce: NumeroBce::parse(&format!("{corps:08}{:02}", 97 - (corps % 97))).unwrap(),
            raison_sociale: "Prestataire".to_string(),
            base: bruxelles(),
            statut: StatutProvider::Actif,
            origine_kyc: Some(OrigineKyc::Demonstration),
            kyc_verifie_le: Some(instant() - Duration::days(kyc_il_y_a_jours)),
            competences: vec![secteur()],
            cree_le: instant(),
        }
    }

    #[derive(Default)]
    struct ProvidersMemoire {
        proches: Vec<ProviderProche>,
    }

    impl ProviderRepository for ProvidersMemoire {
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
            unreachable!()
        }
        async fn mettre_a_jour_etat(&self, _: &Provider) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn definir_disponibilite(&self, _: Uuid, _: bool) -> Result<(), RepositoryError> {
            unreachable!()
        }
        async fn proches(
            &self,
            _: &CodeCatalogue,
            _: Geo,
            _: f64,
            limite: i64,
        ) -> Result<Vec<ProviderProche>, RepositoryError> {
            Ok(self.proches.iter().take(limite as usize).cloned().collect())
        }
    }

    #[derive(Default)]
    struct DemandesMemoire {
        statuts: RefCell<Vec<(Uuid, StatutDemande)>>,
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
        async fn compter_depuis_une_heure(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> Result<i64, RepositoryError> {
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
        async fn changer_statut(
            &self,
            id: Uuid,
            statut: StatutDemande,
            _: DateTime<Utc>,
        ) -> Result<(), RepositoryError> {
            self.statuts.borrow_mut().push((id, statut));
            Ok(())
        }
    }

    #[derive(Default)]
    struct TracesMemoire {
        lignes: RefCell<Vec<LigneTrace>>,
        en_panne: bool,
    }

    impl TraceRepository for TracesMemoire {
        async fn consigner(&self, lignes: &[LigneTrace]) -> Result<(), RepositoryError> {
            if self.en_panne {
                return Err(RepositoryError::Indisponible("test".into()));
            }
            self.lignes.borrow_mut().extend_from_slice(lignes);
            Ok(())
        }
        async fn comptes_retenus_sauf(
            &self,
            _: Uuid,
            _: Uuid,
        ) -> Result<Vec<Uuid>, RepositoryError> {
            unreachable!()
        }
    }

    async fn matcher(
        proches: Vec<ProviderProche>,
        demande: &Demande,
    ) -> (
        Result<ResultatMatching, ErreurMatching>,
        DemandesMemoire,
        TracesMemoire,
    ) {
        let providers = ProvidersMemoire { proches };
        let demandes = DemandesMemoire::default();
        let traces = TracesMemoire::default();
        let r = chercher_candidats(
            &providers,
            &demandes,
            &traces,
            &HorlogeFigee(instant()),
            demande,
        )
        .await;
        (r, demandes, traces)
    }

    fn proche(distance: f64, kyc_jours: i64) -> ProviderProche {
        ProviderProche {
            provider: provider(kyc_jours),
            distance_metres: distance,
        }
    }

    #[tokio::test]
    async fn happy_les_candidats_sont_rendus_du_plus_proche_au_plus_loin() {
        let d = demande();
        let (r, _, traces) = matcher(
            vec![proche(3_000.0, 0), proche(100.0, 0), proche(1_500.0, 0)],
            &d,
        )
        .await;

        let ResultatMatching::Candidats(candidats) = r.unwrap() else {
            panic!("des candidats étaient attendus");
        };
        assert_eq!(candidats.len(), 3);
        let distances: Vec<f64> = candidats.iter().map(|c| c.distance_metres).collect();
        assert_eq!(distances, vec![100.0, 1_500.0, 3_000.0]);
        assert_eq!(traces.lignes.borrow().len(), 3);
    }

    #[tokio::test]
    async fn happy_un_controle_recent_departage_a_distance_egale() {
        let d = demande();
        let (r, _, _) = matcher(vec![proche(1_000.0, 300), proche(1_000.0, 0)], &d).await;
        let ResultatMatching::Candidats(candidats) = r.unwrap() else {
            panic!("des candidats étaient attendus");
        };
        assert!(candidats[0].score.total > candidats[1].score.total);
    }

    #[tokio::test]
    async fn negative_sans_candidat_la_demande_passe_en_no_match() {
        // Celui qui attend a besoin de savoir que personne ne viendra, pour
        // appeler ailleurs.
        let d = demande();
        let (r, demandes, traces) = matcher(vec![], &d).await;
        assert_eq!(r.unwrap(), ResultatMatching::Aucun);
        assert_eq!(
            demandes.statuts.borrow().as_slice(),
            &[(d.id, StatutDemande::SansReponse)]
        );
        assert!(traces.lignes.borrow().is_empty(), "rien à tracer");
    }

    #[tokio::test]
    async fn negative_une_demande_annulee_ne_reveille_personne() {
        // Ce cas d'usage est déclenché par une tâche de fond, qui peut
        // s'exécuter longtemps après la soumission.
        let mut d = demande();
        d.statut = StatutDemande::Annulee;
        let (r, _, traces) = matcher(vec![proche(100.0, 0)], &d).await;
        assert_eq!(r.unwrap_err(), ErreurMatching::DemandeNonDiffusee);
        assert!(traces.lignes.borrow().is_empty());
    }

    #[tokio::test]
    async fn edge_au_dela_de_dix_candidats_seuls_les_dix_premiers_sont_retenus() {
        let d = demande();
        let proches: Vec<_> = (0..15).map(|i| proche(100.0 * (i + 1) as f64, 0)).collect();
        let (r, _, traces) = matcher(proches, &d).await;

        let ResultatMatching::Candidats(candidats) = r.unwrap() else {
            panic!("des candidats étaient attendus");
        };
        assert_eq!(candidats.len(), CANDIDATS_MAX);
        // Les quinze sont tracés, dix retenus et cinq écartés.
        let lignes = traces.lignes.borrow();
        assert_eq!(lignes.len(), 15);
        assert_eq!(lignes.iter().filter(|l| l.retenu).count(), 10);
        assert_eq!(
            lignes
                .iter()
                .filter(|l| l.motif_ecart == Some(MotifEcart::HorsTop))
                .count(),
            5
        );
    }

    #[tokio::test]
    async fn edge_un_candidat_exactement_au_bord_du_rayon_est_retenu() {
        // FR-012 `@edge` : au bord du rayon, il est inclus. Le rayon est inclusif, et
        // c'est le dépôt qui l'applique — ce cas vérifie que le score ne
        // l'exclut pas ensuite en lui donnant zéro.
        let d = demande();
        let (r, _, _) = matcher(vec![proche(RAYONS_METRES[0], 0)], &d).await;
        let ResultatMatching::Candidats(candidats) = r.unwrap() else {
            panic!("le candidat au bord doit être retenu");
        };
        assert_eq!(candidats.len(), 1);
        assert!(candidats[0].score.total > 0.0);
    }

    #[tokio::test]
    async fn edge_un_prestataire_sans_date_de_controle_est_traite_comme_perime() {
        // Il ne devrait pas être actif ; lui accorder le bénéfice du doute
        // reviendrait à récompenser une donnée manquante.
        let d = demande();
        let mut sans_date = proche(1_000.0, 0);
        sans_date.provider.kyc_verifie_le = None;
        let avec_date = proche(1_000.0, 0);

        let (r, _, _) = matcher(vec![sans_date, avec_date], &d).await;
        let ResultatMatching::Candidats(candidats) = r.unwrap() else {
            panic!("des candidats étaient attendus");
        };
        assert!(candidats[0].score.total > candidats[1].score.total);
        assert_eq!(candidats[1].score.controle.valeur, 0.0);
    }

    #[tokio::test]
    async fn security_la_trace_conserve_aussi_les_ecartes() {
        // Ne garder que les retenus rendrait la trace inutile pour celui qui
        // demande des comptes — la seule personne à qui elle est destinée.
        let d = demande();
        let proches: Vec<_> = (0..12).map(|i| proche(100.0 * (i + 1) as f64, 0)).collect();
        let (_, _, traces) = matcher(proches, &d).await;

        let lignes = traces.lignes.borrow();
        let ecartes: Vec<_> = lignes.iter().filter(|l| !l.retenu).collect();
        assert_eq!(ecartes.len(), 2);
        for ligne in ecartes {
            assert_eq!(ligne.motif_ecart, Some(MotifEcart::HorsTop));
            // Le score de l'écarté est conservé : c'est ce qui permet de lui
            // répondre autrement que par « vous n'avez pas été retenu ».
            assert!(ligne.score.total > 0.0);
        }
    }

    #[tokio::test]
    async fn security_aucun_candidat_n_est_rendu_si_la_trace_n_a_pas_pu_etre_ecrite() {
        // Une notification qu'aucune trace n'explique est précisément ce que
        // l'AI Act interdit.
        let d = demande();
        let providers = ProvidersMemoire {
            proches: vec![proche(100.0, 0)],
        };
        let r = chercher_candidats(
            &providers,
            &DemandesMemoire::default(),
            &TracesMemoire {
                en_panne: true,
                ..Default::default()
            },
            &HorlogeFigee(instant()),
            &d,
        )
        .await;
        assert!(matches!(r, Err(ErreurMatching::Indisponible(_))));
    }

    #[tokio::test]
    async fn security_le_classement_est_deterministe_a_score_egal() {
        // Sans départage explicite, deux candidats au même score
        // s'ordonneraient au gré de la base, et la trace cesserait d'expliquer
        // le classement.
        let d = demande();
        let proches: Vec<_> = (0..5).map(|_| proche(1_000.0, 0)).collect();
        let (premier, _, _) = matcher(proches.clone(), &d).await;
        let (second, _, _) = matcher(proches, &d).await;
        assert_eq!(premier.unwrap(), second.unwrap());
    }

    #[tokio::test]
    async fn security_la_ventilation_du_score_est_conservee_pour_chaque_ligne() {
        let d = demande();
        let (_, _, traces) = matcher(vec![proche(1_000.0, 30)], &d).await;
        let lignes = traces.lignes.borrow();
        let ligne = &lignes[0];
        assert!(ligne.score.proximite.poids > 0.0);
        assert!(ligne.score.controle.poids > 0.0);
        // L'absence de note est visible : la trace dit de quoi le score était
        // réellement fait, y compris de ce qui lui manquait.
        assert!(ligne.score.note.is_none());
    }
}
