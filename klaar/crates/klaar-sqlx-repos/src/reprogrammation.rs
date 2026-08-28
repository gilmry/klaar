//! Dépôt PostgreSQL de la reprogrammation (Story 4.8, FR-023).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::evenements::EvenementMission;
use klaar_application::ports::reprogrammation_repository::{
    ContexteReprogrammation, ReprogrammationRepository, ResultatAcceptation,
};
use klaar_intervention::{AuteurAnnulation, Reprogrammation, StatutReprogrammation};

use crate::pool::PoolPg;
use crate::{erreur, notifier};

/// Nom de l'index « une Mission à la fois par prestataire » (migration V13).
const INDEX_UNE_MISSION_A_LA_FOIS: &str = "mission_provider_en_cours_idx";

pub struct PgReprogrammationRepository {
    pool: PoolPg,
}

impl PgReprogrammationRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

impl ReprogrammationRepository for PgReprogrammationRepository {
    async fn contexte(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<ContexteReprogrammation>, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT m.provider_id, d.demandeur_id,
                    a.auteur AS auteur_annulation, a.decidee_le,
                    (SELECT q.id FROM devis q
                     WHERE q.mission_id = m.id AND q.statut = 'ACCEPTED'
                     LIMIT 1) AS devis_accepte
             FROM mission m
             JOIN demande d ON d.id = m.demande_id
             LEFT JOIN annulation_mission a ON a.mission_id = m.id
             WHERE m.id = $1",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        let Some(l) = ligne else { return Ok(None) };
        let auteur: Option<String> = l.get("auteur_annulation");
        let decidee: Option<DateTime<Utc>> = l.get("decidee_le");
        Ok(Some(ContexteReprogrammation {
            demandeur_id: l.get("demandeur_id"),
            provider_id: l.get("provider_id"),
            annulation: match (auteur.as_deref().and_then(AuteurAnnulation::parse), decidee) {
                (Some(a), Some(q)) => Some((a, q)),
                _ => None,
            },
            devis_accepte: l.get("devis_accepte"),
        }))
    }

    async fn proposer(&self, proposition: &Reprogrammation) -> Result<bool, RepositoryError> {
        let ecrit = sqlx::query(
            "INSERT INTO reprogrammation (id, mission_id, devis_id, statut, proposee_le)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (mission_id) DO NOTHING
             RETURNING id",
        )
        .bind(proposition.id)
        .bind(proposition.mission_id)
        .bind(proposition.devis_id)
        .bind(proposition.statut.as_str())
        .bind(proposition.proposee_le)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }

