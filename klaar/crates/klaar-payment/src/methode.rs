//! Méthodes de paiement d'un compte (FR-006, Story 1.7).
//!
//! **Le numéro de carte ne passe par aucun de nos serveurs.** Il est capté par
//! l'iframe du prestataire de paiement, qui rend une référence opaque : c'est
//! elle, et elle seule, que le service conserve. Le périmètre PCI SAQ-A tient à
//! cette seule phrase, et ce module ne manipule jamais autre chose.
//!
//! **Ce qui demande un compte, et ce qui n'en demande pas.** Créer la méthode
//! chez le prestataire, la détacher, lire un refus de carte : cela demande un
//! compte. Combien de cartes on garde, laquelle est celle par défaut quand on
//! en supprime une, et si celle qu'on s'apprête à débiter est encore
//! valable : cela ne demande rien, et c'est là que se jouent les erreurs qui
//! coûtent — une Demande partie sur une carte expirée envoie un prestataire
//! chez quelqu'un dont le paiement échouera.

use chrono::{DateTime, Datelike, Utc};
use std::fmt;
use uuid::Uuid;

/// Cartes enregistrables par compte (FR-006 `@edge`).
///
/// Cinq. Au-delà, ce n'est plus un choix mais une liste qu'on ne relit pas, et
/// chaque référence conservée est une donnée de plus à effacer le jour du
/// droit à l'oubli.
pub const CARTES_MAX: usize = 5;

/// Une méthode de paiement, réduite à ce que le service a le droit de garder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodePaiement {
    pub id: Uuid,
    pub utilisateur_id: Uuid,
    /// Référence opaque chez le prestataire de paiement (`pm_…`).
    ///
    /// **Ce n'est pas un numéro de carte**, et le type ne permet pas d'en
    /// stocker un : il n'y a aucun champ où le mettre.
    pub reference: String,
    /// Les quatre derniers chiffres. Autorisés par PCI, et nécessaires pour
    /// que quelqu'un distingue la carte qu'il supprime de celle qu'il garde.
    pub derniers_chiffres: String,
    pub marque: String,
    pub expire_mois: u32,
    pub expire_annee: i32,
    pub par_defaut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodeError {
    /// Le plafond est atteint (FR-006 `@edge`).
    PlafondAtteint,
    /// La carte est expirée (FR-006 `@edge`).
    CarteExpiree,
    /// Référence vide, ou qui ressemble à un numéro de carte.
    ReferenceInvalide,
    /// Quatre chiffres attendus.
    DerniersChiffresInvalides,
    /// Mois hors de 1..=12.
    EcheanceInvalide,
    /// La méthode n'appartient pas à ce compte, ou n'existe pas.
    Introuvable,
}

impl MethodeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PlafondAtteint => "MAX_CARDS_REACHED",
            Self::CarteExpiree => "CARD_EXPIRED",
            Self::ReferenceInvalide => "PAYMENT_REFERENCE_INVALID",
            Self::DerniersChiffresInvalides => "CARD_LAST4_INVALID",
            Self::EcheanceInvalide => "CARD_EXPIRY_INVALID",
            Self::Introuvable => "PAYMENT_METHOD_NOT_FOUND",
        }
    }
}

impl fmt::Display for MethodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlafondAtteint => write!(f, "{CARTES_MAX} cartes au maximum"),
            Self::CarteExpiree => write!(f, "cette carte a expiré"),
            Self::ReferenceInvalide => write!(f, "référence de paiement invalide"),
            Self::DerniersChiffresInvalides => write!(f, "quatre chiffres attendus"),
            Self::EcheanceInvalide => write!(f, "échéance invalide"),
            Self::Introuvable => write!(f, "méthode de paiement introuvable"),
        }
    }
}

impl std::error::Error for MethodeError {}

/// Ce que le prestataire de paiement rend après l'enregistrement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarteEnregistree {
    pub reference: String,
    pub derniers_chiffres: String,
    pub marque: String,
    pub expire_mois: u32,
    pub expire_annee: i32,
}

