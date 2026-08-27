//! Dépôt PostgreSQL du catalogue (Story 2.1, FR-008).

use sqlx::Row;

use klaar_application::ports::catalogue_repository::CatalogueRepository;
use klaar_application::ports::erreurs::RepositoryError;
use klaar_catalog::{CodeCatalogue, Libelles, Secteur, Skill};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgCatalogueRepository {
    pool: PoolPg,
}

impl PgCatalogueRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn libelles(ligne: &sqlx::postgres::PgRow, prefixe: &str) -> Libelles {
    Libelles::new(
        ligne.get::<String, _>(format!("{prefixe}_fr").as_str()),
        ligne.get::<String, _>(format!("{prefixe}_nl").as_str()),
        ligne.get::<String, _>(format!("{prefixe}_en").as_str()),
    )
}

fn code(valeur: &str) -> Result<CodeCatalogue, RepositoryError> {
    CodeCatalogue::parse(valeur)
        .map_err(|e| RepositoryError::Contrainte(format!("code de catalogue illisible : {e}")))
}

impl CatalogueRepository for PgCatalogueRepository {
    async fn secteurs(&self) -> Result<Vec<Secteur>, RepositoryError> {
        // Une seule requête, jointure à gauche : un secteur sans Skill doit
        // apparaître quand même. Deux requêtes séparées coûteraient un
        // aller-retour de plus et pourraient tomber sur deux états différents
        // du catalogue si une mise à jour passe entre les deux.
        let lignes = sqlx::query(
            "SELECT s.code AS secteur_code,
                    s.libelle_fr AS secteur_fr, s.libelle_nl AS secteur_nl,
                    s.libelle_en AS secteur_en, s.ordre AS secteur_ordre,
                    k.code AS skill_code,
                    k.libelle_fr AS skill_fr, k.libelle_nl AS skill_nl,
                    k.libelle_en AS skill_en
             FROM secteur s
             LEFT JOIN skill k ON k.secteur_code = s.code
             ORDER BY s.ordre, k.ordre",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        let mut secteurs: Vec<Secteur> = Vec::new();
        for ligne in &lignes {
            let code_secteur: String = ligne.get("secteur_code");
            if secteurs
                .last()
                .is_none_or(|s| s.code.as_str() != code_secteur)
            {
                secteurs.push(Secteur {
                    code: code(&code_secteur)?,
                    libelles: libelles(ligne, "secteur"),
                    skills: Vec::new(),
                });
            }
            // `NULL` quand la jointure n'a rien trouvé : le secteur existe,
            // simplement sans Skill décrit.
            if let Some(code_skill) = ligne.get::<Option<String>, _>("skill_code") {
                if let Some(secteur) = secteurs.last_mut() {
                    secteur.skills.push(Skill {
                        code: code(&code_skill)?,
                        libelles: libelles(ligne, "skill"),
                    });
                }
            }
        }
        Ok(secteurs)
    }
}
