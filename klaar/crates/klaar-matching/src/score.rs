//! Score de candidature d'un prestataire (FR-012, Story 3.2).
//!
//! **L'AI Act est la raison d'être de ce module.** FR-012 exige que le score
//! soit documenté et qu'aucun attribut protégé ne le biaise. La réponse n'est
//! pas une promesse dans un commentaire : c'est la **signature de la fonction**.
//! `calculer` ne reçoit que trois nombres — une distance, une ancienneté de
//! contrôle, une note éventuelle. Elle ne peut pas voir un nom, une adresse,
//! une langue ou une photo, parce qu'on ne les lui donne pas. Un biais sur un
//! attribut protégé demanderait d'abord de changer cette signature, ce qui se
//! voit à la relecture d'une ligne.
//!
//! **Le rating n'existe pas encore.** FR-012 le nomme comme critère ; le
//! bounded context Trust arrive plus tard. Le score le traite comme absent
//! plutôt que comme nul : un prestataire sans historique ne doit pas être
//! pénalisé pour n'avoir pas encore travaillé, sinon aucun nouveau venu ne
//! reçoit jamais rien et le classement se fige sur les premiers arrivés.
//! L'absence est **inscrite dans la ventilation**, pour que la trace dise de
//! quoi le score était réellement fait.

use serde::{Deserialize, Serialize};

/// Rayon du premier tour, en mètres (FR-012).
pub const RAYON_METRES: f64 = 5_000.0;

/// Nombre maximal de prestataires notifiés par tour (FR-012 `@edge`).
///
/// Notifier soixante personnes pour une fuite en réveille cinquante pour rien,
/// et la première qui accepte fait perdre leur temps aux autres. Dix suffit à
/// ce qu'une réponse arrive vite.
pub const CANDIDATS_MAX: usize = 10;

/// Poids de la proximité dans le score.
///
/// Prépondérante, et volontairement : sur un dépannage, dix minutes de trajet
/// pèsent plus que deux dixièmes de note. Les poids somment à 1 pour que le
/// score reste lisible comme une proportion.
const POIDS_PROXIMITE: f64 = 0.7;
/// Poids de la fraîcheur du contrôle d'entreprise.
const POIDS_CONTROLE: f64 = 0.1;
/// Poids de la note, quand elle existe.
const POIDS_NOTE: f64 = 0.2;

/// Ancienneté de contrôle au-delà de laquelle la fraîcheur ne compte plus.
const CONTROLE_PERIME_JOURS: f64 = 365.0;

/// Contribution d'un critère au score final.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Contribution {
    /// Valeur du critère ramenée entre 0 et 1.
    pub valeur: f64,
    pub poids: f64,
}

impl Contribution {
    fn apport(&self) -> f64 {
        self.valeur * self.poids
    }
}

/// Score et sa ventilation.
///
/// La ventilation n'est pas un confort de débogage : c'est ce que FR-012
/// appelle la Trace, et ce que l'AI Act exige de pouvoir produire quand
/// quelqu'un demande pourquoi il n'a pas été retenu.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub proximite: Contribution,
    pub controle: Contribution,
    /// Absente tant que le bounded context Trust n'existe pas. `None` se
    /// distingue d'une note de zéro : n'avoir pas encore travaillé n'est pas
    /// avoir mal travaillé.
    pub note: Option<Contribution>,
    /// Score final, entre 0 et 1.
    pub total: f64,
}

