//! Effacement PostgreSQL (Story 1.9, FR-005, RGPD art. 17).

use chrono::{DateTime, Utc};
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::utilisateur_repository::EffacementRepository;
use klaar_application::usecases::effacer::adresse_effacee;

use crate::erreur;
use crate::utilisateur::PgUtilisateurRepository;

impl EffacementRepository for PgUtilisateurRepository {
    async fn programmer_effacement(
        &self,
        utilisateur_id: Uuid,
        efface_le: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError> {
        // `statut <> 'ERASED'` : un jeton d'accès encore valide au moment de
        // l'exécution pourrait arriver juste après, et ne doit pas ressusciter
        // une échéance sur un compte déjà vidé.
        sqlx::query(
            "UPDATE utilisateur SET statut = 'ERASED_PENDING', efface_le = $1
             WHERE id = $2 AND statut <> 'ERASED'",
        )
        .bind(efface_le)
        .bind(utilisateur_id)
        .execute(self.pool())
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn annuler_effacement(&self, utilisateur_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE utilisateur SET statut = 'ACTIVE', efface_le = NULL
             WHERE id = $1 AND statut = 'ERASED_PENDING'",
        )
        .bind(utilisateur_id)
        .execute(self.pool())
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn effacements_echus(
        &self,
        maintenant: DateTime<Utc>,
    ) -> Result<Vec<Uuid>, RepositoryError> {
        sqlx::query_scalar(
            "SELECT id FROM utilisateur
             WHERE statut = 'ERASED_PENDING' AND efface_le <= $1
             ORDER BY efface_le",
        )
        .bind(maintenant)
        .fetch_all(self.pool())
        .await
        .map_err(erreur)
    }

    async fn effacer(
        &self,
        utilisateur_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        // Une seule transaction : un compte à moitié effacé serait pire que
        // pas effacé du tout, puisque personne ne saurait ce qu'il en reste.
        let mut tx = self.pool().begin().await.map_err(erreur)?;

        // La mise à jour du compte passe **en premier**, gardée par son statut.
        // Elle pose le verrou de ligne : une seconde exécution du job attend
        // ici, puis ne trouve plus `ERASED_PENDING` et repart les mains vides.
        // Sans cette garde, deux exécutions concurrentes écrivaient chacune une
        // entrée `USER_ERASED`, et le journal prétendait que le droit avait été
        // exercé deux fois — trouvé par un test, pas par relecture.
        //
        // La ligne est **vidée, pas supprimée** : la supprimer emporterait par
        // cascade les entrées du journal d'audit, que le scénario `@security`
        // de FR-005 exige de conserver.
        //
        // `cree_le` est ramené à l'instant de l'effacement : la date
        // d'inscription est une donnée sur la personne, et la garder
        // permettrait de rapprocher le compte vidé d'une cohorte connue.
        let touches = sqlx::query(
            "UPDATE utilisateur
             SET statut = 'ERASED',
                 email = $1,
                 empreinte_mot_de_passe = NULL,
                 locale = 'fr',
                 cree_le = $2,
                 efface_le = NULL,
                 echecs_consecutifs = 0,
                 dernier_echec_le = NULL,
                 verrouille_jusqu_a = NULL
             WHERE id = $3 AND statut = 'ERASED_PENDING'",
        )
        .bind(adresse_effacee(utilisateur_id))
        .bind(maintenant)
        .bind(utilisateur_id)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?
        .rows_affected();

        if touches == 0 {
            tx.rollback().await.map_err(erreur)?;
            return Ok(false);
        }

        // Les jetons de vérification et les abonnements push partent par
        // cascade avec les lignes qui les portent ; les sessions, non — leur
        // clé étrangère cascade à la suppression de la ligne de compte, et
        // cette ligne, elle, est conservée. Elles sont donc supprimées
        // explicitement.
        sqlx::query("DELETE FROM session_refresh WHERE utilisateur_id = $1")
            .bind(utilisateur_id)
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;

        sqlx::query("DELETE FROM jeton_verification_email WHERE utilisateur_id = $1")
            .bind(utilisateur_id)
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;

        sqlx::query("DELETE FROM push_subscription WHERE sujet_id = $1")
            .bind(utilisateur_id)
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;

        tx.commit().await.map_err(erreur)?;
        Ok(true)
    }
}
