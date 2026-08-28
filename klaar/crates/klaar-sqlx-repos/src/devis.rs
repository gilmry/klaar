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
use klaar_application::ports::evenements::EvenementMission;
use klaar_payment::{Devis, MotifRefus, StatutDevis};
use klaar_shared_kernel::{Money, VatRate};

use crate::pool::PoolPg;
use crate::{erreur, notifier};

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
                        statut, motif_refus, cree_le, expire_le";

pub struct PgDevisRepository {
    pool: PoolPg,
}

impl PgDevisRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }

    /// Le devis en attente, lu **dans la transaction en cours**.
    ///
    /// La même lecture que `en_cours_pour_mission`, mais sur la transaction
    /// plutôt que sur le pool : passer par une seconde connexion depuis
    /// l'intérieur d'une transaction ouverte donnerait une réponse prise à un
    /// autre instant, et c'est précisément l'ordre des deux refus qui se joue
    /// ici.
    async fn en_cours_pour_mission_dans(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        mission_id: Uuid,
    ) -> Result<Option<Devis>, RepositoryError> {
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM devis WHERE mission_id = $1 AND statut = 'SENT'"
        ))
        .bind(mission_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
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
        motif_refus: ligne
            .get::<Option<String>, _>("motif_refus")
            .as_deref()
            .and_then(MotifRefus::parse),
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
        // Une transaction pour deux écritures : l'insertion et l'avis temps
        // réel. PostgreSQL ne délivre un `NOTIFY` qu'au `COMMIT`, donc un devis
        // refusé n'annonce rien et un devis écrit annonce toujours.
        let mut tx = self.pool.begin().await.map_err(erreur)?;

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
        .fetch_optional(&mut *tx)
        .await;

        match ecrit {
            Ok(Some(_)) => {
                notifier(
                    &mut tx,
                    &EvenementMission::devis_emis(devis.mission_id, devis.cree_le),
                )
                .await?;
                tx.commit().await.map_err(erreur)?;
                Ok(ResultatEmission::Emis(devis.clone()))
            }
            Ok(None) => {
                // Aucune ligne : le plafond était atteint. **Mais l'ordre des
                // deux refus compte.** PostgreSQL évalue le `WHERE` avant
                // l'index, donc trois devis dont un attend encore une réponse
                // rendraient « plafond » — et l'appelant annulerait la Mission
                // alors qu'une offre vivante est sur la table du demandeur.
                // Une requête de plus, sur le seul chemin de refus, remet les
                // deux dans le bon ordre.
                let vivant = self
                    .en_cours_pour_mission_dans(&mut tx, devis.mission_id)
                    .await?;
                // Rien n'a été écrit : la transaction se ferme explicitement
                // plutôt que de mourir avec la variable, ce qui se relit mieux.
                tx.rollback().await.map_err(erreur)?;
                if vivant.is_some() {
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
                tx.rollback().await.map_err(erreur)?;
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

    async fn repondre(
        &self,
        devis_id: Uuid,
        reponse: StatutDevis,
        motif: Option<&str>,
    ) -> Result<bool, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        // La garde `statut = 'SENT'` **et** l'heure de validité, dans la même
        // instruction : deux « accepter » simultanés ne doivent pas tous deux
        // aboutir, et un devis que le balayage vient d'expirer ne doit pas
        // s'accepter entre la lecture et l'écriture.
        let ecrit = sqlx::query(
            "UPDATE devis SET statut = $2, motif_refus = $3
             WHERE id = $1 AND statut = 'SENT' AND expire_le > now()
             RETURNING mission_id",
        )
        .bind(devis_id)
        .bind(reponse.as_str())
        .bind(motif)
        .fetch_optional(&mut *tx)
        .await
        .map_err(erreur)?;

        let Some(ligne) = ecrit else {
            tx.rollback().await.map_err(erreur)?;
            return Ok(false);
        };

        // L'avis prévient le prestataire, qui attend cette réponse. Même
        // transaction : une réponse écrite sans avis le laisserait devant un
        // écran qui ne bouge pas.
        let mission_id: Uuid = ligne.get("mission_id");
        notifier(
            &mut tx,
            &EvenementMission::devis_repondu(mission_id, reponse.as_str(), Utc::now()),
        )
        .await?;

        tx.commit().await.map_err(erreur)?;
        Ok(true)
    }

    async fn par_id(&self, devis_id: Uuid) -> Result<Option<Devis>, RepositoryError> {
        let ligne = sqlx::query(&format!("SELECT {COLONNES} FROM devis WHERE id = $1"))
            .bind(devis_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn expirer_les_echus(
        &self,
        maintenant: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<Devis>, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;
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
        .fetch_all(&mut *tx)
        .await
        .map_err(erreur)?;

        let eteints: Vec<Devis> = lignes.iter().map(depuis_ligne).collect::<Result<_, _>>()?;

        // Un avis par devis éteint, dans la même transaction que l'extinction :
        // deux passages du balayage ne peuvent donc pas annoncer deux fois la
        // même expiration.
        for devis in &eteints {
            notifier(
                &mut tx,
                &EvenementMission::devis_expire(devis.mission_id, maintenant),
            )
            .await?;
        }

        tx.commit().await.map_err(erreur)?;
        Ok(eteints)
    }
}
