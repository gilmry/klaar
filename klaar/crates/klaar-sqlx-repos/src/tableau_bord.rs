//! Indicateurs d'exploitation (Story 8.3, FR-040).
//!
//! **Une requête, pas dix.** Chaque agrégat est indépendant, mais les émettre
//! séparément donnerait dix instantanés pris à dix moments différents : le
//! nombre de Demandes et celui des attributions ne se rapporteraient plus au
//! même instant, et le taux calculé dessus pourrait dépasser cent pour cent.
//! Une seule instruction les prend sur la même vue de la base.

use chrono::{DateTime, Utc};
use sqlx::Row;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::tableau_bord_repository::{Indicateurs, TableauBordRepository};

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgTableauBordRepository {
    pool: PoolPg,
}

impl PgTableauBordRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

impl TableauBordRepository for PgTableauBordRepository {
    async fn indicateurs(&self, depuis: DateTime<Utc>) -> Result<Indicateurs, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT
                 -- MAU : comptes ayant ouvert une session sur la fenêtre. La
                 -- création d'un refresh marque une authentification ; compter
                 -- les comptes créés dirait la croissance, pas l'usage.
                 (SELECT count(DISTINCT utilisateur_id) FROM session_refresh
                   WHERE cree_le >= $1) AS comptes_actifs,
                 (SELECT count(*) FROM demande WHERE cree_le >= $1) AS demandes,
                 -- Attribuée une fois pour toutes : une Demande qui a trouvé
                 -- puis dont l'intervention a été annulée avait bien trouvé.
                 -- Compter sur le statut courant ferait baisser le taux de
                 -- remplissage à chaque désistement, ce qui mesurerait autre
                 -- chose.
                 (SELECT count(DISTINCT demande_id) FROM mission m
                   JOIN demande d ON d.id = m.demande_id
                   WHERE d.cree_le >= $1) AS demandes_attribuees,
                 -- GMV et commission : les devis effectivement libérés. Les
                 -- devis acceptés mais non validés ne sont pas encore du
                 -- chiffre d'affaires.
                 -- `::bigint` : `SUM` sur un `BIGINT` rend un `NUMERIC`, que le
                 -- décodeur refuserait. Le total tient largement dans un entier
                 -- de soixante-quatre bits — le devis est plafonné à 10 000 €.
                 (SELECT COALESCE(SUM(q.montant_htva_cents), 0)::bigint FROM liberation l
                   JOIN devis q ON q.id = l.devis_id
                   WHERE l.decidee_le >= $1) AS gmv_htva_cents,
                 (SELECT COALESCE(SUM(commission_htva_cents), 0)::bigint FROM liberation
                   WHERE decidee_le >= $1) AS commission_htva_cents,
                 -- Sans borne de date : un litige de six semaines compte
                 -- encore, et c'est même celui qui compte le plus.
                 (SELECT count(*) FROM litige WHERE statut = 'OPENED') AS litiges_ouverts,
                 (SELECT count(*) FROM notation WHERE cree_le >= $1) AS notes,
                 (SELECT COALESCE(SUM(note), 0)::bigint FROM notation WHERE cree_le >= $1) AS somme_notes,
                 (SELECT count(*) FROM mission_transition
                   WHERE hors_zone AND enregistre_le >= $1) AS sorties_de_zone,
                 (SELECT count(*) FROM provider WHERE statut = 'PENDING_KYC') AS kyc_en_attente",
        )
        .bind(depuis)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(Indicateurs {
            comptes_actifs: ligne.get("comptes_actifs"),
            demandes: ligne.get("demandes"),
            demandes_attribuees: ligne.get("demandes_attribuees"),
            gmv_htva_cents: ligne.get("gmv_htva_cents"),
            commission_htva_cents: ligne.get("commission_htva_cents"),
            litiges_ouverts: ligne.get("litiges_ouverts"),
            notes: ligne.get("notes"),
            somme_notes: ligne.get("somme_notes"),
            sorties_de_zone: ligne.get("sorties_de_zone"),
            kyc_en_attente: ligne.get("kyc_en_attente"),
        })
    }
}
