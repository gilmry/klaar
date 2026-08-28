//! Lectures de suivi : ce que chacun a le droit de voir (Story 4.10).
//!
//! **Deux vues d'une même Demande, et elles ne montrent pas la même chose.**
//!
//! - Le **demandeur** voit l'état de sa Demande, et le nom de l'entreprise qui
//!   vient une fois qu'elle est attribuée. Savoir qui va sonner à sa porte est
//!   le minimum.
//! - Le **prestataire** voit ce qu'il faut pour décider : le secteur, la
//!   description, l'urgence, une distance. **Pas l'adresse.** Elle ne lui est
//!   révélée qu'une fois la Mission à lui. Faire l'inverse donnerait à dix
//!   entreprises l'adresse d'un foyer pour un dépannage que neuf d'entre elles
//!   ne feront pas.
//!
//! Cette asymétrie est la raison d'être de ce module : deux fonctions plutôt
//! qu'une vue paramétrée, pour qu'on ne puisse pas se tromper de paramètre.

use chrono::Duration;
use klaar_intervention::StatutMission;
use klaar_matching::{Demande, StatutDemande, DUREE_DIFFUSION_SECONDES};
use klaar_payment::{Devis, StatutDevis, DEVIS_MAX_PAR_MISSION};
use klaar_shared_kernel::Geo;
use std::fmt;
use uuid::Uuid;

use crate::ports::demande_repository::DemandeRepository;
use crate::ports::devis_repository::DevisRepository;
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::mission_repository::MissionRepository;
use crate::ports::provider_repository::ProviderRepository;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurConsultation {
    /// Introuvable, ou appartenant à quelqu'un d'autre. Un seul cas pour les
    /// deux, par la même précédence anti-énumération que le reste du service.
    Introuvable,
    PasPrestataire,
    Indisponible(String),
}

impl ErreurConsultation {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "REQUEST_NOT_FOUND",
            Self::PasPrestataire => "NOT_A_PROVIDER",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurConsultation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "Demande introuvable"),
            Self::PasPrestataire => write!(f, "ce compte n'est pas un prestataire"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurConsultation {}

impl From<RepositoryError> for ErreurConsultation {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Ce que le demandeur voit de sa propre Demande.
#[derive(Debug, Clone, PartialEq)]
pub struct VueDemandeur {
    pub demande: Demande,
    /// Vrai si le tour de diffusion est écoulé sans que le balayage soit passé.
    ///
    /// Exposé plutôt que masqué : sans lui, quelqu'un verrait « diffusion » sur
    /// une Demande que plus personne ne peut accepter, et attendrait pour rien.
    pub tour_ecoule: bool,
    /// Nom de l'entreprise attribuée, une fois la Mission créée.
    ///
    /// Savoir qui va sonner à sa porte est le minimum. Rien d'autre du
    /// prestataire n'est exposé.
    pub prestataire: Option<String>,
    pub mission_id: Option<Uuid>,
    pub mission_statut: Option<StatutMission>,
    /// Dernier devis reçu, quel que soit son statut (FR-016).
    ///
    /// Un devis refusé ou expiré reste visible : le faire disparaître laisserait
    /// l'écran vide sans dire ce qui s'est passé.
    pub devis: Option<VueDevis>,
}

/// Ce qu'un prestataire voit d'une Demande qui lui est proposée.
///
/// **Sans position.** La structure n'a pas de champ pour l'adresse : c'est le
/// type qui porte la garantie, pas une consigne.
#[derive(Debug, Clone, PartialEq)]
pub struct VuePrestataire {
    pub demande_id: Uuid,
    pub secteur: String,
    pub description: String,
    pub urgence: String,
    pub distance_metres: f64,
    /// Secondes restantes avant la fin du tour. Zéro si déjà écoulé.
    pub secondes_restantes: i64,
}

/// Le dernier devis d'une Mission, tel que les deux parties le voient.
///
/// **Une seule structure pour le demandeur et pour le prestataire**, contrairement
/// à la Demande. Un devis n'a rien d'asymétrique : c'est un document que l'un
/// envoie et que l'autre lit, et le cacher à moitié à l'une des deux parties
/// n'aurait aucun sens — celui qui l'a écrit connaît son prix, celui qui doit
/// l'accepter aussi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VueDevis {
    pub devis_id: Uuid,
    pub montant_htva_cents: i64,
    /// Taux en points de base : 2100, 1200 ou 600.
    pub taux_tva_bp: u16,
    pub tva_cents: i64,
    pub total_ttc_cents: i64,
    pub delai_minutes: i64,
    pub note: Option<String>,
    pub statut: StatutDevis,
    /// Secondes avant expiration, jamais négatives.
    pub secondes_restantes: i64,
    /// Vrai si l'heure de validité est passée sans que le balayage soit venu.
    ///
    /// Exposé plutôt que masqué, pour la même raison que `tour_ecoule` : sans
    /// lui, le demandeur verrait « en attente de votre réponse » sur un devis
    /// que plus personne n'honorera, et le prestataire attendrait une réponse
    /// qui ne peut plus arriver.
    pub echu: bool,
}

impl VueDevis {
    fn depuis(devis: Devis, maintenant: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            devis_id: devis.id,
            montant_htva_cents: devis.montant_htva.cents(),
            taux_tva_bp: devis.taux_tva.basis_points(),
            tva_cents: devis.tva.cents(),
            total_ttc_cents: devis.total_ttc.cents(),
            delai_minutes: devis.delai_minutes,
            note: devis.note.clone(),
            statut: devis.statut,
            secondes_restantes: devis.secondes_restantes(maintenant),
            echu: devis.est_expire(maintenant),
        }
    }
}

