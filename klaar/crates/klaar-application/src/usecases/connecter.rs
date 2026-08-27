//! Cas d'usage « se connecter » (FR-004, Story 1.3).
//!
//! Trois refus possibles, et un seul message pour deux d'entre eux.
//!
//! - Adresse inconnue et mot de passe faux donnent la **même** réponse. Les
//!   distinguer ferait de la connexion un moyen de tester la présence de
//!   n'importe quelle adresse, exactement ce que l'inscription évite.
//! - Compte non vérifié est distingué, lui, parce que l'atteindre suppose déjà
//!   de connaître le bon mot de passe : rien n'y est révélé qui ne le soit
//!   déjà, et l'utilisateur a besoin de savoir qu'il lui reste un courriel à
//!   ouvrir.
//!
//! Le temps de réponse compte autant que le message. Une adresse inconnue
//! économiserait la vérification argon2 et répondrait en une milliseconde là
//! où un mot de passe faux en prend cinquante : le chronomètre distinguerait
//! ce que la réponse tait. Une empreinte leurre est donc vérifiée dans le vide.

use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, MotDePasseError, ParametresArgon2, Verrouillage,
    DUREE_VERROU_MINUTES,
};
use klaar_shared_kernel::{Email, EmailError};
use std::fmt;
use uuid::Uuid;

use crate::ports::audit::{CodeAudit, EntreeAudit, JournalAudit};
use crate::ports::courriel::{CourrielSecurite, EnvoiCourriel};
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::jeton_acces::{ClaimsAcces, EmetteurJetonAcces, VALIDITE_ACCES_SECONDES};
use crate::ports::session_repository::{
    SessionAConserver, SessionRepository, VALIDITE_REFRESH_JOURS,
};
use crate::ports::utilisateur_repository::UtilisateurRepository;

#[derive(Debug, Clone)]
pub struct CommandeConnexion {
    pub email: String,
    pub mot_de_passe: String,
}

/// Ce que la connexion produit.
#[derive(Debug, Clone)]
pub struct Session {
    pub utilisateur_id: Uuid,
    pub jeton_acces: String,
    pub expire_dans_secondes: i64,
    /// Refresh **en clair**, à poser en cookie. Rien ne le conserve sous cette
    /// forme : seule son empreinte est écrite en base.
    pub refresh: String,
    pub refresh_expire_dans_secondes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurConnexion {
    Email(EmailError),
    MotDePasse(MotDePasseError),
    /// Adresse inconnue **ou** mot de passe faux. La variante est unique
    /// exprès : deux variantes finiraient par produire deux messages.
    IdentifiantsInvalides,
    /// Compte existant, mot de passe correct, adresse non encore confirmée.
    CompteNonVerifie,
    /// Compte verrouillé après échecs répétés (FR-007). N'est renvoyé qu'à
    /// quelqu'un ayant donné le bon mot de passe.
    CompteVerrouille {
        retry_after: i64,
    },
    Indisponible(String),
}

impl ErreurConnexion {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Email(EmailError::Empty) => "EMAIL_EMPTY",
            Self::Email(_) => "EMAIL_MALFORMED",
            Self::MotDePasse(e) => e.code(),
            Self::IdentifiantsInvalides => "INVALID_CREDENTIALS",
            Self::CompteNonVerifie => "ACCOUNT_NOT_VERIFIED",
            Self::CompteVerrouille { .. } => "ACCOUNT_LOCKED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }

