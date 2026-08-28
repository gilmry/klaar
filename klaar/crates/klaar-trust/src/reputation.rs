//! Réputation d'un prestataire, calculée en borne basse de Wilson (FR-037).
//!
//! **Pourquoi pas la moyenne.** Un prestataire avec une seule note de cinq
//! étoiles afficherait 5,0 et passerait devant quelqu'un qui en a cinquante à
//! 4,5. La borne basse de l'intervalle de confiance répond à une autre question,
//! la bonne : « quelle est la note la plus basse compatible avec ce qu'on a
//! observé ». Une note isolée est peu informative, et le calcul le dit.
//!
//! **Ce que le PRD dit, et ce que sa formule donne.** FR-037 illustre la
//! formule avec « 1 note 5 ★ → Wilson ≈ 0,45 » et « 50 notes 4,5 ★ → ≈ 0,83 ».
//! Ces valeurs ne découlent pas de la formule qu'il écrit juste au-dessus :
//! celle-ci donne 0,21 et 0,79. La formule est la partie normative — elle est
//! posée explicitement — et les nombres sont illustratifs. Ce qui compte est la
//! propriété qu'ils servent à montrer, et elle tient : une note isolée ne classe
//! pas devant cinquante notes solides. Les deux valeurs exactes sont dans un
//! test, pour que l'écart soit constaté plutôt que redécouvert.

/// Quantile de la loi normale pour un intervalle à 95 % (FR-037).
pub const Z_95: f64 = 1.96;

/// Note maximale d'une étoile à cinq.
pub const NOTE_MAX: u32 = 5;

/// Borne basse de Wilson, entre 0 et 1.
///
/// Rend `None` sans aucune note : **il n'y a rien à calculer**, et rendre zéro
/// ferait passer un prestataire qui n'a jamais travaillé pour un prestataire
/// détestable. C'est à l'appelant de dire « pas encore noté ».
///
/// `somme_notes` est la somme des étoiles, `nombre` le nombre de notes.
pub fn wilson(somme_notes: u32, nombre: u32) -> Option<f64> {
    if nombre == 0 {
        return None;
    }
    let n = f64::from(nombre);
    // Proportion de satisfaction : la somme rapportée au maximum atteignable.
    // Une note de 5 vaut 1, une note de 1 vaut 0,2 — et non 0 : une étoile
    // reste une intervention faite, pas un échec total.
    let p = (f64::from(somme_notes) / (f64::from(NOTE_MAX) * n)).clamp(0.0, 1.0);
    let z2 = Z_95 * Z_95;

    let centre = p + z2 / (2.0 * n);
    let marge = Z_95 * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    Some(((centre - marge) / (1.0 + z2 / n)).clamp(0.0, 1.0))
}

/// Réputation prêtée à un prestataire que personne n'a encore noté (FR-037).
///
/// 0,80, soit quatre étoiles sur cinq.
pub const PRIOR_SANS_NOTE: f64 = 0.80;

