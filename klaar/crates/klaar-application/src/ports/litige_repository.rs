//! Port des litiges (FR-034, FR-035, Story 7.2).

use chrono::{DateTime, Utc};
use klaar_trust::Litige;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Issue d'une tentative d'ouverture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatOuverture {
    Ouvert(Litige),
    /// Cette intervention a déjà son litige (FR-034 `@edge`).
    DejaLitigee,
}

/// Ce qu'il faut savoir avant d'ouvrir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContexteLitige {
    /// Instant de fin de l'intervention, d'où court la fenêtre de quatorze
    /// jours. `None` tant qu'elle n'est pas terminée : on ne conteste pas un
    /// travail en cours.
    pub close_depuis: Option<DateTime<Utc>>,
    pub provider_id: Uuid,
    pub demandeur_id: Uuid,
}

#[allow(async_fn_in_trait)]
pub trait LitigeRepository {
    async fn ouvrir(&self, litige: &Litige) -> Result<ResultatOuverture, RepositoryError>;

    async fn par_mission(&self, mission_id: Uuid) -> Result<Option<Litige>, RepositoryError>;

    /// Contexte d'une Mission, ou `None` si elle n'existe pas.
    async fn contexte(&self, mission_id: Uuid) -> Result<Option<ContexteLitige>, RepositoryError>;

    /// Litiges tranchés **contre** ce prestataire depuis un instant donné.
    ///
    /// Ceux qu'il a perdus, et eux seuls : un prestataire attaqué trois fois et
    /// blanchi trois fois n'a rien fait de mal.
    async fn perdus_par_prestataire(
        &self,
        provider_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError>;

    /// Litiges ouverts par ce compte depuis un instant donné.
    async fn ouverts_par(
        &self,
        auteur_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError>;
}