    pub fn est_indisponibilite(&self) -> bool {
        matches!(self, Self::Indisponible(_))
    }
}

impl fmt::Display for ErreurConnexion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Email(e) => write!(f, "{e}"),
            Self::MotDePasse(e) => write!(f, "{e}"),
            Self::IdentifiantsInvalides => write!(f, "identifiants invalides"),
            Self::CompteNonVerifie => write!(f, "compte non vérifié"),
            Self::CompteVerrouille { retry_after } => {
                write!(f, "compte verrouillé, réessayer dans {retry_after} s")
            }
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurConnexion {}

impl From<RepositoryError> for ErreurConnexion {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Empreinte vérifiée dans le vide quand l'adresse est inconnue.
///
/// Calculée avec les paramètres réellement employés, pour que le temps dépensé
/// soit celui d'une vraie vérification. Un `sleep` fixe ne conviendrait pas :
/// il ne suit pas les paramètres, et sa régularité se repère.
fn empreinte_leurre(parametres: ParametresArgon2) -> Option<EmpreinteMotDePasse> {
    let leurre = MotDePasse::parse("leurre-anti-enumeration-klaar").ok()?;
    EmpreinteMotDePasse::calculer(&leurre, parametres).ok()
}

#[allow(clippy::too_many_arguments)]
pub async fn connecter<R, S, J, H, E, C>(
    depot: &R,
    sessions: &S,
    journal: &J,
    horloge: &H,
    emetteur: &E,
    courriel: &C,
    parametres: ParametresArgon2,
    refresh_en_clair: &str,
    contexte: Option<&str>,
    commande: CommandeConnexion,
) -> Result<Session, ErreurConnexion>
where
    R: UtilisateurRepository,
    S: SessionRepository,
    J: JournalAudit,
    H: Horloge,
    // `?Sized` : l'appelant tient l'émetteur derrière un `dyn`, parce que le
    // format du jeton se choisit à la composition et non à la compilation du
    // cas d'usage.
    E: EmetteurJetonAcces + ?Sized,
    C: EnvoiCourriel,
{
    let email = Email::parse(&commande.email).map_err(ErreurConnexion::Email)?;
    // La longueur du mot de passe est validée ici comme à l'inscription : une
    // saisie de trois caractères ne peut correspondre à aucun compte, autant
    // le dire sans faire travailler argon2.
    let mot_de_passe =
        MotDePasse::parse(&commande.mot_de_passe).map_err(ErreurConnexion::MotDePasse)?;

    let maintenant = horloge.maintenant();
    let compte = depot.par_email(&email).await?;

    let Some(compte) = compte else {
        // Vérification dans le vide : même coût que le chemin nominal.
        if let Some(leurre) = empreinte_leurre(parametres) {
            let _ = leurre.verifier(&mot_de_passe);
        }
        journal_echec(journal, maintenant).await?;
        return Err(ErreurConnexion::IdentifiantsInvalides);
    };

    // Le mot de passe est vérifié **avant** l'état du verrou, contrairement à
    // ce que décrit FR-007 (« il tente un login, correct ou non, → 423 »).
    // Répondre 423 à qui ne connaît pas le mot de passe apprendrait à un
    // attaquant que l'adresse existe, ce que le scénario `@security` du même FR
    // interdit. Vérifier d'abord coûte le même temps dans les deux cas et ne
    // révèle rien : pour obtenir un 423, il faut déjà avoir le mot de passe.
    // `None` sur un compte effacé : rien à comparer, donc rien qui puisse
    // réussir. Le leurre est vérifié quand même pour que le temps de réponse
    // reste celui des autres chemins.
    let mot_de_passe_correct = match &compte.empreinte_mot_de_passe {
        Some(empreinte) => empreinte.verifier(&mot_de_passe),
        None => {
            if let Some(leurre) = empreinte_leurre(parametres) {
                let _ = leurre.verifier(&mot_de_passe);
            }
            false
        }
    };

    if !mot_de_passe_correct {
        let precedent = compte.verrouillage;
        let apres = precedent.apres_echec(maintenant);
        depot.mettre_a_jour_verrouillage(compte.id, &apres).await?;

        if apres.vient_de_verrouiller(&precedent) {
            journal
                .consigner(EntreeAudit {
                    code: CodeAudit::AccountLocked,
                    // Ici l'identifiant est légitime : l'événement concerne le
                    // compte lui-même, pas la tentative, et un titulaire doit
                    // pouvoir retrouver quand son compte a été bloqué.
                    sujet_id: Some(compte.id),
                    horodatage: maintenant,
                })
                .await?;
            // Une alerte par échec transformerait le service en relais de
            // courriels vers une adresse non sollicitée : seul le
            // franchissement du seuil en déclenche une.
            if let Err(e) = courriel
                .envoyer_securite(
                    &compte.email,
                    compte.locale,
                    CourrielSecurite::CompteVerrouille {
                        minutes: DUREE_VERROU_MINUTES,
                    },
                )
                .await
            {
                tracing::error!(erreur = %e, "alerte de verrouillage non envoyée");
            }
        }

        journal_echec(journal, maintenant).await?;
        return Err(ErreurConnexion::IdentifiantsInvalides);
    }

    // À partir d'ici, l'appelant a prouvé qu'il connaît le mot de passe : les
    // refus suivants ne révèlent donc rien qu'il ne sache déjà.
    if let Some(retry_after) = compte.verrouillage.secondes_restantes(maintenant) {
        journal_echec(journal, maintenant).await?;
        return Err(ErreurConnexion::CompteVerrouille { retry_after });
    }

    // Un effacement demandé n'empêche pas la connexion : son titulaire doit
    // pouvoir se connecter pour annuler sa propre demande.
    if !compte.peut_ouvrir_session() {
        journal_echec(journal, maintenant).await?;
        return Err(ErreurConnexion::CompteNonVerifie);
    }

    // Le compteur repart de zéro : sans cela, quatre oublis suivis d'une
    // connexion réussie laisseraient le compte à un échec du verrou.
    if compte.verrouillage != Verrouillage::default() {
        depot
            .mettre_a_jour_verrouillage(compte.id, &Verrouillage::apres_succes())
            .await?;
    }

    let expire_le = maintenant + chrono::Duration::seconds(VALIDITE_ACCES_SECONDES);
    let jeton_acces = emetteur
        .emettre(&ClaimsAcces {
            utilisateur_id: compte.id,
            emis_le: maintenant,
            expire_le,
        })
        .map_err(|e| ErreurConnexion::Indisponible(e.to_string()))?;

    let refresh_expire_le = maintenant + chrono::Duration::days(VALIDITE_REFRESH_JOURS);
    sessions
        .ouvrir(&SessionAConserver {
            empreinte: klaar_identity::JetonVerification::depuis_chaine(refresh_en_clair)
                .empreinte(),
            utilisateur_id: compte.id,
            // Une authentification ouvre une famille neuve : deux appareils
            // n'ont pas à partager de destin, couper l'un ne doit pas couper
            // l'autre.
            famille_id: Uuid::new_v4(),
            expire_le: refresh_expire_le,
            // Enregistré à l'ouverture pour que la rotation puisse comparer
            // (Story 1.4). Une empreinte, jamais l'agent utilisateur lui-même.
            empreinte_contexte: contexte
                .map(|c| klaar_identity::JetonVerification::depuis_chaine(c).empreinte()),
        })
        .await?;

    journal
        .consigner(EntreeAudit {
            code: CodeAudit::UserLogin,
            sujet_id: Some(compte.id),
            horodatage: maintenant,
        })
        .await?;

    Ok(Session {
        utilisateur_id: compte.id,
        jeton_acces,
        expire_dans_secondes: VALIDITE_ACCES_SECONDES,
        refresh: refresh_en_clair.to_string(),
        refresh_expire_dans_secondes: VALIDITE_REFRESH_JOURS * 86_400,
    })
}

async fn journal_echec<J: JournalAudit>(
    journal: &J,
    horodatage: chrono::DateTime<chrono::Utc>,
) -> Result<(), RepositoryError> {
    journal
        .consigner(EntreeAudit {
            code: CodeAudit::UserLoginFailed,
            // Jamais l'identifiant du compte visé : le journal d'audit
            // deviendrait l'oracle d'énumération que la réponse refuse d'être.
            sujet_id: None,
            horodatage,
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::jeton_acces::ErreurJeton;
    use crate::ports::utilisateur_repository::{JetonAConserver, ResultatJeton};
    use chrono::{DateTime, TimeZone, Utc};
    use klaar_identity::{EmpreinteJeton, Utilisateur};
    use klaar_shared_kernel::Locale;
    use std::cell::RefCell;

    const P: ParametresArgon2 = ParametresArgon2::tests();
    const MDP: &str = "Marie@2026Secure";
    const REFRESH: &str = "refresh-de-test-tres-long-et-aleatoire";

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    #[derive(Default)]
    struct DepotMemoire {
        comptes: RefCell<Vec<Utilisateur>>,
        en_panne: bool,
    }

    #[derive(Default)]
    struct BoiteAuxLettres {
        alertes: RefCell<Vec<CourrielSecurite>>,
    }

    impl EnvoiCourriel for BoiteAuxLettres {
        async fn envoyer_securite(
            &self,
            _: &Email,
            _: Locale,
            contenu: CourrielSecurite,
        ) -> Result<(), crate::ports::courriel::ErreurEnvoi> {
            self.alertes.borrow_mut().push(contenu);
            Ok(())
        }

        async fn envoyer_inscription(
            &self,
            _: &Email,
            _: Locale,
            _: crate::ports::courriel::CourrielInscription,
        ) -> Result<(), crate::ports::courriel::ErreurEnvoi> {
            unreachable!("hors du périmètre de ce cas d'usage")
        }
    }

    impl DepotMemoire {
        fn avec_compte(actif: bool) -> (Self, Uuid) {
            let mdp = MotDePasse::parse(MDP).unwrap();
            let mut u = Utilisateur::inscrire(
                Email::parse("marie@example.eu").unwrap(),
                EmpreinteMotDePasse::calculer(&mdp, P).unwrap(),
                Locale::Fr,
                instant(),
            );
            if actif {
                u.verifier_email();
            }
            let id = u.id;
            let depot = Self::default();
            depot.comptes.borrow_mut().push(u);
            (depot, id)
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

        async fn par_email(&self, email: &Email) -> Result<Option<Utilisateur>, RepositoryError> {
            if self.en_panne {
                return Err(RepositoryError::Indisponible("test".into()));
            }
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

        async fn mettre_a_jour_verrouillage(
            &self,
            utilisateur_id: Uuid,
            verrouillage: &klaar_identity::Verrouillage,
        ) -> Result<(), RepositoryError> {
            if let Some(compte) = self
                .comptes
                .borrow_mut()
                .iter_mut()
                .find(|u| u.id == utilisateur_id)
            {
                compte.verrouillage = *verrouillage;
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct SessionsMemoire {
        ouvertes: RefCell<Vec<SessionAConserver>>,
    }

    impl SessionRepository for SessionsMemoire {
        async fn ouvrir(&self, session: &SessionAConserver) -> Result<(), RepositoryError> {
            self.ouvertes.borrow_mut().push(session.clone());
            Ok(())
        }

        async fn rotationner(
            &self,
            _: &EmpreinteJeton,
            _: &EmpreinteJeton,
            _: Option<&EmpreinteJeton>,
            _: DateTime<Utc>,
            _: DateTime<Utc>,
        ) -> Result<crate::ports::session_repository::ResultatRotation, RepositoryError> {
            unreachable!("hors du périmètre de ce cas d'usage")
        }

        async fn famille_de(&self, _: &EmpreinteJeton) -> Result<Option<Uuid>, RepositoryError> {
            unreachable!("hors du périmètre de ce cas d'usage")
        }

        async fn revoquer_famille(
            &self,
            _: Uuid,
            _: DateTime<Utc>,
        ) -> Result<u64, RepositoryError> {
            unreachable!()
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

    struct EmetteurFactice;

    impl EmetteurJetonAcces for EmetteurFactice {
        fn emettre(&self, claims: &ClaimsAcces) -> Result<String, ErreurJeton> {
            Ok(format!(
                "acces:{}:{}",
                claims.utilisateur_id,
                claims.expire_le.timestamp()
            ))
        }

        fn verifier(&self, _: &str) -> Result<ClaimsAcces, ErreurJeton> {
            unreachable!()
        }
    }

    struct Bac {
        depot: DepotMemoire,
        sessions: SessionsMemoire,
        journal: JournalMemoire,
        boite: BoiteAuxLettres,
    }

    impl Bac {
        fn neuf(actif: bool) -> (Self, Uuid) {
            let (depot, id) = DepotMemoire::avec_compte(actif);
            (
                Self {
                    depot,
                    sessions: SessionsMemoire::default(),
                    journal: JournalMemoire::default(),
                    boite: BoiteAuxLettres::default(),
                },
                id,
            )
        }

        async fn connecter(
            &self,
            email: &str,
            mot_de_passe: &str,
        ) -> Result<Session, ErreurConnexion> {
            self.connecter_a(email, mot_de_passe, instant()).await
        }

        async fn connecter_a(
            &self,
            email: &str,
            mot_de_passe: &str,
            maintenant: DateTime<Utc>,
        ) -> Result<Session, ErreurConnexion> {
            connecter(
                &self.depot,
                &self.sessions,
                &self.journal,
                &HorlogeFigee(maintenant),
                &EmetteurFactice,
                &self.boite,
                P,
                REFRESH,
                None,
                CommandeConnexion {
                    email: email.to_string(),
                    mot_de_passe: mot_de_passe.to_string(),
                },
            )
            .await
        }
    }

    #[tokio::test]
    async fn happy_un_compte_actif_obtient_un_acces_et_un_refresh() {
        let (bac, id) = Bac::neuf(true);
        let session = bac.connecter("marie@example.eu", MDP).await.unwrap();

        assert_eq!(session.utilisateur_id, id);
        assert_eq!(session.expire_dans_secondes, 3600, "FR-004 : accès 1 h");
        assert_eq!(
            session.refresh_expire_dans_secondes,
            30 * 86_400,
            "FR-004 : refresh 30 j"
        );
        assert_eq!(bac.sessions.ouvertes.borrow().len(), 1);

        let entrees = bac.journal.entrees.borrow();
        assert_eq!(entrees[0].code, CodeAudit::UserLogin);
        assert_eq!(entrees[0].sujet_id, Some(id));
    }

    #[tokio::test]
    async fn happy_la_casse_de_l_adresse_n_empeche_pas_la_connexion() {
        let (bac, _) = Bac::neuf(true);
        assert!(bac.connecter("Marie@Example.EU", MDP).await.is_ok());
    }

    #[tokio::test]
    async fn negative_un_mot_de_passe_faux_est_refuse_sans_ouvrir_de_session() {
        let (bac, _) = Bac::neuf(true);
        let e = bac
            .connecter("marie@example.eu", "Marie@2026Secur3")
            .await
            .unwrap_err();
        assert_eq!(e.code(), "INVALID_CREDENTIALS");
        assert!(bac.sessions.ouvertes.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_une_adresse_inconnue_donne_le_meme_refus() {
        let (bac, _) = Bac::neuf(true);
        let e = bac.connecter("personne@example.eu", MDP).await.unwrap_err();
        assert_eq!(e.code(), "INVALID_CREDENTIALS");
    }

    #[tokio::test]
    async fn negative_un_compte_non_verifie_est_distingue() {
        // Atteindre ce refus suppose déjà de connaître le bon mot de passe :
        // rien n'y est révélé qui ne le soit déjà, et l'utilisateur a besoin de
        // savoir qu'il lui reste un courriel à ouvrir.
        let (bac, _) = Bac::neuf(false);
        let e = bac.connecter("marie@example.eu", MDP).await.unwrap_err();
        assert_eq!(e.code(), "ACCOUNT_NOT_VERIFIED");
        assert!(bac.sessions.ouvertes.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_un_mot_de_passe_trop_court_ne_fait_pas_travailler_argon2() {
        let (bac, _) = Bac::neuf(true);
        let e = bac
            .connecter("marie@example.eu", "court")
            .await
            .unwrap_err();
        assert_eq!(e.code(), "PASSWORD_TOO_SHORT");
        // Aucun échec consigné : la requête n'a pas atteint la vérification.
        assert!(bac.journal.entrees.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_une_panne_ne_passe_pas_pour_un_refus_d_identifiants() {
        let bac = Bac {
            depot: DepotMemoire {
                en_panne: true,
                ..Default::default()
            },
            sessions: SessionsMemoire::default(),
            journal: JournalMemoire::default(),
            boite: BoiteAuxLettres::default(),
        };
        let e = bac.connecter("marie@example.eu", MDP).await.unwrap_err();
        assert_eq!(e.code(), "SERVICE_UNAVAILABLE");
        assert!(e.est_indisponibilite());
    }

    #[tokio::test]
    async fn edge_deux_connexions_ouvrent_deux_familles_distinctes() {
        // Deux appareils n'ont pas à partager de destin : couper la session de
        // l'un ne doit pas déconnecter l'autre.
        let (bac, _) = Bac::neuf(true);
        bac.connecter("marie@example.eu", MDP).await.unwrap();
        bac.connecter("marie@example.eu", MDP).await.unwrap();

        let ouvertes = bac.sessions.ouvertes.borrow();
        assert_eq!(ouvertes.len(), 2);
        assert_ne!(ouvertes[0].famille_id, ouvertes[1].famille_id);
    }

    #[tokio::test]
    async fn edge_le_refresh_expire_trente_jours_apres_l_ouverture() {
        let (bac, _) = Bac::neuf(true);
        bac.connecter("marie@example.eu", MDP).await.unwrap();
        assert_eq!(
            bac.sessions.ouvertes.borrow()[0].expire_le,
            instant() + chrono::Duration::days(30)
        );
    }

    #[tokio::test]
    async fn happy_cinq_echecs_verrouillent_et_alertent_une_fois() {
        let (bac, id) = Bac::neuf(true);
        for i in 0..5 {
            let e = bac
                .connecter_a("marie@example.eu", "Marie@2026Secur3", instant())
                .await
                .unwrap_err();
            assert_eq!(e.code(), "INVALID_CREDENTIALS", "échec {i}");
        }

        assert_eq!(bac.boite.alertes.borrow().len(), 1, "une seule alerte");
        let verrouillages: Vec<_> = bac
            .journal
            .entrees
            .borrow()
            .iter()
            .filter(|e| e.code == CodeAudit::AccountLocked)
            .map(|e| e.sujet_id)
            .collect();
        assert_eq!(verrouillages, vec![Some(id)]);
    }

    #[tokio::test]
    async fn negative_le_bon_mot_de_passe_sur_compte_verrouille_donne_423() {
        let (bac, _) = Bac::neuf(true);
        for _ in 0..5 {
            let _ = bac
                .connecter_a("marie@example.eu", "Marie@2026Secur3", instant())
                .await;
        }

        let e = bac.connecter("marie@example.eu", MDP).await.unwrap_err();
        match e {
            ErreurConnexion::CompteVerrouille { retry_after } => {
                assert_eq!(retry_after, 15 * 60, "FR-007 : verrou de 15 minutes");
            }
            autre => panic!("attendu un verrou, obtenu {autre:?}"),
        }
        assert!(bac.sessions.ouvertes.borrow().is_empty());
    }

    #[tokio::test]
    async fn happy_le_verrou_expire_et_la_connexion_repasse() {
        let (bac, _) = Bac::neuf(true);
        for _ in 0..5 {
            let _ = bac
                .connecter_a("marie@example.eu", "Marie@2026Secur3", instant())
                .await;
        }
        let apres = instant() + chrono::Duration::minutes(16);
        assert!(bac
            .connecter_a("marie@example.eu", MDP, apres)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn happy_une_connexion_reussie_remet_le_compteur_a_zero() {
        // Sans cela, quatre oublis suivis d'une connexion réussie laisseraient
        // le compte à un seul échec du verrou.
        let (bac, id) = Bac::neuf(true);
        for _ in 0..4 {
            let _ = bac
                .connecter_a("marie@example.eu", "Marie@2026Secur3", instant())
                .await;
        }
        bac.connecter("marie@example.eu", MDP).await.unwrap();

        let comptes = bac.depot.comptes.borrow();
        let compte = comptes.iter().find(|u| u.id == id).unwrap();
        assert_eq!(compte.verrouillage.echecs_consecutifs, 0);
        assert_eq!(compte.verrouillage.verrouille_jusqu_a, None);
    }

    #[tokio::test]
    async fn edge_quatre_echecs_ne_verrouillent_pas_et_n_alertent_pas() {
        let (bac, _) = Bac::neuf(true);
        for _ in 0..4 {
            let _ = bac
                .connecter_a("marie@example.eu", "Marie@2026Secur3", instant())
                .await;
        }
        assert!(bac.boite.alertes.borrow().is_empty());
        assert!(bac.connecter("marie@example.eu", MDP).await.is_ok());
    }

    #[tokio::test]
    async fn security_un_mauvais_mot_de_passe_sur_compte_verrouille_reste_un_401() {
        // Le coeur de l'arbitrage FR-007 : répondre 423 à qui ne connaît pas le
        // mot de passe apprendrait que l'adresse existe, ce que le scénario
        // `@security` du même FR interdit.
        let (bac, _) = Bac::neuf(true);
        for _ in 0..5 {
            let _ = bac
                .connecter_a("marie@example.eu", "Marie@2026Secur3", instant())
                .await;
        }

        let verrouille = bac
            .connecter("marie@example.eu", "EncoreUnAutre@2026")
            .await
            .unwrap_err();
        let inconnue = bac
            .connecter("personne@example.eu", "EncoreUnAutre@2026")
            .await
            .unwrap_err();
        assert_eq!(verrouille, inconnue);
        assert_eq!(verrouille.code(), "INVALID_CREDENTIALS");
    }

    #[tokio::test]
    async fn security_marteler_un_compte_verrouille_ne_prolonge_pas_le_verrou() {
        let (bac, id) = Bac::neuf(true);
        for _ in 0..5 {
            let _ = bac
                .connecter_a("marie@example.eu", "Marie@2026Secur3", instant())
                .await;
        }
        let fin = bac.depot.comptes.borrow()[0]
            .verrouillage
            .verrouille_jusqu_a;

        for minute in 1..10 {
            let _ = bac
                .connecter_a(
                    "marie@example.eu",
                    "Marie@2026Secur3",
                    instant() + chrono::Duration::minutes(minute),
                )
                .await;
        }

        let comptes = bac.depot.comptes.borrow();
        let compte = comptes.iter().find(|u| u.id == id).unwrap();
        assert_eq!(compte.verrouillage.verrouille_jusqu_a, fin);
        // Et toujours une seule alerte, quel que soit le nombre de tentatives.
        assert_eq!(bac.boite.alertes.borrow().len(), 1);
    }

    #[tokio::test]
    async fn security_un_compte_inexistant_ne_declenche_aucune_alerte() {
        // Sinon, un attaquant se servirait du service pour expédier des
        // courriels vers des adresses de son choix.
        let (bac, _) = Bac::neuf(true);
        for _ in 0..10 {
            let _ = bac
                .connecter_a("personne@example.eu", "Marie@2026Secur3", instant())
                .await;
        }
        assert!(bac.boite.alertes.borrow().is_empty());
    }

    #[tokio::test]
    async fn security_le_refus_est_identique_que_l_adresse_existe_ou_non() {
        // C'est ce test qui échouerait si quelqu'un ajoutait un
        // `USER_NOT_FOUND` par confort de débogage.
        let (bac, _) = Bac::neuf(true);
        let inconnue = bac.connecter("personne@example.eu", MDP).await.unwrap_err();
        let faux = bac
            .connecter("marie@example.eu", "Marie@2026Secur3")
            .await
            .unwrap_err();
        assert_eq!(inconnue, faux);
        assert_eq!(inconnue.to_string(), faux.to_string());
    }

    #[tokio::test]
    async fn security_un_echec_n_est_jamais_relie_a_un_compte_dans_l_audit() {
        let (bac, id) = Bac::neuf(true);
        let _ = bac.connecter("marie@example.eu", "Marie@2026Secur3").await;
        let _ = bac.connecter("personne@example.eu", MDP).await;

        let entrees = bac.journal.entrees.borrow();
        assert_eq!(entrees.len(), 2);
        for entree in entrees.iter() {
            assert_eq!(entree.code, CodeAudit::UserLoginFailed);
            assert_eq!(entree.sujet_id, None);
            assert_ne!(entree.sujet_id, Some(id));
        }
    }

    #[tokio::test]
    async fn security_le_refresh_n_est_conserve_que_hache() {
        let (bac, _) = Bac::neuf(true);
        let session = bac.connecter("marie@example.eu", MDP).await.unwrap();
        let conserve = &bac.sessions.ouvertes.borrow()[0].empreinte;
        assert_ne!(conserve.as_str(), session.refresh);
        assert_eq!(conserve.as_str().len(), 64);
        assert!(!conserve.as_str().contains(REFRESH));
    }

    #[tokio::test]
    async fn security_un_compte_non_verifie_n_obtient_jamais_de_jeton() {
        let (bac, _) = Bac::neuf(false);
        assert!(bac.connecter("marie@example.eu", MDP).await.is_err());
        assert!(bac.sessions.ouvertes.borrow().is_empty());
    }

    #[tokio::test]
    async fn security_l_erreur_ne_repete_jamais_le_mot_de_passe() {
        let (bac, _) = Bac::neuf(true);
        let e = bac
            .connecter("marie@example.eu", "MotDePasseTresParticulier@2026")
            .await
            .unwrap_err();
        assert!(!e.to_string().contains("MotDePasse"));
        assert!(!format!("{e:?}").contains("MotDePasse"));
    }
}