/// Ce que le prestataire attribué voit de sa Mission.
///
/// C'est ici, et seulement ici, que l'adresse apparaît : il doit s'y rendre.
#[derive(Debug, Clone, PartialEq)]
pub struct VueMission {
    pub mission_id: Uuid,
    pub statut: StatutMission,
    pub secteur: String,
    pub description: String,
    pub urgence: String,
    pub position: Geo,
    /// Statuts atteignables depuis l'état courant, pour que l'interface
    /// n'invente pas de bouton que le domaine refusera.
    pub suites: Vec<&'static str>,
    /// Dernier devis envoyé pour cette Mission (FR-016).
    pub devis: Option<VueDevis>,
    /// Devis restants avant que le plafond de FR-016 `@edge` ne soit atteint.
    ///
    /// Rendu pour que le prestataire sache ce qu'il lui reste **avant** de
    /// perdre la Mission, et non après. Zéro signifie qu'un nouvel envoi
    /// l'annulera.
    pub devis_restants: usize,
}

/// Lit une Demande pour son auteur.
pub async fn demande_du_demandeur<D, M, P, Q, H>(
    demandes: &D,
    missions: &M,
    prestataires: &P,
    devis_repo: &Q,
    horloge: &H,
    utilisateur_id: Uuid,
    demande_id: Uuid,
) -> Result<VueDemandeur, ErreurConsultation>
where
    D: DemandeRepository,
    M: MissionRepository,
    P: ProviderRepository,
    Q: DevisRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let demande = demandes
        .par_id(demande_id)
        .await?
        .filter(|d| d.demandeur_id == utilisateur_id)
        .ok_or(ErreurConsultation::Introuvable)?;

    let tour_ecoule = demande.statut == StatutDemande::Diffusion && demande.est_expiree(maintenant);

    // La Mission n'est cherchée que si la Demande est attribuée : interroger
    // systématiquement coûterait une requête pour rien sur le cas le plus
    // fréquent, celui d'une Demande encore en diffusion.
    let (prestataire, mission_id, mission_statut, devis) =
        if demande.statut == StatutDemande::Attribuee {
            match missions.par_demande(demande.id).await? {
                Some(mission) => {
                    let nom = prestataires
                        .par_id(mission.provider_id)
                        .await?
                        .map(|p| p.raison_sociale);
                    // Le devis n'est lu que pour une Demande attribuée, comme la
                    // Mission : sans Mission il ne peut pas en exister, et le
                    // chercher coûterait une requête pour rien sur le cas le plus
                    // fréquent.
                    let devis = devis_repo
                        .dernier_pour_mission(mission.id)
                        .await?
                        .map(|d| VueDevis::depuis(d, maintenant));
                    (nom, Some(mission.id), Some(mission.statut), devis)
                }
                None => (None, None, None, None),
            }
        } else {
            (None, None, None, None)
        };

    Ok(VueDemandeur {
        demande,
        tour_ecoule,
        prestataire,
        mission_id,
        mission_statut,
        devis,
    })
}

