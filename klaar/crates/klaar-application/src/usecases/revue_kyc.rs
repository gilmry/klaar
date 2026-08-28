//! Revue du contrôle d'entreprise par l'exploitation (FR-038, Story 8.1).
//!
//! **Valider ouvre, refuser ferme, et les deux ne coûtent pas le même prix.**
//! Une validation trop généreuse se corrige : le prestataire sera suspendu au
//! premier incident. Un refus injuste ne se corrige pas — l'entreprise est
//! partie voir ailleurs. D'où les quatre yeux sur le refus seul.
//!
//! **Ce qui n'est pas fait, et pourquoi.** Aucun courriel n'est envoyé à
//! l'entreprise (FR-038 `@happy` en demande un) : le service de courriel est
//! journalisé et non expédié tant qu'aucun fournisseur n'est provisionné. La
//! décision est écrite et lisible ; l'avis suivra avec l'envoi transactionnel.

use chrono::Duration;
use klaar_identity::{DecisionKyc, Permission, RevueError, RevueKyc, StatutProvider};
use std::fmt;
use uuid::Uuid;

use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::ops_repository::OpsRepository;
use crate::ports::provider_repository::ProviderRepository;
use crate::ports::revue_kyc_repository::{DossierKyc, RevueKycRepository};
use crate::usecases::ops::{autoriser_et_consigner, ErreurOps};

/// Dossiers rendus en une page.
pub const DOSSIERS_PAR_PAGE: i64 = 50;

/// Ancienneté au-delà de laquelle une demande en attente est signalée.
///
/// Sept jours. Une entreprise qui attend une semaine son autorisation d'exercer
/// a déjà commencé à travailler ailleurs.
pub const ATTENTE_SIGNALEE_JOURS: i64 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurRevue {
    Introuvable,
    /// Refus du domaine : motif manquant, entreprise déjà traitée ou retirée.
    Domaine(RevueError),
    Ops(ErreurOps),
    /// Une revue est déjà en attente de confirmation pour cette entreprise.
    DejaProposee,
    Indisponible(String),
}

impl ErreurRevue {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "PROVIDER_NOT_FOUND",
            Self::Domaine(e) => e.code(),
            Self::Ops(e) => e.code(),
            Self::DejaProposee => "REVIEW_ALREADY_PROPOSED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurRevue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "entreprise introuvable"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Ops(e) => write!(f, "{e}"),
            Self::DejaProposee => write!(f, "un refus attend déjà sa confirmation"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurRevue {}

impl From<RepositoryError> for ErreurRevue {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

impl From<ErreurOps> for ErreurRevue {
    fn from(e: ErreurOps) -> Self {
        Self::Ops(e)
    }
}

/// Ce qu'une décision produit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IssueRevue {
    /// Statut atteint par l'entreprise, ou `None` si le refus attend encore sa
    /// seconde paire d'yeux.
    pub statut: Option<StatutProvider>,
    /// La décision attend une confirmation (FR-038 `@edge`).
    pub attend_confirmation: bool,
    /// **Aucun courriel n'est parti.** Rendu explicitement pour que la console
    /// ne laisse pas croire que l'entreprise a été prévenue.
    pub notifie: bool,
}

/// Un dossier, avec son signalement d'attente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VueDossierKyc {
    pub dossier: DossierKyc,
    /// En attente depuis plus de sept jours.
    pub attente_longue: bool,
}

/// La file des entreprises à contrôler.
pub async fn file<R, O, H>(
    revues: &R,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
) -> Result<Vec<VueDossierKyc>, ErreurRevue>
where
    R: RevueKycRepository,
    O: OpsRepository,
    H: Horloge,
{
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::ReviserKyc,
        Some("queue"),
    )
    .await?;

    Ok(revues
        .en_attente(DOSSIERS_PAR_PAGE)
        .await?
        .into_iter()
        .map(|dossier| VueDossierKyc {
            attente_longue: dossier.attente_jours >= ATTENTE_SIGNALEE_JOURS,
            dossier,
        })
        .collect())
}

