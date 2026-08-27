//! Droit à l'effacement (FR-005, RGPD art. 17, Story 1.9).
//!
//! **Ce que l'effacement fait ici, et ce qu'il ne peut pas encore faire.**
//! L'article 17 vise les données à caractère personnel. Dans l'état actuel du
//! code, celles d'un compte sont son adresse, l'empreinte de son mot de passe,
//! ses jetons de vérification, ses sessions et ses abonnements push. Toutes
//! disparaissent. Les Missions, factures et traces de géolocalisation que
//! décrit FR-005 n'existent pas encore : leurs bounded contexts arrivent aux
//! Epics 3 et suivants, et l'effacement devra les traiter à ce moment-là.
//! Écrit dans `COMPLIANCE.md` plutôt que passé sous silence.
//!
//! **Le journal d'audit survit**, comme l'exige le scénario `@security`. Il ne
//! porte ni adresse ni contenu, seulement des codes et des horodatages
//! rattachés à un identifiant devenu anonyme : la ligne de compte subsiste,
//! vidée, pour que ce rattachement reste possible sans désigner personne.
//!
//! **Le délai de trente jours n'a de sens que s'il est réversible.** FR-005 ne
//! décrit pas d'annulation ; un effacement immédiat n'aurait pourtant pas
//! besoin de délai. `annuler` existe donc, et le compte reste utilisable
//! pendant l'attente — le verrouiller ferait du délai de grâce une impasse.

use klaar_identity::StatutUtilisateur;
use std::fmt;
use uuid::Uuid;

use crate::ports::audit::{CodeAudit, EntreeAudit, JournalAudit};
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::utilisateur_repository::{EffacementRepository, UtilisateurRepository};

/// Mot que le client doit reproduire pour confirmer (FR-005).
///
/// Une confirmation typée plutôt qu'un simple booléen : un `{"confirme": true}`
/// se coche par mégarde ou se rejoue depuis un autre onglet, alors que
/// reproduire un mot demande une intention.
pub const CONFIRMATION_ATTENDUE: &str = "DELETE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultatDemande {
    /// L'effacement vient d'être programmé.
    Programme { dans_jours: i64 },
    /// Il l'était déjà. Idempotent, et l'échéance n'a pas bougé.
    DejaProgramme,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurEffacement {
    /// Confirmation absente ou différente de `DELETE`.
    ConfirmationInvalide,
    /// Le jeton désigne un compte qui n'existe plus.
    CompteIntrouvable,
    /// Aucune demande à annuler.
    AucuneDemande,
    Indisponible(String),
}

