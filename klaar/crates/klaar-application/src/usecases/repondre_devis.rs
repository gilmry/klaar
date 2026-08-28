//! Cas d'usage « accepter ou refuser un devis » (FR-017, Story 4.2).
//!
//! **Livré sans le séquestre, et c'est écrit.** FR-017 fait de l'acceptation le
//! moment où Stripe capture l'argent. Le compte n'est pas ouvert ; ce cas
//! d'usage enregistre donc l'accord, et la capture rejoindra l'Epic 5. Un
//! accord enregistré sans capture est un état honnête — le devis dit ce qui a
//! été convenu — là où attendre Stripe aurait laissé le demandeur devant un
//! devis qu'il ne peut ni accepter ni refuser.
//!
//! **Seul le demandeur répond.** Le chemin d'autorisation passe par la Demande
//! dont la Mission est née : le devis appartient à une Mission, la Mission à une
//! Demande, la Demande à un compte. Aucun identifiant reçu n'entre dans cette
//! chaîne.
//!
//! **La course est tenue par la base.** Deux « accepter » simultanés, ou un
//! « accepter » à l'instant où le balayage expire le devis : c'est le
//! compare-and-swap du dépôt qui tranche, pas la lecture faite ici.

use klaar_payment::{Devis, DevisError, MotifRefus, StatutDevis};
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::devis_repository::DevisRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::mission_repository::MissionRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurReponse {
    /// Devis inconnu, ou qui ne concerne pas ce compte.
    ///
    /// Un seul cas pour les deux, par la même précédence anti-énumération que
    /// le reste du service.
    Introuvable,
    /// Motif de refus hors du vocabulaire.
    MotifInconnu,
    /// Refusé par le domaine : expiré, ou déjà répondu.
    Domaine(DevisError),
    /// Le devis a changé entre la lecture et l'écriture.
    Concurrence,
    Indisponible(String),
}

impl ErreurReponse {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "QUOTE_NOT_FOUND",
            Self::MotifInconnu => "REASON_UNKNOWN",
            Self::Domaine(e) => e.code(),
            // Le même code qu'un devis déjà répondu, et c'est juste : dans les
            // deux cas, le devis n'était plus celui que le demandeur croyait
            // avoir sous les yeux.
            Self::Concurrence => "QUOTE_ALREADY_ANSWERED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurReponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "devis introuvable"),
            Self::MotifInconnu => write!(f, "motif de refus inconnu"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Concurrence => write!(f, "le devis a changé entre-temps"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurReponse {}

impl From<RepositoryError> for ErreurReponse {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que le demandeur décide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reponse<'a> {
    Accepter,
    /// Motif facultatif : exiger une raison obligerait à en choisir une pour
    /// dire non, ce qui n'est pas dû.
    Refuser(Option<&'a str>),
}

/// Ce que la réponse produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevisRepondu {
    pub devis: Devis,
    /// La Mission concernée, pour prévenir le prestataire sans la relire.
    pub mission_id: Uuid,
    pub provider_id: Uuid,
}

/// Enregistre la réponse du demandeur à un devis.
pub async fn repondre<D, M, Q, H>(
    demandes: &D,
    missions: &M,
    devis_repo: &Q,
    horloge: &H,
    utilisateur_id: Uuid,
    devis_id: Uuid,
    reponse: Reponse<'_>,
) -> Result<DevisRepondu, ErreurReponse>
where
    D: DemandeRepository,
    M: MissionRepository,
    Q: DevisRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();

    // Le motif est validé avant toute lecture : un vocabulaire inconnu est une
    // erreur de client, et la traiter après aurait dit au passage que ce devis
    // existe.
    let motif = match reponse {
        Reponse::Refuser(Some(brut)) => {
            Some(MotifRefus::parse(brut).ok_or(ErreurReponse::MotifInconnu)?)
        }
        _ => None,
    };

    let mut devis = devis_repo
        .par_id(devis_id)
        .await?
        .ok_or(ErreurReponse::Introuvable)?;

    // La chaîne d'appartenance, maillon par maillon. Chaque échec rend le même
    // refus indistinct : le demandeur légitime ne les distingue pas non plus.
    let mission = missions
        .par_id(devis.mission_id)
        .await?
        .ok_or(ErreurReponse::Introuvable)?;
    let demande = demandes
        .par_id(mission.demande_id)
        .await?
        .ok_or(ErreurReponse::Introuvable)?;
    if demande.demandeur_id != utilisateur_id {
        return Err(ErreurReponse::Introuvable);
    }

    // Le domaine décide, avec l'heure du serveur. Cette vérification double
    // celle du dépôt et ne la remplace pas : elle donne le bon code d'erreur —
    // « expiré » plutôt que « déjà répondu » — là où le compare-and-swap ne
    // rend qu'un booléen.
    match reponse {
        Reponse::Accepter => devis.accepter(maintenant),
        Reponse::Refuser(_) => devis.refuser(motif, maintenant),
    }
    .map_err(ErreurReponse::Domaine)?;

    let ecrit = devis_repo
        .repondre(devis.id, devis.statut, motif.map(|m| m.as_str()))
        .await?;
    if !ecrit {
        return Err(ErreurReponse::Concurrence);
    }

    Ok(DevisRepondu {
        devis,
        mission_id: mission.id,
        provider_id: mission.provider_id,
    })
}

/// Traduit le statut écrit en code rendu au client.
pub fn code_reponse(statut: StatutDevis) -> &'static str {
    match statut {
        StatutDevis::Accepte => "QUOTE_ACCEPTED",
        StatutDevis::Refuse => "QUOTE_REFUSED",
        // Injoignable par ce cas d'usage : `accepter` et `refuser` sont les
        // deux seules issues. Le `match` reste exhaustif pour qu'un statut
        // ajouté un jour passe par ici.
        StatutDevis::Envoye | StatutDevis::Expire => "QUOTE_ANSWERED",
    }
}
