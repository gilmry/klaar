//! Dépôt PostgreSQL de la notation et de la réputation (Story 7.1, FR-033, FR-037).

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use klaar_application::ports::erreurs::RepositoryError;
use klaar_application::ports::notation_repository::{
    NotationRepository, NotesDeMission, Reputation, ResultatNotation,
};
use klaar_trust::{Cible, Notation};

use crate::erreur;
use crate::pool::PoolPg;

/// Nom de la contrainte qui tient « une note par côté » (migration V24).
///
/// Comparé plutôt que deviné : une autre violation d'unicité signalerait un
/// tout autre problème.
const CONTRAINTE_UNE_PAR_COTE: &str = "notation_une_par_cote";

const COLONNES: &str = "id, mission_id, auteur_id, cible, note, commentaire, cree_le";

pub struct PgNotationRepository {
    pool: PoolPg,
}

impl PgNotationRepository {
    pub fn new(pool: PoolPg) -> Self {
        Self { pool }
    }
}

fn depuis_ligne(ligne: &sqlx::postgres::PgRow) -> Result<Notation, RepositoryError> {
    let cible: String = ligne.get("cible");
    let note: i16 = ligne.get("note");
    Ok(Notation {
        id: ligne.get("id"),
        mission_id: ligne.get("mission_id"),
        // `auteur_id` est nullable depuis l'anonymisation RGPD : une note dont
        // l'auteur a été effacé reste lisible, sans nom. L'identifiant nul est
        // remplacé par le nul d'UUID, qui ne désigne personne.
        auteur_id: ligne
            .get::<Option<Uuid>, _>("auteur_id")
            .unwrap_or(Uuid::nil()),
        cible: Cible::parse(&cible)
            .ok_or_else(|| RepositoryError::Contrainte(format!("cible inconnue : {cible}")))?,
        note: u8::try_from(note)
            .map_err(|_| RepositoryError::Contrainte(format!("note hors échelle : {note}")))?,
        commentaire: ligne.get("commentaire"),
        cree_le: ligne.get("cree_le"),
    })
}

impl NotationRepository for PgNotationRepository {
    async fn noter(&self, notation: &Notation) -> Result<ResultatNotation, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(erreur)?;

        let ecrit = sqlx::query(
            "INSERT INTO notation (id, mission_id, auteur_id, cible, note, commentaire, cree_le)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(notation.id)
        .bind(notation.mission_id)
        .bind(notation.auteur_id)
        .bind(notation.cible.as_str())
        .bind(i16::from(notation.note))
        .bind(notation.commentaire.as_deref())
        .bind(notation.cree_le)
        .execute(&mut *tx)
        .await;

        if let Err(e) = ecrit {
            let deja = match &e {
                sqlx::Error::Database(db) => {
                    db.is_unique_violation() && db.constraint() == Some(CONTRAINTE_UNE_PAR_COTE)
                }
                _ => false,
            };
            tx.rollback().await.map_err(erreur)?;
            return if deja {
                Ok(ResultatNotation::DejaNotee)
            } else {
                Err(erreur(e))
            };
        }

        // L'agrégat n'avance que pour les notes **du prestataire** : c'est la
        // seule réputation que le matching consulte. Celle du demandeur est
        // lisible sur ses interventions, et n'entre dans aucun classement — la
        // classer serait décider qui mérite d'être dépanné.
        if notation.cible == Cible::Prestataire {
            sqlx::query(
                "INSERT INTO reputation_provider
                     (provider_id, somme_notes, nombre_notes, mise_a_jour_le)
                 SELECT m.provider_id, $2, 1, $3 FROM mission m WHERE m.id = $1
                 ON CONFLICT (provider_id) DO UPDATE
                     SET somme_notes = reputation_provider.somme_notes + EXCLUDED.somme_notes,
                         nombre_notes = reputation_provider.nombre_notes + 1,
                         mise_a_jour_le = EXCLUDED.mise_a_jour_le",
            )
            .bind(notation.mission_id)
            .bind(i32::from(notation.note))
            .bind(notation.cree_le)
            .execute(&mut *tx)
            .await
            .map_err(erreur)?;
        }

        tx.commit().await.map_err(erreur)?;
        Ok(ResultatNotation::Ecrite(notation.clone()))
    }

    async fn notes_de_mission(&self, mission_id: Uuid) -> Result<NotesDeMission, RepositoryError> {
        let lignes = sqlx::query(&format!(
            "SELECT {COLONNES} FROM notation WHERE mission_id = $1"
        ))
        .bind(mission_id)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        let mut notes = NotesDeMission::default();
        for ligne in &lignes {
            let notation = depuis_ligne(ligne)?;
            match notation.cible {
                Cible::Prestataire => notes.sur_le_prestataire = Some(notation),
                Cible::Demandeur => notes.sur_le_demandeur = Some(notation),
            }
        }
        Ok(notes)
    }

    async fn reputation(&self, provider_id: Uuid) -> Result<Reputation, RepositoryError> {
        let ligne = sqlx::query(
            "SELECT somme_notes, nombre_notes FROM reputation_provider WHERE provider_id = $1",
        )
        .bind(provider_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;

        // Aucune ligne : personne n'a encore noté. Zéro partout, et c'est à
        // l'appelant de dire « pas encore noté » plutôt que « zéro étoile ».
        Ok(ligne
            .map(|l| Reputation {
                somme_notes: u32::try_from(l.get::<i32, _>("somme_notes")).unwrap_or(0),
                nombre_notes: u32::try_from(l.get::<i32, _>("nombre_notes")).unwrap_or(0),
            })
            .unwrap_or_default())
    }

    async fn reputations_de(
        &self,
        provider_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Reputation>, RepositoryError> {
        if provider_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let lignes = sqlx::query(
            "SELECT provider_id, somme_notes, nombre_notes
             FROM reputation_provider WHERE provider_id = ANY($1)",
        )
        .bind(provider_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(erreur)?;

        Ok(lignes
            .iter()
            .map(|l| {
                (
                    l.get::<Uuid, _>("provider_id"),
                    Reputation {
                        somme_notes: u32::try_from(l.get::<i32, _>("somme_notes")).unwrap_or(0),
                        nombre_notes: u32::try_from(l.get::<i32, _>("nombre_notes")).unwrap_or(0),
                    },
                )
            })
            .collect())
    }

    async fn validee_le(&self, mission_id: Uuid) -> Result<Option<DateTime<Utc>>, RepositoryError> {
        // La date vient de l'historique : c'est la seule source qui dise
        // **quand** la validation a eu lieu, et c'est de là que court la
        // fenêtre de quatorze jours.
        let ligne = sqlx::query(
            "SELECT t.horodate_le
             FROM mission m
             JOIN mission_transition t ON t.mission_id = m.id AND t.statut = 'VALIDATED'
             WHERE m.id = $1 AND m.statut = 'VALIDATED'
             ORDER BY t.horodate_le
             LIMIT 1",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(erreur)?;
        Ok(ligne.map(|l| l.get("horodate_le")))
    }
}
