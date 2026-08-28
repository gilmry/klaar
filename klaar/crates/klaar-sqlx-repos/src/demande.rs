//! Dépôt PostgreSQL des Demandes (Story 3.1, FR-011).

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::demande_repository::{DemandeRepository, PaiementRepository};
use klaar_application::ports::erreurs::RepositoryError;
use klaar_catalog::CodeCatalogue;
use klaar_matching::{Demande, MotifAnnulation, StatutDemande, Urgence, FENETRE_DOUBLON_MINUTES};
use klaar_shared_kernel::Geo;

use crate::erreur;
use crate::pool::PoolPg;

pub struct PgDemandeRepository {
    pool: PoolPg,
}

impl PgDemandeRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

/// Colonnes lues, avec la position décomposée en deux réels.
///
/// `ST_Y`/`ST_X` plutôt que le type `geography` brut : sqlx ne sait pas le
/// décoder sans dépendance supplémentaire, et deux réels suffisent au domaine,
/// qui ne connaît que `Geo`.
const COLONNES: &str = "id, demandeur_id, secteur_code, description, urgence, statut, \
     rayon_metres, elargissements, diffuse_depuis, motif_annulation, cree_le, \
     ST_Y(position::geometry) AS lat, ST_X(position::geometry) AS lon";

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<Demande, RepositoryError> {
    let secteur: String = ligne.get("secteur_code");
    let urgence: String = ligne.get("urgence");
    let statut: String = ligne.get("statut");
    let (lat, lon): (f64, f64) = (ligne.get("lat"), ligne.get("lon"));

    Ok(Demande {
        id: ligne.get("id"),
        demandeur_id: ligne.get("demandeur_id"),
        secteur: CodeCatalogue::parse(&secteur)
            .map_err(|e| RepositoryError::Contrainte(format!("secteur illisible : {e}")))?,
        description: ligne.get("description"),
        position: Geo::new(lat, lon)
            .map_err(|e| RepositoryError::Contrainte(format!("position illisible : {e:?}")))?,
        urgence: Urgence::parse(&urgence)
            .ok_or_else(|| RepositoryError::Contrainte(format!("urgence inconnue : {urgence}")))?,
        statut: StatutDemande::parse(&statut)
            .ok_or_else(|| RepositoryError::Contrainte(format!("statut inconnu : {statut}")))?,
        rayon_metres: ligne.get("rayon_metres"),
        // `SMALLINT` se lit en `i16` ; le domaine compte en `u8` parce qu'un
        // nombre d'élargissements négatif n'existe pas. La conversion est
        // bornée par la contrainte de la base.
        elargissements: ligne.get::<i16, _>("elargissements").clamp(0, 255) as u8,
        diffuse_depuis: ligne.get("diffuse_depuis"),
        motif_annulation: ligne
            .get::<Option<String>, _>("motif_annulation")
            .as_deref()
            .map(|m| {
                MotifAnnulation::parse(m)
                    .ok_or_else(|| RepositoryError::Contrainte(format!("motif inconnu : {m}")))
            })
            .transpose()?,
        cree_le: ligne.get("cree_le"),
    })
}