    async fn par_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<Reprogrammation>, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT id, mission_id, devis_id, statut, proposee_le
             FROM reprogrammation WHERE mission_id = $1",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        ligne
            .map(|l| {
                let statut: String = l.get("statut");
                Ok(Reprogrammation {
                    id: l.get("id"),
                    mission_id: l.get("mission_id"),
                    devis_id: l.get("devis_id"),
                    statut: StatutReprogrammation::parse(&statut).ok_or_else(|| {
                        RepositoryError::Contrainte(format!("statut inconnu : {statut}"))
                    })?,
                    proposee_le: l.get("proposee_le"),
                })
            })
            .transpose()
    }

    async fn accepter(
        &self,
        mission_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<ResultatAcceptation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // **Le verrou d'abord, la bascule à la fin.** La contrainte de V28 lie
        // le statut `ACCEPTED` à la présence d'une nouvelle Mission, et
        // PostgreSQL la vérifie à chaque instruction : basculer le statut avant
        // d'avoir créé la Mission la violerait. `FOR UPDATE` tient la ligne
        // pendant ce temps, ce qui sérialise deux acceptations simultanées
        // aussi sûrement qu'un compare-and-swap.
        let prise = sqlx::query(
            "SELECT devis_id FROM reprogrammation
             WHERE mission_id = $1 AND statut = 'PROPOSED'
             FOR UPDATE",
        )
        .bind(mission_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        let Some(ligne) = prise else {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatAcceptation::DejaClose);
        };
        let devis_id: Uuid = ligne.get("devis_id");

        // La nouvelle intervention reprend la Demande et le prestataire de
        // l'ancienne. L'index partiel de V28 l'autorise puisque l'ancienne est
        // annulée ; celui de V13 refusera si le prestataire s'est engagé
        // ailleurs entre-temps.
        let nouvelle = Uuid::new_v4();
        let creation = sqlx::query(
            "INSERT INTO mission (id, demande_id, provider_id, statut, cree_le)
             SELECT $2, m.demande_id, m.provider_id, 'ACCEPTED', $3
             FROM mission m WHERE m.id = $1",
        )
        .bind(mission_id)
        .bind(nouvelle)
        .bind(maintenant)
        .execute(&mut *tx)
        .await;

        if let Err(e) = creation {
            let occupe = match &e {
                sqlx::Error::Database(db) => {
                    db.is_unique_violation() && db.constraint() == Some(INDEX_UNE_MISSION_A_LA_FOIS)
                }
                _ => false,
            };
            tx.rollback().await.map_err(erreur)?;
            return if occupe {
                Ok(ResultatAcceptation::ProviderOccupe)
            } else {
                Err(erreur(e))
            };
        }

        sqlx::query(
            "INSERT INTO mission_transition
                 (mission_id, provider_id, statut, horodate_le, enregistre_le, position, hors_zone)
             SELECT $1, m.provider_id, 'ACCEPTED', $2, $2, NULL, FALSE
             FROM mission m WHERE m.id = $1",
        )
        .bind(nouvelle)
        .bind(maintenant)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        // **Le devis est recopié, pas déplacé.** Celui de l'intervention
        // annulée reste attaché à elle : c'est lui qui explique ce qui avait été
        // convenu, et le déplacer réécrirait l'histoire de l'annulation.
        //
        // La copie naît acceptée : les deux parties se sont déjà mises d'accord
        // sur ce prix, et le refaire valider serait leur demander deux fois la
        // même chose.
        sqlx::query(
            "INSERT INTO devis (id, mission_id, provider_id, montant_htva_cents, taux_tva_bp,
                                tva_cents, total_ttc_cents, delai_minutes, note,
                                preuve_tva_reduite, statut, cree_le, expire_le)
             SELECT gen_random_uuid(), $2, d.provider_id, d.montant_htva_cents, d.taux_tva_bp,
                    d.tva_cents, d.total_ttc_cents, d.delai_minutes, d.note,
                    d.preuve_tva_reduite, 'ACCEPTED', $3, $3 + interval '1 hour'
             FROM devis d WHERE d.id = $1",
        )
        .bind(devis_id)
        .bind(nouvelle)
        .bind(maintenant)
        .execute(&mut *tx)
        .await
        .map_err(erreur)?;

        // Le statut et la Mission avancent **ensemble**, en une instruction :
        // c'est ce que la contrainte exige, et cela referme la fenêtre où une
        // proposition serait acceptée sans rien derrière.
        let bascule = sqlx::query(
            "UPDATE reprogrammation SET statut = 'ACCEPTED', nouvelle_mission_id = $2
             WHERE mission_id = $1 AND statut = 'PROPOSED'
             RETURNING id",
        )
        .bind(mission_id)
        .bind(nouvelle)
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        if bascule.is_none() {
            tx.rollback().await.map_err(erreur)?;
            return Ok(ResultatAcceptation::DejaClose);
        }

        notifier(
            &mut tx,
            &EvenementMission::statut(nouvelle, "ACCEPTED", maintenant),
        )
        .await?;

        tx.commit().await.map_err(erreur)?;
        Ok(ResultatAcceptation::Reprise {
            nouvelle_mission: nouvelle,
        })
    }

    async fn refuser(&self, mission_id: Uuid) -> Result<bool, RepositoryError> {
        let ecrit = sqlx::query(
            "UPDATE reprogrammation SET statut = 'DECLINED'
             WHERE mission_id = $1 AND statut = 'PROPOSED'
             RETURNING id",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ecrit.is_some())
    }
}
