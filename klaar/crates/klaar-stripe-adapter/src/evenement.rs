//! Événements de webhook : vocabulaire, idempotence, ordre (FR-028, Story 5.5).
//!
//! **Un webhook n'arrive ni une fois ni dans l'ordre.** Stripe garantit
//! l'« au moins une fois » et rien de plus : le même événement peut arriver
//! trois fois, et une capture peut arriver après le remboursement qui l'a
//! suivie. Un service qui suppose l'inverse encaisse deux fois ou rembourse un
//! paiement qu'il croit encore autorisé.
//!
//! **Ce module ne parle à personne.** Il décide de ce qu'un événement veut
//! dire et de ce qu'il faut en faire ; l'appel réseau et la table
//! d'idempotence sont ailleurs. C'est ce qui permet de l'écrire et de le
//! vérifier entièrement sans compte Stripe.

use chrono::{DateTime, Utc};
use std::fmt;

/// Les événements que le service traite.
///
/// **Une liste fermée, et non un `String`.** Stripe en envoie des dizaines ;
/// n'accepter que ceux qui déclenchent une écriture évite qu'un type nouveau
/// tombe dans un chemin qui n'a pas été pensé pour lui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeEvenement {
    /// La pré-autorisation a réussi : le séquestre est en place.
    PaiementAutorise,
    /// La capture a réussi : l'argent a bougé.
    PaiementCapture,
    /// Le paiement a échoué ou a été refusé par la banque.
    PaiementEchoue,
    /// Un remboursement, total ou partiel, a été exécuté.
    Rembourse,
    /// Le compte Connect d'un prestataire a changé d'état (FR-024).
    CompteConnectMisAJour,
    /// Le versement au prestataire a été exécuté.
    VersementEffectue,
}

impl TypeEvenement {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PaiementAutorise => "payment_intent.amount_capturable_updated",
            Self::PaiementCapture => "payment_intent.succeeded",
            Self::PaiementEchoue => "payment_intent.payment_failed",
            Self::Rembourse => "charge.refunded",
            Self::CompteConnectMisAJour => "account.updated",
            Self::VersementEffectue => "transfer.paid",
        }
    }

    /// Rend `None` pour un type que le service ne traite pas.
    ///
    /// **`None` n'est pas une erreur.** FR-028 veut un 200 sur un événement
    /// inconnu : répondre autre chose ferait réessayer Stripe indéfiniment pour
    /// un message dont on n'a que faire, et finirait par faire désactiver
    /// l'endpoint.
    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "payment_intent.amount_capturable_updated" => Some(Self::PaiementAutorise),
            "payment_intent.succeeded" => Some(Self::PaiementCapture),
            "payment_intent.payment_failed" => Some(Self::PaiementEchoue),
            "charge.refunded" => Some(Self::Rembourse),
            "account.updated" => Some(Self::CompteConnectMisAJour),
            "transfer.paid" => Some(Self::VersementEffectue),
            _ => None,
        }
    }
}

/// Un événement reçu, réduit à ce dont le service a besoin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evenement {
    /// Identifiant Stripe (`evt_…`). C'est la clé d'idempotence.
    pub id: String,
    pub type_: TypeEvenement,
    /// Horodatage **de Stripe**, et non celui de la réception.
    ///
    /// C'est lui qui donne l'ordre réel : deux webhooks arrivés à l'envers ont
    /// des dates de création qui, elles, ne mentent pas.
    pub cree_le: DateTime<Utc>,
    /// L'objet concerné (`pi_…`, `acct_…`, `tr_…`).
    pub objet_id: String,
}

/// Ce qu'il faut faire d'un événement reçu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suite {
    /// Appliquer l'effet, puis consigner l'identifiant.
    Appliquer,
    /// Déjà traité : accuser réception sans rien rejouer (FR-028 `@negative`).
    DejaTraite,
    /// Arrivé après un événement plus récent déjà appliqué : accuser réception
    /// sans revenir en arrière (FR-028 `@edge`).
    ///
    /// **Ce n'est pas un rejet.** Un remboursement suivi d'une capture retardée
    /// ne doit pas défaire le remboursement ; l'état final est celui du plus
    /// récent, et l'ancien est consigné pour la trace sans être appliqué.
    Depasse,
    /// Type non traité : accuser réception, ne rien faire.
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvenementError {
    /// Identifiant vide ou hors format.
    IdentifiantInvalide,
}

