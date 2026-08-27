//! Dépôt PostgreSQL des comptes utilisateur (Story 1.1, FR-001).

use sqlx::Row;
use uuid::Uuid;

use chrono::{DateTime, Utc};
use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::utilisateur_repository::{
    JetonAConserver, ResultatJeton, UtilisateurRepository,
};
use klaar_identity::{
    EmpreinteJeton, EmpreinteMotDePasse, StatutUtilisateur, Utilisateur, Verrouillage,
};
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

    /// Le pool, pour les `impl` d'autres ports portés par le même type
    /// (`EffacementRepository`, dans `effacement.rs`).
    pub(crate) fn pool(&self) -> &PoolPg {
        &self.pool
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
    let phc: Option<String> = ligne.get("empreinte_mot_de_passe");

    Ok(Utilisateur {
        id: ligne.get("id"),
        email: Email::parse(&email)
            .map_err(|e| RepositoryError::Contrainte(format!("email en base illisible : {e}")))?,
        empreinte_mot_de_passe: phc
            .map(|p| EmpreinteMotDePasse::depuis_phc(&p))
            .transpose()
            .map_err(|e| {
                RepositoryError::Contrainte(format!("empreinte en base illisible : {e}"))
            })?,
        statut: StatutUtilisateur::parse(&statut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {statut}")))?,
        locale: Locale::parse(&locale)
            .map_err(|e| RepositoryError::Contrainte(format!("locale en base illisible : {e}")))?,
        cree_le: ligne.get("cree_le"),
        efface_le: ligne.get("efface_le"),
        verrouillage: Verrouillage {
            echecs_consecutifs: ligne.get("echecs_consecutifs"),
            dernier_echec_le: ligne.get("dernier_echec_le"),
            verrouille_jusqu_a: ligne.get("verrouille_jusqu_a"),
        },
    })
}

const COLONNES: &str = "id, email, empreinte_mot_de_passe, statut, locale, cree_le, \
     echecs_consecutifs, dernier_echec_le, verrouille_jusqu_a, efface_le";

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
        .bind(
            utilisateur
                .empreinte_mot_de_passe
                .as_ref()
                .map(|e| e.as_str()),
        )
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

    async fn consommer_jeton_verification(
        &self,
        empreinte: &EmpreinteJeton,
        maintenant: DateTime<Utc>,
    ) -> Result<ResultatJeton, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // FOR UPDATE : deux clics simultanés sur le même lien sérialisent ici.
        // Sans ce verrou, les deux liraient `consomme_le IS NULL` et
        // consommeraient le jeton chacun de leur côté, produisant deux entrées
        // d'audit pour une seule vérification.
        let ligne = sqlx::query(
            "SELECT utilisateur_id, expire_le, consomme_le
             FROM jeton_verification_email WHERE empreinte = $1 FOR UPDATE",
        )
        .bind(empreinte.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        let Some(ligne) = ligne else {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatJeton::Inconnu);
        };

        let utilisateur_id: Uuid = ligne.get("utilisateur_id");
        let consomme_le: Option<DateTime<Utc>> = ligne.get("consomme_le");
        let expire_le: DateTime<Utc> = ligne.get("expire_le");

        // L'ordre compte : un jeton déjà consommé reste « déjà consommé » même
        // une fois passée son heure de validité. L'inverse afficherait « lien
        // expiré » à quelqu'un dont le compte est actif depuis longtemps.
        if consomme_le.is_some() {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatJeton::DejaConsomme { utilisateur_id });
        }
        if expire_le <= maintenant {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatJeton::Expire);
        }

        sqlx::query("UPDATE jeton_verification_email SET consomme_le = $1 WHERE empreinte = $2")
            .bind(maintenant)
            .bind(empreinte.as_str())
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;

        let touches = sqlx::query("UPDATE utilisateur SET statut = $1 WHERE id = $2")
            .bind(StatutUtilisateur::Actif.as_str())
            .bind(utilisateur_id)
            .execute(&mut *tx)
            .await
            .map_err(erreur)?
            .rows_affected();

        if touches == 0 {
            // La clé étrangère rend ce cas normalement impossible. S'il
            // survient, annuler vaut mieux que marquer consommé un jeton dont
            // le compte n'existe pas : le second laisserait une ligne dont
            // plus rien ne peut être fait.
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatJeton::Inconnu);
        }

        tx.commit().await.map_err(erreur)?;
        Ok(ResultatJeton::Consomme { utilisateur_id })
    }

    async fn mettre_a_jour_verrouillage(
        &self,
        utilisateur_id: Uuid,
        verrouillage: &Verrouillage,
    ) -> Result<(), RepositoryError> {
        // Seules ces trois colonnes sont écrites : un échec d'authentification
        // ne doit pas pouvoir écraser un profil modifié entre-temps par un
        // autre chemin.
        sqlx::query(
            "UPDATE utilisateur
             SET echecs_consecutifs = $1, dernier_echec_le = $2, verrouille_jusqu_a = $3
             WHERE id = $4",
        )
        .bind(verrouillage.echecs_consecutifs)
        .bind(verrouillage.dernier_echec_le)
        .bind(verrouillage.verrouille_jusqu_a)
        .bind(utilisateur_id)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
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
