//! Cas d'usage « écrire et lire dans une conversation » (FR-030, FR-032).
//!
//! **Seules les deux parties d'une Mission se parlent.** Le contrôle passe par
//! la Demande et la fiche prestataire, jamais par un identifiant reçu.
//!
//! **Une tentative d'échange de coordonnées est refusée et consignée** (FR-032
//! `@security`). Le message, lui, n'est pas conservé : garder le texte
//! reviendrait à constituer un fichier de ce que les gens ont essayé de
//! s'écrire, pour une finalité — compter les récidives — qui n'en a pas besoin.

use chrono::Duration;
use klaar_messaging::{Message, MessageError};
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::message_repository::MessageRepository;
use crate::ports::mission_repository::MissionRepository;
use crate::ports::provider_repository::ProviderRepository;

/// Tentatives d'échange de coordonnées avant signalement (FR-032 `@security`).
pub const TENTATIVES_AVANT_SIGNALEMENT: i64 = 3;

/// Fenêtre glissante des tentatives, en jours.
pub const FENETRE_TENTATIVES_JOURS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurConversation {
    /// Mission inconnue, ou qui ne regarde pas ce compte.
    Introuvable,
    /// Refusé par le domaine.
    Domaine(MessageError),
    Indisponible(String),
}

impl ErreurConversation {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::Domaine(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurConversation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurConversation {}

impl From<RepositoryError> for ErreurConversation {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que l'envoi produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEnvoye {
    pub message: Message,
    /// Le compte à prévenir : l'autre partie.
    pub destinataire: Uuid,
}

/// Ce qu'un refus pour coordonnées entraîne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefusPourCoordonnees {
    pub tentatives: i64,
    /// Vrai au-delà du seuil : à signaler à l'exploitation.
    pub a_signaler: bool,
}

/// Les dépôts dont la conversation a besoin, groupés.
pub struct Depots<'a, D, M, P, C, H> {
    pub demandes: &'a D,
    pub missions: &'a M,
    pub prestataires: &'a P,
    pub messages: &'a C,
    pub horloge: &'a H,
}

/// Écrit un message dans la conversation d'une Mission.
pub async fn ecrire<D, M, P, C, H>(
    depots: Depots<'_, D, M, P, C, H>,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    corps: &str,
) -> Result<MessageEnvoye, ErreurConversation>
where
    D: DemandeRepository,
    M: MissionRepository,
    P: ProviderRepository,
    C: MessageRepository,
    H: Horloge,
{
    let Depots {
        demandes,
        missions,
        prestataires,
        messages,
        horloge,
    } = depots;
    let maintenant = horloge.maintenant();

    let destinataire =
        autre_partie(demandes, missions, prestataires, utilisateur_id, mission_id).await?;
    let etat = messages.etat(mission_id).await?;

    match Message::ecrire(
        mission_id,
        utilisateur_id,
        corps,
        etat.deja_ecrits,
        etat.close_depuis,
        maintenant,
    ) {
        Ok(message) => {
            messages.ecrire(&message).await?;
            Ok(MessageEnvoye {
                message,
                destinataire,
            })
        }
        Err(MessageError::CoordonneesInterdites(quoi)) => {
            // La tentative est consignée avant d'être refusée : l'échec de
            // l'écriture ne doit pas laisser passer le message, mais il ne doit
            // pas non plus faire croire à une panne. Un échec de journalisation
            // est signalé et le refus tient.
            if let Err(e) = messages
                .consigner_tentative(mission_id, utilisateur_id, quoi.as_str(), maintenant)
                .await
            {
                tracing::error!(erreur = %e, "tentative de contournement non consignée");
            }
            Err(ErreurConversation::Domaine(
                MessageError::CoordonneesInterdites(quoi),
            ))
        }
        Err(autre) => Err(ErreurConversation::Domaine(autre)),
    }
}

/// Compte les tentatives récentes de ce compte et dit s'il faut le signaler.
pub async fn bilan_tentatives<C, H>(
    messages: &C,
    horloge: &H,
    utilisateur_id: Uuid,
) -> Result<RefusPourCoordonnees, RepositoryError>
where
    C: MessageRepository,
    H: Horloge,
{
    let depuis = horloge.maintenant() - Duration::days(FENETRE_TENTATIVES_JOURS);
    let tentatives = messages.tentatives_depuis(utilisateur_id, depuis).await?;
    Ok(RefusPourCoordonnees {
        tentatives,
        a_signaler: tentatives >= TENTATIVES_AVANT_SIGNALEMENT,
    })
}

/// Lit le fil d'une Mission, pour l'une des deux parties.
pub async fn lire<D, M, P, C>(
    demandes: &D,
    missions: &M,
    prestataires: &P,
    messages: &C,
    utilisateur_id: Uuid,
    mission_id: Uuid,
) -> Result<Vec<Message>, ErreurConversation>
where
    D: DemandeRepository,
    M: MissionRepository,
    P: ProviderRepository,
    C: MessageRepository,
{
    autre_partie(demandes, missions, prestataires, utilisateur_id, mission_id).await?;
    Ok(messages.fil(mission_id).await?)
}

/// Le compte de l'autre partie, ou un refus si l'appelant n'en est pas une.
async fn autre_partie<D, M, P>(
    demandes: &D,
    missions: &M,
    prestataires: &P,
    utilisateur_id: Uuid,
    mission_id: Uuid,
) -> Result<Uuid, ErreurConversation>
where
    D: DemandeRepository,
    M: MissionRepository,
    P: ProviderRepository,
{
    let mission = missions
        .par_id(mission_id)
        .await?
        .ok_or(ErreurConversation::Introuvable)?;
    let demande = demandes
        .par_id(mission.demande_id)
        .await?
        .ok_or(ErreurConversation::Introuvable)?;

    if demande.demandeur_id == utilisateur_id {
        // Le compte du prestataire, et non sa fiche : c'est un compte qu'on
        // prévient.
        return match prestataires.par_id(mission.provider_id).await? {
            Some(p) => Ok(p.utilisateur_id),
            None => Err(ErreurConversation::Introuvable),
        };
    }

    match prestataires.par_utilisateur_id(utilisateur_id).await? {
        Some(p) if mission.appartient_a(p.id) => Ok(demande.demandeur_id),
        _ => Err(ErreurConversation::Introuvable),
    }
}
