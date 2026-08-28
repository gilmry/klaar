//! Cas d'usage « annuler une Mission en cours » (FR-022, Story 4.7).
//!
//! **Les deux parties peuvent annuler, et cela ne veut pas dire la même chose.**
//! Le demandeur qui renonce récupère son argent, moins le déplacement si
//! quelqu'un était déjà chez lui. Le prestataire qui se désiste rend tout, et
//! son désistement est compté — trois en trente jours suspendent son compte.
//!
//! **Livré sans le remboursement.** Le mouvement d'argent est Stripe ; ce cas
//! d'usage calcule ce qui est dû à qui et l'enregistre. L'Epic 5 lira ces lignes
//! plutôt que de recalculer.
//!
//! **La suspension suit l'annulation mais n'en fait pas partie.** Une écriture
//! de suspension qui échoue ne doit pas défaire une annulation déjà prononcée :
//! la Mission est close, c'est le fait principal, et le compteur se rattrape au
//! désistement suivant.

use chrono::{Duration, Utc};
use klaar_identity::StatutProvider;
use klaar_intervention::{
    AnnulationError, AnnulationMission, AuteurAnnulation, MotifAnnulationMission,
    DESISTEMENTS_AVANT_SUSPENSION, FENETRE_DESISTEMENTS_JOURS,
};
use klaar_shared_kernel::Money;
use std::fmt;
use uuid::Uuid;

use crate::ports::annulation_repository::{AnnulationRepository, ResultatAnnulation};
use crate::ports::demande_repository::DemandeRepository;
use crate::ports::devis_repository::DevisRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::mission_repository::MissionRepository;
use crate::ports::provider_repository::ProviderRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurAnnulationMission {
    /// Mission inconnue, ou qui ne regarde ni ce demandeur ni ce prestataire.
    Introuvable,
    /// Motif hors vocabulaire.
    MotifInconnu,
    /// Refusé par le domaine : intervention faite, ou déjà annulée.
    Domaine(AnnulationError),
    /// La Mission a changé d'état entre la lecture et l'écriture.
    Concurrence,
    Indisponible(String),
}

impl ErreurAnnulationMission {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::MotifInconnu => "REASON_UNKNOWN",
            Self::Domaine(e) => e.code(),
            // Le même code qu'une Mission déjà annulée : dans les deux cas,
            // elle n'était plus dans l'état où l'appelant la croyait.
            Self::Concurrence => "MISSION_ALREADY_CANCELLED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurAnnulationMission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::MotifInconnu => write!(f, "motif d'annulation inconnu"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Concurrence => write!(f, "la Mission a changé d'état entre-temps"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurAnnulationMission {}

impl From<RepositoryError> for ErreurAnnulationMission {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que l'annulation produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionAnnulee {
    pub annulation: AnnulationMission,
    /// Vrai si ce désistement a suspendu le prestataire (FR-022 `@edge`).
    pub prestataire_suspendu: bool,
    /// Le compte à prévenir : celui de l'autre partie.
    pub a_prevenir: Uuid,
}

/// Les dépôts dont l'annulation a besoin, groupés.
///
/// **Six dépôts à la suite finissent par se remplir dans le mauvais ordre**, et
/// le compilateur n'en rattraperait qu'une partie : plusieurs d'entre eux sont
/// des paramètres génériques distincts, donc interchangeables du point de vue
/// de la signature. Les nommer coûte une structure et supprime la classe
/// d'erreur.
pub struct Depots<'a, D, M, Q, A, P, H> {
    pub demandes: &'a D,
    pub missions: &'a M,
    pub devis: &'a Q,
    pub annulations: &'a A,
    pub prestataires: &'a P,
    pub horloge: &'a H,
}

