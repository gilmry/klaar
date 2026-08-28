//! Revue KYC (Story 8.1, FR-038), en PostgreSQL.

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::revue_kyc_repository::{
    DossierKyc, RefusEnAttente, RevueKycRepository,
};
use klaar_identity::{DecisionKyc, RevueKyc, StatutProvider};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgRevueKycRepository {
    pool: PoolPg,
}

impl PgRevueKycRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

/// Le dossier et, s'il y en a un, le refus qui attend sa confirmation.
const COLONNES_DOSSIER: &str = "p.id AS provider_id, p.numero_bce, p.raison_sociale, p.cree_le,
     COALESCE(
         (SELECT array_agg(secteur_code ORDER BY secteur_code)
            FROM provider_competence WHERE provider_id = p.id),
         '{}') AS secteurs,
     r.id AS revue_id, r.motif, r.premier_ops, r.propose_le";

const JOINTURE_REVUE: &str =
    "LEFT JOIN revue_kyc r ON r.provider_id = p.id AND r.confirme_le IS NULL";

fn dossier_depuis_ligne(
    ligne: &sqlx::postgres::PgRow,
    maintenant: DateTime<Utc>,
) -> Result<DossierKyc, RepositoryError> {
    let inscrit_le: DateTime<Utc> = ligne.get("cree_le");
    let revue_id: Option<Uuid> = ligne.get("revue_id");
    Ok(DossierKyc {
        provider_id: ligne.get("provider_id"),
        numero_bce: ligne.get("numero_bce"),
        raison_sociale: ligne.get("raison_sociale"),
        secteurs: ligne.get("secteurs"),
        inscrit_le,
        attente_jours: (maintenant - inscrit_le).num_days().max(0),
        refus_en_attente: revue_id.map(|id| RefusEnAttente {
            revue_id: id,
            motif: ligne.get::<Option<String>, _>("motif").unwrap_or_default(),
            propose_par: ligne.get("premier_ops"),
            propose_le: ligne.get("propose_le"),
        }),
    })
}

impl RevueKycRepository for PgRevueKycRepository {
    async fn en_attente(&self, limite: i64) -> Result<Vec<DossierKyc>, RepositoryError> {
        // De la plus ancienne à la plus récente : c'est l'entreprise qui attend
        // depuis le plus longtemps qui doit remonter, pas la dernière inscrite.
        let lignes = sqlx::query(&format!(
            "SELECT {COLONNES_DOSSIER}
             FROM provider p {JOINTURE_REVUE}
             WHERE p.statut = 'PENDING_KYC'
             ORDER BY p.cree_le ASC
             LIMIT $1"
        ))
        .bind(limite)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        let maintenant = Utc::now();
        lignes
            .iter()
            .map(|l| dossier_depuis_ligne(l, maintenant))
            .collect()
    }

    async fn dossier(
        &self,
        provider_id: Uuid,
    ) -> Result<Option<(DossierKyc, StatutProvider)>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES_DOSSIER}, p.statut
             FROM provider p {JOINTURE_REVUE}
             WHERE p.id = $1"
        ))
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        let Some(ligne) = ligne else {
            return Ok(None);
        };
        let brut: String = ligne.get("statut");
        let statut = StatutProvider::parse(&brut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {brut}")))?;
        Ok(Some((dossier_depuis_ligne(&ligne, Utc::now())?, statut)))
    }

    async fn proposer(&self, revue: &RevueKyc) -> Result<bool, RepositoryError> {
        let ecrit = sqlx::query(
            "INSERT INTO revue_kyc
                 (id, provider_id, decision, motif, premier_ops, propose_le,
                  second_ops, confirme_le)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT DO NOTHING
             RETURNING id",
        )
        .bind(revue.id)
        .bind(revue.provider_id)
        .bind(revue.decision.as_str())
        .bind(revue.motif.as_deref())
        .bind(revue.premier_ops)
        .bind(revue.propose_le)
        .bind(revue.second_ops)
        .bind(revue.confirme_le)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }

    async fn en_attente_de_confirmation(
        &self,
        provider_id: Uuid,
    ) -> Result<Option<RevueKyc>, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT id, provider_id, decision, motif, premier_ops, propose_le,
                    second_ops, confirme_le
             FROM revue_kyc WHERE provider_id = $1 AND confirme_le IS NULL",
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        ligne
            .map(|l| {
                let brut: String = l.get("decision");
                Ok(RevueKyc {
                    id: l.get("id"),
                    provider_id: l.get("provider_id"),
                    decision: DecisionKyc::parse(&brut).ok_or_else(|| {
                        RepositoryError::Contrainte(format!("décision inconnue : {brut}"))
                    })?,
                    motif: l.get("motif"),
                    // `premier_ops` est nullable en base : le compte qui a
                    // proposé peut avoir quitté la société. Un `nil` marque ce
                    // cas plutôt que de faire échouer la lecture du dossier.
                    premier_ops: l
                        .get::<Option<Uuid>, _>("premier_ops")
                        .unwrap_or(Uuid::nil()),
                    propose_le: l.get("propose_le"),
                    second_ops: l.get("second_ops"),
                    confirme_le: l.get("confirme_le"),
                })
            })
            .transpose()
    }

    async fn clore(
        &self,
        revue: &RevueKyc,
        statut: StatutProvider,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // **Compare-and-swap sur le statut du prestataire.** C'est lui qui ferme
        // la course entre deux examinateurs, et le cas où l'entreprise s'est
        // retirée entre-temps : l'`UPDATE` ne trouve rien, et rien n'est écrit.
        let bascule = sqlx::query(
            "UPDATE provider
                SET statut = $2,
                    origine_kyc = CASE WHEN $2 = 'ACTIVE' THEN 'OPS_REVIEW' ELSE origine_kyc END,
                    kyc_verifie_le = CASE WHEN $2 = 'ACTIVE' THEN $3 ELSE kyc_verifie_le END
              WHERE id = $1 AND statut = 'PENDING_KYC'
          RETURNING id",
        )
        .bind(revue.provider_id)
        .bind(statut.as_str())
        .bind(maintenant)
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        if bascule.is_none() {
            tx.rollback().await.map_err(erreur)?;
            return Ok(false);
        }

        sqlx::query(
            "INSERT INTO revue_kyc
                 (id, provider_id, decision, motif, premier_ops, propose_le,
                  second_ops, confirme_le)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE
                 SET second_ops = EXCLUDED.second_ops, confirme_le = EXCLUDED.confirme_le",
        )
        .bind(revue.id)
        .bind(revue.provider_id)
        .bind(revue.decision.as_str())
        .bind(revue.motif.as_deref())
        .bind(revue.premier_ops)
        .bind(revue.propose_le)
        .bind(revue.second_ops)
        .bind(revue.confirme_le)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        tx.commit().await.map_err(erreur)?;
        Ok(true)
    }

    async fn retirer(&self, provider_id: Uuid) -> Result<bool, RepositoryError> {
        let ecrit = sqlx::query(
            "UPDATE provider SET statut = 'WITHDRAWN'
              WHERE id = $1 AND statut = 'PENDING_KYC'
          RETURNING id",
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }
}
