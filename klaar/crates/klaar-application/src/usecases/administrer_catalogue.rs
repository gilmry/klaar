//! Administration du catalogue par l'exploitation (FR-010, Story 2.4).
//!
//! **Le catalogue est une des rares parties du produit dont personne d'autre ne
//! détient la clé.** Ni Stripe, ni itsme, ni la BCE, ni un seau d'objets : les
//! secteurs sont à nous, et cette story n'attendait aucun tiers. Elle n'a pas
//! été faite plus tôt parce qu'elle demandait une console d'exploitation, qui
//! existe maintenant.

use klaar_catalog::{
    valider_creation, valider_desactivation, valider_publication, AdministrationError,
    SecteurACreer,
};
use klaar_identity::Permission;
use std::fmt;
use uuid::Uuid;

use crate::ports::catalogue_admin_repository::{CatalogueAdminRepository, SecteurAdmin};
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::ops_repository::OpsRepository;
use crate::usecases::ops::{autoriser_et_consigner, ErreurOps};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurCatalogue {
    Introuvable,
    Domaine(AdministrationError),
    Ops(ErreurOps),
    /// La base a refusé : quelqu'un est passé avant.
    ///
    /// **Distinct du refus du domaine.** Le domaine dit « ce geste est
    /// impossible d'après ce que je vois » ; celui-ci dit « l'état a changé
    /// entre la lecture et l'écriture ». La suite à donner est de relire, pas
    /// de corriger la saisie.
    DejaFait,
    Indisponible(String),
}

impl ErreurCatalogue {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Introuvable => "SECTOR_NOT_FOUND",
            Self::Domaine(e) => e.code(),
            Self::Ops(e) => e.code(),
            Self::DejaFait => "SECTOR_TRANSITION_INVALID",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurCatalogue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Introuvable => write!(f, "secteur introuvable"),
            Self::Domaine(e) => write!(f, "{e}"),
            Self::Ops(e) => write!(f, "{e}"),
            Self::DejaFait => write!(f, "l'état du secteur a changé entre-temps"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurCatalogue {}

impl From<RepositoryError> for ErreurCatalogue {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

impl From<ErreurOps> for ErreurCatalogue {
    fn from(e: ErreurOps) -> Self {
        Self::Ops(e)
    }
}

/// Tous les secteurs, brouillons compris.
pub async fn lister<C, O, H>(
    catalogue: &C,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
) -> Result<Vec<SecteurAdmin>, ErreurCatalogue>
where
    C: CatalogueAdminRepository,
    O: OpsRepository,
    H: Horloge,
{
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::GererCatalogue,
        Some("list"),
    )
    .await?;
    Ok(catalogue.tous().await?)
}

/// Crée un secteur en brouillon.
pub async fn creer<C, O, H>(
    catalogue: &C,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
    secteur: SecteurACreer,
) -> Result<(), ErreurCatalogue>
where
    C: CatalogueAdminRepository,
    O: OpsRepository,
    H: Horloge,
{
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::GererCatalogue,
        Some(secteur.code.as_str()),
    )
    .await?;

    let deja = catalogue.par_code(secteur.code.as_str()).await?.is_some();
    valider_creation(&secteur, deja).map_err(ErreurCatalogue::Domaine)?;

    // C'est la clé primaire qui tranche : le contrôle ci-dessus évite un
    // aller-retour dans le cas courant, il ne remplace pas la contrainte.
    if !catalogue
        .creer(&secteur, ops_id, horloge.maintenant())
        .await?
    {
        return Err(ErreurCatalogue::Domaine(AdministrationError::CodeDejaPris));
    }
    Ok(())
}

/// Publie un brouillon — par un **autre** compte que son créateur.
pub async fn publier<C, O, H>(
    catalogue: &C,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
    code: &str,
) -> Result<(), ErreurCatalogue>
where
    C: CatalogueAdminRepository,
    O: OpsRepository,
    H: Horloge,
{
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::GererCatalogue,
        Some(code),
    )
    .await?;

    let secteur = catalogue
        .par_code(code)
        .await?
        .ok_or(ErreurCatalogue::Introuvable)?;
    valider_publication(secteur.statut, secteur.cree_par, ops_id)
        .map_err(ErreurCatalogue::Domaine)?;

    // La garde des quatre yeux est **aussi** dans le `WHERE` : le contrôle
    // ci-dessus explique le refus, celui-là le rend impossible.
    if !catalogue
        .publier(code, ops_id, horloge.maintenant())
        .await?
    {
        return Err(ErreurCatalogue::DejaFait);
    }
    Ok(())
}

/// Retire un secteur du public.
pub async fn desactiver<C, O, H>(
    catalogue: &C,
    comptes: &O,
    horloge: &H,
    ops_id: Uuid,
    code: &str,
) -> Result<(), ErreurCatalogue>
where
    C: CatalogueAdminRepository,
    O: OpsRepository,
    H: Horloge,
{
    autoriser_et_consigner(
        comptes,
        horloge,
        ops_id,
        Permission::GererCatalogue,
        Some(code),
    )
    .await?;

    let secteur = catalogue
        .par_code(code)
        .await?
        .ok_or(ErreurCatalogue::Introuvable)?;
    // Le domaine explique le refus avec le nombre d'interventions en cours ;
    // c'est ce nombre que l'écran doit montrer, et non un « impossible » nu.
    valider_desactivation(secteur.statut, secteur.missions_en_cours)
        .map_err(ErreurCatalogue::Domaine)?;

    if !catalogue.desactiver(code).await? {
        // Une Mission a démarré entre la lecture et l'écriture. C'est
        // exactement pourquoi la condition est aussi dans la requête.
        return Err(ErreurCatalogue::DejaFait);
    }
    Ok(())
}
