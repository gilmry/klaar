//! Cas d'usage « noter après intervention » (FR-033, Story 7.1).
//!
//! **Qui note qui est déduit, jamais reçu.** Le demandeur note le prestataire,
//! le prestataire note le demandeur ; accepter une cible en entrée laisserait
//! quelqu'un se noter lui-même, ou noter à la place de l'autre.
//!
//! **Les deux notes se dévoilent ensemble.** Tant que l'une manque et que la
//! fenêtre est ouverte, aucune n'est rendue : si la note du demandeur
//! s'affichait avant celle du prestataire, celui-ci ajusterait la sienne, et les
//! deux perdraient toute valeur. Le service connaît les deux ; c'est la lecture
//! qui décide de ce qu'elle montre.

use chrono::{DateTime, Utc};
use klaar_trust::{publiables, Cible, Notation, NotationError};
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::mission_repository::MissionRepository;
use crate::ports::notation_repository::{NotationRepository, NotesDeMission, ResultatNotation};
use crate::ports::provider_repository::ProviderRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurNotation {
    /// Mission inconnue, ou qui ne regarde pas ce compte.
    Introuvable,
    /// L'intervention n'est pas validée : il n'y a rien à noter.
    PasValidee,
    /// Ce côté a déjà noté (FR-033 `@edge`).
    DejaNotee,
    /// Refusé par le domaine : note hors échelle, commentaire trop long,
    /// fenêtre fermée.
    Domaine(NotationError),
    Indisponible(String),
}

impl ErreurNotation {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::PasValidee => "MISSION_NOT_VALIDATED",
            Self::DejaNotee => "ALREADY_RATED",
            Self::Domaine(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurNotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::PasValidee => write!(f, "l'intervention n'est pas validée"),
            Self::DejaNotee => write!(f, "vous avez déjà noté cette intervention"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurNotation {}

impl From<RepositoryError> for ErreurNotation {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que la notation produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotationEcrite {
    pub notation: Notation,
    /// Vrai si les deux notes sont désormais visibles.
    pub publiee: bool,
}

/// Les dépôts dont la notation a besoin, groupés.
///
/// Quatre dépôts et une horloge à la suite : plusieurs sont des paramètres
/// génériques distincts, donc interchangeables du point de vue de la signature.
/// Les nommer coûte une structure et supprime la classe d'erreur.
pub struct Depots<'a, D, M, P, N, H> {
    pub demandes: &'a D,
    pub missions: &'a M,
    pub prestataires: &'a P,
    pub notations: &'a N,
    pub horloge: &'a H,
}

/// Ce que l'auteur écrit.
pub struct Avis {
    pub note: u8,
    pub commentaire: Option<String>,
}

/// Écrit la note d'une des deux parties.
pub async fn noter<D, M, P, N, H>(
    depots: Depots<'_, D, M, P, N, H>,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    avis: Avis,
) -> Result<NotationEcrite, ErreurNotation>
where
    D: DemandeRepository,
    M: MissionRepository,
    P: ProviderRepository,
    N: NotationRepository,
    H: Horloge,
{
    let Depots {
        demandes,
        missions,
        prestataires,
        notations,
        horloge,
    } = depots;
    let Avis { note, commentaire } = avis;
    let maintenant = horloge.maintenant();
    let cible = cible_de(demandes, missions, prestataires, utilisateur_id, mission_id).await?;

    // L'intervention doit être validée : noter avant que quelqu'un ait dit que
    // c'était fini reviendrait à juger un travail en cours.
    let validee_le = notations
        .validee_le(mission_id)
        .await?
        .ok_or(ErreurNotation::PasValidee)?;

    let notation = Notation::emettre(
        mission_id,
        utilisateur_id,
        cible,
        note,
        commentaire,
        validee_le,
        maintenant,
    )
    .map_err(ErreurNotation::Domaine)?;

    match notations.noter(&notation).await? {
        ResultatNotation::Ecrite(ecrite) => {
            let notes = notations.notes_de_mission(mission_id).await?;
            Ok(NotationEcrite {
                notation: ecrite,
                publiee: publiables(notes.les_deux_presentes(), validee_le, maintenant),
            })
        }
        ResultatNotation::DejaNotee => Err(ErreurNotation::DejaNotee),
    }
}

/// Les notes visibles d'une intervention, pour l'une ou l'autre partie.
///
/// Rend une paire vide tant que l'anti-représailles retient : le service
/// connaît les deux notes, la lecture n'en montre aucune.
pub async fn notes_visibles<N>(
    notations: &N,
    mission_id: Uuid,
    maintenant: DateTime<Utc>,
) -> Result<NotesDeMission, ErreurNotation>
where
    N: NotationRepository,
{
    let Some(validee_le) = notations.validee_le(mission_id).await? else {
        return Ok(NotesDeMission::default());
    };
    let notes = notations.notes_de_mission(mission_id).await?;
    if publiables(notes.les_deux_presentes(), validee_le, maintenant) {
        Ok(notes)
    } else {
        Ok(NotesDeMission::default())
    }
}

/// Qui l'appelant note, d'après son rôle dans cette intervention.
///
/// Le demandeur note le prestataire, et réciproquement. Tout autre compte
/// reçoit le même refus indistinct que partout ailleurs.
async fn cible_de<D, M, P>(
    demandes: &D,
    missions: &M,
    prestataires: &P,
    utilisateur_id: Uuid,
    mission_id: Uuid,
) -> Result<Cible, ErreurNotation>
where
    D: DemandeRepository,
    M: MissionRepository,
    P: ProviderRepository,
{
    let mission = missions
        .par_id(mission_id)
        .await?
        .ok_or(ErreurNotation::Introuvable)?;
    let demande = demandes
        .par_id(mission.demande_id)
        .await?
        .ok_or(ErreurNotation::Introuvable)?;

    if demande.demandeur_id == utilisateur_id {
        return Ok(Cible::Prestataire);
    }
    match prestataires.par_utilisateur_id(utilisateur_id).await? {
        Some(p) if mission.appartient_a(p.id) => Ok(Cible::Demandeur),
        _ => Err(ErreurNotation::Introuvable),
    }
}
