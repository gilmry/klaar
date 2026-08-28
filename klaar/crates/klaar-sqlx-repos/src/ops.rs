//! Dépôt PostgreSQL des comptes d'exploitation (Story 8.4, FR-041, FR-042).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::ops_repository::{GesteOps, OpsRepository};
use klaar_identity::{CompteOps, EmpreinteMotDePasse, RoleOps};
use klaar_shared_kernel::Email;

use crate::erreur;
use crate::pool::PoolPg;

const COLONNES: &str = "id, email, empreinte_mot_de_passe, role, secret_totp, \
                        dernier_pas_totp, actif, derniere_activite, cree_le";

pub struct PgOpsRepository {
    pool: PoolPg,
}

impl PgOpsRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<CompteOps, RepositoryError> {
    let email: String = ligne.get("email");
    let role: String = ligne.get("role");
    let empreinte: String = ligne.get("empreinte_mot_de_passe");
    Ok(CompteOps {
        id: ligne.get("id"),
        email: Email::parse(&email)
            .map_err(|e| RepositoryError::Contrainte(format!("email illisible : {e}")))?,
        empreinte_mot_de_passe: EmpreinteMotDePasse::depuis_phc(&empreinte)
            .map_err(|e| RepositoryError::Contrainte(format!("empreinte illisible : {e}")))?,
        role: RoleOps::parse(&role)
            .ok_or_else(|| RepositoryError::Contrainte(format!("rôle inconnu : {role}")))?,
        secret_totp: ligne.get("secret_totp"),
        actif: ligne.get("actif"),
        derniere_activite: ligne.get("derniere_activite"),
        cree_le: ligne.get("cree_le"),
    })
}

impl OpsRepository for PgOpsRepository {
    async fn creer(&self, compte: &CompteOps) -> Result<bool, RepositoryError> {
        let ecrit = sqlx::query(
            "INSERT INTO compte_ops
                 (id, email, empreinte_mot_de_passe, role, secret_totp, actif,
                  derniere_activite, cree_le)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (email) DO NOTHING
             RETURNING id",
        )
        .bind(compte.id)
        .bind(compte.email.as_str())
        .bind(compte.empreinte_mot_de_passe.as_str())
        .bind(compte.role.as_str())
        .bind(compte.secret_totp.as_deref())
        .bind(compte.actif)
        .bind(compte.derniere_activite)
        .bind(compte.cree_le)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }

    async fn par_email(&self, email: &Email) -> Result<Option<CompteOps>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM compte_ops WHERE email = $1"
        ))
        .bind(email.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn par_id(&self, id: Uuid) -> Result<Option<CompteOps>, RepositoryError> {
        let ligne = sqlx::query(&format!("SELECT {COLONNES} FROM compte_ops WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn configurer_totp(&self, ops_id: Uuid, secret: &[u8]) -> Result<bool, RepositoryError> {
        // `secret_totp IS NULL` en garde : remplacer un secret existant
        // permettrait à quelqu'un qui a volé une session de reconfigurer la
        // seconde authentification sur son propre téléphone.
        let ecrit = sqlx::query(
            "UPDATE compte_ops SET secret_totp = $2
             WHERE id = $1 AND secret_totp IS NULL
             RETURNING id",
        )
        .bind(ops_id)
        .bind(secret)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }

    async fn consommer_pas_totp(
        &self,
        ops_id: Uuid,
        pas: i64,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        // Le compare-and-swap ferme le rejeu au niveau où les écritures sont
        // sérialisées : deux requêtes portant le même code voient la même
        // ligne, et une seule la fait avancer.
        let ecrit = sqlx::query(
            "UPDATE compte_ops
             SET dernier_pas_totp = $2, derniere_activite = $3
             WHERE id = $1 AND (dernier_pas_totp IS NULL OR dernier_pas_totp < $2)
             RETURNING id",
        )
        .bind(ops_id)
        .bind(pas)
        .bind(maintenant)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }

    async fn revoquer_les_inactifs(&self, avant: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let issue = sqlx::query(
            "UPDATE compte_ops SET actif = FALSE WHERE actif AND derniere_activite < $1",
        )
        .bind(avant)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(issue.rows_affected())
    }

    async fn consigner(&self, geste: &GesteOps) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO journal_ops (ops_id, geste, cible, fait_le) VALUES ($1, $2, $3, $4)",
        )
        .bind(geste.ops_id)
        .bind(&geste.geste)
        .bind(geste.cible.as_deref())
        .bind(geste.fait_le)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn journal(
        &self,
        acteur: Option<Uuid>,
        limite: i64,
        decalage: i64,
    ) -> Result<Vec<GesteOps>, RepositoryError> {
        // `$1 IS NULL OR ops_id = $1` plutôt que deux requêtes : le filtre est
        // optionnel et le plan reste le même, l'index couvrant les deux cas.
        let lignes = sqlx::query(
            "SELECT ops_id, geste, cible, fait_le FROM journal_ops
             WHERE $1::uuid IS NULL OR ops_id = $1
             ORDER BY fait_le DESC, id DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(acteur)
        .bind(limite)
        .bind(decalage)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(lignes
            .iter()
            .map(|l| GesteOps {
                ops_id: l.get("ops_id"),
                geste: l.get("geste"),
                cible: l.get("cible"),
                fait_le: l.get("fait_le"),
            })
            .collect())
    }
}