impl ErreurEffacement {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ConfirmationInvalide => "CONFIRMATION_REQUIRED",
            Self::CompteIntrouvable => "ACCOUNT_NOT_FOUND",
            Self::AucuneDemande => "NO_ERASURE_PENDING",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurEffacement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfirmationInvalide => {
                write!(f, "confirmation attendue : {CONFIRMATION_ATTENDUE}")
            }
            Self::CompteIntrouvable => write!(f, "compte introuvable"),
            Self::AucuneDemande => write!(f, "aucun effacement en attente"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurEffacement {}

impl From<RepositoryError> for ErreurEffacement {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Programme l'effacement d'un compte.
pub async fn demander<R, J, H>(
    depot: &R,
    journal: &J,
    horloge: &H,
    utilisateur_id: Uuid,
    confirmation: &str,
) -> Result<ResultatDemande, ErreurEffacement>
where
    R: UtilisateurRepository + EffacementRepository,
    J: JournalAudit,
    H: Horloge,
{
    // Comparaison stricte, sans normalisation de casse : « delete » en
    // minuscules est le genre de chose qu'on tape sans y penser, et
    // l'effacement d'un compte n'est pas une action qu'on veut faciliter.
    if confirmation != CONFIRMATION_ATTENDUE {
        return Err(ErreurEffacement::ConfirmationInvalide);
    }

    let mut compte = depot
        .par_id(utilisateur_id)
        .await?
        .ok_or(ErreurEffacement::CompteIntrouvable)?;

    if compte.statut == StatutUtilisateur::EffacementDemande {
        return Ok(ResultatDemande::DejaProgramme);
    }
    if compte.statut == StatutUtilisateur::Efface {
        // Un jeton encore valide sur un compte déjà effacé : il ne reste rien
        // à effacer, et le dire évite un second passage inutile.
        return Err(ErreurEffacement::CompteIntrouvable);
    }

    let maintenant = horloge.maintenant();
    compte.demander_effacement(maintenant);
    depot
        .programmer_effacement(compte.id, compte.efface_le)
        .await?;

    journal
        .consigner(EntreeAudit {
            code: CodeAudit::UserErasureRequested,
            sujet_id: Some(compte.id),
            horodatage: maintenant,
        })
        .await?;

    Ok(ResultatDemande::Programme {
        dans_jours: klaar_identity::DELAI_EFFACEMENT_JOURS,
    })
}

/// Annule une demande d'effacement.
pub async fn annuler<R, J, H>(
    depot: &R,
    journal: &J,
    horloge: &H,
    utilisateur_id: Uuid,
) -> Result<(), ErreurEffacement>
where
    R: UtilisateurRepository + EffacementRepository,
    J: JournalAudit,
    H: Horloge,
{
    let mut compte = depot
        .par_id(utilisateur_id)
        .await?
        .ok_or(ErreurEffacement::CompteIntrouvable)?;

    if !compte.annuler_effacement() {
        return Err(ErreurEffacement::AucuneDemande);
    }

    depot.annuler_effacement(compte.id).await?;
    journal
        .consigner(EntreeAudit {
            code: CodeAudit::UserErasureCancelled,
            sujet_id: Some(compte.id),
            horodatage: horloge.maintenant(),
        })
        .await?;
    Ok(())
}

/// Exécute les effacements arrivés à échéance.
///
/// Rend le nombre de comptes effacés. Destinée à être appelée par une tâche
/// périodique : rien ne se déclenche tout seul, un compte dont l'échéance est
/// passée attend simplement le prochain passage.
pub async fn executer_les_echus<R, J, H>(
    depot: &R,
    journal: &J,
    horloge: &H,
) -> Result<usize, ErreurEffacement>
where
    R: UtilisateurRepository + EffacementRepository,
    J: JournalAudit,
    H: Horloge,
{
    let maintenant = horloge.maintenant();
    let echus = depot.effacements_echus(maintenant).await?;

    let mut effaces = 0;
    for id in echus {
        // Le dépôt tranche : un compte que cet appel n'a pas effacé a été pris
        // par une exécution concurrente, et n'a pas à être consigné deux fois.
        if !depot.effacer(id, maintenant).await? {
            continue;
        }
        // Consigné **après** l'effacement, et rattaché à l'identifiant : la
        // ligne de compte subsiste, vidée, précisément pour que cette entrée
        // reste rattachable sans désigner quiconque.
        journal
            .consigner(EntreeAudit {
                code: CodeAudit::UserErased,
                sujet_id: Some(id),
                horodatage: maintenant,
            })
            .await?;
        effaces += 1;
    }
    Ok(effaces)
}

/// Adresse de remplacement d'un compte effacé.
///
/// Dérivée de l'identifiant, qui n'est pas l'adresse : impossible de remonter
/// à l'adresse d'origine à partir d'elle. Le domaine `.invalid` est réservé par
/// la RFC 2606 et ne peut être enregistré par personne, ce qui garantit qu'un
/// envoi accidentel n'atteindra jamais de boîte réelle.
pub fn adresse_effacee(utilisateur_id: Uuid) -> String {
    format!("erased_{}@klaar.invalid", utilisateur_id.simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::utilisateur_repository::{JetonAConserver, ResultatJeton};
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use klaar_identity::{
        EmpreinteJeton, EmpreinteMotDePasse, MotDePasse, ParametresArgon2, Utilisateur,
        Verrouillage,
    };
    use klaar_shared_kernel::{Email, Locale};
    use std::cell::RefCell;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    #[derive(Default)]
    struct DepotMemoire {
        comptes: RefCell<Vec<Utilisateur>>,
    }

    impl DepotMemoire {
        fn avec_compte_actif() -> (Self, Uuid) {
            let mdp = MotDePasse::parse("Marie@2026Secure").unwrap();
            let mut u = Utilisateur::inscrire(
                Email::parse("marie@example.eu").unwrap(),
                EmpreinteMotDePasse::calculer(&mdp, ParametresArgon2::tests()).unwrap(),
                Locale::Fr,
                instant(),
            );
            u.verifier_email();
            let id = u.id;
            let depot = Self::default();
            depot.comptes.borrow_mut().push(u);
            (depot, id)
        }

        fn compte(&self, id: Uuid) -> Utilisateur {
            self.comptes
                .borrow()
                .iter()
                .find(|u| u.id == id)
                .cloned()
                .expect("compte de test")
        }
    }

    impl UtilisateurRepository for DepotMemoire {
        async fn creer_si_absent(
            &self,
            _: &Utilisateur,
            _: &JetonAConserver,
        ) -> Result<bool, RepositoryError> {
            unreachable!()
        }

        async fn consommer_jeton_verification(
            &self,
            _: &EmpreinteJeton,
            _: DateTime<Utc>,
        ) -> Result<ResultatJeton, RepositoryError> {
            unreachable!()
        }

        async fn mettre_a_jour_verrouillage(
            &self,
            _: Uuid,
            _: &Verrouillage,
        ) -> Result<(), RepositoryError> {
            unreachable!()
        }

        async fn par_email(&self, email: &Email) -> Result<Option<Utilisateur>, RepositoryError> {
            Ok(self
                .comptes
                .borrow()
                .iter()
                .find(|u| &u.email == email)
                .cloned())
        }

        async fn par_id(&self, id: Uuid) -> Result<Option<Utilisateur>, RepositoryError> {
            Ok(self.comptes.borrow().iter().find(|u| u.id == id).cloned())
        }
    }

    impl EffacementRepository for DepotMemoire {
        async fn programmer_effacement(
            &self,
            utilisateur_id: Uuid,
            efface_le: Option<DateTime<Utc>>,
        ) -> Result<(), RepositoryError> {
            if let Some(u) = self
                .comptes
                .borrow_mut()
                .iter_mut()
                .find(|u| u.id == utilisateur_id)
            {
                u.statut = StatutUtilisateur::EffacementDemande;
                u.efface_le = efface_le;
            }
            Ok(())
        }

        async fn annuler_effacement(&self, utilisateur_id: Uuid) -> Result<(), RepositoryError> {
            if let Some(u) = self
                .comptes
                .borrow_mut()
                .iter_mut()
                .find(|u| u.id == utilisateur_id)
            {
                u.statut = StatutUtilisateur::Actif;
                u.efface_le = None;
            }
            Ok(())
        }

        async fn effacements_echus(
            &self,
            maintenant: DateTime<Utc>,
        ) -> Result<Vec<Uuid>, RepositoryError> {
            Ok(self
                .comptes
                .borrow()
                .iter()
                .filter(|u| u.effacement_du(maintenant))
                .map(|u| u.id)
                .collect())
        }

        async fn effacer(
            &self,
            utilisateur_id: Uuid,
            _: DateTime<Utc>,
        ) -> Result<bool, RepositoryError> {
            let mut comptes = self.comptes.borrow_mut();
            let Some(u) = comptes.iter_mut().find(|u| u.id == utilisateur_id) else {
                return Ok(false);
            };
            // Même garde que le dépôt réel : un compte qui n'est plus en
            // attente a été pris par une exécution concurrente.
            if u.statut != StatutUtilisateur::EffacementDemande {
                return Ok(false);
            }
            u.statut = StatutUtilisateur::Efface;
            u.empreinte_mot_de_passe = None;
            u.email = Email::parse(&adresse_effacee(utilisateur_id)).unwrap();
            u.efface_le = None;
            Ok(true)
        }
    }

    #[derive(Default)]
    struct JournalMemoire {
        entrees: RefCell<Vec<EntreeAudit>>,
    }

    impl JournalMemoire {
        fn codes(&self) -> Vec<CodeAudit> {
            self.entrees.borrow().iter().map(|e| e.code).collect()
        }
    }

    impl JournalAudit for JournalMemoire {
        async fn consigner(&self, entree: EntreeAudit) -> Result<(), RepositoryError> {
            self.entrees.borrow_mut().push(entree);
            Ok(())
        }
    }

    #[tokio::test]
    async fn happy_une_demande_confirmee_programme_l_effacement() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();

        let r = demander(&depot, &journal, &HorlogeFigee(instant()), id, "DELETE")
            .await
            .unwrap();

        assert_eq!(r, ResultatDemande::Programme { dans_jours: 30 });
        let compte = depot.compte(id);
        assert_eq!(compte.statut.as_str(), "ERASED_PENDING");
        assert_eq!(compte.efface_le, Some(instant() + Duration::days(30)));
        assert_eq!(journal.codes(), vec![CodeAudit::UserErasureRequested]);
    }

    #[tokio::test]
    async fn happy_l_annulation_rend_le_compte_actif() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();
        let horloge = HorlogeFigee(instant());

        demander(&depot, &journal, &horloge, id, "DELETE")
            .await
            .unwrap();
        annuler(&depot, &journal, &horloge, id).await.unwrap();

        let compte = depot.compte(id);
        assert!(compte.est_actif());
        assert_eq!(compte.efface_le, None);
        assert_eq!(
            journal.codes(),
            vec![
                CodeAudit::UserErasureRequested,
                CodeAudit::UserErasureCancelled
            ]
        );
    }

    #[tokio::test]
    async fn happy_le_job_efface_les_comptes_echus() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();

        demander(&depot, &journal, &HorlogeFigee(instant()), id, "DELETE")
            .await
            .unwrap();

        let apres = HorlogeFigee(instant() + Duration::days(30));
        assert_eq!(
            executer_les_echus(&depot, &journal, &apres).await.unwrap(),
            1
        );

        let compte = depot.compte(id);
        assert_eq!(compte.statut.as_str(), "ERASED");
        assert!(compte.empreinte_mot_de_passe.is_none());
        assert!(compte.email.as_str().ends_with("@klaar.invalid"));
    }

    #[tokio::test]
    async fn negative_une_confirmation_manquante_ou_fautive_est_refusee() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();
        let horloge = HorlogeFigee(instant());

        for saisie in ["", "delete", "Delete", "SUPPRIMER", "DELETE "] {
            let e = demander(&depot, &journal, &horloge, id, saisie)
                .await
                .unwrap_err();
            assert_eq!(e.code(), "CONFIRMATION_REQUIRED", "saisie {saisie:?}");
        }
        assert!(depot.compte(id).est_actif());
        assert!(journal.entrees.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_annuler_sans_demande_est_refuse() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();
        let e = annuler(&depot, &journal, &HorlogeFigee(instant()), id)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "NO_ERASURE_PENDING");
    }