/// Calcule le score d'un candidat.
///
/// `note` est attendue entre 0 et 5, comme une note d'étoiles. Toute valeur
/// hors bornes est ramenée dedans plutôt que refusée : un score n'a pas à
/// faire échouer un matching parce qu'une moyenne a débordé d'un millième.
pub fn calculer(distance_metres: f64, anciennete_kyc_jours: f64, note: Option<f64>) -> Score {
    // Proximité linéaire décroissante sur le rayon : à zéro mètre elle vaut 1,
    // au bord du rayon elle vaut 0. Un candidat exactement au bord marque donc
    // zéro sur ce critère sans être exclu — c'est le rayon qui exclut, pas le
    // score.
    let proximite = Contribution {
        valeur: (1.0 - (distance_metres.max(0.0) / RAYON_METRES)).clamp(0.0, 1.0),
        poids: POIDS_PROXIMITE,
    };

    // Fraîcheur du contrôle : pleine le jour du contrôle, nulle après un an.
    // Elle ne pénalise pas un prestataire ancien mais recontrôlé, ce qui est
    // exactement ce que FR-012 veut encourager.
    let controle = Contribution {
        valeur: (1.0 - (anciennete_kyc_jours.max(0.0) / CONTROLE_PERIME_JOURS)).clamp(0.0, 1.0),
        poids: POIDS_CONTROLE,
    };

    let note = note.map(|n| Contribution {
        valeur: (n.clamp(0.0, 5.0)) / 5.0,
        poids: POIDS_NOTE,
    });

    // Quand la note manque, son poids est **redistribué** au lieu d'être perdu.
    // Le laisser tomber plafonnerait tous les scores à 0,8 et rendrait le
    // classement de deux prestataires sans historique indistinguable de celui
    // de deux prestataires mal notés.
    let poids_total = proximite.poids + controle.poids + note.map_or(0.0, |n| n.poids);
    let brut = proximite.apport() + controle.apport() + note.map_or(0.0, |n| n.apport());
    let total = if poids_total > 0.0 {
        (brut / poids_total).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Score {
        proximite,
        controle,
        note,
        total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_le_plus_proche_marque_le_plus() {
        let ici = calculer(0.0, 0.0, None);
        let loin = calculer(4_000.0, 0.0, None);
        assert!(ici.total > loin.total);
        assert!(
            (ici.total - 1.0).abs() < 1e-9,
            "à zéro mètre : {}",
            ici.total
        );
    }

    #[test]
    fn happy_le_score_reste_entre_zero_et_un() {
        for distance in [0.0, 100.0, 2_500.0, RAYON_METRES, 50_000.0] {
            for anciennete in [0.0, 30.0, 400.0, 10_000.0] {
                for note in [None, Some(0.0), Some(2.5), Some(5.0)] {
                    let s = calculer(distance, anciennete, note);
                    assert!(
                        (0.0..=1.0).contains(&s.total),
                        "{distance}/{anciennete}/{note:?} → {}",
                        s.total
                    );
                }
            }
        }
    }

    #[test]
    fn happy_un_controle_recent_vaut_mieux_qu_un_controle_ancien() {
        let recent = calculer(1_000.0, 0.0, None);
        let ancien = calculer(1_000.0, 300.0, None);
        assert!(recent.total > ancien.total);
    }

    #[test]
    fn negative_une_note_hors_bornes_ne_fait_pas_deborder_le_score() {
        // Un score n'a pas à faire échouer un matching parce qu'une moyenne a
        // débordé d'un millième.
        for note in [-1.0, 5.5, f64::MAX] {
            let s = calculer(1_000.0, 0.0, Some(note));
            assert!((0.0..=1.0).contains(&s.total), "note {note} → {}", s.total);
        }
    }

    #[test]
    fn negative_une_distance_negative_est_traitee_comme_nulle() {
        // PostGIS n'en rend pas, mais un appelant futur pourrait.
        assert_eq!(
            calculer(-10.0, 0.0, None).total,
            calculer(0.0, 0.0, None).total
        );
    }

    #[test]
    fn edge_au_bord_du_rayon_la_proximite_est_nulle_sans_exclure() {
        // C'est le rayon qui exclut, pas le score : un candidat au bord marque
        // zéro sur ce critère et reste candidat.
        let bord = calculer(RAYON_METRES, 0.0, None);
        assert_eq!(bord.proximite.valeur, 0.0);
        assert!(bord.total > 0.0, "il marque encore sur le contrôle");
    }

    #[test]
    fn edge_l_absence_de_note_ne_penalise_pas() {
        // Un prestataire sans historique ne doit pas être classé derrière un
        // prestataire mal noté : n'avoir pas encore travaillé n'est pas avoir
        // mal travaillé. Sinon aucun nouveau venu ne reçoit rien, et le
        // classement se fige sur les premiers arrivés.
        let sans = calculer(1_000.0, 0.0, None);
        let mal_note = calculer(1_000.0, 0.0, Some(1.0));
        assert!(
            sans.total > mal_note.total,
            "sans note {} vs mal noté {}",
            sans.total,
            mal_note.total
        );
    }

    #[test]
    fn edge_l_absence_de_note_ne_plafonne_pas_le_score() {
        // Le poids manquant est redistribué : sans cela, tous les scores
        // seraient plafonnés à 0,8 et deux prestataires sans historique
        // seraient indistinguables de deux mal notés.
        let parfait_sans_note = calculer(0.0, 0.0, None);
        assert!((parfait_sans_note.total - 1.0).abs() < 1e-9);
        assert!(parfait_sans_note.note.is_none());
    }

    #[test]
    fn edge_l_absence_de_note_est_neutre_et_non_equivalente_a_cinq_etoiles() {
        // Redistribuer le poids manquant revient à noter le prestataire comme
        // il se comporte sur les autres critères : l'absence ne monte ni ne
        // descend le score, elle le laisse tel quel.
        //
        // Une première version de ce test affirmait qu'une note maximale vaut
        // l'absence de note. C'est faux, et le calcul avait raison : cinq
        // étoiles valent mieux que la moyenne des autres critères, donc tirent
        // le score vers le haut.
        let sans = calculer(1_000.0, 10.0, None);
        let zero = calculer(1_000.0, 10.0, Some(0.0));
        let cinq = calculer(1_000.0, 10.0, Some(5.0));

        assert!(zero.total < sans.total, "une mauvaise note doit coûter");
        assert!(
            sans.total < cinq.total,
            "une excellente note doit rapporter"
        );

        // Et l'absence vaut exactement la moyenne pondérée de ce qui reste.
        let attendu = (sans.proximite.apport() + sans.controle.apport())
            / (sans.proximite.poids + sans.controle.poids);
        assert!((sans.total - attendu).abs() < 1e-9);
    }

    #[test]
    fn security_le_calcul_ne_voit_que_trois_nombres() {
        // La réponse à l'AI Act n'est pas une promesse, c'est la signature :
        // `calculer` ne reçoit ni nom, ni adresse, ni langue, ni photo. Un
        // biais sur un attribut protégé demanderait d'abord de changer cette
        // signature.
        //
        // Ce test fixe l'intention et échouera si un paramètre s'ajoute.
        let s: fn(f64, f64, Option<f64>) -> Score = calculer;
        let _ = s(1_000.0, 0.0, None);
    }

    #[test]
    fn security_la_ventilation_dit_de_quoi_le_score_est_fait() {
        // C'est ce que FR-012 appelle la Trace, et ce que l'AI Act exige de
        // pouvoir produire quand quelqu'un demande pourquoi il n'a pas été
        // retenu.
        let s = calculer(1_000.0, 30.0, None);
        assert_eq!(s.proximite.poids, POIDS_PROXIMITE);
        assert_eq!(s.controle.poids, POIDS_CONTROLE);
        assert!(s.note.is_none(), "l'absence de note doit être visible");

        let avec = calculer(1_000.0, 30.0, Some(4.2));
        assert_eq!(avec.note.map(|n| n.poids), Some(POIDS_NOTE));
    }

    #[test]
    fn security_deux_candidats_identiques_obtiennent_le_meme_score() {
        // Déterminisme : sans lui, le classement varierait d'un tour à l'autre
        // et personne ne pourrait expliquer un refus.
        for _ in 0..10 {
            assert_eq!(
                calculer(1_234.5, 42.0, Some(4.2)),
                calculer(1_234.5, 42.0, Some(4.2))
            );
        }
    }

    #[test]
    fn security_aucune_entree_ne_produit_un_nan() {
        // Un `NaN` dans un tri produit un ordre incohérent, et le classement
        // cesse silencieusement d'être un classement.
        for distance in [0.0, f64::MAX, -0.0] {
            for anciennete in [0.0, f64::MAX] {
                let s = calculer(distance, anciennete, Some(f64::MAX));
                assert!(!s.total.is_nan(), "{distance}/{anciennete}");
            }
        }
    }
}
