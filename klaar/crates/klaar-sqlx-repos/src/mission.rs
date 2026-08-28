//! Dépôt PostgreSQL des Missions et attribution atomique (Story 3.4, FR-013).
//!
//! **Tout l'enjeu de ce fichier tient dans une clause `WHERE`.** Cinq
//! prestataires notifiés peuvent toucher « accepter » dans la même seconde.
//! Lire le statut puis l'écrire laisserait deux d'entre eux passer ; c'est
//! l'`UPDATE … WHERE statut = 'BROADCASTING' RETURNING id` qui tranche, parce
//! que PostgreSQL sérialise les écritures sur une même ligne et que le second
//! arrivant ré-évalue la condition après le premier — et ne voit alors plus
//! rien.
//!
//! La bascule de la Demande et la création de la Mission sont dans **une seule
//! transaction**. Une Demande `MATCHED` sans Mission laisserait le demandeur
//! devant un statut qui promet une intervention dont personne ne porte la
//! trace.

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::mission_repository::{MissionRepository, ResultatAttribution};
use klaar_intervention::{Mission, StatutMission};

use crate::erreur;
use crate::pool::PoolPg;

/// Nom de l'index qui tient « une Mission à la fois » (migration V13).
///
/// Comparé plutôt que deviné : une autre violation d'unicité sur cette table
/// signalerait un tout autre problème, et la traduire en « prestataire occupé »
/// enverrait chercher la panne au mauvais endroit.
const INDEX_UNE_MISSION_A_LA_FOIS: &str = "mission_provider_en_cours_idx";

pub struct PgMissionRepository {
    pool: PoolPg,
}

impl PgMissionRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<Mission, RepositoryError> {
    let statut: String = ligne.get("statut");
    Ok(Mission {
        id: ligne.get("id"),
        demande_id: ligne.get("demande_id"),
        provider_id: ligne.get("provider_id"),
        statut: StatutMission::parse(&statut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {statut}")))?,
        cree_le: ligne.get("cree_le"),
    })
}

impl MissionRepository for PgMissionRepository {
    async fn attribuer(
        &self,
        demande_id: Uuid,
        provider_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<ResultatAttribution, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // Le compare-and-swap. `RETURNING` sert de témoin : aucune ligne rendue
        // signifie que la Demande n'était plus en diffusion au moment où cette
        // transaction a pu écrire, quel que soit ce qu'une lecture antérieure
        // avait vu.
        let gagne = sqlx::query(
            "UPDATE demande SET statut = 'MATCHED'
             WHERE id = $1 AND statut = 'BROADCASTING'
             RETURNING id",
        )
        .bind(demande_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        if gagne.is_none() {
            // Rien à défaire, mais la transaction se ferme explicitement :
            // la laisser mourir avec la variable marcherait aussi et se
            // relirait moins bien.
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatAttribution::DemandeNonDiffusee);
        }

        let mission = Mission::attribuer(demande_id, provider_id, maintenant);
        let insertion = sqlx::query(
            "INSERT INTO mission (id, demande_id, provider_id, statut, cree_le)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(mission.id)
        .bind(mission.demande_id)
        .bind(mission.provider_id)
        .bind(mission.statut.as_str())
        .bind(mission.cree_le)
        .execute(&mut *tx)
        .await;

        match insertion {
            Ok(_) => {
                tx.commit().await.map_err(erreur)?;
                Ok(ResultatAttribution::Attribuee(mission))
            }
            Err(e) => {
                let occupe = match &e {
                    sqlx::Error::Database(db) => {
                        db.is_unique_violation()
                            && db.constraint() == Some(INDEX_UNE_MISSION_A_LA_FOIS)
                    }
                    _ => false,
                };
                // La transaction entière est défaite : la Demande repasse en
                // diffusion, et un autre prestataire pourra la prendre. Sans
                // cela, un prestataire déjà occupé éteindrait une Demande en
                // essayant de l'accepter.
                tx.rollback().await.map_err(erreur)?;
                if occupe {
                    Ok(ResultatAttribution::ProviderOccupe)
                } else {
                    Err(erreur(e))
                }
            }
        }
    }

    async fn en_cours_pour(&self, provider_id: Uuid) -> Result<Option<Mission>, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT id, demande_id, provider_id, statut, cree_le FROM mission
             WHERE provider_id = $1 AND statut IN ('ASSIGNED')
             ORDER BY cree_le DESC
             LIMIT 1",
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }
}
