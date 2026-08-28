//! Dépôt PostgreSQL des litiges (Story 7.2, FR-034, FR-035).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::litige_repository::{
    ContexteLitige, LitigeRepository, ResultatOuverture,
};
use klaar_trust::{Litige, MotifLitige, PartieLitige, StatutLitige};

use crate::erreur;
use crate::pool::PoolPg;

/// Nom de la contrainte « un litige par Mission » (migration V26).
const CONTRAINTE_UN_PAR_MISSION: &str = "litige_mission_id_key";

const COLONNES: &str = "id, mission_id, auteur_id, partie, motif, description, statut, ouvert_le";

pub struct PgLitigeRepository {
    pool: PoolPg,
}

impl PgLitigeRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<Litige, RepositoryError> {
    let partie: String = ligne.get("partie");
    let motif: String = ligne.get("motif");
    let statut: String = ligne.get("statut");
    Ok(Litige {
        id: ligne.get("id"),
        mission_id: ligne.get("mission_id"),
        auteur_id: ligne
            .get::<Option<Uuid>, _>("auteur_id")
            .unwrap_or(Uuid::nil()),
        partie: PartieLitige::parse(&partie)
            .ok_or_else(|| RepositoryError::Contrainte(format!("partie inconnue : {partie}")))?,
        motif: MotifLitige::parse(&motif)
            .ok_or_else(|| RepositoryError::Contrainte(format!("motif inconnu : {motif}")))?,
        description: ligne.get("description"),
        statut: StatutLitige::parse(&statut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {statut}")))?,
        ouvert_le: ligne.get("ouvert_le"),
    })
}

impl LitigeRepository for PgLitigeRepository {
    async fn ouvrir(&self, litige: &Litige) -> Result<ResultatOuverture, RepositoryError> {
        let ecrit = sqlx::query(
            "INSERT INTO litige
                 (id, mission_id, auteur_id, partie, motif, description, statut, ouvert_le)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(litige.id)
        .bind(litige.mission_id)
        .bind(litige.auteur_id)
        .bind(litige.partie.as_str())
        .bind(litige.motif.as_str())
        .bind(&litige.description)
        .bind(litige.statut.as_str())
        .bind(litige.ouvert_le)
        .execute(&self.pool)
        .await;

        match ecrit {
            Ok(_) => Ok(ResultatOuverture::Ouvert(litige.clone())),
            Err(e) => {
                let deja = match &e {
                    sqlx::Error::Database(db) => {
                        db.is_unique_violation()
                            && db.constraint() == Some(CONTRAINTE_UN_PAR_MISSION)
                    }
                    _ => false,
                };
                if deja {
                    Ok(ResultatOuverture::DejaLitigee)
                } else {
                    Err(erreur(e))
                }
            }
        }
    }

    async fn par_mission(&self, mission_id: Uuid) -> Result<Option<Litige>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM litige WHERE mission_id = $1"
        ))
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn contexte(&self, mission_id: Uuid) -> Result<Option<ContexteLitige>, RepositoryError> {
        // La fin vient de l'historique : c'est de la **fin des travaux** que
        // court la fenêtre, et non de la validation qui peut arriver trois
        // jours plus tard.
        let ligne = sqlx::query(
            "SELECT m.provider_id, d.demandeur_id,
                    (SELECT min(horodate_le) FROM mission_transition
                     WHERE mission_id = m.id AND statut IN ('COMPLETED', 'CANCELLED'))
                        AS close_depuis
             FROM mission m
             JOIN demande d ON d.id = m.demande_id
             WHERE m.id = $1",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(ligne.map(|l| ContexteLitige {
            close_depuis: l.get("close_depuis"),
            provider_id: l.get("provider_id"),
            demandeur_id: l.get("demandeur_id"),
        }))
    }

    async fn perdus_par_prestataire(
        &self,
        provider_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT count(*) AS total
             FROM litige l
             JOIN mission m ON m.id = l.mission_id
             WHERE m.provider_id = $1
               AND l.statut = 'RESOLVED_USER_FAVOR'
               AND l.tranche_le >= $2",
        )
        .bind(provider_id)
        .bind(depuis)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ligne.get("total"))
    }

    async fn ouverts_par(
        &self,
        auteur_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT count(*) AS total FROM litige WHERE auteur_id = $1 AND ouvert_le >= $2",
        )
        .bind(auteur_id)
        .bind(depuis)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ligne.get("total"))
    }
}