/// Liste les Demandes encore ouvertes proposées à ce prestataire.
pub async fn demandes_proposees<P, D, H>(
    prestataires: &P,
    demandes: &D,
    horloge: &H,
    utilisateur_id: Uuid,
) -> Result<Vec<VuePrestataire>, ErreurConsultation>
where
    P: ProviderRepository,
    D: DemandeRepository,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let provider = prestataires
        .par_utilisateur_id(utilisateur_id)
        .await?
        .ok_or(ErreurConsultation::PasPrestataire)?;

    let depuis = maintenant - Duration::seconds(DUREE_DIFFUSION_SECONDES);
    let proposees = demandes.proposees_a(provider.id, depuis).await?;

    Ok(proposees
        .into_iter()
        .map(|p| {
            let fin = p.demande.diffuse_depuis + Duration::seconds(DUREE_DIFFUSION_SECONDES);
            VuePrestataire {
                demande_id: p.demande.id,
                secteur: p.demande.secteur.to_string(),
                description: p.demande.description,
                urgence: p.demande.urgence.as_str().to_string(),
                distance_metres: p.distance_metres,
                secondes_restantes: (fin - maintenant).num_seconds().max(0),
            }
        })
        .collect())
}

/// Lit une Mission pour le prestataire à qui elle est attribuée.
pub async fn mission_du_prestataire<P, M, D, Q, H>(
    prestataires: &P,
    missions: &M,
    demandes: &D,
    devis_repo: &Q,
    horloge: &H,
    utilisateur_id: Uuid,
    mission_id: Uuid,
) -> Result<VueMission, ErreurConsultation>
where
    P: ProviderRepository,
    M: MissionRepository,
    D: DemandeRepository,
    Q: DevisRepository,
    H: Horloge,
{
    let provider = prestataires
        .par_utilisateur_id(utilisateur_id)
        .await?
        .ok_or(ErreurConsultation::PasPrestataire)?;

    let mission = missions
        .par_id(mission_id)
        .await?
        .filter(|m| m.appartient_a(provider.id))
        .ok_or(ErreurConsultation::Introuvable)?;

    let demande = demandes
        .par_id(mission.demande_id)
        .await?
        .ok_or(ErreurConsultation::Introuvable)?;

    let maintenant = horloge.maintenant();
    let devis = devis_repo
        .dernier_pour_mission(mission.id)
        .await?
        .map(|d| VueDevis::depuis(d, maintenant));
    let deja_envoyes = devis_repo.compter_pour_mission(mission.id).await?;

    Ok(VueMission {
        mission_id: mission.id,
        statut: mission.statut,
        secteur: demande.secteur.to_string(),
        description: demande.description,
        urgence: demande.urgence.as_str().to_string(),
        // L'adresse, enfin : il doit s'y rendre.
        position: demande.position,
        suites: mission
            .statut
            .transitions_possibles()
            .iter()
            .map(|s| s.as_str())
            .collect(),
        devis,
        devis_restants: DEVIS_MAX_PAR_MISSION.saturating_sub(deja_envoyes),
    })
}
