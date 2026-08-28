//! Dépôt PostgreSQL des litiges (Story 7.2, FR-034, FR-035).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::litige_repository::{
    ContexteLitige, DossierLitige, LitigeRepository, ResultatOuverture,
};
use klaar_trust::{Issue, Litige, MotifLitige, PartieLitige, StatutLitige};

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

    async fn ouverts(&self, limite: i64) -> Result<Vec<DossierLitige>, RepositoryError> {
        // Du plus ancien au plus récent : c'est celui qui approche des trente
        // jours qui doit remonter en premier, pas le dernier arrivé.
        let lignes = sqlx::query(&format!(
            "SELECT {COLONNES_DOSSIER} FROM litige l
             LEFT JOIN devis q ON q.mission_id = l.mission_id AND q.statut = 'ACCEPTED'
             WHERE l.statut = 'OPENED'
             ORDER BY l.ouvert_le ASC
             LIMIT $1"
        ))
        .bind(limite)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        lignes.iter().map(dossier_depuis_ligne).collect()
    }

    async fn dossier(&self, litige_id: Uuid) -> Result<Option<DossierLitige>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES_DOSSIER} FROM litige l
             LEFT JOIN devis q ON q.mission_id = l.mission_id AND q.statut = 'ACCEPTED'
             WHERE l.id = $1"
        ))
        .bind(litige_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(dossier_depuis_ligne).transpose()
    }

    async fn trancher(
        &self,
        litige_id: Uuid,
        issue: Issue,
        ops_id: Uuid,
        tranche_le: DateTime<Utc>,
    ) -> Result<Option<Litige>, RepositoryError> {
        // **Compare-and-swap sur le statut.** Deux médiateurs qui ouvrent le
        // même dossier ne doivent pas produire deux décisions ; le second
        // obtient `None` et voit que l'affaire est réglée. Lire puis écrire
        // laisserait passer les deux, et le second remboursement partirait sans
        // que personne ne s'en aperçoive.
        let ligne = sqlx::query(&format!(
            "UPDATE litige
                SET statut = $2, tranche_le = $3, tranche_par = $4, remboursement_cents = $5
              WHERE id = $1 AND statut = 'OPENED'
          RETURNING {COLONNES}"
        ))
        .bind(litige_id)
        .bind(issue.statut.as_str())
        .bind(tranche_le)
        .bind(ops_id)
        .bind(issue.remboursement_cents)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        ligne.as_ref().map(depuis_ligne).transpose()
    }
}

/// Les colonnes d'un dossier de médiation. Le montant vient du devis accepté,
/// s'il y en a un : un litige peut naître d'un travail jamais commencé.
const COLONNES_DOSSIER: &str = "l.id, l.mission_id, l.partie, l.motif, l.description,
     l.ouvert_le, COALESCE(q.total_ttc_cents, 0) AS total_ttc_cents";

fn dossier_depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<DossierLitige, RepositoryError> {
    let partie: String = ligne.get("partie");
    let motif: String = ligne.get("motif");
    Ok(DossierLitige {
        id: ligne.get("id"),
        mission_id: ligne.get("mission_id"),
        partie: PartieLitige::parse(&partie)
            .ok_or_else(|| RepositoryError::Contrainte(format!("partie inconnue : {partie}")))?,
        motif: MotifLitige::parse(&motif)
            .ok_or_else(|| RepositoryError::Contrainte(format!("motif inconnu : {motif}")))?,
        description: ligne.get("description"),
        ouvert_le: ligne.get("ouvert_le"),
        total_ttc_cents: ligne.get("total_ttc_cents"),
    })
}
