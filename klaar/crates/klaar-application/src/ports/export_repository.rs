//! Port des exports réglementaires (FR-039, Story 8.2).
//!
//! **Un export RGPD ne vaut que s'il est exhaustif.** L'article 15 donne droit
//! à *toutes* les données à caractère personnel, pas à celles qu'on a pensé à
//! inclure. Le dépôt rend donc aussi la liste des tables qui référencent un
//! compte, pour qu'un test puisse comparer ce que la base contient à ce que
//! l'export couvre — et échouer le jour où quelqu'un ajoute une table sans y
//! penser.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Une table qui porte des données rattachées à un compte.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TablePersonnelle {
    pub table: String,
    pub colonne: String,
}

#[allow(async_fn_in_trait)]
pub trait ExportRepository {
    /// Toutes les données d'un compte, en JSON, table par table.
    ///
    /// Rend `None` si le compte n'existe pas : un export vide et un export
    /// d'inexistant ne veulent pas dire la même chose, et une autorité qui
    /// reçoit le premier alors que c'était le second en tirera la mauvaise
    /// conclusion.
    async fn donnees_personnelles(
        &self,
        utilisateur_id: Uuid,
    ) -> Result<Option<serde_json::Value>, RepositoryError>;

    /// Les tables qui référencent un compte, telles que la base les déclare.
    ///
    /// Lu depuis `information_schema` et non depuis une liste écrite à la main :
    /// une liste se désynchronise, un schéma non.
    async fn tables_personnelles(&self) -> Result<Vec<TablePersonnelle>, RepositoryError>;

    /// Lignes de TVA d'une période, pour l'export comptable.
    async fn lignes_tva(
        &self,
        debut: DateTime<Utc>,
        fin: DateTime<Utc>,
    ) -> Result<Vec<LigneTva>, RepositoryError>;
}

/// Une ligne de l'export TVA : un devis accepté et libéré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LigneTva {
    pub devis_id: Uuid,
    pub decidee_le: DateTime<Utc>,
    /// Taux du devis, en points de base.
    pub taux_tva_bp: u16,
    pub montant_htva_cents: i64,
    pub tva_cents: i64,
    pub total_ttc_cents: i64,
    /// Commission de la plateforme, hors TVA.
    pub commission_htva_cents: i64,
    pub tva_commission_cents: i64,
}
