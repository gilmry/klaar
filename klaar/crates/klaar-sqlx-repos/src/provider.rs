//! Dépôt PostgreSQL des prestataires (Story 1.6 partielle, FR-003).

use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::provider_repository::{ProviderProche, ProviderRepository};
use klaar_catalog::CodeCatalogue;
use klaar_identity::{NumeroBce, OrigineKyc, Provider, StatutProvider};
use klaar_shared_kernel::Geo;

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgProviderRepository {
    pool: PoolPg,
}

impl PgProviderRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

const COLONNES: &str = "p.id, p.utilisateur_id, p.numero_bce, p.raison_sociale, p.statut, \
     p.origine_kyc, p.kyc_verifie_le, p.cree_le, ST_Y(p.base::geometry) AS lat, ST_X(p.base::geometry) AS lon, \
     COALESCE(ARRAY_AGG(c.secteur_code ORDER BY c.secteur_code) \
              FILTER (WHERE c.secteur_code IS NOT NULL), '{}') AS competences";

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<Provider, RepositoryError> {
    let bce: String = ligne.get("numero_bce");
    let statut: String = ligne.get("statut");
    let origine: Option<String> = ligne.get("origine_kyc");
    let codes: Vec<String> = ligne.get("competences");
    let (lat, lon): (f64, f64) = (ligne.get("lat"), ligne.get("lon"));

    let competences = codes
        .iter()
        .map(|c| {
            CodeCatalogue::parse(c)
                .map_err(|e| RepositoryError::Contrainte(format!("secteur illisible : {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Provider {
        id: ligne.get("id"),
        utilisateur_id: ligne.get("utilisateur_id"),
        numero_bce: NumeroBce::parse(&bce)
            .map_err(|e| RepositoryError::Contrainte(format!("numéro BCE illisible : {e}")))?,
        raison_sociale: ligne.get("raison_sociale"),
        base: Geo::new(lat, lon)
            .map_err(|e| RepositoryError::Contrainte(format!("base illisible : {e:?}")))?,
        statut: StatutProvider::parse(&statut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {statut}")))?,
        origine_kyc: origine
            .as_deref()
            .map(|o| {
                OrigineKyc::parse(o)
                    .ok_or_else(|| RepositoryError::Contrainte(format!("origine inconnue : {o}")))
            })
            .transpose()?,
        kyc_verifie_le: ligne.get("kyc_verifie_le"),
        competences,
        cree_le: ligne.get("cree_le"),
    })
}

impl ProviderRepository for PgProviderRepository {
    async fn creer(&self, provider: &Provider) -> Result<(), RepositoryError> {
        // Fiche et compétences dans la même transaction : un prestataire sans
        // compétence ne reçoit rien, et son compte semblerait fonctionner sans
        // jamais être sollicité.
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        sqlx::query(
            "INSERT INTO provider
                 (id, utilisateur_id, numero_bce, raison_sociale, base, statut, origine_kyc,
                  kyc_verifie_le, cree_le)
             VALUES ($1, $2, $3, $4, ST_SetSRID(ST_MakePoint($5, $6), 4326)::geography, $7, $8,
                     $9, $10)",
        )
        .bind(provider.id)
        .bind(provider.utilisateur_id)
        .bind(provider.numero_bce.as_str())
        .bind(&provider.raison_sociale)
        // Longitude d'abord : `ST_MakePoint` prend X puis Y.
        .bind(provider.base.lon())
        .bind(provider.base.lat())
        .bind(provider.statut.as_str())
        .bind(provider.origine_kyc.map(|o| o.as_str()))
        .bind(provider.kyc_verifie_le)
        .bind(provider.cree_le)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        for secteur in &provider.competences {
            sqlx::query(
                "INSERT INTO provider_competence (provider_id, secteur_code) VALUES ($1, $2)",
            )
            .bind(provider.id)
            .bind(secteur.as_str())
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;
        }

        tx.commit().await.map_err(erreur)?;
        Ok(())
    }

    async fn par_id(&self, id: Uuid) -> Result<Option<Provider>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM provider p
             LEFT JOIN provider_competence c ON c.provider_id = p.id
             WHERE p.id = $1
             GROUP BY p.id"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn par_numero_bce(
        &self,
        numero: &NumeroBce,
    ) -> Result<Option<Provider>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM provider p
             LEFT JOIN provider_competence c ON c.provider_id = p.id
             WHERE p.numero_bce = $1
             GROUP BY p.id"
        ))
        .bind(numero.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn mettre_a_jour_etat(&self, provider: &Provider) -> Result<(), RepositoryError> {
        // Ni la raison sociale ni la base ne sont réécrites : activer un
        // prestataire ne doit pas pouvoir écraser une fiche modifiée entre-temps.
        sqlx::query(
            "UPDATE provider SET statut = $1, origine_kyc = $2, kyc_verifie_le = $3 WHERE id = $4",
        )
        .bind(provider.statut.as_str())
        .bind(provider.origine_kyc.map(|o| o.as_str()))
        .bind(provider.kyc_verifie_le)
        .bind(provider.id)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn definir_disponibilite(
        &self,
        provider_id: Uuid,
        disponible: bool,
    ) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE provider SET disponible = $1 WHERE id = $2")
            .bind(disponible)
            .bind(provider_id)
            .execute(&self.pool)
            .await
            .map_err(erreur)?;
        Ok(())
    }

    async fn proches(
        &self,
        secteur: &CodeCatalogue,
        position: Geo,
        rayon_metres: f64,
        limite: i64,
    ) -> Result<Vec<ProviderProche>, RepositoryError> {
        // `ST_DWithin` sur une `geography` travaille en mètres et se sert de
        // l'index GIST ; `ST_Distance` rendrait la même mesure mais forcerait
        // un balayage complet s'il servait de filtre.
        //
        // Le filtre de compétence est un `EXISTS` et non une jointure : joindre
        // dupliquerait la ligne du prestataire par compétence, et la limite
        // porterait sur les couples plutôt que sur les prestataires.
        let lignes = sqlx::query(&format!(
            "SELECT {COLONNES},
                    ST_Distance(p.base, ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography) AS distance
             FROM provider p
             LEFT JOIN provider_competence c ON c.provider_id = p.id
             WHERE p.statut = 'ACTIVE'
               AND p.disponible
               AND ST_DWithin(p.base, ST_SetSRID(ST_MakePoint($1, $2), 4326)::geography, $3)
               AND EXISTS (
                   SELECT 1 FROM provider_competence pc
                   WHERE pc.provider_id = p.id AND pc.secteur_code = $4
               )
             GROUP BY p.id
             ORDER BY distance
             LIMIT $5"
        ))
        .bind(position.lon())
        .bind(position.lat())
        .bind(rayon_metres)
        .bind(secteur.as_str())
        .bind(limite)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        lignes
            .iter()
            .map(|l| {
                Ok(ProviderProche {
                    provider: depuis_ligne(l)?,
                    distance_metres: l.get("distance"),
                })
            })
            .collect()
    }
}
