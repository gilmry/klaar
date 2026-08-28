//! Administration du catalogue par l'exploitation (FR-010, Story 2.4).
//!
//! **Publier un secteur est un geste qui ne se défait pas.** Il devient
//! proposable à toute la Région : les Demandes s'y rangent, les prestataires
//! s'y déclarent compétents, et le retirer ensuite laisse des Missions
//! orphelines. D'où la seconde paire d'yeux — la même règle que pour un refus
//! de contrôle d'entreprise, et pour la même raison : ce qui ne se défait pas
//! se décide à deux.
//!
//! **Rien ici n'attend un tiers.** Le catalogue est à nous ; c'est une des
//! rares parties du produit dont personne d'autre ne détient la clé.

use std::fmt;

use crate::libelles::Libelles;
use crate::secteur::CodeCatalogue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatutSecteur {
    /// Créé, pas encore proposable. Invisible du public.
    Brouillon,
    Publie,
    /// Retiré du public sans être effacé.
    ///
    /// **Distinct d'une suppression.** Les Demandes et les Missions passées y
    /// renvoient ; effacer la ligne les rendrait illisibles.
    Desactive,
}

impl StatutSecteur {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Brouillon => "DRAFT",
            Self::Publie => "PUBLISHED",
            Self::Desactive => "DISABLED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "DRAFT" => Some(Self::Brouillon),
            "PUBLISHED" => Some(Self::Publie),
            "DISABLED" => Some(Self::Desactive),
            _ => None,
        }
    }

    /// Vrai si le public doit le voir.
    pub fn visible_du_public(&self) -> bool {
        matches!(self, Self::Publie)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdministrationError {
    /// Un secteur porte déjà ce code (FR-010 `@negative`, 409).
    CodeDejaPris,
    /// Publier ce qu'on a soi-même créé n'est pas une validation.
    MemeAuteur,
    /// Le secteur n'est pas dans l'état qu'exige ce geste.
    TransitionInterdite { depuis: StatutSecteur },
    /// Des interventions sont en cours dans ce secteur (FR-010 `@edge`).
    MissionsEnCours { combien: i64 },
    /// Libellé vide dans l'une des trois langues.
    ///
    /// **Les trois sont exigées à la création.** Un secteur publié avec un
    /// libellé néerlandais manquant s'afficherait en français à un
    /// néerlandophone, dans une région où c'est précisément ce qu'il ne faut
    /// pas faire — et le corriger après publication demanderait de rattraper
    /// tous ceux qui l'ont déjà lu.
    LibelleManquant { langue: &'static str },
}

impl AdministrationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::CodeDejaPris => "SECTOR_CODE_TAKEN",
            Self::MemeAuteur => "FOUR_EYES_REQUIRED",
            Self::TransitionInterdite { .. } => "SECTOR_TRANSITION_INVALID",
            Self::MissionsEnCours { .. } => "SECTOR_HAS_ACTIVE_MISSIONS",
            Self::LibelleManquant { .. } => "LABEL_REQUIRED",
        }
    }
}

impl fmt::Display for AdministrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CodeDejaPris => write!(f, "un secteur porte déjà ce code"),
            Self::MemeAuteur => write!(
                f,
                "un secteur se publie par un autre compte que celui qui l'a créé"
            ),
            Self::TransitionInterdite { depuis } => {
                write!(f, "geste impossible depuis l'état {}", depuis.as_str())
            }
            Self::MissionsEnCours { combien } => {
                write!(f, "{combien} intervention(s) en cours dans ce secteur")
            }
            Self::LibelleManquant { langue } => {
                write!(f, "le libellé {langue} manque")
            }
        }
    }
}

impl std::error::Error for AdministrationError {}

/// Ce qu'il faut pour créer un secteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecteurACreer {
    pub code: CodeCatalogue,
    pub libelles: Libelles,
    pub ordre: i32,
}

