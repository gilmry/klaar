//! Médiation d'un litige par l'exploitation (FR-036, Story 7.4).
//!
//! **Ce que ce module fait, et ce qu'il ne fait pas.** Il présente les dossiers
//! ouverts, applique une décision et la consigne. Il **n'exécute aucun
//! mouvement d'argent** : le séquestre est chez Stripe, qui n'est pas
//! provisionné (Epic 5). La décision écrit donc le montant dû et le laisse en
//! attente, plutôt que d'annoncer un remboursement qui n'aura pas lieu. C'est
//! une limite écrite plutôt que découverte au premier litige réel.
//!
//! **Le remboursement partiel de FR-036 `@happy` est calculé, pas approximé.**
//! Le domaine en fixe les bornes et l'arrondi ; ce module ne fait que fournir le
//! montant en jeu.

use chrono::{DateTime, Utc};
use klaar_identity::Permission;
use klaar_trust::{doit_escalader, trancher as trancher_domaine, Decision, Issue, MediationError};
use std::fmt;
use uuid::Uuid;

use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::litige_repository::{DossierLitige, LitigeRepository};
use crate::ports::ops_repository::OpsRepository;
use crate::usecases::ops::{autoriser_et_consigner, ErreurOps};

/// Dossiers rendus en une page.
///
/// Cinquante. Au-delà, ce n'est plus une file de médiation mais un arriéré, et
/// la réponse n'est pas de faire défiler une liste plus longue.
pub const DOSSIERS_PAR_PAGE: i64 = 50;

/// Un dossier, avec ce que l'exploitation doit voir d'un coup d'œil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VueDossier {
    pub dossier: DossierLitige,
    /// Jours écoulés depuis l'ouverture.
    pub age_jours: i64,
    /// Ouvert depuis plus de trente jours (FR-036 `@edge`).
    ///
    /// **Exposé plutôt que recalculé par l'écran.** Deux calculs du même seuil
    /// finissent par diverger, et c'est le seuil d'alerte qui se tairait.
    pub a_escalader: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurMediation {
    /// Dossier inconnu.
    Introuvable,
    /// Refus du domaine : déjà tranché, part hors bornes.
    Domaine(MediationError),
    /// Refus d'exploitation : droit manquant, compte désactivé.
    Ops(ErreurOps),
    Indisponible(String),
}

impl ErreurMediation {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "DISPUTE_NOT_FOUND",
            Self::Domaine(e) => e.code(),
            Self::Ops(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurMediation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "litige introuvable"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Ops(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurMediation {}

impl From<RepositoryError> for ErreurMediation {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

impl From<ErreurOps> for ErreurMediation {
    fn from(e: ErreurOps) -> Self {
        Self::Ops(e)
    }
}

/// La file de médiation, du plus ancien au plus récent.
pub async fn file<L, O, H>(
    litiges: &L,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
) -> Result<Vec<VueDossier>, ErreurMediation>
where
    L: LitigeRepository,
    O: OpsRepository,
    H: Horloge,
{
    // **La consultation de la file est journalisée elle aussi.** Savoir qui
    // regarde des litiges, et pas seulement qui les tranche, est ce qu'un audit
    // vient chercher.
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::TrancherLitige,
        Some("queue"),
    )
    .await?;

    let maintenant = horloge.maintenant();
    Ok(litiges
        .ouverts(DOSSIERS_PAR_PAGE)
        .await?
        .into_iter()
        .map(|dossier| vue(dossier, maintenant))
        .collect())
}

/// Un dossier précis.
pub async fn dossier<L, O, H>(
    litiges: &L,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
    litige_id: Uuid,
) -> Result<VueDossier, ErreurMediation>
where
    L: LitigeRepository,
    O: OpsRepository,
    H: Horloge,
{
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::TrancherLitige,
        Some(&litige_id.to_string()),
    )
    .await?;

    let maintenant = horloge.maintenant();
    litiges
        .dossier(litige_id)
        .await?
        .map(|d| vue(d, maintenant))
        .ok_or(ErreurMediation::Introuvable)
}

/// Tranche un litige.
///
/// **L'autorisation vient avant la lecture du dossier.** L'ordre inverse
/// permettrait à quelqu'un sans droit d'apprendre si un identifiant de litige
/// existe, en distinguant un 403 d'un 404.
pub async fn trancher<L, O, H>(
    litiges: &L,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
    litige_id: Uuid,
    decision: Decision,
) -> Result<Issue, ErreurMediation>
where
    L: LitigeRepository,
    O: OpsRepository,
    H: Horloge,
{
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::TrancherLitige,
        Some(&litige_id.to_string()),
    )
    .await?;

    let dossier = litiges
        .dossier(litige_id)
        .await?
        .ok_or(ErreurMediation::Introuvable)?;

    // Le domaine borne la part et calcule la répartition ; le statut passé est
    // « ouvert » parce que seul un litige ouvert est présenté au calcul — la
    // vérification qui compte est le compare-and-swap ci-dessous, qu'aucun
    // appelant ne peut contourner.
    let issue = trancher_domaine(
        klaar_trust::StatutLitige::Ouvert,
        decision,
        dossier.total_ttc_cents,
    )
    .map_err(ErreurMediation::Domaine)?;

    // **C'est la base qui refuse la seconde décision.** Deux médiateurs sur le
    // même dossier : le second obtient `None`, et voit que l'affaire est réglée
    // au lieu de déclencher un second remboursement.
    litiges
        .trancher(litige_id, issue, ops_id, horloge.maintenant())
        .await?
        .ok_or(ErreurMediation::Domaine(MediationError::DejaTranche))?;

    Ok(issue)
}

fn vue(dossier: DossierLitige, maintenant: DateTime<Utc>) -> VueDossier {
    let age_jours = (maintenant - dossier.ouvert_le).num_days().max(0);
    let a_escalader = doit_escalader(dossier.ouvert_le, maintenant);
    VueDossier {
        dossier,
        age_jours,
        a_escalader,
    }
}
