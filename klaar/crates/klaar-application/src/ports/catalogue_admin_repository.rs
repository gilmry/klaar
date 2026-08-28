//! Port de l'administration du catalogue (FR-010, Story 2.4).

use chrono::{DateTime, Utc};
use klaar_catalog::{SecteurACreer, StatutSecteur};
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Un secteur tel que la console d'exploitation le voit.
///
/// **Il porte son statut et sa provenance**, contrairement à la vue publique
/// qui n'expose que ce qui est publié : c'est précisément ce que
/// l'exploitation doit voir pour décider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecteurAdmin {
    pub code: String,
    pub libelle_fr: String,
    pub libelle_nl: String,
    pub libelle_en: String,
    pub ordre: i32,
    pub statut: StatutSecteur,
    /// `None` pour les secteurs du peuplement initial : ils ne viennent
    /// d'aucune décision d'exploitation, et leur inventer un auteur écrirait
    /// une décision qui n'a pas eu lieu.
    pub cree_par: Option<Uuid>,
    pub publie_par: Option<Uuid>,
    /// Interventions en cours dans ce secteur — ce qui empêche de le retirer.
    pub missions_en_cours: i64,
}

#[allow(async_fn_in_trait)]
pub trait CatalogueAdminRepository {
    /// Tous les secteurs, quel que soit leur statut.
    async fn tous(&self) -> Result<Vec<SecteurAdmin>, RepositoryError>;

    /// Un secteur précis.
    async fn par_code(&self, code: &str) -> Result<Option<SecteurAdmin>, RepositoryError>;

    /// Crée un secteur en brouillon.
    ///
    /// Rend `false` si le code est déjà pris : c'est la clé primaire qui
    /// tranche, et non une lecture préalable qui laisserait deux créations
    /// simultanées passer.
    async fn creer(
        &self,
        secteur: &SecteurACreer,
        ops_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// Publie un brouillon.
    ///
    /// **Compare-and-swap sur le statut** : deux publications concurrentes ne
    /// doivent pas toutes deux aboutir. Rend `false` si le secteur n'était plus
    /// en brouillon.
    async fn publier(
        &self,
        code: &str,
        ops_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError>;

    /// Retire un secteur publié.
    ///
    /// Rend `false` s'il n'était plus publié.
    async fn desactiver(&self, code: &str) -> Result<bool, RepositoryError>;
}