/// Valide une création (FR-010 `@happy`, `@negative`).
///
/// `code_pris` vient de la base : c'est elle qui tranche l'unicité, et le
/// contrôle ici ne fait qu'éviter un aller-retour dans le cas courant.
pub fn valider_creation(
    a_creer: &SecteurACreer,
    code_pris: bool,
) -> Result<StatutSecteur, AdministrationError> {
    if code_pris {
        return Err(AdministrationError::CodeDejaPris);
    }
    verifier_libelles(&a_creer.libelles)?;
    // **Toujours en brouillon.** Créer directement publié contournerait la
    // seconde paire d'yeux, ce qui reviendrait à ne pas l'avoir.
    Ok(StatutSecteur::Brouillon)
}

/// Valide une publication (FR-010 `@happy`, `@security`).
pub fn valider_publication(
    statut: StatutSecteur,
    cree_par: Option<uuid::Uuid>,
    publie_par: uuid::Uuid,
) -> Result<(), AdministrationError> {
    if statut != StatutSecteur::Brouillon {
        return Err(AdministrationError::TransitionInterdite { depuis: statut });
    }
    // `None` : un secteur du peuplement initial, sans auteur. Il n'est jamais
    // en brouillon, donc ce cas ne se produit pas ; le traiter ici évite
    // d'écrire un `unwrap` qui deviendrait faux le jour où il se produirait.
    if cree_par == Some(publie_par) {
        return Err(AdministrationError::MemeAuteur);
    }
    Ok(())
}

/// Valide un retrait du public (FR-010 `@edge`).
///
/// **Le refus porte sur les interventions en cours, pas sur les Demandes.**
/// Une Demande en diffusion se rediffusera ailleurs ou expirera ; une Mission
/// en cours engage deux personnes et un montant, et retirer son secteur
/// pendant qu'elle se déroule casserait un écran au milieu d'une intervention.
pub fn valider_desactivation(
    statut: StatutSecteur,
    missions_en_cours: i64,
) -> Result<(), AdministrationError> {
    if statut != StatutSecteur::Publie {
        return Err(AdministrationError::TransitionInterdite { depuis: statut });
    }
    if missions_en_cours > 0 {
        return Err(AdministrationError::MissionsEnCours {
            combien: missions_en_cours,
        });
    }
    Ok(())
}

