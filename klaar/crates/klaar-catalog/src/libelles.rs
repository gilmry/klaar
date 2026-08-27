//! Libellés trilingues (FR-008, FR-043).

use klaar_shared_kernel::Locale;
use serde::{Deserialize, Serialize};

/// Libellé d'une entrée de catalogue dans les trois langues.
///
/// Les trois sont **obligatoires**. Un `Option` par langue laisserait entrer un
/// secteur traduit en français seulement, et le catalogue néerlandophone se
/// remplirait de trous que personne ne remarquerait avant un utilisateur réel.
/// Bruxelles est officiellement bilingue : une entrée sans néerlandais n'est
/// pas une entrée incomplète, c'est une entrée qui ne devrait pas exister.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Libelles {
    pub fr: String,
    pub nl: String,
    pub en: String,
}

impl Libelles {
    pub fn new(fr: impl Into<String>, nl: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            fr: fr.into(),
            nl: nl.into(),
            en: en.into(),
        }
    }

    pub fn pour(&self, locale: Locale) -> &str {
        match locale {
            Locale::Fr => &self.fr,
            Locale::Nl => &self.nl,
            Locale::En => &self.en,
        }
    }

    /// Vrai si aucune des trois traductions n'est vide.
    ///
    /// Sert au contrôle du jeu de données à l'amorçage : une chaîne vide passe
    /// le typage mais produit une case blanche dans l'interface.
    pub fn complet(&self) -> bool {
        ![&self.fr, &self.nl, &self.en]
            .iter()
            .any(|t| t.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn libelles() -> Libelles {
        Libelles::new("Plomberie", "Loodgieterij", "Plumbing")
    }

    #[test]
    fn happy_rend_la_traduction_demandee() {
        assert_eq!(libelles().pour(Locale::Fr), "Plomberie");
        assert_eq!(libelles().pour(Locale::Nl), "Loodgieterij");
        assert_eq!(libelles().pour(Locale::En), "Plumbing");
    }

    #[test]
    fn happy_un_libelle_complet_est_reconnu() {
        assert!(libelles().complet());
    }

    #[test]
    fn negative_une_traduction_vide_rend_le_libelle_incomplet() {
        assert!(!Libelles::new("Plomberie", "", "Plumbing").complet());
        assert!(!Libelles::new("", "Loodgieterij", "Plumbing").complet());
        assert!(!Libelles::new("Plomberie", "Loodgieterij", "").complet());
    }

    #[test]
    fn edge_une_traduction_faite_d_espaces_ne_compte_pas_comme_traduite() {
        // Le cas le plus courant d'un jeu de données recopié à la main.
        assert!(!Libelles::new("Plomberie", "   ", "Plumbing").complet());
    }

    #[test]
    fn security_aucune_langue_ne_sert_de_repli_silencieux() {
        // Renvoyer le français quand le néerlandais manque donnerait un
        // catalogue qui paraît traduit sans l'être. Le type impose les trois,
        // et `pour` rend ce qu'il y a — vide compris, ce que `complet` détecte.
        let bancal = Libelles::new("Plomberie", "", "Plumbing");
        assert_eq!(bancal.pour(Locale::Nl), "");
        assert_ne!(bancal.pour(Locale::Nl), bancal.pour(Locale::Fr));
    }
}
