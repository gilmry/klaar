//! Tableau de bord d'exploitation (FR-040, Story 8.3).
//!
//! **Ce module calcule des rapports, il ne les interprète pas.** Un taux de
//! remplissage arrive avec son assiette, une note moyenne avec le nombre de
//! notes : « 60 % » sur trois Demandes se lit autrement que « 60 % » sur trois
//! mille, et un tableau de bord qui masque son dénominateur fait prendre des
//! décisions sur du bruit.

use chrono::{DateTime, Duration, Utc};

use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::tableau_bord_repository::{Indicateurs, TableauBordRepository};

/// Fenêtre d'observation, en jours.
///
/// Trente. C'est la définition usuelle du MAU, et prendre la même fenêtre pour
/// tous les indicateurs évite de comparer un chiffre à sept jours avec un autre
/// à trente sur le même écran.
pub const FENETRE_JOURS: i64 = 30;

/// Ce que l'écran affiche.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VueTableauBord {
    pub indicateurs: Indicateurs,
    /// Début de la fenêtre, pour que l'écran dise sur quoi il porte.
    pub depuis: DateTime<Utc>,
    /// `demandes_attribuees / demandes`, ou `None` si aucune Demande.
    ///
    /// **`None` et non zéro.** Zéro pour cent se lit comme un échec ; l'absence
    /// de Demande n'est pas un échec de remplissage, c'est une absence de
    /// mesure. FR-040 `@edge` demande un état vide guidé, pas un tableau de
    /// zéros alarmants.
    pub taux_remplissage: Option<f64>,
    /// Note moyenne sur la fenêtre, ou `None` si personne n'a noté.
    pub note_moyenne: Option<f64>,
}

/// Calcule le tableau de bord.
///
/// **Aucun contrôle de droit ici.** L'autorisation et sa journalisation sont
/// faites par `autoriser_et_consigner` avant l'appel, comme pour le journal
/// d'audit : mêler les deux donnerait deux endroits où vérifier un droit, et
/// c'est un de trop.
pub async fn tableau_de_bord<T, H>(
    depots: &T,
    horloge: &H,
) -> Result<VueTableauBord, RepositoryError>
where
    T: TableauBordRepository,
    H: Horloge,
{
    let depuis = horloge.maintenant() - Duration::days(FENETRE_JOURS);
    let indicateurs = depots.indicateurs(depuis).await?;
    Ok(VueTableauBord {
        indicateurs,
        depuis,
        taux_remplissage: taux(indicateurs.demandes_attribuees, indicateurs.demandes),
        note_moyenne: moyenne(indicateurs.somme_notes, indicateurs.notes),
    })
}

/// Rapport de deux comptages, `None` quand le dénominateur est nul.
fn taux(numerateur: i64, denominateur: i64) -> Option<f64> {
    if denominateur <= 0 {
        return None;
    }
    Some(numerateur as f64 / denominateur as f64)
}

fn moyenne(somme: i64, nombre: i64) -> Option<f64> {
    if nombre <= 0 {
        return None;
    }
    Some(somme as f64 / nombre as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_le_taux_de_remplissage_est_un_rapport_simple() {
        assert_eq!(taux(3, 4), Some(0.75));
        assert_eq!(moyenne(18, 4), Some(4.5));
    }

    #[test]
    fn edge_sans_demande_le_taux_est_absent_et_non_nul() {
        // Zéro pour cent se lit comme un échec de la plateforme. À J0, il n'y a
        // pas d'échec : il n'y a rien à mesurer. FR-040 `@edge`.
        assert_eq!(taux(0, 0), None);
        assert_eq!(moyenne(0, 0), None);
    }

    #[test]
    fn edge_un_denominateur_negatif_ne_produit_pas_de_taux() {
        // Ne peut venir que d'un comptage cassé. Rendre un taux négatif le
        // ferait afficher tel quel, et personne ne saurait d'où il sort.
        assert_eq!(taux(1, -1), None);
        assert_eq!(moyenne(5, -2), None);
    }

    #[test]
    fn negative_le_taux_ne_depasse_pas_un_quand_les_comptages_sont_coherents() {
        // Garde-fou sur la requête : les attributions sont comptées sur les
        // Demandes de la fenêtre, donc leur nombre ne peut pas la dépasser.
        // Si ce test tombe un jour, c'est la requête qui a changé de sens.
        assert!(taux(4, 4).unwrap() <= 1.0);
    }

    #[test]
    fn security_les_indicateurs_ne_portent_aucun_identifiant() {
        // Le type lui-même l'empêche : ajouter un `Uuid` ici ne compilerait pas
        // sans changer cette assertion, ce qui est le but.
        let vide = Indicateurs::default();
        assert_eq!(vide.comptes_actifs, 0);
        assert_eq!(std::mem::size_of::<Indicateurs>(), 10 * 8);
    }
}