/// Enregistre une carte déjà créée chez le prestataire.
///
/// `deja` est le nombre de cartes du compte. La première devient celle par
/// défaut **automatiquement** (FR-006 `@happy`) : sans cela, quelqu'un qui
/// ajoute sa seule carte n'en aurait aucune de sélectionnée et sa Demande
/// serait refusée sans qu'il comprenne.
pub fn enregistrer(
    utilisateur_id: Uuid,
    carte: CarteEnregistree,
    deja: usize,
    maintenant: DateTime<Utc>,
) -> Result<MethodePaiement, MethodeError> {
    if deja >= CARTES_MAX {
        return Err(MethodeError::PlafondAtteint);
    }
    verifier_reference(&carte.reference)?;
    if carte.derniers_chiffres.len() != 4
        || !carte.derniers_chiffres.chars().all(|c| c.is_ascii_digit())
    {
        return Err(MethodeError::DerniersChiffresInvalides);
    }
    if !(1..=12).contains(&carte.expire_mois) {
        return Err(MethodeError::EcheanceInvalide);
    }
    // Enregistrer une carte déjà expirée serait accepter une méthode
    // inutilisable : le refus arriverait à la première Demande, au pire moment.
    if expiree(carte.expire_mois, carte.expire_annee, maintenant) {
        return Err(MethodeError::CarteExpiree);
    }

    Ok(MethodePaiement {
        id: Uuid::new_v4(),
        utilisateur_id,
        reference: carte.reference,
        derniers_chiffres: carte.derniers_chiffres,
        marque: carte.marque,
        expire_mois: carte.expire_mois,
        expire_annee: carte.expire_annee,
        par_defaut: deja == 0,
    })
}

/// Qui devient la carte par défaut après une suppression (FR-006 `@happy`).
///
/// **La plus récemment ajoutée parmi celles qui restent.** Un compte sans carte
/// par défaut ne peut plus rien demander ; laisser le choix vide après une
/// suppression transformerait un geste anodin en blocage silencieux.
///
/// Rend `None` s'il ne reste rien — ce qui est un état légitime : on a le droit
/// de retirer sa dernière carte.
pub fn defaut_apres_suppression(restantes: &[MethodePaiement]) -> Option<Uuid> {
    restantes.last().map(|m| m.id)
}

/// Vrai si la carte est expirée à cette date (FR-006 `@edge`).
///
/// **Une carte expire à la fin de son mois**, pas au premier jour : une carte
/// « 08/2026 » vaut jusqu'au 31 août inclus, et la refuser le 1er août priverait
/// quelqu'un d'un mois d'usage légitime.
pub fn expiree(mois: u32, annee: i32, maintenant: DateTime<Utc>) -> bool {
    let (annee_courante, mois_courant) = (maintenant.year(), maintenant.month());
    annee < annee_courante || (annee == annee_courante && mois < mois_courant)
}

/// La carte utilisable pour une Demande, s'il y en a une.
///
/// **L'expiration est contrôlée au moment de s'en servir**, pas seulement à
/// l'enregistrement : une carte valable en janvier ne l'est plus en mars, et
/// c'est exactement le scénario `@edge` du FR — la Demande partirait, le
/// prestataire se mettrait en route, et le paiement échouerait ensuite.
pub fn utilisable(
    methodes: &[MethodePaiement],
    maintenant: DateTime<Utc>,
) -> Result<&MethodePaiement, MethodeError> {
    let Some(defaut) = methodes.iter().find(|m| m.par_defaut) else {
        return Err(MethodeError::Introuvable);
    };
    if expiree(defaut.expire_mois, defaut.expire_annee, maintenant) {
        return Err(MethodeError::CarteExpiree);
    }
    Ok(defaut)
}

