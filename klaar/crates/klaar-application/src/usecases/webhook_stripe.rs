//! Réception d'un webhook Stripe (FR-028, Story 5.5).
//!
//! **L'endpoint est public, et c'est délibéré.** Stripe appelle depuis des
//! adresses qui changent, sans jeton à présenter : la signature HMAC tient lieu
//! d'authentification. Exiger autre chose reviendrait à ne pas recevoir les
//! webhooks du tout.
//!
//! **Ce module ne parle pas à Stripe.** Il reçoit, vérifie, décide et consigne.
//! C'est ce qui permet de l'écrire et de le vérifier entièrement sans compte —
//! et c'est la partie qu'on ne voudrait surtout pas improviser le jour où les
//! clés arrivent, puisqu'elle est le seul rempart entre un inconnu et une
//! écriture sur l'argent de quelqu'un.
//!
//! **Ce qui n'est pas fait, et pourquoi.** L'effet des événements — marquer un
//! séquestre capturé, enregistrer un remboursement — n'est pas appliqué : il
//! n'existe aucun séquestre en base, puisque rien ne peut en ouvrir sans
//! passerelle. Le décider ici et l'appliquer sur du vide donnerait l'illusion
//! d'un chemin éprouvé. Ce qui est éprouvé est la réception : signature,
//! fenêtre, idempotence, ordre.

use chrono::DateTime;
use klaar_stripe_adapter::{
    decider, valider_id, Evenement, EvenementError, SignatureError, Suite, TypeEvenement,
};
use std::fmt;

use crate::ports::erreurs::RepositoryError;
use crate::ports::evenement_stripe_repository::{Consignation, EvenementStripeRepository};
use crate::ports::horloge::Horloge;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurWebhook {
    /// Signature absente, fausse, ou hors fenêtre. Un seul code pour les trois.
    Signature(SignatureError),
    /// Corps illisible, ou identifiant hors format.
    Charge(EvenementError),
    /// Corps qui n'est pas du JSON, ou dont les champs attendus manquent.
    ChargeIllisible,
    Indisponible(String),
}

impl ErreurWebhook {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Signature(e) => e.code(),
            Self::Charge(e) => e.code(),
            Self::ChargeIllisible => "PAYLOAD_INVALID",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurWebhook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Signature(e) => write!(f, "{e}"),
            Self::Charge(e) => write!(f, "{e}"),
            Self::ChargeIllisible => write!(f, "charge de webhook illisible"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurWebhook {}

impl From<RepositoryError> for ErreurWebhook {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que la réception a produit, pour le journal et pour la réponse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reception {
    pub suite: Suite,
    /// Le type reconnu, ou `None` s'il n'est pas traité.
    pub type_: Option<TypeEvenement>,
    /// **Faux tant qu'aucun séquestre n'existe.** Rendu explicitement pour que
    /// personne ne déduise d'un 200 que l'argent a bougé.
    pub effet_applique: bool,
}

/// Lit la charge d'un webhook déjà authentifié.
///
/// Séparé de la vérification : une charge illisible et une signature fausse
/// n'ont pas la même cause, et les traiter au même endroit finirait par
/// confondre « Stripe a changé de format » avec « quelqu'un essaie ».
pub fn lire_charge(corps: &[u8]) -> Result<(Evenement, Option<TypeEvenement>), ErreurWebhook> {
    let valeur: serde_json::Value =
        serde_json::from_slice(corps).map_err(|_| ErreurWebhook::ChargeIllisible)?;

    let id = valeur
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(ErreurWebhook::ChargeIllisible)?;
    valider_id(id).map_err(ErreurWebhook::Charge)?;

    let brut = valeur
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or(ErreurWebhook::ChargeIllisible)?;
    let type_ = TypeEvenement::parse(brut);

    let secondes = valeur
        .get("created")
        .and_then(|v| v.as_i64())
        .ok_or(ErreurWebhook::ChargeIllisible)?;
    let cree_le = DateTime::from_timestamp(secondes, 0).ok_or(ErreurWebhook::ChargeIllisible)?;

    // L'objet concerné vit sous `data.object.id`. Son absence rend l'ordre
    // incalculable, donc la charge inexploitable.
    let objet_id = valeur
        .get("data")
        .and_then(|d| d.get("object"))
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .ok_or(ErreurWebhook::ChargeIllisible)?;

    Ok((
        Evenement {
            id: id.trim().to_string(),
            // Un type non traité est consigné sous celui-ci faute de mieux ;
            // `type_` rendu à part dit la vérité à l'appelant.
            type_: type_.unwrap_or(TypeEvenement::CompteConnectMisAJour),
            cree_le,
            objet_id: objet_id.to_string(),
        },
        type_,
    ))
}

/// Reçoit un webhook : vérifie, décide, consigne.
///
/// La signature est vérifiée **avant tout le reste**, y compris avant de lire
/// le JSON : analyser la charge d'un inconnu serait lui donner une surface
/// d'attaque gratuite.
pub async fn recevoir<E, H>(
    journal: &E,
    horloge: &H,
    corps: &[u8],
    entete_signature: &str,
    secret: &[u8],
) -> Result<Reception, ErreurWebhook>
where
    E: EvenementStripeRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    klaar_stripe_adapter::verifier_signature(corps, entete_signature, secret, maintenant)
        .map_err(ErreurWebhook::Signature)?;

    let (evenement, type_) = lire_charge(corps)?;

    // Un type non traité est consigné et acquitté. Répondre autre chose ferait
    // réessayer Stripe indéfiniment, puis désactiver l'endpoint.
    if type_.is_none() {
        journal
            .consigner(&evenement, Suite::Ignore, maintenant)
            .await?;
        return Ok(Reception {
            suite: Suite::Ignore,
            type_: None,
            effet_applique: false,
        });
    }

    let dernier = journal.dernier_applique(&evenement.objet_id).await?;
    // `deja_vu` vaut faux ici : c'est l'insertion qui tranche, et non une
    // lecture préalable qui laisserait passer deux réceptions simultanées.
    let suite = decider(&evenement, false, dernier);

    let suite = match journal.consigner(&evenement, suite, maintenant).await? {
        Consignation::Neuf => suite,
        Consignation::DejaVu => Suite::DejaTraite,
    };

    Ok(Reception {
        suite,
        type_,
        // **Toujours faux à ce jour.** Aucun séquestre n'existe en base : il
        // n'y a rien sur quoi appliquer l'effet, et le prétendre serait pire
        // que de ne rien faire.
        effet_applique: false,
    })
}
