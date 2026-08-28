//! Cas d'usage « suivre le trajet du prestataire » (FR-019, Story 4.4).
//!
//! **Le consentement est demandé au prestataire, la position montrée au
//! demandeur.** Deux personnes, deux droits : celui qui partage doit pouvoir
//! refuser sans conséquence, celui qui attend doit savoir s'il peut espérer une
//! position ou non.
//!
//! **Aucun chemin n'écrit une position hors intervention en route et
//! consentie** (invariant §10.5) : c'est le domaine qui refuse, et ce cas
//! d'usage ne fait que lui fournir les deux réponses dont il a besoin.

use chrono::Duration;
use klaar_intervention::{
    etat_suivi, relever, EtatSuivi, PositionSuivie, StatutMission, SuiviError, PURGE_HEURES,
};
use klaar_shared_kernel::Geo;
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::mission_repository::MissionRepository;
use crate::ports::provider_repository::ProviderRepository;
use crate::ports::suivi_repository::SuiviRepository;

/// Relevés traités en un passage de purge.
pub const PAR_PASSAGE_MAX: i64 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurSuivi {
    /// Mission inconnue, ou qui ne regarde pas ce compte.
    Introuvable,
    /// Refusé par le domaine : hors trajet, ou sans consentement.
    Domaine(SuiviError),
    Indisponible(String),
}

impl ErreurSuivi {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::Domaine(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurSuivi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurSuivi {}

impl From<RepositoryError> for ErreurSuivi {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que le demandeur voit du trajet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VueSuivi {
    pub etat: EtatSuivi,
    /// Dernière position connue, déjà dégradée. `None` quand rien n'a été
    /// partagé — l'écran dit alors « position non partagée » plutôt que de
    /// laisser une carte vide sans explication.
    pub derniere: Option<PositionSuivie>,
}

/// Le prestataire accepte ou retire le partage pour cette intervention.
pub async fn consentir<M, P, S, H>(
    missions: &M,
    prestataires: &P,
    suivis: &S,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    accepte: bool,
) -> Result<bool, ErreurSuivi>
where
    M: MissionRepository,
    P: ProviderRepository,
    S: SuiviRepository,
    H: Horloge,
{
    let mission = missions
        .par_id(mission_id)
        .await?
        .ok_or(ErreurSuivi::Introuvable)?;
    match prestataires.par_utilisateur_id(utilisateur_id).await? {
        Some(p) if mission.appartient_a(p.id) => {}
        _ => return Err(ErreurSuivi::Introuvable),
    }

    let maintenant = horloge.maintenant();
    if accepte {
        suivis.consentir(mission_id, maintenant).await?;
        Ok(true)
    } else {
        // **Le retrait ne supprime pas les positions déjà relevées.** Elles ont
        // été partagées de plein gré et le demandeur les a vues ; les effacer
        // rétroactivement lui ferait perdre l'information sur laquelle il s'est
        // organisé. Elles disparaissent avec la purge, comme les autres.
        suivis.retirer_consentement(mission_id, maintenant).await?;
        Ok(false)
    }
}

/// Le prestataire envoie sa position.
pub async fn relever_position<M, P, S, H>(
    missions: &M,
    prestataires: &P,
    suivis: &S,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    position: Geo,
) -> Result<PositionSuivie, ErreurSuivi>
where
    M: MissionRepository,
    P: ProviderRepository,
    S: SuiviRepository,
    H: Horloge,
{
    let mission = missions
        .par_id(mission_id)
        .await?
        .ok_or(ErreurSuivi::Introuvable)?;
    match prestataires.par_utilisateur_id(utilisateur_id).await? {
        Some(p) if mission.appartient_a(p.id) => {}
        _ => return Err(ErreurSuivi::Introuvable),
    }

    let consenti = suivis.consenti(mission_id).await?;
    // C'est le domaine qui refuse, avec les deux réponses qu'on vient de lui
    // donner : aucun chemin n'écrit une position sans elles.
    let releve = relever(
        mission_id,
        mission.statut,
        consenti,
        position,
        horloge.maintenant(),
    )
    .map_err(ErreurSuivi::Domaine)?;

    suivis.relever(&releve).await?;
    Ok(releve)
}

/// Le demandeur regarde où en est le prestataire.
pub async fn consulter<D, M, S, H>(
    demandes: &D,
    missions: &M,
    suivis: &S,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
) -> Result<VueSuivi, ErreurSuivi>
where
    D: DemandeRepository,
    M: MissionRepository,
    S: SuiviRepository,
    H: Horloge,
{
    let mission = missions
        .par_id(mission_id)
        .await?
        .ok_or(ErreurSuivi::Introuvable)?;
    let demande = demandes
        .par_id(mission.demande_id)
        .await?
        .ok_or(ErreurSuivi::Introuvable)?;
    if demande.demandeur_id != utilisateur_id {
        return Err(ErreurSuivi::Introuvable);
    }

    // **La dernière position n'est lue que pendant le trajet.** La chercher
    // après l'arrivée serait interroger la base pour une donnée que la vue
    // n'affichera pas, et surtout la sortir alors que plus rien ne le justifie.
    let derniere = if mission.statut == StatutMission::EnRoute {
        suivis.derniere(mission_id).await?
    } else {
        None
    };

    Ok(VueSuivi {
        etat: etat_suivi(derniere.as_ref(), mission.statut, horloge.maintenant()),
        derniere,
    })
}

/// Purge les positions des interventions finies depuis plus de vingt-quatre
/// heures (FR-019 `@security`).
pub async fn purger_les_traces<S, H>(suivis: &S, horloge: &H) -> Result<u64, RepositoryError>
where
    S: SuiviRepository,
    H: Horloge,
{
    let avant = horloge.maintenant() - Duration::hours(PURGE_HEURES);
    suivis.purger_les_echues(avant, PAR_PASSAGE_MAX).await
}
