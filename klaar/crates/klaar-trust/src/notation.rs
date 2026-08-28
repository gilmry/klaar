//! Notation double sens après intervention (FR-033, Story 7.1).
//!
//! **Les deux notes sont publiées ensemble, ou pas du tout.** C'est la seule
//! protection contre les représailles : si la note du demandeur s'affichait
//! avant celle du prestataire, celui-ci ajusterait la sienne en conséquence, et
//! les deux perdraient toute valeur. Elles se dévoilent donc quand les deux
//! existent, ou quand la fenêtre se ferme — celui qui n'a pas noté a eu deux
//! semaines pour le faire.
//!
//! **Quatorze jours, puis c'est clos.** Une note écrite trois mois après ne dit
//! plus rien de l'intervention, et laisser la fenêtre ouverte permettrait de
//! faire pression bien après coup.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Note minimale : une étoile. Zéro n'existe pas — une intervention faite n'est
/// pas rien, et l'échec total relève du litige, pas de la notation.
pub const NOTE_MIN: u8 = 1;

/// Note maximale : cinq étoiles.
pub const NOTE_MAX: u8 = 5;

/// Longueur du commentaire (FR-033 `@negative`).
pub const COMMENTAIRE_MAX_CARACTERES: usize = 500;

/// Fenêtre de notation après validation, en jours (FR-033 `@edge`).
pub const FENETRE_NOTATION_JOURS: i64 = 14;

/// Qui est noté.
///
/// **La notation est symétrique**, et c'est voulu : un demandeur qui n'ouvre
/// jamais, qui décrit tout de travers ou qui conteste par principe rend le
/// travail impossible, et le prestataire suivant a le droit de le savoir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cible {
    Prestataire,
    Demandeur,
}

impl Cible {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prestataire => "PROVIDER",
            Self::Demandeur => "USER",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "PROVIDER" => Some(Self::Prestataire),
            "USER" => Some(Self::Demandeur),
            _ => None,
        }
    }

    /// L'autre côté. Sert à savoir quelle note attendre pour publier.
    pub fn reciproque(&self) -> Self {
        match self {
            Self::Prestataire => Self::Demandeur,
            Self::Demandeur => Self::Prestataire,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotationError {
    /// Note hors de l'échelle d'une à cinq étoiles (FR-033 `@negative`).
    NoteHorsEchelle,
    CommentaireTropLong,
    /// La fenêtre de quatorze jours est fermée (FR-033 `@edge`).
    FenetreFermee,
}

impl NotationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoteHorsEchelle => "RATING_OUT_OF_RANGE",
            Self::CommentaireTropLong => "COMMENT_TOO_LONG",
            Self::FenetreFermee => "RATING_WINDOW_CLOSED",
        }
    }
}

impl fmt::Display for NotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoteHorsEchelle => write!(f, "la note va de {NOTE_MIN} à {NOTE_MAX} étoiles"),
            Self::CommentaireTropLong => write!(
                f,
                "commentaire au-delà de {COMMENTAIRE_MAX_CARACTERES} caractères"
            ),
            Self::FenetreFermee => write!(
                f,
                "la notation est close {FENETRE_NOTATION_JOURS} jours après l'intervention"
            ),
        }
    }
}

impl std::error::Error for NotationError {}

/// Une note, telle qu'elle sera consignée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notation {
    pub id: Uuid,
    pub mission_id: Uuid,
    /// Le compte qui note.
    pub auteur_id: Uuid,
    /// Qui est noté. Déduit de l'auteur, jamais reçu : sans cela, quelqu'un
    /// pourrait se noter lui-même.
    pub cible: Cible,
    pub note: u8,
    pub commentaire: Option<String>,
    pub cree_le: DateTime<Utc>,
}

