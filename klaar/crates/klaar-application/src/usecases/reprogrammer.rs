//! Cas d'usage « reprogrammer une intervention annulée » (FR-023, Story 4.8).
//!
//! **Le demandeur propose, le prestataire dispose.** Sans le second accord,
//! reprogrammer reviendrait à réattribuer d'office une intervention à quelqu'un
//! qui vient de dire qu'il ne pouvait pas venir.

use klaar_intervention::{Reprogrammation, ReprogrammationError};
use std::fmt;
use uuid::Uuid;

use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::provider_repository::ProviderRepository;
use crate::ports::reprogrammation_repository::{ReprogrammationRepository, ResultatAcceptation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurReprogrammation {
    /// Mission inconnue, ou qui ne regarde pas ce compte.
    Introuvable,
    /// Une proposition existe déjà pour cette intervention.
    DejaProposee,
    /// La proposition a déjà été acceptée ou refusée.
    DejaClose,
    /// Le prestataire s'est engagé ailleurs entre-temps.
    ProviderOccupe,
    /// Refusé par le domaine.
    Domaine(ReprogrammationError),
    Indisponible(String),
}

impl ErreurReprogrammation {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::DejaProposee => "RESCHEDULE_ALREADY_PROPOSED",
            Self::DejaClose => "RESCHEDULE_ALREADY_ANSWERED",
            Self::ProviderOccupe => "PROVIDER_BUSY",
            Self::Domaine(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurReprogrammation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::DejaProposee => write!(f, "une reprogrammation est déjà proposée"),
            Self::DejaClose => write!(f, "cette proposition a déjà reçu une réponse"),
            Self::ProviderOccupe => {
                write!(f, "le prestataire porte déjà une autre intervention")
            }
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurReprogrammation {}

impl From<RepositoryError> for ErreurReprogrammation {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Le demandeur propose de reprendre une intervention annulée.
pub async fn proposer<R, H>(
    reprogrammations: &R,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
) -> Result<Reprogrammation, ErreurReprogrammation>
where
    R: ReprogrammationRepository,
    H: Horloge,
{
    let contexte = reprogrammations
        .contexte(mission_id)
        .await?
        .ok_or(ErreurReprogrammation::Introuvable)?;

    // Seul le demandeur propose : c'est lui qui a besoin de l'intervention.
    if contexte.demandeur_id != utilisateur_id {
        return Err(ErreurReprogrammation::Introuvable);
    }

    let (auteur, annulee_le) = contexte.annulation.ok_or(ErreurReprogrammation::Domaine(
        ReprogrammationError::PasAnnulee,
    ))?;
    let devis_id = contexte
        .devis_accepte
        .ok_or(ErreurReprogrammation::Domaine(
            ReprogrammationError::SansAccord,
        ))?;

    let proposition = Reprogrammation::proposer(
        mission_id,
        devis_id,
        auteur,
        annulee_le,
        horloge.maintenant(),
    )
    .map_err(ErreurReprogrammation::Domaine)?;

    if !reprogrammations.proposer(&proposition).await? {
        // Une proposition existe déjà. Si elle a été refusée, c'est ce refus
        // qu'il faut dire — FR-023 `@negative` rend `PROVIDER_DECLINED`, et
        // « déjà proposée » enverrait le demandeur attendre une réponse qui est
        // déjà tombée.
        return match reprogrammations.par_mission(mission_id).await? {
            Some(existante)
                if existante.statut == klaar_intervention::StatutReprogrammation::Refusee =>
            {
                Err(ErreurReprogrammation::Domaine(
                    ReprogrammationError::DejaRefusee,
                ))
            }
            _ => Err(ErreurReprogrammation::DejaProposee),
        };
    }

    Ok(proposition)
}

/// Ce que l'acceptation produit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepriseAcceptee {
    pub nouvelle_mission: Uuid,
    /// Le compte du demandeur, à prévenir.
    pub demandeur_id: Uuid,
}

/// Le prestataire accepte ou décline.
pub async fn repondre<R, P, H>(
    reprogrammations: &R,
    prestataires: &P,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    accepte: bool,
) -> Result<Option<RepriseAcceptee>, ErreurReprogrammation>
where
    R: ReprogrammationRepository,
    P: ProviderRepository,
    H: Horloge,
{
    let contexte = reprogrammations
        .contexte(mission_id)
        .await?
        .ok_or(ErreurReprogrammation::Introuvable)?;

    // Seul le prestataire concerné répond.
    match prestataires.par_utilisateur_id(utilisateur_id).await? {
        Some(p) if p.id == contexte.provider_id => {}
        _ => return Err(ErreurReprogrammation::Introuvable),
    }

    if !accepte {
        return if reprogrammations.refuser(mission_id).await? {
            Ok(None)
        } else {
            Err(ErreurReprogrammation::DejaClose)
        };
    }

    match reprogrammations
        .accepter(mission_id, horloge.maintenant())
        .await?
    {
        ResultatAcceptation::Reprise { nouvelle_mission } => Ok(Some(RepriseAcceptee {
            nouvelle_mission,
            demandeur_id: contexte.demandeur_id,
        })),
        ResultatAcceptation::DejaClose => Err(ErreurReprogrammation::DejaClose),
        ResultatAcceptation::ProviderOccupe => Err(ErreurReprogrammation::ProviderOccupe),
    }
}