/// Décide du sort d'une entreprise.
///
/// Une **validation** prend effet immédiatement. Un **refus** est proposé, puis
/// n'a d'effet qu'une fois confirmé par un autre compte : c'est le même appel
/// qui sert aux deux gestes, et le second examinateur reconnaît le dossier à
/// son refus en attente.
pub async fn decider<R, O, H>(
    revues: &R,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
    provider_id: Uuid,
    decision: DecisionKyc,
    motif: Option<&str>,
) -> Result<IssueRevue, ErreurRevue>
where
    R: RevueKycRepository,
    O: OpsRepository,
    H: Horloge,
{
    // **L'autorisation vient avant la lecture du dossier.** L'ordre inverse
    // permettrait à quelqu'un sans droit d'apprendre si un identifiant
    // d'entreprise existe, en distinguant un 403 d'un 404.
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::ReviserKyc,
        Some(&provider_id.to_string()),
    )
    .await?;

    let (_, statut) = revues
        .dossier(provider_id)
        .await?
        .ok_or(ErreurRevue::Introuvable)?;

    let maintenant = horloge.maintenant();

    // Un refus déjà proposé : ce geste-ci est la confirmation, pas une seconde
    // proposition.
    if let Some(mut en_attente) = revues.en_attente_de_confirmation(provider_id).await? {
        // **Confirmer, c'est acquiescer au refus proposé, pas en formuler un
        // autre.** Accepter un nouveau motif ici laisserait le dossier porter
        // une raison que le premier examinateur n'a pas écrite.
        if decision != DecisionKyc::Refuser {
            return Err(ErreurRevue::DejaProposee);
        }
        en_attente
            .confirmer(ops_id, maintenant)
            .map_err(ErreurRevue::Domaine)?;
        let statut_final = en_attente
            .statut_resultant()
            .expect("une revue confirmée porte un statut");
        if !revues.clore(&en_attente, statut_final, maintenant).await? {
            // L'entreprise s'est retirée, ou quelqu'un a tranché entre-temps.
            return Err(ErreurRevue::Domaine(RevueError::PlusEnAttente {
                statut: revues
                    .dossier(provider_id)
                    .await?
                    .map(|(_, s)| s)
                    .unwrap_or(StatutProvider::Retire),
            }));
        }
        return Ok(IssueRevue {
            statut: Some(statut_final),
            attend_confirmation: false,
            notifie: false,
        });
    }

    let revue = RevueKyc::proposer(provider_id, statut, decision, motif, ops_id, maintenant)
        .map_err(ErreurRevue::Domaine)?;

    match revue.statut_resultant() {
        // Validation : effet immédiat, dans la même transaction que l'écriture
        // de la revue.
        Some(statut_final) => {
            if !revues.clore(&revue, statut_final, maintenant).await? {
                return Err(ErreurRevue::Domaine(RevueError::PlusEnAttente {
                    statut: StatutProvider::Retire,
                }));
            }
            Ok(IssueRevue {
                statut: Some(statut_final),
                attend_confirmation: false,
                notifie: false,
            })
        }
        // Refus : proposé, sans aucun effet sur l'entreprise.
        None => {
            if !revues.proposer(&revue).await? {
                return Err(ErreurRevue::DejaProposee);
            }
            Ok(IssueRevue {
                statut: None,
                attend_confirmation: true,
                notifie: false,
            })
        }
    }
}

/// L'entreprise retire sa demande d'inscription (FR-038 `@edge`).
///
/// **Ce n'est pas un refus.** Personne n'a rien jugé ; lui inscrire un refus au
/// dossier consignerait une décision qui n'a pas été prise.
pub async fn retirer<R, P>(
    revues: &R,
    prestataires: &P,
    utilisateur_id: Uuid,
) -> Result<bool, ErreurRevue>
where
    R: RevueKycRepository,
    P: ProviderRepository,
{
    let provider = prestataires
        .par_utilisateur_id(utilisateur_id)
        .await?
        .ok_or(ErreurRevue::Introuvable)?;
    Ok(revues.retirer(provider.id).await?)
}

/// Vrai si une demande attend depuis trop longtemps.
pub fn attente_longue(jours: i64) -> bool {
    jours >= ATTENTE_SIGNALEE_JOURS
}

/// Échéance de signalement, pour l'affichage.
pub fn echeance_signalement(
    inscrit_le: chrono::DateTime<chrono::Utc>,
) -> chrono::DateTime<chrono::Utc> {
    inscrit_le + Duration::days(ATTENTE_SIGNALEE_JOURS)
}
