//! Dépôt PostgreSQL des Devis (Story 4.1, FR-016).
//!
//! **Les deux règles de comptage sont dans la base, et pas au-dessus.** « Un
//! seul devis en attente par Mission » et « trois devis au maximum » portent
//! sur des lignes que d'autres transactions écrivent au même moment : lire puis
//! décider laisserait deux envois simultanés poser deux devis, et le demandeur
//! verrait deux prix sans savoir lequel l'engage.
//!
//! Le plafond est un `WHERE` sur un `INSERT … SELECT`, donc évalué au moment de
//! l'écriture ; l'unicité est un index partiel. Les deux se complètent : la
//! course sur le comptage ne peut pas produire un quatrième devis, puisqu'un
//! quatrième suppose que les trois premiers soient déjà terminés, et que deux
//! devis en attente à la fois sont refusés par l'index.

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::devis_repository::{DevisRepository, ResultatEmission};
use klaar_application::ports::erreurs::RepositoryError;
use klaar_payment::{Devis, StatutDevis};
use klaar_shared_kernel::{Money, VatRate};

use crate::erreur;
use crate::pool::PoolPg;

/// Nom de l'index qui tient « un seul devis en attente » (migration V19).
///
/// Comparé plutôt que deviné : une autre violation d'unicité sur cette table
/// signalerait un tout autre problème, et la traduire en « devis déjà en
/// cours » enverrait chercher la panne au mauvais endroit.
const INDEX_UN_SEUL_EN_COURS: &str = "devis_un_seul_en_cours_idx";

/// Colonnes lues, dans l'ordre attendu par `depuis_ligne`.
///
/// Une constante plutôt que trois listes recopiées : une colonne ajoutée à la
/// lecture sans l'être au décodage produit une erreur au premier appel, alors
/// qu'une liste divergente entre deux requêtes ne se voit qu'en production.
const COLONNES: &str = "id, mission_id, provider_id, montant_htva_cents, taux_tva_bp, \
                        tva_cents, total_ttc_cents, delai_minutes, note, preuve_tva_reduite, \
                        statut, cree_le, expire_le";

pub struct PgDevisRepository {
    pool: PoolPg,
}

impl PgDevisRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<Devis, RepositoryError> {
    let statut: String = ligne.get("statut");
    let taux_bp: i16 = ligne.get("taux_tva_bp");
    Ok(Devis {
        id: ligne.get("id"),
        mission_id: ligne.get("mission_id"),
        provider_id: ligne.get("provider_id"),
        montant_htva: Money::from_cents(ligne.get("montant_htva_cents")),
        // Le taux est relu tel qu'il a été écrit, jamais redéduit d'une table
        // de taux courants : un devis relu dans deux ans doit montrer ce qui a
        // été présenté ce jour-là.
        taux_tva: VatRate::from_basis_points(taux_bp.max(0) as u16).map_err(|_| {
            RepositoryError::Contrainte(format!("taux de TVA hors bornes : {taux_bp}"))
        })?,
        tva: Money::from_cents(ligne.get("tva_cents")),
        total_ttc: Money::from_cents(ligne.get("total_ttc_cents")),
        delai_minutes: i64::from(ligne.get::<i32, _>("delai_minutes")),
        note: ligne.get("note"),
        preuve_tva_reduite: ligne.get("preuve_tva_reduite"),
        statut: StatutDevis::parse(&statut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {statut}")))?,
        cree_le: ligne.get("cree_le"),
        expire_le: ligne.get("expire_le"),
    })
}

impl DevisRepository for PgDevisRepository {
    async fn emettre(
        &self,
        devis: &Devis,
        plafond: usize,
    ) -> Result<ResultatEmission, RepositoryError> {
        // Le comptage est **dans** l'instruction d'écriture. `RETURNING` sert
        // de témoin : aucune ligne rendue signifie que le plafond était atteint
        // au moment où cette transaction a pu écrire, quel que soit ce qu'une
        // lecture antérieure avait vu.
        let ecrit = sqlx::query(
            "INSERT INTO devis (id, mission_id, provider_id, montant_htva_cents, taux_tva_bp,
                                tva_cents, total_ttc_cents, delai_minutes, note,
                                preuve_tva_reduite, statut, cree_le, expire_le)
             SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
             WHERE (SELECT count(*) FROM devis WHERE mission_id = $2) < $14
             RETURNING id",
        )
        .bind(devis.id)
        .bind(devis.mission_id)
        .bind(devis.provider_id)
        .bind(devis.montant_htva.cents())
        .bind(i16::try_from(devis.taux_tva.basis_points()).unwrap_or(i16::MAX))
        .bind(devis.tva.cents())
        .bind(devis.total_ttc.cents())
        .bind(i32::try_from(devis.delai_minutes).unwrap_or(i32::MAX))
        .bind(devis.note.as_deref())
        .bind(devis.preuve_tva_reduite.as_deref())
        .bind(devis.statut.as_str())
        .bind(devis.cree_le)
        .bind(devis.expire_le)
        .bind(i64::try_from(plafond).unwrap_or(i64::MAX))
        .fetch_optional(&self.pool)
        .await;

