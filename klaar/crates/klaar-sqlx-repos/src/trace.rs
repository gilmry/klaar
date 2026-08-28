//! Trace de matching PostgreSQL (Story 3.2, FR-012, AI Act art. 12).
//!
//! **La trace est chaînée** depuis la Story 3.8 : chaque ligne porte un
//! HMAC-SHA256 calculé sur son contenu et sur la signature de la précédente.
//! La tête de chaîne vit dans `trace_chaine`, verrouillée le temps de
//! l'écriture — deux tours de matching simultanés s'y sérialisent donc. C'est
//! le prix du chaînage, il se compte en millisecondes, et il achète la
//! détection des suppressions, qu'un HMAC par ligne indépendant ne donnerait
//! pas.

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::trace_repository::{LigneTrace, TraceRepository};
use klaar_audit_adapter::{contenu_canonique, SignataireTrace};
use std::sync::Arc;
use uuid::Uuid;

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgTraceRepository {
    pool: PoolPg,
    /// Signataire de la chaîne.
    ///
    /// `Option` : un déploiement sans clé écrit une trace **non signée** plutôt
    /// que pas de trace du tout. Une trace non signée reste consultable et
    /// explique toujours une décision ; l'absence de trace, elle, ne s'explique
    /// pas. Le rapport d'audit compte les lignes non signées et le dit.
    signataire: Option<Arc<SignataireTrace>>,
}

impl PgTraceRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self {
            pool,
            signataire: None,
        }
    }

    /// Scelle la trace avec une clé HMAC (Story 3.8).
    pub fn avec_signature(pool: PoolPg, signataire: Arc<SignataireTrace>) -> Self {
        Self {
            pool,
            signataire: Some(signataire),
        }
    }
}

impl TraceRepository for PgTraceRepository {
    async fn consigner(&self, lignes: &[LigneTrace]) -> Result<(), RepositoryError> {
        if lignes.is_empty() {
            return Ok(());
        }
        // Toutes ou aucune : une trace partielle est pire qu'absente, elle
        // laisse croire que les candidats manquants n'ont jamais été examinés.
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // Tête de chaîne verrouillée pour la durée de l'écriture. Sans le
        // `FOR UPDATE`, deux tours simultanés liraient la même tête et
        // produiraient deux maillons qui se prétendent successeurs du même
        // prédécesseur : la chaîne serait cassée dès sa deuxième ligne.
        let mut precedente: Option<Vec<u8>> = if self.signataire.is_some() {
            sqlx::query_scalar("SELECT derniere_signature FROM trace_chaine FOR UPDATE")
                .fetch_one(&mut *tx)
                .await
                .map_err(erreur)?
        } else {
            None
        };

        for ligne in lignes {
            let ventilation = serde_json::to_value(ligne.score).map_err(|e| {
                RepositoryError::Contrainte(format!("ventilation non sérialisable : {e}"))
            })?;

            // `ON CONFLICT DO NOTHING` sur le couple Demande/prestataire : un
            // second tour de matching sur la même Demande ne doit pas écraser
            // la trace du premier. Ce qui a été décidé l'a été à un instant
            // donné, et le réécrire effacerait ce qu'on cherche justement à
            // pouvoir expliquer.
            let motif = ligne.motif_ecart.map(|m| m.as_str());
            let signature = self.signataire.as_ref().map(|s| {
                s.signer(
                    precedente.as_deref(),
                    &contenu_canonique(
                        &ligne.demande_id,
                        &ligne.provider_id,
                        ligne.score.total,
                        ligne.distance_metres,
                        ligne.retenu,
                        motif,
                        ligne.tracee_le.timestamp(),
                    ),
                )
            });

            let insere = sqlx::query(
                "INSERT INTO trace_matching
                     (demande_id, provider_id, score, distance_metres, ventilation,
                      retenu, motif_ecart, tracee_le, signature, signature_precedente)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (demande_id, provider_id) DO NOTHING
                 RETURNING id",
            )
            .bind(ligne.demande_id)
            .bind(ligne.provider_id)
            .bind(ligne.score.total)
            .bind(ligne.distance_metres)
            .bind(ventilation)
            .bind(ligne.retenu)
            .bind(motif)
            .bind(ligne.tracee_le)
            .bind(signature.as_deref())
            .bind(precedente.as_deref())
            .fetch_optional(&mut *tx)
            .await
            .map_err(erreur)?;

            // La chaîne n'avance que si la ligne est réellement entrée. Sur
            // conflit, rien n'a été écrit : faire avancer la tête laisserait un
            // maillon qui ne correspond à aucune ligne, et la vérification
            // échouerait sur une trace pourtant intacte.
            if insere.is_some() {
                precedente = signature;
            }
        }

        if self.signataire.is_some() {
            sqlx::query(
                "UPDATE trace_chaine SET derniere_signature = $1, mis_a_jour_le = now()
                 WHERE unique_ligne",
            )
            .bind(precedente.as_deref())
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;
        }

        tx.commit().await.map_err(erreur)?;
        Ok(())
    }

    async fn comptes_retenus_sauf(
        &self,
        demande_id: Uuid,
        sauf_provider_id: Uuid,
    ) -> Result<Vec<Uuid>, RepositoryError> {
        // Jointure sur `provider` : la trace enregistre des prestataires, alors
        // que les abonnements push sont portés par des comptes. Confondre les
        // deux enverrait les notifications dans le vide.
        sqlx::query_scalar(
            "SELECT p.utilisateur_id FROM trace_matching t
             JOIN provider p ON p.id = t.provider_id
             WHERE t.demande_id = $1 AND t.retenu AND t.provider_id <> $2",
        )
        .bind(demande_id)
        .bind(sauf_provider_id)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)
    }
}