impl Notation {
    /// Écrit une note, ou dit pourquoi elle est refusée.
    ///
    /// `validee_le` est l'instant où l'intervention a été validée : c'est de là
    /// que court la fenêtre, et non de la fin déclarée par le prestataire — le
    /// demandeur doit avoir eu l'occasion de constater le travail avant que le
    /// compte à rebours ne démarre.
    pub fn emettre(
        mission_id: Uuid,
        auteur_id: Uuid,
        cible: Cible,
        note: u8,
        commentaire: Option<String>,
        validee_le: DateTime<Utc>,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, NotationError> {
        if !(NOTE_MIN..=NOTE_MAX).contains(&note) {
            return Err(NotationError::NoteHorsEchelle);
        }
        if maintenant >= echeance_notation(validee_le) {
            return Err(NotationError::FenetreFermee);
        }

        let commentaire = match commentaire {
            None => None,
            Some(brut) => {
                let coupe = brut.trim();
                if coupe.is_empty() {
                    // Un commentaire vide et un commentaire absent sont la même
                    // chose ; les distinguer afficherait un cadre vide.
                    None
                } else if coupe.chars().count() > COMMENTAIRE_MAX_CARACTERES {
                    return Err(NotationError::CommentaireTropLong);
                } else {
                    Some(coupe.to_string())
                }
            }
        };

        Ok(Self {
            id: Uuid::new_v4(),
            mission_id,
            auteur_id,
            cible,
            note,
            commentaire,
            cree_le: maintenant,
        })
    }
}

/// Instant où la notation se ferme.
pub fn echeance_notation(validee_le: DateTime<Utc>) -> DateTime<Utc> {
    validee_le + Duration::days(FENETRE_NOTATION_JOURS)
}

/// Les deux notes d'une intervention sont-elles publiables (FR-033 `@happy`) ?
///
/// **Ensemble ou pas du tout.** Publier la première dès son écriture laisserait
/// l'autre partie ajuster la sienne : les deux notes perdraient toute valeur.
/// La fermeture de la fenêtre publie ce qui existe — celui qui n'a pas noté a
/// eu deux semaines.
pub fn publiables(
    les_deux_presentes: bool,
    validee_le: DateTime<Utc>,
    maintenant: DateTime<Utc>,
) -> bool {
    les_deux_presentes || maintenant >= echeance_notation(validee_le)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn emettre(note: u8, quand: DateTime<Utc>) -> Result<Notation, NotationError> {
        Notation::emettre(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Cible::Prestataire,
            note,
            Some("Intervention parfaite".to_string()),
            t0(),
            quand,
        )
    }

    // === @happy ===

    #[test]
    fn happy_une_note_de_cinq_etoiles_est_acceptee() {
        let n = emettre(5, t0()).unwrap();
        assert_eq!(n.note, 5);
        assert_eq!(n.commentaire.as_deref(), Some("Intervention parfaite"));
        assert_eq!(n.cible, Cible::Prestataire);
    }

    #[test]
    fn happy_la_notation_est_symetrique() {
        // Un demandeur qui n'ouvre jamais rend le travail impossible, et le
        // prestataire suivant a le droit de le savoir.
        assert_eq!(Cible::Prestataire.reciproque(), Cible::Demandeur);
        assert_eq!(Cible::Demandeur.reciproque(), Cible::Prestataire);
    }

    // === @negative ===

    #[test]
    fn negative_une_note_hors_echelle_est_refusee() {
        // FR-033 `@negative` : ni zéro ni six.
        for note in [0, 6, 10, u8::MAX] {
            assert_eq!(
                emettre(note, t0()),
                Err(NotationError::NoteHorsEchelle),
                "note {note}"
            );
        }
    }

    #[test]
    fn negative_un_commentaire_trop_long_est_refuse() {
        let refus = Notation::emettre(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Cible::Prestataire,
            5,
            Some("x".repeat(COMMENTAIRE_MAX_CARACTERES + 1)),
            t0(),
            t0(),
        );
        assert_eq!(refus, Err(NotationError::CommentaireTropLong));
    }

    // === @edge ===

    #[test]
    fn edge_la_notation_se_ferme_au_quatorzieme_jour() {
        // FR-033 `@edge` : 410 après quinze jours.
        let dedans = t0() + Duration::days(FENETRE_NOTATION_JOURS) - Duration::seconds(1);
        assert!(emettre(5, dedans).is_ok());

        let dehors = t0() + Duration::days(FENETRE_NOTATION_JOURS);
        assert_eq!(emettre(5, dehors), Err(NotationError::FenetreFermee));
    }

    #[test]
    fn edge_les_deux_bornes_de_l_echelle_passent() {
        for note in [NOTE_MIN, NOTE_MAX] {
            assert!(emettre(note, t0()).is_ok(), "note {note}");
        }
    }

    #[test]
    fn edge_un_commentaire_vide_vaut_un_commentaire_absent() {
        let n = Notation::emettre(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Cible::Prestataire,
            4,
            Some("   ".to_string()),
            t0(),
            t0(),
        )
        .unwrap();
        assert_eq!(n.commentaire, None);
    }

    #[test]
    fn edge_la_fermeture_de_la_fenetre_publie_ce_qui_existe() {
        // Celui qui n'a pas noté a eu deux semaines.
        assert!(!publiables(false, t0(), t0()));
        assert!(publiables(
            false,
            t0(),
            t0() + Duration::days(FENETRE_NOTATION_JOURS)
        ));
    }

    // === @security ===

    #[test]
    fn security_une_note_seule_reste_cachee_jusqu_a_la_reciproque() {
        // **C'est la protection anti-représailles.** Si la note du demandeur
        // s'affichait avant celle du prestataire, celui-ci ajusterait la sienne,
        // et les deux perdraient toute valeur.
        assert!(!publiables(false, t0(), t0() + Duration::days(1)));
        assert!(publiables(true, t0(), t0() + Duration::days(1)));
    }

    #[test]
    fn security_la_cible_ne_se_choisit_pas_librement() {
        // Elle est déduite de qui note ; le vocabulaire est fermé pour qu'une
        // valeur inventée ne passe pas par le transport.
        for cible in [Cible::Prestataire, Cible::Demandeur] {
            assert_eq!(Cible::parse(cible.as_str()), Some(cible));
        }
        assert_eq!(Cible::parse("MYSELF"), None);
        assert_eq!(Cible::parse(""), None);
    }

    #[test]
    fn security_la_note_est_un_entier_borne_et_non_un_flottant() {
        // Un flottant permettrait 4,999 999 et rendrait toute somme d'étoiles
        // dépendante d'arrondis. L'échelle est discrète, le type le dit.
        let n = emettre(3, t0()).unwrap();
        assert_eq!(n.note, 3u8);
    }

    #[test]
    fn security_le_commentaire_est_borne_avant_d_etre_conserve() {
        // Il est affiché tel quel et conservé : un champ non borné est une
        // porte ouverte à l'écriture de masse dans une table que personne ne
        // purge.
        let juste = Notation::emettre(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Cible::Demandeur,
            5,
            Some("x".repeat(COMMENTAIRE_MAX_CARACTERES)),
            t0(),
            t0(),
        );
        assert!(juste.is_ok());
    }
}
