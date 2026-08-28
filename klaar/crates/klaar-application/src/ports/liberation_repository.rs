//! Port des libérations (FR-021, Story 4.6).
//!
//! **L'atomicité est la raison d'être de ce port.** FR-021 `@security` demande
//! que la bascule de la Mission, le calcul de la répartition et l'enregistrement
//! forment une seule transaction : une Mission validée sans libération
//! laisserait un prestataire attendre un versement dont plus rien ne porte la
//! trace, et une libération sans bascule ferait payer deux fois au passage
//! suivant.

use chrono::{DateTime, Utc};
use klaar_payment::Liberation;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Issue d'une tentative de validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatLiberation {
    Prononcee(Liberation),
    /// La Mission n'était plus `COMPLETED` : elle a déjà été validée, ou elle
    /// n'en est pas là.
    MissionNonTerminee,
}

/// Une Mission terminée qui attend encore sa validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationEnAttente {
    pub mission_id: Uuid,
    pub demandeur_id: Uuid,
    pub provider_id: Uuid,
    /// Quand l'intervention a été déclarée terminée. C'est de cet instant que
    /// court le délai, et non de l'écriture en base : une transition déclarée
    /// hors connexion garde sa date.
    pub terminee_le: DateTime<Utc>,
}

#[allow(async_fn_in_trait)]
pub trait LiberationRepository {
    /// Fait basculer la Mission en `VALIDATED` **et** écrit la libération.
    ///
    /// Le statut de départ sert de garde : deux validations concurrentes — le
    /// demandeur et le balayage dans la même seconde — ne doivent pas toutes
    /// deux aboutir.
    async fn prononcer(
        &self,
        liberation: &Liberation,
        decidee_le: DateTime<Utc>,
    ) -> Result<ResultatLiberation, RepositoryError>;

    /// Libération d'une Mission, s'il y en a une.
    async fn par_mission(&self, mission_id: Uuid) -> Result<Option<Liberation>, RepositoryError>;

    /// Missions terminées depuis plus longtemps que le délai, et pas encore
    /// validées.
    ///
    /// Rend les plus anciennes d'abord : c'est celles dont l'attente est la
    /// plus longue, et un balayage borné doit les traiter en premier.
    async fn a_valider_automatiquement(
        &self,
        avant: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<ValidationEnAttente>, RepositoryError>;
}
