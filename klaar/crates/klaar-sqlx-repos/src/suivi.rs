//! Dépôt PostgreSQL du suivi de position (Story 4.4, FR-019).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::suivi_repository::{SuiviRepository, TrajetAgrege};
use klaar_intervention::PositionSuivie;
use klaar_shared_kernel::Geo;

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgSuiviRepository {
    pool: PoolPg,
}

impl PgSuiviRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

impl SuiviRepository for PgSuiviRepository {
    async fn consentir(
        &self,
        mission_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        // Un consentement redonné après retrait remet la ligne en service :
        // c'est un accord neuf, daté du jour, et le retrait précédent n'a plus
        // à peser.
        sqlx::query(
            "INSERT INTO consentement_suivi (mission_id, consenti_le, retire_le)
             VALUES ($1, $2, NULL)
             ON CONFLICT (mission_id) DO UPDATE
                 SET consenti_le = EXCLUDED.consenti_le, retire_le = NULL",
        )
        .bind(mission_id)
        .bind(maintenant)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn retirer_consentement(
        &self,
        mission_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        // La ligne reste : une révocation qui la supprimerait effacerait la
        // preuve que le consentement avait été donné, et c'est précisément ce
        // qu'un contrôle vient vérifier.
        let ecrit = sqlx::query(
            "UPDATE consentement_suivi SET retire_le = $2
             WHERE mission_id = $1 AND retire_le IS NULL
             RETURNING mission_id",
        )
        .bind(mission_id)
        .bind(maintenant)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }

    async fn consenti(&self, mission_id: Uuid) -> Result<bool, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT 1 AS present FROM consentement_suivi
             WHERE mission_id = $1 AND retire_le IS NULL",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ligne.is_some())
    }

    async fn relever(&self, position: &PositionSuivie) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO position_suivi (mission_id, position, hors_zone, relevee_le)
             VALUES ($1, ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, $4, $5)",
        )
        .bind(position.mission_id)
        // `ST_MakePoint` prend la longitude d'abord : l'inverser place
        // Bruxelles au large de la Somalie sans qu'aucune contrainte ne s'en
        // aperçoive.
        .bind(position.position.lon())
        .bind(position.position.lat())
        .bind(position.hors_zone)
        .bind(position.relevee_le)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn derniere(&self, mission_id: Uuid) -> Result<Option<PositionSuivie>, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT ST_Y(position::geometry) AS lat, ST_X(position::geometry) AS lon,
                    hors_zone, relevee_le
             FROM position_suivi WHERE mission_id = $1
             ORDER BY relevee_le DESC, id DESC LIMIT 1",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        ligne
            .map(|l| {
                let position = Geo::new(l.get("lat"), l.get("lon")).map_err(|e| {
                    RepositoryError::Contrainte(format!("position illisible : {e:?}"))
                })?;
                Ok(PositionSuivie {
                    mission_id,
                    position,
                    hors_zone: l.get("hors_zone"),
                    relevee_le: l.get("relevee_le"),
                })
            })
            .transpose()
    }

    async fn purger_les_echues(
        &self,
        avant: DateTime<Utc>,
        limite: i64,
    ) -> Result<u64, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // Les interventions finies avant l'échéance, et qui portent encore des
        // positions.
        //
        // **`enregistre_le` et non `horodate_le`.** La seconde est déclarée par
        // le prestataire et peut précéder l'enregistrement ; l'adosser à un
        // délai de suppression laisserait une date antidatée effacer des
        // positions plus tôt que prévu, et une date avancée les garder plus
        // longtemps. Une échéance de purge se compte sur l'horloge du serveur.
        let missions: Vec<Uuid> = sqlx::query_scalar(
            "SELECT DISTINCT p.mission_id
             FROM position_suivi p
             JOIN mission_transition t ON t.mission_id = p.mission_id
              AND t.statut IN ('COMPLETED', 'CANCELLED')
             WHERE t.enregistre_le <= $1
             LIMIT $2",
        )
        .bind(avant)
        .bind(limite)
        .fetch_all(&mut *tx)
        .await
        .map_err(erreur)?;

        let mut purgees = 0u64;
        for mission_id in &missions {
            // **Agréger et supprimer en une seule instruction.** En deux, une
            // panne entre les deux laisserait soit la trace fine sans mesure,
            // soit la mesure sans trace mais comptée deux fois. Le `DELETE`
            // prend les verrous de ligne, donc une purge concurrente attend
            // puis ne trouve plus rien : `ON CONFLICT DO NOTHING` la fait
            // repartir sans rien écraser.
            //
            // `ST_Distance` entre points successifs donne la longueur du trajet
            // réellement parcouru, à la maille de cinquante mètres près.
            let ecrit = sqlx::query(
                "WITH supprimees AS (
                     DELETE FROM position_suivi WHERE mission_id = $1
                     RETURNING id, position, relevee_le
                 ), suite AS (
                     SELECT position, relevee_le,
                            LAG(position) OVER (ORDER BY relevee_le, id) AS precedente
                     FROM supprimees
                 )
                 INSERT INTO trajet_agrege
                     (mission_id, distance_metres, duree_secondes, releves, calcule_le)
                 SELECT $1,
                        COALESCE(SUM(ST_Distance(precedente, position)), 0),
                        COALESCE(
                            EXTRACT(EPOCH FROM (MAX(relevee_le) - MIN(relevee_le)))::bigint, 0),
                        COUNT(*)::int,
                        $2
                 FROM suite
                 ON CONFLICT (mission_id) DO NOTHING
                 RETURNING mission_id",
            )
            .bind(mission_id)
            .bind(avant)
            .fetch_optional(&mut *tx)
            .await
            .map_err(erreur)?;

            if ecrit.is_some() {
                purgees += 1;
            }
        }

        tx.commit().await.map_err(erreur)?;
        Ok(purgees)
    }

    async fn trajet(&self, mission_id: Uuid) -> Result<Option<TrajetAgrege>, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT distance_metres, duree_secondes, releves
             FROM trajet_agrege WHERE mission_id = $1",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(ligne.map(|l| TrajetAgrege {
            distance_metres: l.get("distance_metres"),
            duree_secondes: l.get("duree_secondes"),
            releves: l.get("releves"),
        }))
    }
}
