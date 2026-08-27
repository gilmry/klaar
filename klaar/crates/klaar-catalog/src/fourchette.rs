//! Fourchette de prix indicative par Secteur (FR-009, Story 2.3).
//!
//! **Ce que la fourchette protège.** Le scénario `@security` de FR-009 exige que
//! les prix individuels ne soient jamais exposés. Une fourchette est pourtant
//! faite de deux prix réels : son minimum et son maximum **sont** des montants
//! qu'un Provider a effectivement facturés. Publier une fourchette calculée sur
//! deux Missions revient donc à publier ces deux prix, et sur trois, à en
//! publier deux sur trois.
//!
//! D'où le seuil : en dessous de `MINIMUM_MISSIONS`, aucune fourchette n'est
//! rendue. Ce n'est pas une précaution statistique — c'est la condition pour
//! que l'agrégat en soit un.
//!
//! **Pourquoi l'IQR, et pas simplement min et max.** Le scénario `@edge` du même
//! FR le montre : sur `[80, 120, 150, 200, 1000]`, la fourchette attendue est
//! 80–200, pas 80–1000. Un seul remorquage de nuit sur autoroute ferait sinon
//! croire à un tarif de plomberie à quatre chiffres.

use klaar_shared_kernel::Money;
use serde::{Deserialize, Serialize};

/// Nombre minimal de Missions avant qu'une fourchette soit publiable.
///
/// Cinq, comme l'exemple de FR-009. En dessous, les bornes de la fourchette
/// sont des prix individuels à peine déguisés.
pub const MINIMUM_MISSIONS: usize = 5;

/// Multiplicateur de l'écart interquartile pour écarter les valeurs aberrantes.
///
/// 1,5 est la valeur usuelle depuis Tukey. Elle n'a rien de sacré, mais la
/// choisir autrement demanderait un jeu de données réel à observer — qui
/// n'existe pas encore.
const FACTEUR_TUKEY: f64 = 1.5;

/// Fourchette indicative, en centimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FourchettePrix {
    pub min: Money,
    pub max: Money,
}

/// Quantile par interpolation linéaire sur les rangs (méthode dite R-7, celle
/// de `numpy` et de la plupart des tableurs).
///
/// Le choix de la méthode n'est pas anodin : sur un petit échantillon, deux
/// méthodes de quantile donnent des bornes différentes, donc des valeurs
/// aberrantes différentes. Celle-ci est retenue parce qu'elle reproduit
/// l'exemple de FR-009, qui fait donc office de vecteur de test.
fn quantile(tries: &[i64], p: f64) -> f64 {
    let n = tries.len();
    if n == 1 {
        return tries[0] as f64;
    }
    let rang = (n - 1) as f64 * p;
    let bas = rang.floor() as usize;
    let haut = rang.ceil() as usize;
    if bas == haut {
        return tries[bas] as f64;
    }
    let fraction = rang - bas as f64;
    tries[bas] as f64 + (tries[haut] - tries[bas]) as f64 * fraction
}

