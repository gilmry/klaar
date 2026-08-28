//! Dépôt PostgreSQL des annulations de Mission (Story 4.7, FR-022).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::annulation_repository::{AnnulationRepository, ResultatAnnulation};
use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::evenements::EvenementMission;
use klaar_intervention::{
    AnnulationMission, AuteurAnnulation, ConsequenceAnnulation, MotifAnnulationMission,
    StatutMission,
};
use klaar_shared_kernel::Money;

use crate::pool::PoolPg;
use crate::{erreur, notifier};

const COLONNES: &str = "id, mission_id, auteur, depuis, motif, forfait_deplacement_cents, \
                        remboursement_cents, penalise_le_prestataire, decidee_le";

pub struct PgAnnulationRepository {
    pool: PoolPg,
}

impl PgAnnulationRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<AnnulationMission, RepositoryError> {
    let auteur: String = ligne.get("auteur");
    let depuis: String = ligne.get("depuis");
    Ok(AnnulationMission {
        id: ligne.get("id"),
        mission_id: ligne.get("mission_id"),
        auteur: AuteurAnnulation::parse(&auteur)
            .ok_or_else(|| RepositoryError::Contrainte(format!("auteur inconnu : {auteur}")))?,
        depuis: StatutMission::parse(&depuis)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {depuis}")))?,
        motif: ligne
            .get::<Option<String>, _>("motif")
            .as_deref()
            .and_then(MotifAnnulationMission::parse),
        consequence: ConsequenceAnnulation {
            forfait_deplacement: Money::from_cents(ligne.get("forfait_deplacement_cents")),
            remboursement: Money::from_cents(ligne.get("remboursement_cents")),
            penalise_le_prestataire: ligne.get("penalise_le_prestataire"),
        },
        decidee_le: ligne.get("decidee_le"),
    })
}

impl AnnulationRepository for PgAnnulationRepository {
    async fn prononcer(
        &self,
        annulation: &AnnulationMission,
    ) -> Result<ResultatAnnulation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // Le compare-and-swap porte sur le statut d'où l'annulation a été
        // décidée : si la Mission a avancé entre-temps, le forfait calculé ne
        // correspond plus à la réalité, et l'écrire quand même ferait payer un
        // déplacement qui n'avait pas eu lieu — ou l'inverse.
        let bascule = sqlx::query(
            "UPDATE mission SET statut = 'CANCELLED'
             WHERE id = $1 AND statut = $2
             RETURNING provider_id",
        )
        .bind(annulation.mission_id)
        .bind(annulation.depuis.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        let Some(ligne) = bascule else {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatAnnulation::MissionDejaClose);
        };
        let provider_id: Uuid = ligne.get("provider_id");

        sqlx::query(
            "INSERT INTO mission_transition
                 (mission_id, provider_id, statut, horodate_le, enregistre_le, position, hors_zone)
             VALUES ($1, $2, 'CANCELLED', $3, $3, NULL, FALSE)",
        )
        .bind(annulation.mission_id)
        .bind(provider_id)
        .bind(annulation.decidee_le)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        sqlx::query(
            "INSERT INTO annulation_mission
                 (id, mission_id, auteur, depuis, motif, forfait_deplacement_cents,
                  remboursement_cents, penalise_le_prestataire, decidee_le)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(annulation.id)
        .bind(annulation.mission_id)
        .bind(annulation.auteur.as_str())
        .bind(annulation.depuis.as_str())
        .bind(annulation.motif.map(|m| m.as_str()))
        .bind(annulation.consequence.forfait_deplacement.cents())
        .bind(annulation.consequence.remboursement.cents())
        .bind(annulation.consequence.penalise_le_prestataire)
        .bind(annulation.decidee_le)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        notifier(
            &mut tx,
            &EvenementMission::statut(annulation.mission_id, "CANCELLED", annulation.decidee_le),
        )
        .await?;

        tx.commit().await.map_err(erreur)?;
        Ok(ResultatAnnulation::Prononcee(annulation.clone()))
    }

    async fn desistements_depuis(
        &self,
        provider_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError> {
        // Le prestataire se lit sur la Mission et non sur l'annulation : c'est
        // la Mission qui dit à qui elle était attribuée, et recopier
        // l'identifiant ici en ferait une seconde source à tenir d'accord.
        let ligne = sqlx::query(
            "SELECT count(*) AS total
             FROM annulation_mission a
             JOIN mission m ON m.id = a.mission_id
             WHERE m.provider_id = $1 AND a.penalise_le_prestataire AND a.decidee_le >= $2",
        )
        .bind(provider_id)
        .bind(depuis)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ligne.get("total"))
    }

    async fn annulations_du_demandeur_depuis(
        &self,
        demandeur_id: Uuid,
        depuis: DateTime<Utc>,
    ) -> Result<i64, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT count(*) AS total
             FROM annulation_mission a
             JOIN mission m ON m.id = a.mission_id
             JOIN demande d ON d.id = m.demande_id
             WHERE d.demandeur_id = $1 AND a.auteur = 'CANCELLED_USER' AND a.decidee_le >= $2",
        )
        .bind(demandeur_id)
        .bind(depuis)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ligne.get("total"))
    }

    async fn par_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<AnnulationMission>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM annulation_mission WHERE mission_id = $1"
        ))
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }
}
