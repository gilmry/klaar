//! Dépôt PostgreSQL des libérations (Story 4.6, FR-021).
//!
//! **Une seule transaction pour trois écritures.** La bascule de la Mission en
//! `VALIDATED`, l'entrée d'historique et la ligne de libération : FR-021
//! `@security` demande que l'ensemble soit atomique, et il a raison — une
//! Mission validée sans libération laisserait un prestataire attendre un
//! versement dont plus rien ne porte la trace.
//!
//! La garde sur le statut de départ ferme la course entre le demandeur qui
//! valide et le balayage qui valide à sa place dans la même seconde.

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::evenements::EvenementMission;
use klaar_application::ports::liberation_repository::{
    LiberationRepository, ResultatLiberation, ValidationEnAttente,
};
use klaar_payment::{Liberation, OrigineValidation, Repartition, StatutLiberation};
use klaar_shared_kernel::Money;

use crate::pool::PoolPg;
use crate::{erreur, notifier};

const COLONNES: &str = "id, mission_id, devis_id, provider_id, total_ttc_cents, \
                        commission_htva_cents, tva_commission_cents, commission_ttc_cents, \
                        reversement_cents, origine, statut, decidee_le";

pub struct PgLiberationRepository {
    pool: PoolPg,
}

impl PgLiberationRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<Liberation, RepositoryError> {
    let origine: String = ligne.get("origine");
    let statut: String = ligne.get("statut");
    Ok(Liberation {
        id: ligne.get("id"),
        mission_id: ligne.get("mission_id"),
        devis_id: ligne.get("devis_id"),
        provider_id: ligne.get("provider_id"),
        repartition: Repartition {
            total_ttc: Money::from_cents(ligne.get("total_ttc_cents")),
            commission_htva: Money::from_cents(ligne.get("commission_htva_cents")),
            tva_commission: Money::from_cents(ligne.get("tva_commission_cents")),
            commission_ttc: Money::from_cents(ligne.get("commission_ttc_cents")),
            reversement: Money::from_cents(ligne.get("reversement_cents")),
        },
        origine: OrigineValidation::parse(&origine)
            .ok_or_else(|| RepositoryError::Contrainte(format!("origine inconnue : {origine}")))?,
        statut: StatutLiberation::parse(&statut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {statut}")))?,
        decidee_le: ligne.get("decidee_le"),
    })
}

impl LiberationRepository for PgLiberationRepository {
    async fn prononcer(
        &self,
        liberation: &Liberation,
        decidee_le: DateTime<Utc>,
    ) -> Result<ResultatLiberation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // Le compare-and-swap. `RETURNING` sert de témoin : aucune ligne rendue
        // signifie que la Mission n'était plus terminée au moment où cette
        // transaction a pu écrire, quel que soit ce qu'une lecture antérieure
        // avait vu.
        let bascule = sqlx::query(
            "UPDATE mission SET statut = 'VALIDATED'
             WHERE id = $1 AND statut = 'COMPLETED'
             RETURNING provider_id",
        )
        .bind(liberation.mission_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        let Some(ligne) = bascule else {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatLiberation::MissionNonTerminee);
        };
        let provider_id: Uuid = ligne.get("provider_id");

        // L'historique, comme pour toute transition (FR-018 `@security`). La
        // validation n'a ni position ni horodatage client : elle est le fait du
        // serveur, déclenchée par le demandeur ou par le délai.
        sqlx::query(
            "INSERT INTO mission_transition
                 (mission_id, provider_id, statut, horodate_le, enregistre_le, position, hors_zone)
             VALUES ($1, $2, 'VALIDATED', $3, $3, NULL, FALSE)",
        )
        .bind(liberation.mission_id)
        .bind(provider_id)
        .bind(decidee_le)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        sqlx::query(
            "INSERT INTO liberation
                 (id, mission_id, devis_id, provider_id, total_ttc_cents, commission_htva_cents,
                  tva_commission_cents, commission_ttc_cents, reversement_cents, origine, statut,
                  decidee_le)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(liberation.id)
        .bind(liberation.mission_id)
        .bind(liberation.devis_id)
        .bind(liberation.provider_id)
        .bind(liberation.repartition.total_ttc.cents())
        .bind(liberation.repartition.commission_htva.cents())
        .bind(liberation.repartition.tva_commission.cents())
        .bind(liberation.repartition.commission_ttc.cents())
        .bind(liberation.repartition.reversement.cents())
        .bind(liberation.origine.as_str())
        .bind(liberation.statut.as_str())
        .bind(liberation.decidee_le)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        notifier(
            &mut tx,
            &EvenementMission::statut(liberation.mission_id, "VALIDATED", decidee_le),
        )
        .await?;

        tx.commit().await.map_err(erreur)?;
        Ok(ResultatLiberation::Prononcee(liberation.clone()))
    }

    async fn par_mission(&self, mission_id: Uuid) -> Result<Option<Liberation>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM liberation WHERE mission_id = $1"
        ))
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn a_valider_automatiquement(
        &self,
        avant: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<ValidationEnAttente>, RepositoryError> {
        // L'instant de fin vient de l'historique et non de la table `mission` :
        // c'est la seule source qui dise **quand** l'intervention s'est
        // terminée, et une transition déclarée hors connexion garde sa date.
        //
        // `DISTINCT ON` plutôt qu'un `GROUP BY` : une Mission n'a qu'une entrée
        // `COMPLETED` en pratique, mais s'appuyer là-dessus ferait échouer la
        // requête le jour où ce ne serait plus vrai, au lieu de prendre la
        // première.
        let lignes = sqlx::query(
            "SELECT DISTINCT ON (m.id)
                    m.id AS mission_id, m.provider_id, d.demandeur_id, t.horodate_le
             FROM mission m
             JOIN demande d ON d.id = m.demande_id
             JOIN mission_transition t ON t.mission_id = m.id AND t.statut = 'COMPLETED'
             WHERE m.statut = 'COMPLETED' AND t.horodate_le <= $1
             ORDER BY m.id, t.horodate_le
             LIMIT $2",
        )
        .bind(avant)
        .bind(limite)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(lignes
            .iter()
            .map(|l| ValidationEnAttente {
                mission_id: l.get("mission_id"),
                demandeur_id: l.get("demandeur_id"),
                provider_id: l.get("provider_id"),
                terminee_le: l.get("horodate_le"),
            })
            .collect())
    }
}