        match ecrit {
            Ok(Some(_)) => Ok(ResultatEmission::Emis(devis.clone())),
            Ok(None) => {
                // Aucune ligne : le plafond était atteint. **Mais l'ordre des
                // deux refus compte.** PostgreSQL évalue le `WHERE` avant
                // l'index, donc trois devis dont un attend encore une réponse
                // rendraient « plafond » — et l'appelant annulerait la Mission
                // alors qu'une offre vivante est sur la table du demandeur.
                // Une requête de plus, sur le seul chemin de refus, remet les
                // deux dans le bon ordre.
                if self
                    .en_cours_pour_mission(devis.mission_id)
                    .await?
                    .is_some()
                {
                    return Ok(ResultatEmission::DejaEnCours);
                }
                Ok(ResultatEmission::PlafondAtteint)
            }
            Err(e) => {
                let deja = match &e {
                    sqlx::Error::Database(db) => {
                        db.is_unique_violation() && db.constraint() == Some(INDEX_UN_SEUL_EN_COURS)
                    }
                    _ => false,
                };
                if deja {
                    Ok(ResultatEmission::DejaEnCours)
                } else {
                    Err(erreur(e))
                }
            }
        }
    }

    async fn en_cours_pour_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<Devis>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM devis WHERE mission_id = $1 AND statut = 'SENT'"
        ))
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn dernier_pour_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<Devis>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM devis WHERE mission_id = $1
             ORDER BY cree_le DESC, id DESC LIMIT 1"
        ))
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn compter_pour_mission(&self, mission_id: Uuid) -> Result<usize, RepositoryError> {
        let ligne = sqlx::query("SELECT count(*) AS total FROM devis WHERE mission_id = $1")
            .bind(mission_id)
            .fetch_one(&self.pool)
            .await
            .map_err(erreur)?;
        let total: i64 = ligne.get("total");
        Ok(usize::try_from(total).unwrap_or(usize::MAX))
    }

    async fn expirer_les_echus(
        &self,
        maintenant: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<Devis>, RepositoryError> {
        // Sélection et extinction en une seule instruction, avec
        // `FOR UPDATE SKIP LOCKED` : deux balayages simultanés ne peuvent pas
        // éteindre le même devis, donc ne peuvent pas prévenir deux fois le
        // même prestataire.
        let lignes = sqlx::query(&format!(
            "WITH echus AS (
                 SELECT id FROM devis
                 WHERE statut = 'SENT' AND expire_le <= $1
                 ORDER BY expire_le
                 LIMIT $2
                 FOR UPDATE SKIP LOCKED
             )
             UPDATE devis SET statut = 'EXPIRED'
             WHERE id IN (SELECT id FROM echus)
             RETURNING {COLONNES}"
        ))
        .bind(maintenant)
        .bind(limite)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        lignes.iter().map(depuis_ligne).collect()
    }
}