impl EvenementError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::IdentifiantInvalide => "EVENT_ID_INVALID",
        }
    }
}

impl fmt::Display for EvenementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdentifiantInvalide => write!(f, "identifiant d'événement invalide"),
        }
    }
}

impl std::error::Error for EvenementError {}

/// Longueur maximale d'un identifiant Stripe accepté.
///
/// Ils font une trentaine de caractères ; la borne évite qu'un envoi
/// fabriqué remplisse la table d'idempotence avec des clés d'un mégaoctet.
pub const ID_MAX_CARACTERES: usize = 255;

/// Décide du sort d'un événement.
///
/// `deja_vu` dit si cet identifiant précis a déjà été consigné.
/// `dernier_applique` est l'horodatage Stripe du dernier événement **appliqué
/// au même objet**, s'il y en a un.
///
/// **La décision est prise ici, hors de toute base.** C'est ce qui permet de
/// vérifier les quatre cas — nominal, doublon, désordre, type inconnu — sans
/// PostgreSQL ni Stripe.
pub fn decider(
    evenement: &Evenement,
    deja_vu: bool,
    dernier_applique: Option<DateTime<Utc>>,
) -> Suite {
    if deja_vu {
        return Suite::DejaTraite;
    }
    match dernier_applique {
        // **Strictement antérieur.** Deux événements du même horodatage
        // s'appliquent tous deux : Stripe date à la seconde, et deux
        // changements dans la même seconde sont possibles. Les écarter
        // perdrait le second sans que rien ne le dise.
        Some(dernier) if evenement.cree_le < dernier => Suite::Depasse,
        _ => Suite::Appliquer,
    }
}

/// Valide la forme d'un identifiant d'événement.
pub fn valider_id(id: &str) -> Result<(), EvenementError> {
    let propre = id.trim();
    if propre.is_empty()
        || propre.len() > ID_MAX_CARACTERES
        || !propre.starts_with("evt_")
        // Les identifiants Stripe sont alphanumériques avec des soulignés.
        // Le vérifier évite qu'une clé porteuse de séparateurs se retrouve
        // dans un journal ou une requête construite ailleurs.
        || !propre
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(EvenementError::IdentifiantInvalide);
    }
    Ok(())
}