fn verifier_libelles(libelles: &Libelles) -> Result<(), AdministrationError> {
    for (langue, valeur) in [
        ("français", libelles.fr.as_str()),
        ("néerlandais", libelles.nl.as_str()),
        ("anglais", libelles.en.as_str()),
    ] {
        if valeur.trim().is_empty() {
            return Err(AdministrationError::LibelleManquant { langue });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn libelles() -> Libelles {
        Libelles {
            fr: "Chauffage".to_string(),
            nl: "Verwarming".to_string(),
            en: "Heating".to_string(),
        }
    }

    fn a_creer() -> SecteurACreer {
        SecteurACreer {
            code: CodeCatalogue::parse("chauffage").unwrap(),
            libelles: libelles(),
            ordre: 6,
        }
    }

    #[test]
    fn happy_un_secteur_neuf_naît_en_brouillon() {
        // Créer directement publié contournerait la seconde paire d'yeux, ce
        // qui reviendrait à ne pas l'avoir.
        assert_eq!(
            valider_creation(&a_creer(), false),
            Ok(StatutSecteur::Brouillon)
        );
    }

    #[test]
    fn negative_un_code_deja_pris_est_refuse() {
        // FR-010 `@negative` : 409.
        assert_eq!(
            valider_creation(&a_creer(), true),
            Err(AdministrationError::CodeDejaPris)
        );
    }

    #[test]
    fn security_les_trois_libelles_sont_exiges_a_la_creation() {
        // Un secteur publié sans libellé néerlandais s'afficherait en français
        // à un néerlandophone, dans une région où c'est précisément ce qu'il ne
        // faut pas faire — et le corriger après coup ne rattraperait pas ceux
        // qui l'ont déjà lu.
        for (champ, langue) in [("fr", "français"), ("nl", "néerlandais"), ("en", "anglais")] {
            let mut sujet = a_creer();
            match champ {
                "fr" => sujet.libelles.fr = "   ".to_string(),
                "nl" => sujet.libelles.nl = String::new(),
                _ => sujet.libelles.en = " ".to_string(),
            }
            assert_eq!(
                valider_creation(&sujet, false),
                Err(AdministrationError::LibelleManquant { langue }),
                "libellé {champ} accepté à tort"
            );
        }
    }

    #[test]
    fn security_on_ne_publie_pas_ce_qu_on_a_cree() {
        // FR-010 `@security` : quatre yeux. Publier son propre brouillon ne
        // serait pas une validation, ce serait un second clic.
        let moi = Uuid::new_v4();
        assert_eq!(
            valider_publication(StatutSecteur::Brouillon, Some(moi), moi),
            Err(AdministrationError::MemeAuteur)
        );
    }

    #[test]
    fn happy_un_autre_compte_publie_le_brouillon() {
        assert_eq!(
            valider_publication(
                StatutSecteur::Brouillon,
                Some(Uuid::new_v4()),
                Uuid::new_v4()
            ),
            Ok(())
        );
    }

    #[test]
    fn negative_on_ne_publie_pas_deux_fois() {
        for statut in [StatutSecteur::Publie, StatutSecteur::Desactive] {
            assert!(matches!(
                valider_publication(statut, Some(Uuid::new_v4()), Uuid::new_v4()),
                Err(AdministrationError::TransitionInterdite { .. })
            ));
        }
    }

    #[test]
    fn edge_un_secteur_avec_des_interventions_en_cours_ne_se_retire_pas() {
        // FR-010 `@edge` : 409 `SECTOR_HAS_ACTIVE_MISSIONS`. Retirer le secteur
        // pendant qu'une intervention s'y déroule casserait un écran au milieu.
        assert_eq!(
            valider_desactivation(StatutSecteur::Publie, 10),
            Err(AdministrationError::MissionsEnCours { combien: 10 })
        );
        assert_eq!(valider_desactivation(StatutSecteur::Publie, 0), Ok(()));
    }

    #[test]
    fn negative_on_ne_retire_pas_un_brouillon() {
        // Il n'a jamais été public : il n'y a rien à retirer.
        assert!(matches!(
            valider_desactivation(StatutSecteur::Brouillon, 0),
            Err(AdministrationError::TransitionInterdite { .. })
        ));
    }

    #[test]
    fn security_seul_publie_est_visible_du_public() {
        assert!(StatutSecteur::Publie.visible_du_public());
        // Un brouillon visible laisserait soumettre des Demandes dans un
        // secteur où aucun prestataire ne s'est encore déclaré.
        assert!(!StatutSecteur::Brouillon.visible_du_public());
        // Et un désactivé continuerait d'être proposé alors qu'on vient
        // justement de le retirer.
        assert!(!StatutSecteur::Desactive.visible_du_public());
    }

    #[test]
    fn edge_le_vocabulaire_fait_l_aller_retour() {
        for statut in [
            StatutSecteur::Brouillon,
            StatutSecteur::Publie,
            StatutSecteur::Desactive,
        ] {
            assert_eq!(StatutSecteur::parse(statut.as_str()), Some(statut));
        }
        assert_eq!(StatutSecteur::parse("ARCHIVE"), None);
    }

    #[test]
    fn edge_un_secteur_du_peuplement_initial_n_a_pas_d_auteur() {
        // `cree_par = None` : il vient du jeu de départ, pas d'une décision
        // d'exploitation. Le cas ne se produit pas — un tel secteur n'est
        // jamais en brouillon — mais l'écrire évite un `unwrap` qui deviendrait
        // faux le jour où il se produirait.
        assert_eq!(
            valider_publication(StatutSecteur::Brouillon, None, Uuid::new_v4()),
            Ok(())
        );
    }
}
