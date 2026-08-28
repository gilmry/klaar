//! Dépôt PostgreSQL du catalogue (Story 2.1, FR-008).

use sqlx::Row;

use klaar_application::ports::catalogue_repository::CatalogueRepository;
use klaar_application::ports::erreurs::RepositoryError;
use klaar_catalog::{CodeCatalogue, FourchettePrix, Libelles, Secteur, Skill};
use klaar_shared_kernel::Money;

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
                    f.min_cents AS fourchette_min, f.max_cents AS fourchette_max,
                    k.code AS skill_code,
                    k.libelle_fr AS skill_fr, k.libelle_nl AS skill_nl,
                    k.libelle_en AS skill_en
             FROM secteur s
             LEFT JOIN skill k ON k.secteur_code = s.code
             LEFT JOIN fourchette_prix f ON f.secteur_code = s.code
             -- **Les publiés seulement** (Story 2.4). Un brouillon proposé au
             -- public laisserait soumettre des Demandes dans un secteur où
             -- aucun prestataire ne s'est déclaré ; un secteur retiré
             -- continuerait d'être offert alors qu'on vient de le retirer. Le
             -- filtre est ici plutôt que dans l'appelant : c'est la lecture
             -- publique, et il n'y a pas de cas où elle voudrait autre chose.
             WHERE s.statut = 'PUBLISHED'
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
                // `NULL` tant qu'aucune fourchette n'a été calculée pour ce
                // secteur : c'est l'état attendu au lancement, et il se traduit
                // par « prix sur devis » plutôt que par une absence de réponse.
                let fourchette = match (
                    ligne.get::<Option<i64>, _>("fourchette_min"),
                    ligne.get::<Option<i64>, _>("fourchette_max"),
                ) {
                    (Some(min), Some(max)) => Some(FourchettePrix {
                        min: Money::from_cents(min),
                        max: Money::from_cents(max),
                    }),
                    _ => None,
                };
                secteurs.push(Secteur {
                    code: code(&code_secteur)?,
                    libelles: libelles(ligne, "secteur"),
                    skills: Vec::new(),
                    fourchette,
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
