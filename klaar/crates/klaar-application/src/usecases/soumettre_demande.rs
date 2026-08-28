//! Cas d'usage « soumettre une Demande » (FR-011, Story 3.1).
//!
//! **Une précondition de FR-011 n'est pas tenue ici** : « méthode paiement
//! valide ». Elle suppose la Story 1.7, bloquée faute de compte Stripe. Le
//! contrôle existe pourtant, avec son port et son `422` — il est simplement
//! désactivable par configuration, et l'est dans le déploiement vitrine.
//! Activé par défaut : un contrôle de paiement qu'on oublie de rallumer est
//! pire que pas de contrôle du tout, parce que personne ne s'en aperçoit.
//!
//! **Ce que cette story ne fait pas** : déclencher le matching. FR-011 dit
//! qu'un job asynchrone part à la création ; il appartient aux Stories 3.2 et
//! 3.3. Une Demande est donc créée en `BROADCASTING` et y reste, faute de
//! prestataires à qui la diffuser.

use klaar_catalog::CodeCatalogue;
use klaar_matching::{Demande, DemandeError, Urgence};
use klaar_shared_kernel::Geo;
use std::fmt;
use uuid::Uuid;

use crate::ports::audit::{CodeAudit, EntreeAudit, JournalAudit};
use crate::ports::catalogue_repository::CatalogueRepository;
use crate::ports::demande_repository::{DemandeRepository, PaiementRepository};
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;

/// Demandes autorisées par utilisateur et par heure (FR-011 `@edge`).
///
/// Compté en base et non en mémoire, contrairement à la limitation par adresse
/// IP : celle-ci protège d'un flot anonyme et peut se permettre d'oublier au
/// redémarrage, alors qu'un quota par compte est une règle métier dont
/// l'utilisateur se souvient, lui.
pub const MAX_DEMANDES_PAR_HEURE: i64 = 5;

/// Ce que le déploiement autorise, quota compris.
///
/// Groupé parce que ces réglages voyagent ensemble et qu'une liste de
/// paramètres booléens finit par se remplir dans le mauvais ordre. Le quota est
/// paramétré et non constant pour le seul déploiement de démonstration, où le
/// même compte soumet plusieurs Demandes en quelques minutes. C'est un
/// **chiffre** et non un interrupteur : un quota qu'on peut éteindre finit
/// éteint en production, un chiffre annoncé au démarrage se remarque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReglesSoumission {
    pub exiger_methode_paiement: bool,
    pub max_demandes_par_heure: i64,
}

