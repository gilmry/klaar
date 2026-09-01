//! Cas d'usage « valider la fin d'une Mission » (FR-021, Story 4.6).
//!
//! **Livré sans le virement, et c'est écrit.** FR-021 fait de la validation le
//! moment où Stripe libère le séquestre. Le compte n'est pas ouvert : ce cas
//! d'usage prononce la libération et l'enregistre ; le versement rejoindra
//! l'Epic 5, et il lira ces lignes plutôt que de recalculer.
//!
//! **Seul le demandeur valide.** Le prestataire déclare avoir terminé ; c'est
//! une autre personne qui dit que c'est fait. Confondre les deux reviendrait à
//! laisser quelqu'un signer la réception de son propre travail.
//!
//! **Soixante-douze heures, puis le service valide à sa place.** Sans ce délai,
//! un demandeur qui ne rouvre jamais l'application retiendrait indéfiniment
//! l'argent d'un travail fait, et c'est le prestataire qui paierait le silence.

use chrono::{DateTime, Duration, Utc};
use klaar_intervention::StatutMission;
use klaar_payment::{Liberation, LiberationError, OrigineValidation, DELAI_VALIDATION_HEURES};
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::devis_repository::DevisRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::liberation_repository::{
    LiberationRepository, ResultatLiberation, ValidationEnAttente,
};
use crate::ports::mission_repository::MissionRepository;

/// Missions traitées en un passage du balayage.
pub const PAR_PASSAGE_MAX: i64 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurValidation {
    /// Mission inconnue, ou qui ne regarde pas ce compte.
    Introuvable,
    /// La Mission n'est pas terminée : il n'y a rien à valider.
    PasTerminee,
    /// Déjà validée. FR-021 `@negative` demande 409 `ALREADY_RELEASED`.
    DejaValidee,
    /// Aucun devis accepté : il n'y a pas d'accord à honorer.
    Domaine(LiberationError),
    Indisponible(String),
}

