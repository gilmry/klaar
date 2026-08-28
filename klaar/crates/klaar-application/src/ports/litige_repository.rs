//! Port des litiges (FR-034, FR-035, Story 7.2).

use chrono::{DateTime, Utc};
use klaar_trust::{Issue, Litige, MotifLitige, PartieLitige};
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

/// Un litige tel que la console de médiation le voit (FR-036).
///
/// **Il porte le montant en jeu et l'ancienneté.** Sans le premier, trancher
/// « partiellement » ne veut rien dire ; sans la seconde, l'exploitation ne sait
/// pas lequel des dossiers ouverts est en train de dépasser les trente jours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DossierLitige {
    pub id: Uuid,
    pub mission_id: Uuid,
    pub partie: PartieLitige,
    pub motif: MotifLitige,
    pub description: String,
    pub ouvert_le: DateTime<Utc>,
    /// Montant du devis convenu, en centimes. Zéro si l'intervention n'en avait
    /// pas — un litige peut naître d'un travail non fait.
    pub total_ttc_cents: i64,
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

    /// Les litiges non tranchés, du plus ancien au plus récent (FR-036).
    ///
    /// **Du plus ancien d'abord.** C'est celui qui approche des trente jours
    /// qui doit sauter aux yeux, pas le dernier arrivé.
    async fn ouverts(&self, limite: i64) -> Result<Vec<DossierLitige>, RepositoryError>;

    /// Le dossier d'un litige, quel que soit son statut.
    async fn dossier(&self, litige_id: Uuid) -> Result<Option<DossierLitige>, RepositoryError>;

    /// Écrit la décision, **si et seulement si** le litige est encore ouvert.
    ///
    /// **Compare-and-swap sur le statut.** Deux médiateurs qui tranchent le même
    /// dossier en même temps ne doivent pas produire deux décisions ; le second
    /// obtient `None` et voit que l'affaire est réglée. Lire puis écrire
    /// laisserait passer les deux.
    async fn trancher(
        &self,
        litige_id: Uuid,
        issue: Issue,
        ops_id: Uuid,
        tranche_le: DateTime<Utc>,
    ) -> Result<Option<Litige>, RepositoryError>;
}
