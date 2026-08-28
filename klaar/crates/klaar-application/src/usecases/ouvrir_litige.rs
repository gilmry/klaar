//! Cas d'usage « ouvrir un litige » (FR-034, Story 7.2).
//!
//! **C'est l'issue que l'annulation refuse.** Une intervention faite ne
//! s'annule pas — elle a eu lieu — mais elle peut être contestée. Sans ce
//! recours, le seul geste possible après un travail mal fait serait une
//! mauvaise note, ce qui ne rend l'argent à personne.
//!
//! **La partie est déduite, jamais reçue.** Le demandeur ouvre en tant que
//! demandeur, le prestataire en tant que prestataire ; accepter le rôle en
//! entrée laisserait quelqu'un se plaindre au nom de l'autre, et fausserait
//! tout comptage de sanctions.
//!
//! **Ce cas d'usage n'applique aucune sanction.** Il ouvre, il compte, il
//! signale. Trancher demande un humain, et suspendre sur une accusation non
//! examinée serait exactement l'inverse de ce qu'un recours doit permettre.

use chrono::Duration;
use klaar_trust::{
    examen_merite, Litige, LitigeError, MotifLitige, PartieLitige, FENETRE_DEMANDEUR_JOURS,
};
use std::fmt;
use uuid::Uuid;

use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::litige_repository::{LitigeRepository, ResultatOuverture};
use crate::ports::provider_repository::ProviderRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurLitige {
    /// Mission inconnue, ou qui ne regarde pas ce compte.
    Introuvable,
    /// L'intervention n'est pas terminée : on ne conteste pas un travail en
    /// cours.
    PasTerminee,
    /// Cette intervention a déjà son litige (FR-034 `@edge`).
    DejaLitigee,
    /// Motif hors vocabulaire.
    MotifInconnu,
    /// Refusé par le domaine.
    Domaine(LitigeError),
    Indisponible(String),
}

impl ErreurLitige {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "MISSION_NOT_FOUND",
            Self::PasTerminee => "MISSION_NOT_FINISHED",
            Self::DejaLitigee => "ALREADY_DISPUTED",
            Self::MotifInconnu => "MOTIVE_UNKNOWN",
            Self::Domaine(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurLitige {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Mission introuvable"),
            Self::PasTerminee => write!(f, "l'intervention n'est pas terminée"),
            Self::DejaLitigee => write!(f, "un litige existe déjà pour cette intervention"),
            Self::MotifInconnu => write!(f, "motif inconnu"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurLitige {}

impl From<RepositoryError> for ErreurLitige {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que l'ouverture produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LitigeOuvert {
    pub litige: Litige,
    /// Vrai quand ce compte a ouvert plusieurs litiges en peu de temps
    /// (FR-034 `@edge`).
    ///
    /// **Ce n'est pas une sanction**, c'est un signal d'exploitation : quelqu'un
    /// peut légitimement tomber deux fois sur un mauvais prestataire.
    pub a_examiner: bool,
}

/// Ce que le plaignant écrit.
pub struct Grief<'a> {
    pub motif: &'a str,
    pub description: &'a str,
}

/// Ouvre un litige sur une intervention terminée.
pub async fn ouvrir<L, P, H>(
    litiges: &L,
    prestataires: &P,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
    grief: Grief<'_>,
) -> Result<LitigeOuvert, ErreurLitige>
where
    L: LitigeRepository,
    P: ProviderRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();

    // Le motif est validé avant toute lecture : un vocabulaire inconnu est une
    // erreur de client, et le refuser après aurait dit au passage que cette
    // Mission existe.
    let motif = MotifLitige::parse(grief.motif).ok_or(ErreurLitige::MotifInconnu)?;

    let contexte = litiges
        .contexte(mission_id)
        .await?
        .ok_or(ErreurLitige::Introuvable)?;

    // La partie est déduite du rôle réel dans cette intervention.
    let partie = if contexte.demandeur_id == utilisateur_id {
        PartieLitige::Demandeur
    } else {
        match prestataires.par_utilisateur_id(utilisateur_id).await? {
            Some(p) if p.id == contexte.provider_id => PartieLitige::Prestataire,
            _ => return Err(ErreurLitige::Introuvable),
        }
    };

    // On ne conteste pas un travail en cours : il peut encore bien se terminer,
    // et ouvrir un litige à mi-parcours transformerait chaque contrariété en
    // procédure.
    let close_depuis = contexte.close_depuis.ok_or(ErreurLitige::PasTerminee)?;

    let litige = Litige::ouvrir(
        mission_id,
        utilisateur_id,
        partie,
        motif,
        grief.description,
        close_depuis,
        maintenant,
    )
    .map_err(ErreurLitige::Domaine)?;

    match litiges.ouvrir(&litige).await? {
        ResultatOuverture::Ouvert(ecrit) => {
            let depuis = maintenant - Duration::days(FENETRE_DEMANDEUR_JOURS);
            // L'échec du comptage ne remet pas le litige en cause : il est
            // ouvert, c'est le fait principal, et le signal se rattrapera.
            let ouverts = litiges
                .ouverts_par(utilisateur_id, depuis)
                .await
                .unwrap_or(0);
            Ok(LitigeOuvert {
                litige: ecrit,
                a_examiner: examen_merite(ouverts),
            })
        }
        ResultatOuverture::DejaLitigee => Err(ErreurLitige::DejaLitigee),
    }
}
