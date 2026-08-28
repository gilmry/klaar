//! Dépôt PostgreSQL des exports réglementaires (Story 8.2, FR-039).
//!
//! **Le JSON est assemblé par PostgreSQL et non en Rust.** Une douzaine de
//! requêtes suivies d'une sérialisation manuelle aurait demandé une structure
//! par table, et chaque colonne ajoutée aurait dû être reportée à la main —
//! c'est-à-dire oubliée. `to_jsonb(t)` prend la ligne telle qu'elle est.

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::export_repository::{ExportRepository, LigneTva, TablePersonnelle};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgExportRepository {
    pool: PoolPg,
}

impl PgExportRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

impl ExportRepository for PgExportRepository {
    async fn donnees_personnelles(
        &self,
        utilisateur_id: Uuid,
    ) -> Result<Option<serde_json::Value>, RepositoryError> {
        // Le compte d'abord : son absence distingue « rien à exporter » de
        // « ce compte n'existe pas ».
        let compte = sqlx::query("SELECT to_jsonb(u) AS ligne FROM utilisateur u WHERE u.id = $1")
            .bind(utilisateur_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(erreur)?;
        let Some(compte) = compte else {
            return Ok(None);
        };

        // **Chaque table qui porte quelque chose de cette personne**, sous son
        // nom exact. C'est ce qui rend le contrôle d'exhaustivité possible : un
        // test compare cette liste à ce que `information_schema` déclare, et
        // tombe le jour où une table nouvelle est oubliée.
        //
        // Les colonnes ne sont pas énumérées : `to_jsonb` prend la ligne
        // entière, donc une colonne ajoutée demain sort toute seule.
        let requete = "
            SELECT jsonb_build_object(
                'utilisateur', (SELECT to_jsonb(u) FROM utilisateur u WHERE u.id = $1),
                'jeton_verification_email', COALESCE((SELECT jsonb_agg(to_jsonb(j))
                    FROM jeton_verification_email j WHERE j.utilisateur_id = $1), '[]'::jsonb),
                'session_refresh', COALESCE((SELECT jsonb_agg(to_jsonb(s))
                    FROM session_refresh s WHERE s.utilisateur_id = $1), '[]'::jsonb),
                'methode_paiement', COALESCE((SELECT jsonb_agg(to_jsonb(mp))
                    FROM methode_paiement mp WHERE mp.utilisateur_id = $1), '[]'::jsonb),
                'journal_audit', COALESCE((SELECT jsonb_agg(to_jsonb(a))
                    FROM journal_audit a WHERE a.sujet_id = $1), '[]'::jsonb),
                'demande', COALESCE((SELECT jsonb_agg(to_jsonb(d))
                    FROM demande d WHERE d.demandeur_id = $1), '[]'::jsonb),
                'push_subscription', COALESCE((SELECT jsonb_agg(to_jsonb(p))
                    FROM push_subscription p WHERE p.sujet_id = $1), '[]'::jsonb),
                'provider', (SELECT to_jsonb(pr) FROM provider pr
                    WHERE pr.utilisateur_id = $1),
                'notation', COALESCE((SELECT jsonb_agg(to_jsonb(n))
                    FROM notation n WHERE n.auteur_id = $1), '[]'::jsonb),
                'message', COALESCE((SELECT jsonb_agg(to_jsonb(m))
                    FROM message m WHERE m.auteur_id = $1), '[]'::jsonb),
                'tentative_contournement', COALESCE((SELECT jsonb_agg(to_jsonb(t))
                    FROM tentative_contournement t WHERE t.auteur_id = $1), '[]'::jsonb),
                'litige', COALESCE((SELECT jsonb_agg(to_jsonb(l))
                    FROM litige l WHERE l.auteur_id = $1), '[]'::jsonb)
            ) AS export";

        let ligne = sqlx::query(requete)
            .bind(utilisateur_id)
            .fetch_one(&self.pool)
            .await
            .map_err(erreur)?;
        let _ = compte;
        Ok(Some(ligne.get("export")))
    }

    async fn tables_personnelles(&self) -> Result<Vec<TablePersonnelle>, RepositoryError> {
        // Lu depuis le schéma : une liste écrite à la main se désynchronise, un
        // schéma non. C'est ce qui permet à un test de constater qu'une table
        // nouvelle a été oubliée de l'export.
        let lignes = sqlx::query(
            "SELECT c.table_name, c.column_name
             FROM information_schema.table_constraints tc
             JOIN information_schema.key_column_usage c
               ON c.constraint_name = tc.constraint_name
              AND c.constraint_schema = tc.constraint_schema
             JOIN information_schema.constraint_column_usage cible
               ON cible.constraint_name = tc.constraint_name
              AND cible.constraint_schema = tc.constraint_schema
             WHERE tc.constraint_type = 'FOREIGN KEY'
               AND tc.table_schema = 'public'
               AND cible.table_name = 'utilisateur'
             ORDER BY c.table_name, c.column_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(lignes
            .iter()
            .map(|l| TablePersonnelle {
                table: l.get("table_name"),
                colonne: l.get("column_name"),
            })
            .collect())
    }

    async fn lignes_tva(
        &self,
        debut: DateTime<Utc>,
        fin: DateTime<Utc>,
    ) -> Result<Vec<LigneTva>, RepositoryError> {
        // **La date qui fait foi est celle de la libération**, pas celle du
        // devis : c'est au moment où l'argent est dû que la TVA devient
        // exigible, et un devis émis en décembre et validé en janvier appartient
        // à l'exercice suivant.
        let lignes = sqlx::query(
            "SELECT d.id AS devis_id, l.decidee_le, d.taux_tva_bp,
                    d.montant_htva_cents, d.tva_cents, d.total_ttc_cents,
                    l.commission_htva_cents, l.tva_commission_cents
             FROM liberation l
             JOIN devis d ON d.id = l.devis_id
             WHERE l.decidee_le >= $1 AND l.decidee_le < $2
             ORDER BY l.decidee_le, d.id",
        )
        .bind(debut)
        .bind(fin)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        lignes
            .iter()
            .map(|l| {
                let taux: i16 = l.get("taux_tva_bp");
                Ok(LigneTva {
                    devis_id: l.get("devis_id"),
                    decidee_le: l.get("decidee_le"),
                    taux_tva_bp: u16::try_from(taux).map_err(|_| {
                        RepositoryError::Contrainte(format!("taux hors bornes : {taux}"))
                    })?,
                    montant_htva_cents: l.get("montant_htva_cents"),
                    tva_cents: l.get("tva_cents"),
                    total_ttc_cents: l.get("total_ttc_cents"),
                    commission_htva_cents: l.get("commission_htva_cents"),
                    tva_commission_cents: l.get("tva_commission_cents"),
                })
            })
            .collect()
    }
}
