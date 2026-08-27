//! Dépôt PostgreSQL des comptes utilisateur (Story 1.1, FR-001).

use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::utilisateur_repository::{JetonAConserver, UtilisateurRepository};
use klaar_identity::{EmpreinteMotDePasse, StatutUtilisateur, Utilisateur};
use klaar_shared_kernel::{Email, Locale};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgUtilisateurRepository {
    pool: PoolPg,
}

impl PgUtilisateurRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

/// Reconstruit l'agrégat depuis une ligne.
///
/// Une ligne illisible est une erreur d'infrastructure, pas un compte absent :
/// la confondre avec `None` ferait disparaître silencieusement un utilisateur
/// dont le statut aurait été corrompu, et l'inviterait à se réinscrire.
fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<Utilisateur, RepositoryError> {
    let email: String = ligne.get("email");
    let statut: String = ligne.get("statut");
    let locale: String = ligne.get("locale");
    let phc: String = ligne.get("empreinte_mot_de_passe");

    Ok(Utilisateur {
        id: ligne.get("id"),
        email: Email::parse(&email)
            .map_err(|e| RepositoryError::Contrainte(format!("email en base illisible : {e}")))?,
        empreinte_mot_de_passe: EmpreinteMotDePasse::depuis_phc(&phc).map_err(|e| {
            RepositoryError::Contrainte(format!("empreinte en base illisible : {e}"))
        })?,
        statut: StatutUtilisateur::parse(&statut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {statut}")))?,
        locale: Locale::parse(&locale)
            .map_err(|e| RepositoryError::Contrainte(format!("locale en base illisible : {e}")))?,
        cree_le: ligne.get("cree_le"),
    })
}

const COLONNES: &str = "id, email, empreinte_mot_de_passe, statut, locale, cree_le";

impl UtilisateurRepository for PgUtilisateurRepository {
    async fn creer_si_absent(
        &self,
        utilisateur: &Utilisateur,
        jeton: &JetonAConserver,
    ) -> Result<bool, RepositoryError> {
        // Transaction, parce que les deux écritures forment une seule vérité :
        // un compte sans jeton est un compte que personne ne peut activer, et
        // que l'anti-énumération empêchera de recréer.
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // ON CONFLICT DO NOTHING plutôt que « SELECT puis INSERT » : entre le
        // SELECT et l'INSERT, une requête concurrente s'insère (FR-001
        // `@edge`). Ici, c'est la contrainte d'unicité qui tranche, et elle ne
        // laisse pas d'intervalle.
        let insere = sqlx::query(
            r#"
            INSERT INTO utilisateur (id, email, empreinte_mot_de_passe, statut, locale, cree_le)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (email) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(utilisateur.id)
        .bind(utilisateur.email.as_str())
        .bind(utilisateur.empreinte_mot_de_passe.as_str())
        .bind(utilisateur.statut.as_str())
        .bind(utilisateur.locale.as_str())
        .bind(utilisateur.cree_le)
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        if insere.is_none() {
            // Rien à défaire, mais la transaction doit être refermée
            // explicitement plutôt que laissée au `Drop`, qui annule sans le dire.
            tx.rollback().await.map_err(erreur)?;
            return Ok(false);
        }

        sqlx::query(
            "INSERT INTO jeton_verification_email (empreinte, utilisateur_id, expire_le)
             VALUES ($1, $2, $3)",
        )
        .bind(jeton.empreinte.as_str())
        .bind(utilisateur.id)
        .bind(jeton.expire_le)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        tx.commit().await.map_err(erreur)?;
        Ok(true)
    }

    async fn par_email(&self, email: &Email) -> Result<Option<Utilisateur>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM utilisateur WHERE email = $1"
        ))
        .bind(email.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn par_id(&self, id: Uuid) -> Result<Option<Utilisateur>, RepositoryError> {
        let ligne = sqlx::query(&format!("SELECT {COLONNES} FROM utilisateur WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }
}