/// Trie une rafale d'événements par horodatage Stripe (FR-028 `@edge`).
///
/// **Tri stable, et sur l'horodatage de Stripe.** Trier sur l'ordre d'arrivée
/// reviendrait à ne pas trier ; le tri stable garde l'ordre d'arrivée entre
/// deux événements de même seconde, ce qui est la meilleure information
/// disponible à cette granularité.
pub fn ordonner(evenements: &mut [Evenement]) {
    evenements.sort_by_key(|e| e.cree_le);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap()
    }

    fn evenement(id: &str, quand: DateTime<Utc>) -> Evenement {
        Evenement {
            id: id.to_string(),
            type_: TypeEvenement::PaiementCapture,
            cree_le: quand,
            objet_id: "pi_exemple".to_string(),
        }
    }

    #[test]
    fn happy_un_evenement_neuf_est_applique() {
        assert_eq!(
            decider(&evenement("evt_1", t0()), false, None),
            Suite::Appliquer
        );
    }

    #[test]
    fn negative_un_evenement_deja_vu_n_est_pas_rejoue() {
        // FR-028 `@negative` : réponse 200, aucune action rejouée. Sans cela,
        // une capture arrivée trois fois encaisserait trois fois.
        assert_eq!(
            decider(&evenement("evt_1", t0()), true, Some(t0())),
            Suite::DejaTraite
        );
    }

    #[test]
    fn edge_un_evenement_retarde_ne_defait_pas_un_plus_recent() {
        // FR-028 `@edge` : A capturé et B remboursé arrivent dans le désordre.
        // Appliquer A après B rouvrirait une capture déjà remboursée.
        let ancien = evenement("evt_capture", t0());
        let dernier = t0() + Duration::seconds(30);
        assert_eq!(decider(&ancien, false, Some(dernier)), Suite::Depasse);
    }

    #[test]
    fn edge_deux_evenements_de_la_meme_seconde_s_appliquent_tous_deux() {
        // Stripe date à la seconde : les écarter perdrait le second sans que
        // rien ne le dise.
        let e = evenement("evt_2", t0());
        assert_eq!(decider(&e, false, Some(t0())), Suite::Appliquer);
    }

    #[test]
    fn edge_un_webhook_vieux_de_deux_heures_est_traite_normalement() {
        // FR-028 `@edge` : le retard n'est pas un motif de refus **au niveau de
        // l'événement**. La fenêtre de cinq minutes du module de signature
        // porte sur l'anti-rejeu du transport, pas sur l'âge métier ; les
        // confondre ferait perdre des événements réels après un incident réseau.
        let vieux = evenement("evt_retarde", t0() - Duration::hours(2));
        assert_eq!(decider(&vieux, false, None), Suite::Appliquer);
    }

    #[test]
    fn happy_le_tri_remet_les_evenements_dans_l_ordre_de_stripe() {
        let mut rafale = vec![
            evenement("evt_b", t0() + Duration::seconds(30)),
            evenement("evt_a", t0()),
            evenement("evt_c", t0() + Duration::seconds(60)),
        ];
        ordonner(&mut rafale);
        let ids: Vec<&str> = rafale.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["evt_a", "evt_b", "evt_c"]);
    }

    #[test]
    fn edge_le_tri_est_stable_a_horodatage_egal() {
        // L'ordre d'arrivée est la meilleure information disponible quand
        // Stripe ne distingue pas les deux.
        let mut rafale = vec![
            evenement("evt_premier", t0()),
            evenement("evt_second", t0()),
        ];
        ordonner(&mut rafale);
        assert_eq!(rafale[0].id, "evt_premier");
    }

    #[test]
    fn happy_le_vocabulaire_fait_l_aller_retour() {
        for t in [
            TypeEvenement::PaiementAutorise,
            TypeEvenement::PaiementCapture,
            TypeEvenement::PaiementEchoue,
            TypeEvenement::Rembourse,
            TypeEvenement::CompteConnectMisAJour,
            TypeEvenement::VersementEffectue,
        ] {
            assert_eq!(TypeEvenement::parse(t.as_str()), Some(t));
        }
    }

    #[test]
    fn edge_un_type_inconnu_n_est_pas_une_erreur() {
        // Répondre autre chose qu'un accusé de réception ferait réessayer
        // Stripe indéfiniment, puis désactiver l'endpoint.
        assert_eq!(TypeEvenement::parse("invoice.created"), None);
        assert_eq!(TypeEvenement::parse(""), None);
    }

    #[test]
    fn security_un_identifiant_hors_format_est_refuse() {
        for id in [
            "",
            "   ",
            // Sans le préfixe : ce n'est pas un identifiant d'événement.
            "pi_1234",
            // Séparateurs : une clé pareille finirait dans un journal ou une
            // requête construite ailleurs.
            "evt_1;DROP TABLE",
            "evt_1 2",
            "evt_a/b",
            &format!("evt_{}", "a".repeat(ID_MAX_CARACTERES)),
        ] {
            assert_eq!(
                valider_id(id),
                Err(EvenementError::IdentifiantInvalide),
                "identifiant accepté à tort : {id:?}"
            );
        }
    }

    #[test]
    fn happy_un_identifiant_stripe_ordinaire_est_accepte() {
        assert_eq!(valider_id("evt_3MtwBwLkdIwHu7ix28a3tqPa"), Ok(()));
        // Les espaces de bordure sont tolérés : ils viennent d'un en-tête, pas
        // d'une intention.
        assert_eq!(valider_id("  evt_1abc  "), Ok(()));
    }
}