impl Default for ReglesSoumission {
    fn default() -> Self {
        Self {
            exiger_methode_paiement: true,
            max_demandes_par_heure: MAX_DEMANDES_PAR_HEURE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandeSoumission {
    pub demandeur_id: Uuid,
    pub secteur: String,
    pub description: String,
    pub latitude: f64,
    pub longitude: f64,
    pub urgence: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResultatSoumission {
    Creee(Box<Demande>),
    /// Une Demande identique existe déjà. FR-011 `@edge` demande qu'elle soit
    /// rendue plutôt qu'une erreur sèche : l'utilisateur veut retrouver la
    /// sienne, pas apprendre qu'il a cliqué deux fois.
    Doublon(Box<Demande>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurSoumission {
    SecteurInconnu,
    Demande(DemandeError),
    GeoInvalide,
    MethodePaiementAbsente,
    QuotaAtteint,
    Indisponible(String),
}

impl ErreurSoumission {
    pub fn code(&self) -> &'static str {
        match self {
            Self::SecteurInconnu => "SECTOR_NOT_FOUND",
            Self::Demande(e) => e.code(),
            // Une latitude à 200 degrés n'est pas « hors RBC », elle n'est
            // nulle part : les confondre enverrait un message trompeur.
            Self::GeoInvalide => "GEO_INVALID",
            Self::MethodePaiementAbsente => "PAYMENT_METHOD_REQUIRED",
            Self::QuotaAtteint => "RATE_LIMIT_EXCEEDED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurSoumission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SecteurInconnu => write!(f, "secteur inconnu"),
            Self::Demande(e) => write!(f, "{e}"),
            Self::GeoInvalide => write!(f, "coordonnée invalide"),
            Self::MethodePaiementAbsente => write!(f, "aucune méthode de paiement enregistrée"),
            Self::QuotaAtteint => write!(f, "quota de Demandes atteint"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurSoumission {}

impl From<RepositoryError> for ErreurSoumission {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn soumettre<D, C, P, J, H>(
    demandes: &D,
    catalogue: &C,
    paiements: &P,
    journal: &J,
    horloge: &H,
    regles: ReglesSoumission,
    commande: CommandeSoumission,
) -> Result<ResultatSoumission, ErreurSoumission>
where
    D: DemandeRepository,
    C: CatalogueRepository,
    P: PaiementRepository,
    J: JournalAudit,
    H: Horloge,
{
    let secteur =
        CodeCatalogue::parse(&commande.secteur).map_err(|_| ErreurSoumission::SecteurInconnu)?;
    let urgence = Urgence::parse(&commande.urgence)
        .ok_or(ErreurSoumission::Demande(DemandeError::UrgenceInvalide))?;
    let position = Geo::new(commande.latitude, commande.longitude)
        .map_err(|_| ErreurSoumission::GeoInvalide)?;

    // Le secteur est vérifié contre le catalogue avant tout le reste : soumettre
    // une Demande dans un secteur qui n'existe pas ne mène nulle part, et le
    // dire tout de suite évite d'écrire une ligne qu'il faudrait retirer.
    let connus = catalogue.secteurs().await?;
    if !connus.iter().any(|s| s.code == secteur) {
        return Err(ErreurSoumission::SecteurInconnu);
    }

    let maintenant = horloge.maintenant();

    if regles.exiger_methode_paiement && !paiements.possede_methode(commande.demandeur_id).await? {
        return Err(ErreurSoumission::MethodePaiementAbsente);
    }

    // Le doublon est cherché **avant** le quota : quelqu'un qui double-clique
    // cinq fois doit retrouver sa Demande, pas se voir refuser pour excès.
    if let Some(existante) = demandes
        .doublon_recent(commande.demandeur_id, &secteur, position, maintenant)
        .await?
    {
        return Ok(ResultatSoumission::Doublon(Box::new(existante)));
    }

    if demandes
        .compter_depuis_une_heure(commande.demandeur_id, maintenant)
        .await?
        >= regles.max_demandes_par_heure
    {
        return Err(ErreurSoumission::QuotaAtteint);
    }

    let demande = Demande::soumettre(
        commande.demandeur_id,
        secteur,
        &commande.description,
        position,
        urgence,
        maintenant,
    )
    .map_err(ErreurSoumission::Demande)?;

    demandes.creer(&demande).await?;

    journal
        .consigner(EntreeAudit {
            code: CodeAudit::RequestCreated,
            sujet_id: Some(demande.demandeur_id),
            horodatage: maintenant,
        })
        .await?;

    Ok(ResultatSoumission::Creee(Box::new(demande)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use klaar_catalog::{Libelles, Secteur};
    use std::cell::RefCell;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    #[derive(Default)]
    struct DemandesMemoire {
        creees: RefCell<Vec<Demande>>,
    }

    impl DemandeRepository for DemandesMemoire {
        async fn creer(&self, demande: &Demande) -> Result<(), RepositoryError> {
            self.creees.borrow_mut().push(demande.clone());
            Ok(())
        }

        async fn par_id(&self, id: Uuid) -> Result<Option<Demande>, RepositoryError> {
            Ok(self.creees.borrow().iter().find(|d| d.id == id).cloned())
        }

        async fn doublon_recent(
            &self,
            demandeur_id: Uuid,
            secteur: &CodeCatalogue,
            position: Geo,
            maintenant: DateTime<Utc>,
        ) -> Result<Option<Demande>, RepositoryError> {
            Ok(self
                .creees
                .borrow()
                .iter()
                .find(|d| d.est_doublon_de(demandeur_id, secteur, position, maintenant))
                .cloned())
        }

        async fn expirer_echues(
            &self,
            _: DateTime<Utc>,
            _: i64,
        ) -> Result<Vec<Demande>, RepositoryError> {
            unreachable!()
        }
        async fn annuler(
            &self,
            _: Uuid,
            _: Option<klaar_matching::MotifAnnulation>,
        ) -> Result<bool, RepositoryError> {
            unreachable!()
        }
        async fn relancer(&self, _: &Demande) -> Result<bool, RepositoryError> {
            unreachable!()
        }
        async fn changer_statut(
            &self,
            _: Uuid,
            _: klaar_matching::StatutDemande,
            _: DateTime<Utc>,
        ) -> Result<(), RepositoryError> {
            unreachable!("hors du périmètre de ce cas d'usage")
        }

        async fn proposees_a(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> Result<Vec<crate::ports::demande_repository::DemandeProposee>, RepositoryError>
        {
            unreachable!()
        }
        async fn compter_depuis_une_heure(
            &self,
            demandeur_id: Uuid,
            maintenant: DateTime<Utc>,
        ) -> Result<i64, RepositoryError> {
            Ok(self
                .creees
                .borrow()
                .iter()
                .filter(|d| {
                    d.demandeur_id == demandeur_id && maintenant - d.cree_le < Duration::hours(1)
                })
                .count() as i64)
        }
    }

    struct CatalogueFactice;

    impl CatalogueRepository for CatalogueFactice {
        async fn secteurs(&self) -> Result<Vec<Secteur>, RepositoryError> {
            Ok(vec![Secteur {
                code: CodeCatalogue::parse("plomberie").unwrap(),
                libelles: Libelles::new("Plomberie", "Loodgieterij", "Plumbing"),
                skills: Vec::new(),
                fourchette: None,
            }])
        }
    }

    struct Paiements(bool);

    impl PaiementRepository for Paiements {
        async fn possede_methode(&self, _: Uuid) -> Result<bool, RepositoryError> {
            Ok(self.0)
        }
    }

    #[derive(Default)]
    struct JournalMemoire {
        entrees: RefCell<Vec<EntreeAudit>>,
    }

    impl JournalAudit for JournalMemoire {
        async fn consigner(&self, entree: EntreeAudit) -> Result<(), RepositoryError> {
            self.entrees.borrow_mut().push(entree);
            Ok(())
        }
    }

    struct Bac {
        demandes: DemandesMemoire,
        journal: JournalMemoire,
        paiements: Paiements,
        exiger_paiement: bool,
    }

    impl Bac {
        fn neuf() -> Self {
            Self {
                demandes: DemandesMemoire::default(),
                journal: JournalMemoire::default(),
                paiements: Paiements(true),
                exiger_paiement: false,
            }
        }

        async fn soumettre_a(
            &self,
            demandeur_id: Uuid,
            secteur: &str,
            description: &str,
            lat: f64,
            lon: f64,
            maintenant: DateTime<Utc>,
        ) -> Result<ResultatSoumission, ErreurSoumission> {
            soumettre(
                &self.demandes,
                &CatalogueFactice,
                &self.paiements,
                &self.journal,
                &HorlogeFigee(maintenant),
                ReglesSoumission {
                    exiger_methode_paiement: self.exiger_paiement,
                    ..Default::default()
                },
                CommandeSoumission {
                    demandeur_id,
                    secteur: secteur.to_string(),
                    description: description.to_string(),
                    latitude: lat,
                    longitude: lon,
                    urgence: "HIGH".to_string(),
                },
            )
            .await
        }

        async fn soumettre(
            &self,
            demandeur_id: Uuid,
        ) -> Result<ResultatSoumission, ErreurSoumission> {
            self.soumettre_a(
                demandeur_id,
                "plomberie",
                "Fuite",
                50.8467,
                4.3525,
                instant(),
            )
            .await
        }
    }

    #[tokio::test]
    async fn happy_une_demande_valide_est_creee_et_auditee() {
        let bac = Bac::neuf();
        let id = Uuid::new_v4();
        let r = bac.soumettre(id).await.unwrap();

        let ResultatSoumission::Creee(demande) = r else {
            panic!("création attendue");
        };
        assert_eq!(demande.statut.as_str(), "BROADCASTING");
        assert_eq!(bac.demandes.creees.borrow().len(), 1);
        assert_eq!(
            bac.journal.entrees.borrow()[0].code,
            CodeAudit::RequestCreated
        );
    }

    #[tokio::test]
    async fn negative_un_secteur_absent_du_catalogue_est_refuse() {
        let bac = Bac::neuf();
        let e = bac
            .soumettre_a(
                Uuid::new_v4(),
                "chauffage",
                "Panne",
                50.8467,
                4.3525,
                instant(),
            )
            .await
            .unwrap_err();
        assert_eq!(e.code(), "SECTOR_NOT_FOUND");
        assert!(bac.demandes.creees.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_un_code_de_secteur_mal_forme_est_refuse_comme_inconnu() {
        // « Plomberie » avec une majuscule n'est pas un code : le distinguer
        // d'un secteur absent n'apprendrait rien d'utile à l'appelant.
        let bac = Bac::neuf();
        for saisie in ["Plomberie", "plomberie ", "../etc", ""] {
            let e = bac
                .soumettre_a(Uuid::new_v4(), saisie, "Fuite", 50.8467, 4.3525, instant())
                .await
                .unwrap_err();
            assert_eq!(e.code(), "SECTOR_NOT_FOUND", "saisie {saisie:?}");
        }
    }

    #[tokio::test]
    async fn negative_les_erreurs_de_saisie_du_prd_ont_leurs_codes() {
        let bac = Bac::neuf();
        let id = Uuid::new_v4();

        let vide = bac
            .soumettre_a(id, "plomberie", "", 50.8467, 4.3525, instant())
            .await
            .unwrap_err();
        assert_eq!(vide.code(), "DESCRIPTION_EMPTY");

        let longue = bac
            .soumettre_a(
                id,
                "plomberie",
                &"a".repeat(2_001),
                50.8467,
                4.3525,
                instant(),
            )
            .await
            .unwrap_err();
        assert_eq!(longue.code(), "DESCRIPTION_TOO_LONG");

        let hors = bac
            .soumettre_a(id, "plomberie", "Fuite", 51.2194, 4.4025, instant())
            .await
            .unwrap_err();
        assert_eq!(hors.code(), "GEO_OUTSIDE_RBC");
    }

    #[tokio::test]
    async fn negative_une_coordonnee_impossible_n_est_pas_dite_hors_region() {
        // Une latitude à 200 degrés n'est nulle part ; répondre « hors RBC »
        // enverrait l'utilisateur chercher une erreur d'adresse.
        let bac = Bac::neuf();
        let e = bac
            .soumettre_a(Uuid::new_v4(), "plomberie", "Fuite", 200.0, 4.35, instant())
            .await
            .unwrap_err();
        assert_eq!(e.code(), "GEO_INVALID");
    }

    #[tokio::test]
    async fn negative_sans_methode_de_paiement_la_demande_est_refusee() {
        let bac = Bac {
            paiements: Paiements(false),
            exiger_paiement: true,
            ..Bac::neuf()
        };
        let e = bac.soumettre(Uuid::new_v4()).await.unwrap_err();
        assert_eq!(e.code(), "PAYMENT_METHOD_REQUIRED");
        assert!(bac.demandes.creees.borrow().is_empty());
    }

    #[tokio::test]
    async fn edge_un_doublon_rend_la_demande_existante_et_n_en_cree_pas_une_seconde() {
        // FR-011 `@edge` : l'utilisateur veut retrouver la sienne, pas
        // apprendre qu'il a cliqué deux fois.
        let bac = Bac::neuf();
        let id = Uuid::new_v4();
        let ResultatSoumission::Creee(premiere) = bac.soumettre(id).await.unwrap() else {
            panic!("création attendue");
        };

        let r = bac
            .soumettre_a(
                id,
                "plomberie",
                "Fuite",
                50.8467,
                4.3525,
                instant() + Duration::minutes(2),
            )
            .await
            .unwrap();
        let ResultatSoumission::Doublon(rendue) = r else {
            panic!("doublon attendu");
        };
        assert_eq!(rendue.id, premiere.id);
        assert_eq!(bac.demandes.creees.borrow().len(), 1);
    }

    #[tokio::test]
    async fn edge_le_quota_horaire_bloque_la_sixieme_demande() {
        let bac = Bac::neuf();
        let id = Uuid::new_v4();
        // Cinq Demandes espacées, à des positions distinctes pour ne pas être
        // prises pour des doublons.
        for i in 0..5 {
            let r = bac
                .soumettre_a(
                    id,
                    "plomberie",
                    "Fuite",
                    50.8467 + i as f64 * 0.01,
                    4.3525,
                    instant() + Duration::minutes(i * 6),
                )
                .await;
            assert!(r.is_ok(), "demande {i}");
        }

        let e = bac
            .soumettre_a(
                id,
                "plomberie",
                "Fuite",
                50.90,
                4.3525,
                instant() + Duration::minutes(31),
            )
            .await
            .unwrap_err();
        assert_eq!(e.code(), "RATE_LIMIT_EXCEEDED");
    }

    #[tokio::test]
    async fn edge_le_doublon_passe_avant_le_quota() {
        // Quelqu'un qui double-clique cinq fois doit retrouver sa Demande, pas
        // se voir refuser pour excès de Demandes.
        let bac = Bac::neuf();
        let id = Uuid::new_v4();
        for i in 0..5 {
            bac.soumettre_a(
                id,
                "plomberie",
                "Fuite",
                50.8467 + i as f64 * 0.01,
                4.3525,
                instant() + Duration::minutes(i * 6),
            )
            .await
            .unwrap();
        }

        // Même position et même minute que la dernière : c'est un doublon.
        let r = bac
            .soumettre_a(
                id,
                "plomberie",
                "Fuite",
                50.8867,
                4.3525,
                instant() + Duration::minutes(25),
            )
            .await
            .unwrap();
        assert!(matches!(r, ResultatSoumission::Doublon(_)));
    }

    #[tokio::test]
    async fn security_le_quota_est_compte_par_compte_et_non_globalement() {
        // Sinon, cinq Demandes d'un utilisateur empêcheraient tous les autres
        // d'en soumettre.
        let bac = Bac::neuf();
        let premier = Uuid::new_v4();
        for i in 0..5 {
            bac.soumettre_a(
                premier,
                "plomberie",
                "Fuite",
                50.8467 + i as f64 * 0.01,
                4.3525,
                instant() + Duration::minutes(i * 6),
            )
            .await
            .unwrap();
        }
        assert!(bac.soumettre(Uuid::new_v4()).await.is_ok());
    }

    #[tokio::test]
    async fn security_la_demande_est_toujours_rattachee_a_son_auteur() {
        // La commande porte l'identifiant du demandeur, que la route tire du
        // jeton. Ce test fixe le fait que le domaine ne le réécrit pas.
        let bac = Bac::neuf();
        let id = Uuid::new_v4();
        let ResultatSoumission::Creee(demande) = bac.soumettre(id).await.unwrap() else {
            panic!("création attendue");
        };
        assert_eq!(demande.demandeur_id, id);
    }

    #[tokio::test]
    async fn security_rien_n_est_ecrit_quand_la_validation_echoue() {
        let bac = Bac::neuf();
        let _ = bac
            .soumettre_a(Uuid::new_v4(), "plomberie", "", 50.8467, 4.3525, instant())
            .await;
        assert!(bac.demandes.creees.borrow().is_empty());
        assert!(bac.journal.entrees.borrow().is_empty());
    }
}