/// Contrôle sommaire de la référence.
///
/// **Une suite de treize à dix-neuf chiffres est refusée.** Ce n'est pas une
/// référence de prestataire, c'est la forme d'un numéro de carte — et si l'un
/// arrivait jusqu'ici, le refuser vaut mieux que l'écrire en base. Le contrôle
/// ne protège de rien contre un appelant malveillant ; il protège d'une erreur
/// de câblage, qui est le cas réaliste.
fn verifier_reference(reference: &str) -> Result<(), MethodeError> {
    let propre = reference.trim();
    if propre.is_empty() || propre.len() > 255 {
        return Err(MethodeError::ReferenceInvalide);
    }
    let chiffres: String = propre.chars().filter(|c| c.is_ascii_digit()).collect();
    if chiffres.len() == propre.len() && (13..=19).contains(&chiffres.len()) {
        return Err(MethodeError::ReferenceInvalide);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 9, 0, 0).unwrap()
    }

    fn carte() -> CarteEnregistree {
        CarteEnregistree {
            reference: "pm_1MqLiJLkdIwHu7ixUEgbFdYF".to_string(),
            derniers_chiffres: "4242".to_string(),
            marque: "visa".to_string(),
            expire_mois: 12,
            expire_annee: 2030,
        }
    }

    fn enregistree(defaut: bool, mois: u32, annee: i32) -> MethodePaiement {
        MethodePaiement {
            id: Uuid::new_v4(),
            utilisateur_id: Uuid::new_v4(),
            reference: "pm_exemple".to_string(),
            derniers_chiffres: "4242".to_string(),
            marque: "visa".to_string(),
            expire_mois: mois,
            expire_annee: annee,
            par_defaut: defaut,
        }
    }

    #[test]
    fn happy_la_premiere_carte_devient_celle_par_defaut() {
        // Sans cela, quelqu'un qui ajoute sa seule carte n'en aurait aucune de
        // sélectionnée, et sa Demande serait refusée sans qu'il comprenne.
        let m = enregistrer(Uuid::new_v4(), carte(), 0, t0()).unwrap();
        assert!(m.par_defaut);
    }

    #[test]
    fn happy_les_suivantes_ne_prennent_pas_la_place() {
        let m = enregistrer(Uuid::new_v4(), carte(), 1, t0()).unwrap();
        assert!(!m.par_defaut, "changer de défaut est un geste explicite");
    }

    #[test]
    fn edge_le_plafond_de_cinq_cartes_est_tenu() {
        // FR-006 `@edge` : 422 `MAX_CARDS_REACHED`.
        assert!(enregistrer(Uuid::new_v4(), carte(), CARTES_MAX - 1, t0()).is_ok());
        assert_eq!(
            enregistrer(Uuid::new_v4(), carte(), CARTES_MAX, t0()),
            Err(MethodeError::PlafondAtteint)
        );
    }

    #[test]
    fn happy_la_suppression_du_defaut_en_promeut_un_autre() {
        // FR-006 `@happy` : « l'autre carte devient default automatiquement ».
        // Un compte sans carte par défaut ne peut plus rien demander.
        let restantes = vec![enregistree(false, 12, 2030), enregistree(false, 6, 2031)];
        assert_eq!(
            defaut_apres_suppression(&restantes),
            Some(restantes[1].id),
            "la plus récemment ajoutée parmi celles qui restent"
        );
    }

    #[test]
    fn edge_retirer_sa_derniere_carte_est_legitime() {
        // Rendre une erreur ici obligerait à garder une carte enregistrée pour
        // toujours, ce qui va contre le droit à l'effacement.
        assert_eq!(defaut_apres_suppression(&[]), None);
    }

    #[test]
    fn edge_une_carte_expire_a_la_fin_de_son_mois() {
        // Une carte « 08/2026 » vaut jusqu'au 31 août inclus. La refuser le
        // 1er août priverait quelqu'un d'un mois d'usage légitime.
        assert!(!expiree(8, 2026, t0()), "le mois courant vaut encore");
        assert!(expiree(7, 2026, t0()), "le mois précédent est passé");
        assert!(!expiree(9, 2026, t0()));
        assert!(expiree(12, 2025, t0()), "l'année précédente est passée");
        assert!(!expiree(1, 2027, t0()));
    }

    #[test]
    fn negative_une_carte_deja_expiree_ne_s_enregistre_pas() {
        // Le refus arriverait sinon à la première Demande, au pire moment.
        let mut vieille = carte();
        vieille.expire_mois = 7;
        vieille.expire_annee = 2026;
        assert_eq!(
            enregistrer(Uuid::new_v4(), vieille, 0, t0()),
            Err(MethodeError::CarteExpiree)
        );
    }

    #[test]
    fn edge_la_carte_par_defaut_est_recontrolee_a_l_usage() {
        // FR-006 `@edge` : une carte valable en janvier ne l'est plus en mars.
        // Sans ce contrôle, la Demande partirait, le prestataire se mettrait en
        // route, et le paiement échouerait ensuite.
        let methodes = vec![enregistree(true, 7, 2026)];
        assert_eq!(
            utilisable(&methodes, t0()).unwrap_err(),
            MethodeError::CarteExpiree
        );

        let valides = vec![enregistree(true, 12, 2030)];
        assert!(utilisable(&valides, t0()).is_ok());
    }

    #[test]
    fn negative_sans_carte_par_defaut_rien_n_est_utilisable() {
        // Deux cartes dont aucune n'est le défaut : le service ne doit pas en
        // choisir une au hasard, il doit dire qu'il n'y en a pas.
        let methodes = vec![enregistree(false, 12, 2030), enregistree(false, 6, 2031)];
        assert_eq!(
            utilisable(&methodes, t0()).unwrap_err(),
            MethodeError::Introuvable
        );
        assert_eq!(
            utilisable(&[], t0()).unwrap_err(),
            MethodeError::Introuvable
        );
    }

    #[test]
    fn security_une_suite_de_chiffres_ressemblant_a_une_carte_est_refusee() {
        // Le périmètre PCI tient à ce qu'aucun numéro n'atteigne nos serveurs.
        // Ce contrôle ne protège pas d'un appelant malveillant — il protège
        // d'une erreur de câblage, qui est le cas réaliste.
        for suspect in [
            "4242424242424242",
            "4242424242424",
            "4242424242424242424"[..19].to_string().as_str(),
        ] {
            let mut c = carte();
            c.reference = suspect.to_string();
            assert_eq!(
                enregistrer(Uuid::new_v4(), c, 0, t0()),
                Err(MethodeError::ReferenceInvalide),
                "référence acceptée à tort : {suspect}"
            );
        }
    }

    #[test]
    fn happy_une_reference_de_prestataire_ordinaire_passe() {
        // Elle porte des lettres : ce n'est pas une suite de chiffres.
        assert!(enregistrer(Uuid::new_v4(), carte(), 0, t0()).is_ok());
    }

    #[test]
    fn negative_une_reference_vide_est_refusee() {
        for mauvaise in ["", "   ", &"p".repeat(256)] {
            let mut c = carte();
            c.reference = mauvaise.to_string();
            assert_eq!(
                enregistrer(Uuid::new_v4(), c, 0, t0()),
                Err(MethodeError::ReferenceInvalide)
            );
        }
    }

    #[test]
    fn negative_les_quatre_chiffres_sont_exiges() {
        for mauvais in ["424", "42424", "42a2", ""] {
            let mut c = carte();
            c.derniers_chiffres = mauvais.to_string();
            assert_eq!(
                enregistrer(Uuid::new_v4(), c, 0, t0()),
                Err(MethodeError::DerniersChiffresInvalides)
            );
        }
    }

    #[test]
    fn negative_un_mois_hors_bornes_est_refuse() {
        for mois in [0, 13, 99] {
            let mut c = carte();
            c.expire_mois = mois;
            assert_eq!(
                enregistrer(Uuid::new_v4(), c, 0, t0()),
                Err(MethodeError::EcheanceInvalide)
            );
        }
    }

    #[test]
    fn security_le_type_ne_permet_pas_de_stocker_un_numero() {
        // Il n'y a aucun champ où le mettre : c'est la garantie structurelle du
        // périmètre PCI SAQ-A, et elle ne dépend d'aucune vigilance.
        let m = enregistrer(Uuid::new_v4(), carte(), 0, t0()).unwrap();
        assert_eq!(m.derniers_chiffres.len(), 4);
        assert!(m.reference.starts_with("pm_"));
    }
}