/// La note telle que le matching l'attend : toujours entre 0 et 5.
///
/// **Un prior neutre, et voici pourquoi il n'est pas facultatif.** Le score de
/// matching sait redistribuer le poids de la note quand elle manque
/// (`klaar_matching::calculer`), et cela semble plus honnête que d'inventer une
/// réputation. Ce n'en est pas une : redistribuer revient à noter le
/// prestataire **sur sa propre moyenne des autres critères**, c'est-à-dire à
/// lui prêter la meilleure note compatible avec son profil. À distance égale,
/// un compte tout neuf passerait alors devant un artisan qui a cinquante avis à
/// 4,5 — il faudrait à ce dernier une borne de Wilson au-delà de 0,97, donc
/// plus de cent cinquante notes parfaites, pour seulement l'égaler.
///
/// Le prior de FR-037 corrige cela : l'inconnu vaut quatre étoiles, ni la
/// perfection ni le zéro. C'est une convention, elle est écrite, et elle est
/// annoncée — ce que le PRD appelle « transparence ».
pub fn note_de_matching(somme_notes: u32, nombre: u32) -> f64 {
    wilson(somme_notes, nombre).unwrap_or(PRIOR_SANS_NOTE) * f64::from(NOTE_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Comparaison à un millième : au-delà, on testerait la représentation des
    /// flottants plutôt que le calcul.
    fn proche(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    // === @happy ===

    #[test]
    fn happy_cinquante_notes_solides_donnent_une_borne_haute() {
        // 50 notes à 4,5 étoiles : p = 0,9.
        let w = wilson(225, 50).unwrap();
        assert!(proche(w, 0.786), "obtenu {w}");
    }

    #[test]
    fn happy_la_note_de_matching_est_sur_cinq() {
        // La comparaison porte sur la relation — cinq fois la borne — et non
        // sur une valeur recopiée : arrondir à trois décimales puis multiplier
        // par cinq testerait la tolérance plutôt que le calcul.
        let note = note_de_matching(225, 50);
        assert!(
            proche(note, wilson(225, 50).unwrap() * 5.0),
            "obtenu {note}"
        );
        assert!((0.0..=5.0).contains(&note));
        assert!(proche(note, 3.932), "obtenu {note}");
    }

    // === @negative ===

    #[test]
    fn negative_sans_note_il_n_y_a_rien_a_calculer() {
        // Rendre zéro ferait passer un prestataire qui n'a jamais travaillé
        // pour un prestataire détestable.
        assert_eq!(wilson(0, 0), None);
    }

    #[test]
    fn happy_un_prestataire_sans_note_vaut_quatre_etoiles_au_matching() {
        // Ni la perfection ni le zéro : une convention écrite et annoncée.
        assert!(proche(note_de_matching(0, 0), 4.0));
    }

    #[test]
    fn security_le_prior_ne_vaut_pas_mieux_qu_une_bonne_reputation() {
        // **C'est la raison d'être du prior.** Sans lui, le score redistribue le
        // poids de la note et classe le non-noté sur sa propre moyenne, ce qui
        // revient à lui prêter la meilleure note compatible avec son profil :
        // un compte tout neuf passerait devant un artisan à cinquante avis.
        let sans_note = note_de_matching(0, 0);
        let bien_note = note_de_matching(250, 50);
        assert!(
            bien_note > sans_note,
            "cinquante notes parfaites ({bien_note}) doivent valoir mieux que l'inconnu ({sans_note})"
        );
    }

    #[test]
    fn security_le_prior_vaut_mieux_qu_une_mauvaise_reputation() {
        // L'autre moitié : n'avoir pas encore travaillé ne doit pas coûter plus
        // cher qu'avoir mal travaillé.
        let sans_note = note_de_matching(0, 0);
        let mal_note = note_de_matching(60, 50);
        assert!(
            sans_note > mal_note,
            "l'inconnu ({sans_note}) doit valoir mieux qu'une mauvaise réputation ({mal_note})"
        );
    }

    // === @edge ===

    #[test]
    fn edge_une_seule_note_parfaite_reste_modeste() {
        // C'est toute la raison d'être de Wilson : une note isolée est peu
        // informative, et le calcul le dit.
        let w = wilson(5, 1).unwrap();
        assert!(proche(w, 0.207), "obtenu {w}");
        assert!(w < 0.5);
    }

    #[test]
    fn edge_les_valeurs_illustratives_du_prd_ne_suivent_pas_sa_formule() {
        // FR-037 annonce ≈ 0,45 et ≈ 0,83 ; sa propre formule donne 0,207 et
        // 0,786. Ce test fixe l'écart pour qu'il soit constaté plutôt que
        // redécouvert, et pour qu'un changement de formule le fasse tomber.
        assert!(proche(wilson(5, 1).unwrap(), 0.207));
        assert!(proche(wilson(225, 50).unwrap(), 0.786));
    }

    #[test]
    fn edge_que_des_notes_minimales_ne_donnent_pas_zero() {
        // Une étoile reste une intervention faite, pas un échec total : p vaut
        // 0,2 et non 0.
        let w = wilson(10, 10).unwrap();
        assert!(w > 0.0, "obtenu {w}");
        assert!(w < 0.2, "une réputation d'une étoile doit rester basse");
    }

    #[test]
    fn edge_beaucoup_de_notes_parfaites_approchent_un() {
        let w = wilson(5_000, 1_000).unwrap();
        assert!(w > 0.99, "obtenu {w}");
        assert!(w <= 1.0);
    }

    // === @security ===

    #[test]
    fn security_une_note_isolee_ne_classe_pas_devant_un_historique_solide() {
        // La propriété que FR-037 `@happy` demande, et la raison d'être de
        // toute cette story : sans elle, un faux compte avec un avis complice
        // passerait devant un artisan de vingt ans.
        let isolee = wilson(5, 1).unwrap();
        let etabli = wilson(225, 50).unwrap();
        assert!(
            isolee < etabli,
            "note isolée {isolee} contre historique {etabli}"
        );
    }

    #[test]
    fn security_le_resultat_reste_borne_meme_sur_des_donnees_absurdes() {
        // Une somme incohérente ne peut venir que d'une donnée corrompue ; le
        // calcul ne doit ni déborder ni produire un NaN qui contaminerait tout
        // un classement.
        for (somme, nombre) in [(u32::MAX, 1), (0, 1), (1, u32::MAX), (u32::MAX, u32::MAX)] {
            let w = wilson(somme, nombre).expect("une note au moins");
            assert!(w.is_finite(), "{somme}/{nombre} : {w}");
            assert!((0.0..=1.0).contains(&w), "{somme}/{nombre} : {w}");
        }
    }

    #[test]
    fn security_ajouter_une_mauvaise_note_ne_remonte_jamais_la_reputation() {
        // Monotonie : à nombre égal, plus d'étoiles ne peut pas faire baisser,
        // et une note de plus au minimum ne peut pas faire monter. Sans cela,
        // quelqu'un aurait intérêt à se faire mal noter.
        let base = wilson(200, 50).unwrap();
        assert!(
            wilson(205, 50).unwrap() > base,
            "plus d'étoiles doit monter"
        );
        assert!(
            wilson(201, 51).unwrap() < base,
            "une note d'une étoile de plus doit faire baisser"
        );
    }
}