/// Annule une Mission, à la main de l'une ou l'autre partie.
pub async fn annuler_mission<D, M, Q, A, P, H>(
    depots: Depots<'_, D, M, Q, A, P, H>,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    motif: Option<&str>,
) -> Result<MissionAnnulee, ErreurAnnulationMission>
where
    D: DemandeRepository,
    M: MissionRepository,
    Q: DevisRepository,
    A: AnnulationRepository,
    P: ProviderRepository,
    H: Horloge,
{
    let Depots {
        demandes,
        missions,
        devis: devis_repo,
        annulations,
        prestataires,
        horloge,
    } = depots;
    let maintenant = horloge.maintenant();

    // Le motif est validé avant toute lecture : le refuser ensuite aurait dit
    // au passage que cette Mission existe.
    let motif = match motif {
        Some(brut) => {
            Some(MotifAnnulationMission::parse(brut).ok_or(ErreurAnnulationMission::MotifInconnu)?)
        }
        None => None,
    };

    let mission = missions
        .par_id(mission_id)
        .await?
        .ok_or(ErreurAnnulationMission::Introuvable)?;
    let demande = demandes
        .par_id(mission.demande_id)
        .await?
        .ok_or(ErreurAnnulationMission::Introuvable)?;

    // Qui appelle détermine ce que l'annulation coûte. L'ordre compte : le
    // demandeur est le cas le plus fréquent, et il évite de charger la fiche
    // prestataire d'un compte qui n'en a pas.
    let (auteur, a_prevenir) = if demande.demandeur_id == utilisateur_id {
        // Le prestataire est prévenu : quelqu'un l'attendait.
        (AuteurAnnulation::Demandeur, mission.provider_id)
    } else {
        match prestataires.par_utilisateur_id(utilisateur_id).await? {
            Some(p) if mission.appartient_a(p.id) => {
                (AuteurAnnulation::Prestataire, demande.demandeur_id)
            }
            // Ni l'un ni l'autre : un refus indistinct, comme partout.
            _ => return Err(ErreurAnnulationMission::Introuvable),
        }
    };

    // Ce qui était engagé : le total d'un devis accepté, ou rien. Un devis
    // envoyé mais pas accepté n'engage personne — c'est une proposition.
    let engage = devis_repo
        .dernier_pour_mission(mission_id)
        .await?
        .filter(|d| d.statut == klaar_payment::StatutDevis::Accepte)
        .map(|d| d.total_ttc)
        .unwrap_or(Money::from_cents(0));

    let annulation = AnnulationMission::prononcer(
        mission_id,
        mission.statut,
        auteur,
        motif,
        engage,
        maintenant,
    )
    .map_err(ErreurAnnulationMission::Domaine)?;

    let ecrite = match annulations.prononcer(&annulation).await? {
        ResultatAnnulation::Prononcee(a) => a,
        ResultatAnnulation::MissionDejaClose => return Err(ErreurAnnulationMission::Concurrence),
    };

    // La suspension suit, et son échec ne défait rien : la Mission est close,
    // c'est le fait principal, et le compteur se rattrape au désistement
    // suivant.
    let prestataire_suspendu = if ecrite.consequence.penalise_le_prestataire {
        suspendre_si_recidive(annulations, prestataires, mission.provider_id, maintenant).await
    } else {
        false
    };

    Ok(MissionAnnulee {
        annulation: ecrite,
        prestataire_suspendu,
        a_prevenir,
    })
}

/// Suspend le prestataire au troisième désistement en trente jours.
///
/// Rend `false` sur échec plutôt que de propager : voir l'en-tête du module.
async fn suspendre_si_recidive<A, P>(
    annulations: &A,
    prestataires: &P,
    provider_id: Uuid,
    maintenant: chrono::DateTime<Utc>,
) -> bool
where
    A: AnnulationRepository,
    P: ProviderRepository,
{
    let depuis = maintenant - Duration::days(FENETRE_DESISTEMENTS_JOURS);
    let desistements = match annulations.desistements_depuis(provider_id, depuis).await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!(erreur = %e, "comptage des désistements impossible");
            return false;
        }
    };
    if desistements < DESISTEMENTS_AVANT_SUSPENSION {
        return false;
    }

    let Ok(Some(mut provider)) = prestataires.par_id(provider_id).await else {
        return false;
    };
    // Déjà suspendu : rien à faire, et surtout pas à réécrire une date de
    // suspension qui prolongerait la peine à chaque désistement compté.
    if provider.statut == StatutProvider::Suspendu {
        return false;
    }
    provider.statut = StatutProvider::Suspendu;
    match prestataires.mettre_a_jour_etat(&provider).await {
        Ok(()) => {
            tracing::warn!(
                desistements,
                "prestataire suspendu après désistements répétés"
            );
            true
        }
        Err(e) => {
            tracing::error!(erreur = %e, "suspension impossible");
            false
        }
    }
}
