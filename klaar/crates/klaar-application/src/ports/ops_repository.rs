//! Port des comptes d'exploitation (FR-041, FR-042, Story 8.4).

use chrono::{DateTime, Utc};
use klaar_identity::CompteOps;
use klaar_shared_kernel::Email;
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Une ligne du journal d'exploitation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GesteOps {
    pub ops_id: Option<Uuid>,
    pub geste: String,
    pub cible: Option<String>,
    pub fait_le: DateTime<Utc>,
}

#[allow(async_fn_in_trait)]
pub trait OpsRepository {
    /// Crée un compte. Rend `false` si l'adresse est déjà prise.
    async fn creer(&self, compte: &CompteOps) -> Result<bool, RepositoryError>;

    async fn par_email(&self, email: &Email) -> Result<Option<CompteOps>, RepositoryError>;

    async fn par_id(&self, id: Uuid) -> Result<Option<CompteOps>, RepositoryError>;

    /// Écrit le secret TOTP, une seule fois.
    ///
    /// **Rend `false` si un secret existe déjà.** Le remplacer permettrait à
    /// quelqu'un qui a volé une session de reconfigurer la seconde
    /// authentification sur son propre téléphone, ce qui la rendrait inutile.
    /// Réinitialiser demande un super-administrateur.
    async fn configurer_totp(&self, ops_id: Uuid, secret: &[u8]) -> Result<bool, RepositoryError>;

    /// Consomme un pas TOTP et met à jour l'activité, ensemble.
    ///
    /// **Compare-and-swap sur le pas** : deux requêtes portant le même code ne
    /// doivent pas toutes deux aboutir. Rend `false` quand le pas avait déjà
    /// été consommé, ce qui est exactement le cas d'un rejeu.
    async fn consommer_pas_totp(
        &self,
        ops_id: Uuid,
        pas: i64,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// Désactive les comptes inactifs depuis trop longtemps (FR-041 `@edge`).
    ///
    /// Rend le nombre de comptes désactivés.
    async fn revoquer_les_inactifs(&self, avant: DateTime<Utc>) -> Result<u64, RepositoryError>;

    /// Consigne un geste d'exploitation (FR-042).
    async fn consigner(&self, geste: &GesteOps) -> Result<(), RepositoryError>;

    /// Lit le journal d'exploitation, du plus récent au plus ancien.
    async fn journal(
        &self,
        acteur: Option<Uuid>,
        limite: i64,
        decalage: i64,
    ) -> Result<Vec<GesteOps>, RepositoryError>;
}