    #[tokio::test]
    async fn negative_un_compte_inconnu_est_refuse() {
        let depot = DepotMemoire::default();
        let journal = JournalMemoire::default();
        let e = demander(
            &depot,
            &journal,
            &HorlogeFigee(instant()),
            Uuid::new_v4(),
            "DELETE",
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "ACCOUNT_NOT_FOUND");
    }

    #[tokio::test]
    async fn edge_redemander_ne_repousse_pas_l_echeance() {
        // Sinon, redemander deviendrait un moyen de différer l'exécution
        // indéfiniment, ce qui viderait le droit de son effet.
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();

        demander(&depot, &journal, &HorlogeFigee(instant()), id, "DELETE")
            .await
            .unwrap();
        let echeance = depot.compte(id).efface_le;

        let r = demander(
            &depot,
            &journal,
            &HorlogeFigee(instant() + Duration::days(20)),
            id,
            "DELETE",
        )
        .await
        .unwrap();

        assert_eq!(r, ResultatDemande::DejaProgramme);
        assert_eq!(depot.compte(id).efface_le, echeance);
        assert_eq!(journal.codes().len(), 1, "une seule entrée d'audit");
    }

    #[tokio::test]
    async fn edge_le_job_n_efface_rien_avant_l_echeance() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();
        demander(&depot, &journal, &HorlogeFigee(instant()), id, "DELETE")
            .await
            .unwrap();