/// Calcule la fourchette indicative d'un ensemble de prix constatés.
///
/// Rend `None` quand l'échantillon est trop mince pour qu'une fourchette soit
/// autre chose qu'une paire de prix individuels — l'interface affiche alors
/// « prix sur devis », comme le prévoit FR-009 `@negative`.
pub fn calculer(prix: &[Money]) -> Option<FourchettePrix> {
    if prix.len() < MINIMUM_MISSIONS {
        return None;
    }

    let mut tries: Vec<i64> = prix.iter().map(Money::cents).collect();
    tries.sort_unstable();

    let q1 = quantile(&tries, 0.25);
    let q3 = quantile(&tries, 0.75);
    let iqr = q3 - q1;
    let plancher = q1 - FACTEUR_TUKEY * iqr;
    let plafond = q3 + FACTEUR_TUKEY * iqr;

    let retenus: Vec<i64> = tries
        .iter()
        .copied()
        .filter(|c| (*c as f64) >= plancher && (*c as f64) <= plafond)
        .collect();

    // Tout écarter est impossible — les quartiles sont toujours dans les
    // bornes — mais s'en remettre à ce raisonnement plutôt qu'au code rendrait
    // un `unwrap` sur `first()` légitime aujourd'hui et faux demain.
    let (min, max) = (retenus.first()?, retenus.last()?);

    // Le seuil porte sur l'échantillon **d'entrée**, pas sur ce qu'il reste
    // après exclusion. Une première version l'appliquait aux deux, et
    // contredisait alors l'exemple de FR-009 lui-même : cinq Missions dont une
    // aberrante n'en laissent que quatre, et le PRD attend pourtant une
    // fourchette. C'est un test qui l'a montré.
    //
    // Ce que cela laisse subsister est écrit dans `COMPLIANCE.md` : au seuil, la
    // fourchette publie deux prix réels sur cinq. Le seuil du PRD est retenu tel
    // quel ; le relever demanderait un jeu de données réel à observer.

    Some(FourchettePrix {
        min: Money::from_cents(*min),
        max: Money::from_cents(*max),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn euros(montants: &[i64]) -> Vec<Money> {
        montants.iter().copied().map(Money::from_euros).collect()
    }

    #[test]
    fn happy_reproduit_l_exemple_du_prd() {
        // FR-009 `@edge`, mot pour mot : sur [80, 120, 150, 200, 1000], la
        // fourchette attendue est 80–200, et non 80–1000.
        let f = calculer(&euros(&[80, 120, 150, 200, 1000])).expect("une fourchette");
        assert_eq!(f.min, Money::from_euros(80));
        assert_eq!(f.max, Money::from_euros(200));
    }

    #[test]
    fn happy_un_echantillon_homogene_garde_ses_bornes() {
        let f = calculer(&euros(&[80, 100, 120, 140, 160])).expect("une fourchette");
        assert_eq!(f.min, Money::from_euros(80));
        assert_eq!(f.max, Money::from_euros(160));
    }

    #[test]
    fn happy_l_ordre_d_entree_ne_change_rien() {
        let croissant = calculer(&euros(&[80, 120, 150, 200, 1000]));
        let decroissant = calculer(&euros(&[1000, 200, 150, 120, 80]));
        let melange = calculer(&euros(&[150, 1000, 80, 200, 120]));
        assert_eq!(croissant, decroissant);
        assert_eq!(croissant, melange);
    }

    #[test]
    fn negative_aucune_fourchette_sans_historique() {
        // FR-009 `@negative` : au lancement, « prix sur devis ».
        assert_eq!(calculer(&[]), None);
    }

    #[test]
    fn negative_aucune_fourchette_sous_le_seuil() {
        for n in 1..MINIMUM_MISSIONS {
            let echantillon: Vec<Money> = euros(&[100; 10])[..n].to_vec();
            assert_eq!(calculer(&echantillon), None, "{n} Missions");
        }
        assert!(calculer(&euros(&[100; MINIMUM_MISSIONS])).is_some());
    }

    #[test]
    fn edge_une_valeur_aberrante_basse_est_ecartee_aussi() {
        // Un dépannage facturé un euro par erreur, ou une remise commerciale
        // exceptionnelle, tirerait la borne basse vers un tarif qui n'existe
        // pas.
        let f = calculer(&euros(&[1, 120, 130, 140, 150, 160])).expect("une fourchette");
        assert!(f.min > Money::from_euros(1), "borne basse : {:?}", f.min);
    }

    #[test]
    fn edge_des_prix_tous_identiques_donnent_une_fourchette_plate() {
        let f = calculer(&euros(&[100; 8])).expect("une fourchette");
        assert_eq!(f.min, f.max);
        assert_eq!(f.min, Money::from_euros(100));
    }

    #[test]
    fn edge_les_centimes_ne_sont_pas_perdus() {
        // L'agrégat travaille en centimes, comme tout le reste : arrondir ici
        // ferait diverger la fourchette affichée des montants réels.
        let prix: Vec<Money> = [9900, 9975, 10025, 11000, 11500]
            .iter()
            .copied()
            .map(Money::from_cents)
            .collect();
        let f = calculer(&prix).expect("une fourchette");
        assert_eq!(f.min.cents(), 9900);
        assert_eq!(f.max.cents(), 11500);
    }

    #[test]
    fn security_deux_missions_ne_publient_pas_deux_prix() {
        // Le coeur du scénario `@security` : une fourchette de deux Missions
        // **est** la paire de prix facturés.
        assert_eq!(calculer(&euros(&[80, 200])), None);
        assert_eq!(calculer(&euros(&[80, 120, 200])), None);
    }

    #[test]
    fn security_le_seuil_porte_sur_l_echantillon_d_entree() {
        // Une première version appliquait le seuil aussi **après** exclusion, et
        // contredisait alors l'exemple de FR-009 : cinq Missions dont une
        // aberrante n'en laissent que quatre, et le PRD attend une fourchette.
        //
        // Ce que cela laisse subsister est réel et écrit dans COMPLIANCE.md : au
        // seuil, les bornes publiées sont deux prix facturés sur cinq. Le seuil
        // du PRD est retenu tel quel ; le relever demanderait un jeu de données
        // réel à observer.
        let f = calculer(&euros(&[80, 120, 150, 200, 1000])).expect("une fourchette");
        assert_eq!(f.min, Money::from_euros(80));
        assert_eq!(f.max, Money::from_euros(200));
    }

    #[test]
    fn security_les_aberrations_hautes_et_basses_partent_ensemble() {
        let f = calculer(&euros(&[1, 120, 130, 140, 150, 100_000])).expect("une fourchette");
        assert!(f.min >= Money::from_euros(120), "borne basse : {:?}", f.min);
        assert!(f.max <= Money::from_euros(150), "borne haute : {:?}", f.max);
    }

    #[test]
    fn security_la_fourchette_ne_revele_pas_le_prix_le_plus_eleve() {
        // Un Provider qui a facturé bien au-dessus des autres ne doit pas voir
        // son tarif devenir la borne haute publique du secteur.
        let f = calculer(&euros(&[100, 110, 120, 130, 140, 5000])).expect("une fourchette");
        assert!(f.max < Money::from_euros(5000));
    }

    #[test]
    fn security_le_calcul_ne_panique_sur_aucun_echantillon() {
        // Des montants nuls, négatifs ou extrêmes ne doivent pas faire tomber
        // le service : ce calcul tourne dans un job, sans personne pour le
        // relancer.
        let cas: Vec<Vec<i64>> = vec![
            vec![0; 6],
            vec![-100, -50, 0, 50, 100, 150],
            vec![i64::MAX / 200; 6],
            vec![0, 0, 0, 0, 0, i64::MAX / 200],
        ];
        for montants in cas {
            let prix: Vec<Money> = montants.iter().copied().map(Money::from_cents).collect();
            let _ = calculer(&prix);
        }
    }
}