impl DemandeRepository for PgDemandeRepository {
    async fn creer(&self, demande: &Demande) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO demande
                 (id, demandeur_id, secteur_code, description, position, urgence, statut,
                  rayon_metres, elargissements, diffuse_depuis, cree_le)
             VALUES ($1, $2, $3, $4, ST_SetSRID(ST_MakePoint($5, $6), 4326)::geography,
                     $7, $8, $9, $10, $11, $12)",
        )
        .bind(demande.id)
        .bind(demande.demandeur_id)
        .bind(demande.secteur.as_str())
        .bind(&demande.description)
        // `ST_MakePoint` prend la longitude d'abord : c'est l'ordre X, Y, et
        // l'inverser place Bruxelles au large de la Somalie sans qu'aucune
        // contrainte ne s'en aperçoive.
        .bind(demande.position.lon())
        .bind(demande.position.lat())
        .bind(demande.urgence.as_str())
        .bind(demande.statut.as_str())
        .bind(demande.rayon_metres)
        .bind(i16::from(demande.elargissements))
        .bind(demande.diffuse_depuis)
        .bind(demande.cree_le)
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(())
    }

    async fn par_id(&self, id: Uuid) -> Result<Option<Demande>, RepositoryError> {
        let ligne = sqlx::query(&format!("SELECT {COLONNES} FROM demande WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(erreur)?;
        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn doublon_recent(
        &self,
        demandeur_id: Uuid,
        secteur: &CodeCatalogue,
        position: Geo,
        maintenant: DateTime<Utc>,
    ) -> Result<Option<Demande>, RepositoryError> {
        // La proximité est tranchée par PostGIS, en mètres : le domaine
        // raisonne en degrés, ce qui est une approximation acceptable pour une
        // comparaison mais pas pour une requête. Cent mètres correspondent à la
        // tolérance du domaine sous nos latitudes.
        let ligne = sqlx::query(&format!(
            "SELECT {COLONNES} FROM demande
             WHERE demandeur_id = $1
               AND secteur_code = $2
               AND statut = 'BROADCASTING'
               AND cree_le > $3
               AND ST_DWithin(position, ST_SetSRID(ST_MakePoint($4, $5), 4326)::geography, 100)
             ORDER BY cree_le DESC
             LIMIT 1"
        ))
        .bind(demandeur_id)
        .bind(secteur.as_str())
        .bind(maintenant - Duration::minutes(FENETRE_DOUBLON_MINUTES))
        .bind(position.lon())
        .bind(position.lat())
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        ligne.as_ref().map(depuis_ligne).transpose()
    }

    async fn changer_statut(
        &self,
        id: Uuid,
        statut: StatutDemande,
        _maintenant: DateTime<Utc>,
    ) -> Result<(), RepositoryError> {
        // `statut = 'BROADCASTING'` en garde : une Demande annulée par son
        // auteur pendant qu'un tour de matching tourne ne doit pas revenir en
        // arrière parce que ce tour s'est terminé sans candidat.
        sqlx::query("UPDATE demande SET statut = $1 WHERE id = $2 AND statut = 'BROADCASTING'")
            .bind(statut.as_str())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(erreur)?;
        Ok(())
    }

    async fn expirer_echues(
        &self,
        avant: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<Demande>, RepositoryError> {
        // Sélection et écriture en une seule instruction : deux balayages
        // concurrents ne peuvent pas éteindre la même Demande, donc pas
        // notifier deux fois le même demandeur. Un `SELECT` puis un `UPDATE`
        // laisserait cette fenêtre ouverte.
        let lignes = sqlx::query(&format!(
            "UPDATE demande SET statut = 'NO_MATCH'
             WHERE id IN (
                 SELECT id FROM demande
                 WHERE statut = 'BROADCASTING' AND diffuse_depuis <= $1
                 ORDER BY diffuse_depuis
                 LIMIT $2
                 -- Sans ce saut, deux balayages simultanés s'attendraient l'un
                 -- l'autre sur les mêmes lignes au lieu de se partager le
                 -- travail.
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING {COLONNES}"
        ))
        .bind(avant)
        .bind(limite)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        lignes.iter().map(depuis_ligne).collect()
    }

    async fn relancer(&self, demande: &Demande) -> Result<bool, RepositoryError> {
        // Compare-and-swap sur le compteur d'élargissements. Le statut ne
        // suffirait pas : une Demande échue que le balayage n'a pas encore
        // touchée est toujours `BROADCASTING`, et le demandeur a le droit de
        // l'élargir sans attendre le prochain passage. Le compteur, lui,
        // distingue toujours deux clics successifs — le second présenterait la
        // même valeur attendue que le premier vient de consommer.
        //
        // Le filtre sur le statut reste, mais pour une autre raison : une
        // Demande attribuée ou annulée entre la lecture et l'écriture ne doit
        // pas repartir en diffusion.
        let resultat = sqlx::query(
            "UPDATE demande
             SET statut = $1, rayon_metres = $2, elargissements = $3, diffuse_depuis = $4
             WHERE id = $5
               AND statut IN ('BROADCASTING', 'NO_MATCH')
               AND elargissements = $6",
        )
        .bind(demande.statut.as_str())
        .bind(demande.rayon_metres)
        .bind(i16::from(demande.elargissements))
        .bind(demande.diffuse_depuis)
        .bind(demande.id)
        // La valeur d'où part cet élargissement : le domaine vient de
        // l'incrémenter, la garde porte donc sur celle d'avant.
        .bind(i16::from(demande.elargissements.saturating_sub(1)))
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(resultat.rows_affected() > 0)
    }

    async fn annuler(
        &self,
        id: Uuid,
        motif: Option<MotifAnnulation>,
    ) -> Result<bool, RepositoryError> {
        // Deux statuts de départ, et pas un : le quatrième élargissement refusé
        // annule une Demande qui est en `NO_MATCH`, pas en diffusion.
        // `MATCHED` en est exclu — à ce stade, c'est la Mission qu'il faut
        // annuler, et effacer la Demande laisserait un prestataire en route
        // sans que rien ne le dise.
        let resultat = sqlx::query(
            "UPDATE demande SET statut = 'CANCELLED', motif_annulation = $2
             WHERE id = $1 AND statut IN ('BROADCASTING', 'NO_MATCH')",
        )
        .bind(id)
        .bind(motif.map(|m| m.as_str()))
        .execute(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(resultat.rows_affected() > 0)
    }

    async fn compter_depuis_une_heure(
        &self,
        demandeur_id: Uuid,
        maintenant: DateTime<Utc>,
    ) -> Result<i64, RepositoryError> {
        // Toutes les Demandes comptent, y compris annulées : sinon, annuler
        // remettrait le compteur à zéro et le quota ne tiendrait plus.
        sqlx::query_scalar("SELECT COUNT(*) FROM demande WHERE demandeur_id = $1 AND cree_le > $2")
            .bind(demandeur_id)
            .bind(maintenant - Duration::hours(1))
            .fetch_one(&self.pool)
            .await
            .map_err(erreur)
    }
}

/// Vérification de la méthode de paiement (FR-011, précondition).
///
/// Lit la table `methode_paiement`, qui **reste vide** tant que la Story 1.7
/// n'est pas livrée — l'enregistrement d'une carte passe par Stripe, hors du
/// périmètre vitrine. Le contrôle est donc désactivé par configuration dans ce
/// déploiement, et ce dépôt répondra `false` à tout le monde jusque-là.
pub struct PgPaiementRepository {
    pool: PoolPg,
}

impl PgPaiementRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

impl PaiementRepository for PgPaiementRepository {
    async fn possede_methode(&self, utilisateur_id: Uuid) -> Result<bool, RepositoryError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM methode_paiement WHERE utilisateur_id = $1)",
        )
        .bind(utilisateur_id)
        .fetch_one(&self.pool)
        .await
        .map_err(erreur)
    }
}
