//! Administration du catalogue (Story 2.4, FR-010), en PostgreSQL.

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::catalogue_admin_repository::{
    CatalogueAdminRepository, SecteurAdmin,
};
use klaar_application::ports::erreurs::RepositoryError;
use klaar_catalog::{SecteurACreer, StatutSecteur};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgCatalogueAdminRepository {
    pool: PoolPg,
}

impl PgCatalogueAdminRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

/// Les colonnes, avec le comptage des interventions en cours.
///
/// **Compté dans la même requête que le reste.** En deux, le nombre affiché
/// pourrait déjà être faux au moment où l'exploitation clique : une Mission
/// démarre entre les deux lectures, et le retrait passerait alors que l'écran
/// annonçait zéro.
const COLONNES: &str = "s.code, s.libelle_fr, s.libelle_nl, s.libelle_en, s.ordre, s.statut,
     s.cree_par, s.publie_par,
     (SELECT count(*) FROM mission m
        JOIN demande d ON d.id = m.demande_id
       WHERE d.secteur_code = s.code
         AND m.statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE')) AS missions_en_cours";

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<SecteurAdmin, RepositoryError> {
    let brut: String = ligne.get("statut");
    Ok(SecteurAdmin {
        code: ligne.get("code"),
        libelle_fr: ligne.get("libelle_fr"),
        libelle_nl: ligne.get("libelle_nl"),
        libelle_en: ligne.get("libelle_en"),
        ordre: ligne.get("ordre"),
        statut: StatutSecteur::parse(&brut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {brut}")))?,
        cree_par: ligne.get("cree_par"),
        publie_par: ligne.get("publie_par"),
        missions_en_cours: ligne.get("missions_en_cours"),
    })
}

impl CatalogueAdminRepository for PgCatalogueAdminRepository {
    async fn tous(&self) -> Result<Vec<SecteurAdmin>, RepositoryError> {
        let lignes = sqlx::query(&format!(
            "SELECT {COLONNES} FROM secteur s ORDER BY s.ordre, s.code"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;
        lignes.iter().map(depuis_ligne).collect()
    }

    async fn par_code(&self, code: &str) -> Result<Option<SecteurAdmin>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM secteur s WHERE s.code = $1"
        ))
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn creer(
        &self,
        secteur: &SecteurACreer,
        ops_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        // `ON CONFLICT DO NOTHING` : c'est la clé primaire qui tranche
        // l'unicité, et non une lecture préalable qui laisserait deux créations
        // simultanées passer.
        let ecrit = sqlx::query(
            "INSERT INTO secteur
                 (code, libelle_fr, libelle_nl, libelle_en, ordre, statut, cree_par, cree_le)
             VALUES ($1, $2, $3, $4, $5, 'DRAFT', $6, $7)
             ON CONFLICT (code) DO NOTHING
             RETURNING code",
        )
        .bind(secteur.code.as_str())
        .bind(&secteur.libelles.fr)
        .bind(&secteur.libelles.nl)
        .bind(&secteur.libelles.en)
        .bind(secteur.ordre)
        .bind(ops_id)
        .bind(maintenant)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }

    async fn publier(
        &self,
        code: &str,
        ops_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        // **Compare-and-swap sur le statut, et la garde des quatre yeux dans le
        // `WHERE`.** Deux publications concurrentes ne peuvent pas toutes deux
        // aboutir, et un créateur ne peut pas publier son propre brouillon même
        // si le contrôle applicatif était contourné.
        let ecrit = sqlx::query(
            "UPDATE secteur
                SET statut = 'PUBLISHED', publie_par = $2, publie_le = $3
              WHERE code = $1 AND statut = 'DRAFT'
                AND (cree_par IS NULL OR cree_par <> $2)
          RETURNING code",
        )
        .bind(code)
        .bind(ops_id)
        .bind(maintenant)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }

    async fn desactiver(&self, code: &str) -> Result<bool, RepositoryError> {
        // Le comptage des interventions en cours est **dans la requête**, pas
        // au-dessus : entre une lecture et une écriture séparées, une Mission
        // peut démarrer et le retrait passerait quand même.
        let ecrit = sqlx::query(
            "UPDATE secteur SET statut = 'DISABLED'
              WHERE code = $1 AND statut = 'PUBLISHED'
                AND NOT EXISTS (
                    SELECT 1 FROM mission m
                      JOIN demande d ON d.id = m.demande_id
                     WHERE d.secteur_code = $1
                       AND m.statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE')
                )
          RETURNING code",
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }
}