impl ErreurValidation {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::PasTerminee => "MISSION_NOT_COMPLETED",
            Self::DejaValidee => "ALREADY_RELEASED",
            Self::Domaine(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurValidation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::PasTerminee => write!(f, "l'intervention n'est pas déclarée terminée"),
            Self::DejaValidee => write!(f, "cette intervention a déjà été validée"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurValidation {}

impl From<RepositoryError> for ErreurValidation {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que la validation produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionValidee {
    pub liberation: Liberation,
    /// Le compte du prestataire, pour le prévenir sans relire sa fiche.
    pub provider_id: Uuid,
}

/// Valide une Mission terminée, à la main de son demandeur.
pub async fn valider<D, M, Q, L, H>(
    demandes: &D,
    missions: &M,
    devis_repo: &Q,
    liberations: &L,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
) -> Result<MissionValidee, ErreurValidation>
where
    D: DemandeRepository,
    M: MissionRepository,
    Q: DevisRepository,
    L: LiberationRepository,
    H: Horloge,
{
    let mission = missions
        .par_id(mission_id)
        .await?
        .ok_or(ErreurValidation::Introuvable)?;
    let demande = demandes
        .par_id(mission.demande_id)
        .await?
        .ok_or(ErreurValidation::Introuvable)?;
    // Le demandeur, et personne d'autre. Un refus indistinct : celui qui n'a
    // rien à faire là n'apprend pas si cette Mission existe.
    if demande.demandeur_id != utilisateur_id {
        return Err(ErreurValidation::Introuvable);
    }

    // Les deux refus sont distincts parce qu'ils appellent des gestes
    // différents : attendre que le prestataire déclare avoir fini, ou constater
    // que c'est déjà fait.
    match mission.statut {
        StatutMission::Validee => return Err(ErreurValidation::DejaValidee),
        StatutMission::Terminee => {}
        _ => return Err(ErreurValidation::PasTerminee),
    }

    prononcer(
        devis_repo,
        liberations,
        mission_id,
        mission.provider_id,
        OrigineValidation::Demandeur,
        horloge.maintenant(),
    )
    .await
}

/// Bilan d'un passage du balayage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BilanValidation {
    /// Missions passées en `VALIDATED`.
    pub validees: usize,
    /// Missions échues restant sans devis accepté.
    ///
    /// **Comptées, plus parcourues.** Elles figuraient autrefois dans le lot
    /// traité à chaque passage, qu'elles ne quittaient jamais faute de montant
    /// à libérer : passé deux cents, elles remplissaient le lot à elles seules
    /// et le balayage cessait de valider quoi que ce soit de plus récent. Le
    /// chiffre reste rendu parce que c'est un signal d'exploitation ; il vient
    /// désormais d'un décompte, pas d'un parcours.
    pub sans_accord: usize,
}

/// Valide les Missions terminées depuis plus de soixante-douze heures.
///
/// **Ce que le balayage ne force pas.** Une Mission terminée sans devis accepté
/// n'est pas validée : il n'y a pas de montant convenu, donc rien à libérer.
/// Elle est comptée à part plutôt qu'ignorée en silence — c'est un signal
/// d'exploitation, pas un cas normal. Elle ne figure plus dans le lot traité :
/// voir `BilanValidation::sans_accord`.
pub async fn valider_les_echues<Q, L, H>(
    devis_repo: &Q,
    liberations: &L,
    horloge: &H,
) -> Result<BilanValidation, RepositoryError>
where
    Q: DevisRepository,
    L: LiberationRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    // Le seuil se lit à l'envers du délai : une Mission terminée avant cet
    // instant a dépassé les soixante-douze heures.
    let avant = maintenant - Duration::hours(DELAI_VALIDATION_HEURES);
    let en_attente = liberations
        .a_valider_automatiquement(avant, PAR_PASSAGE_MAX)
        .await?;

    let mut bilan = BilanValidation {
        sans_accord: liberations.compter_sans_accord(avant).await? as usize,
        ..BilanValidation::default()
    };
    for ValidationEnAttente {
        mission_id,
        provider_id,
        ..
    } in en_attente
    {
        match prononcer(
            devis_repo,
            liberations,
            mission_id,
            provider_id,
            OrigineValidation::Automatique,
            maintenant,
        )
        .await
        {
            Ok(_) => bilan.validees += 1,
            // Le lot ne contient plus que des Missions à devis accepté : un
            // refus du domaine y est désormais anormal, et non l'absence
            // d'accord. Le compter avec les autres masquerait un vrai défaut.
            Err(ErreurValidation::Domaine(e)) => {
                tracing::warn!(mission_id = %mission_id, erreur = ?e, "validation automatique refusée par le domaine");
            }
            // Une Mission validée entre-temps par son demandeur : le balayage
            // arrive après, et c'est très bien.
            Err(ErreurValidation::DejaValidee | ErreurValidation::PasTerminee) => {}
            Err(ErreurValidation::Indisponible(d)) => return Err(RepositoryError::Indisponible(d)),
            // Injoignable : le balayage ne lit pas de Demande et ne vérifie
            // aucun droit. Le bras reste pour que le `match` soit exhaustif.
            Err(ErreurValidation::Introuvable) => {}
        }
    }
    Ok(bilan)
}

/// Le tronc commun des deux chemins : lire l'accord, prononcer, écrire.
async fn prononcer<Q, L>(
    devis_repo: &Q,
    liberations: &L,
    mission_id: Uuid,
    provider_id: Uuid,
    origine: OrigineValidation,
    maintenant: DateTime<Utc>,
) -> Result<MissionValidee, ErreurValidation>
where
    Q: DevisRepository,
    L: LiberationRepository,
{
    let devis = devis_repo
        .dernier_pour_mission(mission_id)
        .await?
        .ok_or(ErreurValidation::Domaine(LiberationError::DevisNonAccepte))?;

    let liberation = Liberation::prononcer(mission_id, &devis, origine, maintenant)
        .map_err(ErreurValidation::Domaine)?;

    match liberations.prononcer(&liberation, maintenant).await? {
        ResultatLiberation::Prononcee(ecrite) => Ok(MissionValidee {
            liberation: ecrite,
            provider_id,
        }),
        // La Mission a changé d'état entre la lecture et l'écriture : quelqu'un
        // d'autre l'a validée. Ce n'est pas une erreur du demandeur.
        ResultatLiberation::MissionNonTerminee => Err(ErreurValidation::DejaValidee),
    }
}