        let veille = HorlogeFigee(instant() + Duration::days(29));
        assert_eq!(
            executer_les_echus(&depot, &journal, &veille).await.unwrap(),
            0
        );
        assert_eq!(depot.compte(id).statut.as_str(), "ERASED_PENDING");
    }

    #[tokio::test]
    async fn edge_un_compte_annule_n_est_plus_efface_a_l_echeance() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();
        let horloge = HorlogeFigee(instant());

        demander(&depot, &journal, &horloge, id, "DELETE")
            .await
            .unwrap();
        annuler(&depot, &journal, &horloge, id).await.unwrap();

        let apres = HorlogeFigee(instant() + Duration::days(31));
        assert_eq!(
            executer_les_echus(&depot, &journal, &apres).await.unwrap(),
            0
        );
        assert!(depot.compte(id).est_actif());
    }

    #[tokio::test]
    async fn edge_le_job_ne_repasse_pas_sur_un_compte_deja_efface() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();
        demander(&depot, &journal, &HorlogeFigee(instant()), id, "DELETE")
            .await
            .unwrap();

        let apres = HorlogeFigee(instant() + Duration::days(30));
        executer_les_echus(&depot, &journal, &apres).await.unwrap();
        assert_eq!(
            executer_les_echus(&depot, &journal, &apres).await.unwrap(),
            0,
            "un second passage ne doit rien retrouver"
        );
    }

    #[tokio::test]
    async fn security_l_adresse_de_remplacement_ne_permet_pas_de_remonter() {
        // Dérivée de l'identifiant, pas de l'adresse : rien dans la valeur
        // produite ne dépend de ce qui a été effacé.
        let id = Uuid::new_v4();
        let adresse = adresse_effacee(id);
        assert!(!adresse.contains("marie"));
        assert!(adresse.starts_with("erased_"));
        // `.invalid` est réservé par la RFC 2606 : personne ne peut
        // l'enregistrer, donc aucun envoi accidentel n'atteindra de boîte.
        assert!(adresse.ends_with("@klaar.invalid"));
        assert_eq!(adresse_effacee(id), adresse, "déterministe");
        assert_ne!(adresse_effacee(Uuid::new_v4()), adresse);
    }

    #[tokio::test]
    async fn security_le_journal_d_audit_survit_a_l_effacement() {
        // Scénario `@security` de FR-005. Le journal ne porte ni adresse ni
        // contenu : des codes et des horodatages rattachés à un identifiant
        // devenu anonyme.
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();

        demander(&depot, &journal, &HorlogeFigee(instant()), id, "DELETE")
            .await
            .unwrap();
        executer_les_echus(
            &depot,
            &journal,
            &HorlogeFigee(instant() + Duration::days(30)),
        )
        .await
        .unwrap();

        assert_eq!(
            journal.codes(),
            vec![CodeAudit::UserErasureRequested, CodeAudit::UserErased]
        );
        assert!(journal
            .entrees
            .borrow()
            .iter()
            .all(|e| e.sujet_id == Some(id)));
    }

    #[tokio::test]
    async fn security_un_compte_efface_ne_peut_plus_etre_efface() {
        let (depot, id) = DepotMemoire::avec_compte_actif();
        let journal = JournalMemoire::default();
        let horloge = HorlogeFigee(instant());

        demander(&depot, &journal, &horloge, id, "DELETE")
            .await
            .unwrap();
        executer_les_echus(
            &depot,
            &journal,
            &HorlogeFigee(instant() + Duration::days(30)),
        )
        .await
        .unwrap();

        // Un jeton d'accès encore valide au moment de l'effacement pourrait
        // arriver ensuite : il ne doit rien pouvoir faire.
        let e = demander(&depot, &journal, &horloge, id, "DELETE")
            .await
            .unwrap_err();
        assert_eq!(e.code(), "ACCOUNT_NOT_FOUND");
    }
}
