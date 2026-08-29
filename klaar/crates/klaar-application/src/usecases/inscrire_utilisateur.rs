//! Cas d'usage « inscrire un utilisateur » (FR-001, Story 1.1).
//!
//! **Arbitrage tracé.** FR-001 se contredit : son scénario `@negative` demande
//! `409 EMAIL_ALREADY_EXISTS` sur une adresse déjà prise, son scénario
//! `@security` demande une réponse « identique (timing + payload) » que
//! l'adresse existe ou non. Les deux ne peuvent pas être vrais. C'est
//! l'anti-énumération qui l'emporte : un `409` transforme l'inscription en
//! oracle permettant de tester la présence de n'importe quelle adresse, ce que
//! le même FR interdit deux paragraphes plus bas. Le `409` disparaît donc du
//! contrat.
//!
//! Rendre les deux chemins indistinguables demande davantage que de renvoyer
//! le même corps :
//!
//! - le mot de passe est haché **avant** que la base soit interrogée, sans
//!   quoi le chemin « adresse déjà prise » économiserait les dizaines de
//!   millisecondes d'argon2 et se reconnaîtrait au chronomètre ;
//! - un courriel part dans les deux cas (voir `CourrielInscription`), pour la
//!   même raison ;
//! - le journal d'audit distingue les deux cas, lui, mais il n'est pas exposé.

use klaar_identity::{
    EmpreinteMotDePasse, MotDePasse, MotDePasseError, ParametresArgon2, Utilisateur,
};
use klaar_shared_kernel::{Email, EmailError, Locale};
use std::fmt;

use crate::ports::audit::{CodeAudit, EntreeAudit, JournalAudit};
use crate::ports::courriel::{CourrielInscription, EnvoiCourriel};
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::utilisateur_repository::{JetonAConserver, UtilisateurRepository};

/// Locale servie quand celle demandée n'est pas prise en charge (FR-001
/// `@edge`, invariant §10.9 : FR, NL et EN uniquement).
pub const LOCALE_DEFAUT: Locale = Locale::Fr;

#[derive(Debug, Clone)]
pub struct CommandeInscription {
    pub email: String,
    pub mot_de_passe: String,
    /// Absente ou non prise en charge : `LOCALE_DEFAUT` s'applique.
    pub locale: Option<String>,
}

/// Ce que le cas d'usage a réellement fait. **Ne doit pas ressortir tel quel
/// dans la réponse HTTP** : c'est l'information que l'anti-énumération
/// protège.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultatInscription {
    CompteCree,
    AdresseDejaPrise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurInscription {
    Email(EmailError),
    MotDePasse(MotDePasseError),
    /// Panne d'infrastructure. Le client doit pouvoir réessayer ; ce n'est pas
    /// une erreur de sa saisie.
    Indisponible(String),
}

impl ErreurInscription {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Email(EmailError::Empty) => "EMAIL_EMPTY",
            Self::Email(_) => "EMAIL_MALFORMED",
            Self::MotDePasse(e) => e.code(),
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }

    pub fn est_saisie_invalide(&self) -> bool {
        !matches!(self, Self::Indisponible(_))
    }
}

impl fmt::Display for ErreurInscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Email(e) => write!(f, "{e}"),
            Self::MotDePasse(e) => write!(f, "{e}"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurInscription {}

impl From<RepositoryError> for ErreurInscription {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Résout la locale demandée, en repliant sur `LOCALE_DEFAUT`.
///
/// Une locale inconnue n'est pas une erreur de saisie : refuser l'inscription
/// parce que le navigateur annonce `de` punirait l'utilisateur pour une
/// préférence d'affichage.
fn resoudre_locale(demandee: Option<&str>) -> Locale {
    match demandee {
        None => LOCALE_DEFAUT,
        Some(valeur) => Locale::parse(valeur).unwrap_or_else(|_| {
            tracing::warn!(
                code = "LOCALE_FALLBACK",
                demandee = valeur,
                servie = LOCALE_DEFAUT.as_str(),
                "locale non prise en charge, repli"
            );
            LOCALE_DEFAUT
        }),
    }
}

pub async fn inscrire<R, C, J, H>(
    depot: &R,
    courriel: &C,
    journal: &J,
    horloge: &H,
    parametres: ParametresArgon2,
    commande: CommandeInscription,
) -> Result<ResultatInscription, ErreurInscription>
where
    R: UtilisateurRepository,
    C: EnvoiCourriel,
    J: JournalAudit,
    H: Horloge,
{
    let email = Email::parse(&commande.email).map_err(ErreurInscription::Email)?;
    let mot_de_passe =
        MotDePasse::parse(&commande.mot_de_passe).map_err(ErreurInscription::MotDePasse)?;
    let locale = resoudre_locale(commande.locale.as_deref());

    // Haché avant toute interrogation de la base : c'est ce qui rend les deux
    // chemins comparables au chronomètre.
    let empreinte = EmpreinteMotDePasse::calculer(&mot_de_passe, parametres)
        .map_err(ErreurInscription::MotDePasse)?;

    let maintenant = horloge.maintenant();
    let utilisateur = Utilisateur::inscrire(email.clone(), empreinte, locale, maintenant);
    let jeton = Utilisateur::emettre_jeton_verification(maintenant);

    let cree = depot
        .creer_si_absent(
            &utilisateur,
            &JetonAConserver {
                empreinte: jeton.empreinte.clone(),
                expire_le: jeton.expire_le,
            },
        )
        .await?;

    let (contenu, code, sujet_id) = if cree {
        (
            CourrielInscription::Verification {
                jeton: jeton.en_clair,
            },
            CodeAudit::UserSignup,
            Some(utilisateur.id),
        )
    } else {
        (
            CourrielInscription::CompteDejaExistant,
            CodeAudit::UserSignupDuplicate,
            None,
        )
    };

    journal
        .consigner(EntreeAudit {
            code,
            sujet_id,
            horodatage: maintenant,
        })
        .await?;

    // Un échec d'envoi ne défait pas l'inscription. Le compte existe, le jeton
    // est valable une heure, et un renvoi (Story 1.2) rattrape le cas. Faire
    // l'inverse — annuler le compte — rendrait l'adresse indisponible à la
    // tentative suivante pour une panne de messagerie.
    if let Err(e) = courriel.envoyer_inscription(&email, locale, contenu).await {
        tracing::error!(erreur = %e, "courriel d'inscription non envoyé");
    }

    Ok(if cree {
        ResultatInscription::CompteCree
    } else {
        ResultatInscription::AdresseDejaPrise
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use chrono::{DateTime, TimeZone, Utc};
    use klaar_identity::{EmpreinteJeton, JetonVerification};
    use klaar_shared_kernel::Email;
    use std::cell::RefCell;
    use uuid::Uuid;

    const P: ParametresArgon2 = ParametresArgon2::tests();

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    #[derive(Default)]
    struct DepotMemoire {
        comptes: RefCell<Vec<Utilisateur>>,
        jetons: RefCell<Vec<(Uuid, JetonAConserver)>>,
        en_panne: bool,
    }

    impl UtilisateurRepository for DepotMemoire {
        async fn purger_non_verifies(
            &self,
            _avant: DateTime<Utc>,
            _par_passage_max: i64,
        ) -> Result<u64, RepositoryError> {
            // Ce double ne sert pas la purge : les cas d'usage testés ici ne
            // l'appellent pas, et lui donner un comportement inventé ferait croire
            // à une couverture qui n'existe pas.
            unimplemented!("purge non sollicitée par ce cas d'usage")
        }

        async fn definir_locale(
            &self,
            _: Uuid,
            _: klaar_shared_kernel::Locale,
        ) -> Result<bool, RepositoryError> {
            unreachable!()
        }
        async fn creer_si_absent(
            &self,
            utilisateur: &Utilisateur,
            jeton: &JetonAConserver,
        ) -> Result<bool, RepositoryError> {
            if self.en_panne {
                return Err(RepositoryError::Indisponible("test".into()));
            }
            if self
                .comptes
                .borrow()
                .iter()
                .any(|u| u.email == utilisateur.email)
            {
                return Ok(false);
            }
            self.comptes.borrow_mut().push(utilisateur.clone());
            self.jetons
                .borrow_mut()
                .push((utilisateur.id, jeton.clone()));
            Ok(true)
        }

        async fn consommer_jeton_verification(
            &self,
            _: &klaar_identity::EmpreinteJeton,
            _: DateTime<Utc>,
        ) -> Result<crate::ports::utilisateur_repository::ResultatJeton, RepositoryError> {
            unreachable!("hors du périmètre de ce cas d'usage")
        }

        async fn mettre_a_jour_verrouillage(
            &self,
            _: Uuid,
            _: &klaar_identity::Verrouillage,
        ) -> Result<(), RepositoryError> {
            unreachable!("hors du périmètre de ce cas d'usage")
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

    /// Trace d'un envoi. Une structure nommée plutôt qu'un tuple : les
    /// assertions ci-dessous parlent de `genre` et de `jeton`, pas de `.2` et
    /// de `.3`.
    struct Envoi {
        destinataire: String,
        locale: Locale,
        genre: &'static str,
        jeton: Option<String>,
    }

    #[derive(Default)]
    struct BoiteAuxLettres {
        envois: RefCell<Vec<Envoi>>,
    }

    impl EnvoiCourriel for BoiteAuxLettres {
        async fn envoyer_securite(
            &self,
            _: &Email,
            _: Locale,
            _: crate::ports::courriel::CourrielSecurite,
        ) -> Result<(), crate::ports::courriel::ErreurEnvoi> {
            unreachable!("hors du périmètre de ce cas d'usage")
        }

        async fn envoyer_inscription(
            &self,
            destinataire: &Email,
            locale: Locale,
            contenu: CourrielInscription,
        ) -> Result<(), crate::ports::courriel::ErreurEnvoi> {
            let (genre, jeton) = match contenu {
                CourrielInscription::Verification { jeton } => {
                    ("verification", Some(jeton.expose().to_string()))
                }
                CourrielInscription::CompteDejaExistant => ("deja-existant", None),
            };
            self.envois.borrow_mut().push(Envoi {
                destinataire: destinataire.as_str().to_string(),
                locale,
                genre,
                jeton,
            });
            Ok(())
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
        depot: DepotMemoire,
        boite: BoiteAuxLettres,
        journal: JournalMemoire,
        horloge: HorlogeFigee,
    }

    impl Bac {
        fn neuf() -> Self {
            Self {
                depot: DepotMemoire::default(),
                boite: BoiteAuxLettres::default(),
                journal: JournalMemoire::default(),
                horloge: HorlogeFigee(instant()),
            }
        }

        async fn inscrire(
            &self,
            email: &str,
            mot_de_passe: &str,
            locale: Option<&str>,
        ) -> Result<ResultatInscription, ErreurInscription> {
            inscrire(
                &self.depot,
                &self.boite,
                &self.journal,
                &self.horloge,
                P,
                CommandeInscription {
                    email: email.to_string(),
                    mot_de_passe: mot_de_passe.to_string(),
                    locale: locale.map(str::to_string),
                },
            )
            .await
        }
    }

    #[tokio::test]
    async fn happy_cree_un_compte_en_attente_avec_jeton_courriel_et_audit() {
        let bac = Bac::neuf();
        let r = bac
            .inscrire("marie@example.eu", "Marie@2026Secure", Some("fr"))
            .await
            .unwrap();
        assert_eq!(r, ResultatInscription::CompteCree);

        let comptes = bac.depot.comptes.borrow();
        let compte = comptes.first().expect("un compte créé");
        assert_eq!(compte.statut.as_str(), "PENDING_EMAIL_VERIFY");
        assert_eq!(compte.email.as_str(), "marie@example.eu");
        assert_eq!(compte.locale, Locale::Fr);

        // Le jeton conservé est l'empreinte de celui envoyé, jamais sa valeur.
        let jetons = bac.depot.jetons.borrow();
        let (_, conserve) = jetons.first().expect("un jeton conservé");
        assert_eq!(conserve.expire_le, instant() + chrono::Duration::hours(1));

        let envois = bac.boite.envois.borrow();
        let envoi = envois.first().expect("un courriel");
        assert_eq!(envoi.destinataire, "marie@example.eu");
        assert_eq!(envoi.genre, "verification");
        let envoye = envoi.jeton.as_deref().expect("un jeton dans le courriel");
        assert_eq!(
            conserve.empreinte,
            EmpreinteJeton::calculer(&JetonVerification::depuis_chaine(envoye))
        );

        let entrees = bac.journal.entrees.borrow();
        assert_eq!(entrees[0].code, CodeAudit::UserSignup);
        assert_eq!(entrees[0].sujet_id, Some(compte.id));
    }

    #[tokio::test]
    async fn negative_refuse_un_email_malforme_sans_rien_ecrire() {
        let bac = Bac::neuf();
        let e = bac
            .inscrire("invalide", "Marie@2026Secure", None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "EMAIL_MALFORMED");
        assert!(bac.depot.comptes.borrow().is_empty());
        assert!(bac.boite.envois.borrow().is_empty());
        assert!(bac.journal.entrees.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_couvre_les_quatre_saisies_invalides_du_prd() {
        let bac = Bac::neuf();
        let cas = [
            ("invalide", "Marie@2026Secure", "EMAIL_MALFORMED"),
            ("marie@example.eu", "court", "PASSWORD_TOO_SHORT"),
            ("marie@example.eu", "", "PASSWORD_EMPTY"),
            ("", "Marie@2026Secure", "EMAIL_EMPTY"),
        ];
        for (email, mdp, attendu) in cas {
            let e = bac.inscrire(email, mdp, None).await.unwrap_err();
            assert_eq!(e.code(), attendu, "cas {email:?}/{mdp:?}");
            assert!(e.est_saisie_invalide());
        }
    }

    #[tokio::test]
    async fn negative_une_panne_de_depot_ne_passe_pas_pour_une_erreur_de_saisie() {
        let bac = Bac {
            depot: DepotMemoire {
                en_panne: true,
                ..Default::default()
            },
            ..Bac::neuf()
        };
        let e = bac
            .inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "SERVICE_UNAVAILABLE");
        assert!(!e.est_saisie_invalide());
    }

    #[tokio::test]
    async fn edge_une_seconde_inscription_ne_cree_pas_de_doublon() {
        let bac = Bac::neuf();
        bac.inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap();
        let r = bac
            .inscrire("marie@example.eu", "UnAutre@2026Mdp", None)
            .await
            .unwrap();
        assert_eq!(r, ResultatInscription::AdresseDejaPrise);
        assert_eq!(bac.depot.comptes.borrow().len(), 1);
        // Un seul lien de vérification émis : le second courriel n'en porte pas.
        let envois = bac.boite.envois.borrow();
        assert_eq!(envois.len(), 2);
        assert_eq!(envois[1].genre, "deja-existant");
        assert!(envois[1].jeton.is_none());
    }

    #[tokio::test]
    async fn edge_deux_ecritures_differentes_de_la_meme_adresse_sont_le_meme_compte() {
        // La normalisation vit dans `Email`, mais c'est ici qu'elle compte :
        // sans elle, la casse suffit à contourner l'unicité.
        let bac = Bac::neuf();
        bac.inscrire("Marie@Example.EU", "Marie@2026Secure", None)
            .await
            .unwrap();
        let r = bac
            .inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap();
        assert_eq!(r, ResultatInscription::AdresseDejaPrise);
        assert_eq!(bac.depot.comptes.borrow().len(), 1);
    }

    #[tokio::test]
    async fn edge_une_locale_non_prise_en_charge_se_replie_sans_echouer() {
        let bac = Bac::neuf();
        let r = bac
            .inscrire("marie@example.eu", "Marie@2026Secure", Some("de"))
            .await
            .unwrap();
        assert_eq!(r, ResultatInscription::CompteCree);
        assert_eq!(bac.depot.comptes.borrow()[0].locale, Locale::Fr);
        assert_eq!(bac.boite.envois.borrow()[0].locale, Locale::Fr);
    }

    #[tokio::test]
    async fn edge_une_locale_absente_prend_la_valeur_par_defaut() {
        let bac = Bac::neuf();
        bac.inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap();
        assert_eq!(bac.depot.comptes.borrow()[0].locale, LOCALE_DEFAUT);
    }

    #[tokio::test]
    async fn edge_les_trois_locales_prises_en_charge_sont_conservees() {
        for (code, attendue) in [("fr", Locale::Fr), ("NL", Locale::Nl), ("en", Locale::En)] {
            let bac = Bac::neuf();
            bac.inscrire("marie@example.eu", "Marie@2026Secure", Some(code))
                .await
                .unwrap();
            assert_eq!(bac.depot.comptes.borrow()[0].locale, attendue);
        }
    }

    #[tokio::test]
    async fn security_le_resultat_ne_dit_pas_au_client_si_l_adresse_existait() {
        // Le cas d'usage distingue les deux, l'appelant HTTP ne doit pas. Ce
        // test fixe le contrat que la route devra respecter : même variante
        // d'erreur (aucune), donc rien à mapper différemment.
        let bac = Bac::neuf();
        let premier = bac
            .inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await;
        let second = bac
            .inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await;
        assert!(premier.is_ok() && second.is_ok());
    }

    #[tokio::test]
    async fn security_un_courriel_part_dans_les_deux_cas() {
        // Sans cela, le chemin « adresse déjà prise » est plus court d'un
        // envoi, et se reconnaît au temps de réponse.
        let bac = Bac::neuf();
        bac.inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap();
        bac.inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap();
        assert_eq!(bac.boite.envois.borrow().len(), 2);
    }

    #[tokio::test]
    async fn security_l_audit_du_doublon_ne_designe_pas_le_titulaire() {
        let bac = Bac::neuf();
        bac.inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap();
        bac.inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap();
        let entrees = bac.journal.entrees.borrow();
        assert_eq!(entrees[1].code, CodeAudit::UserSignupDuplicate);
        assert_eq!(
            entrees[1].sujet_id, None,
            "le journal ne doit pas relier la tentative au compte existant"
        );
    }

    #[tokio::test]
    async fn security_le_mot_de_passe_n_est_jamais_conserve_en_clair() {
        let bac = Bac::neuf();
        bac.inscrire("marie@example.eu", "Marie@2026Secure", None)
            .await
            .unwrap();
        let comptes = bac.depot.comptes.borrow();
        let phc = comptes[0]
            .empreinte_mot_de_passe
            .as_ref()
            .expect("un compte neuf a toujours une empreinte")
            .as_str();
        assert!(phc.starts_with("$argon2id$"));
        assert!(!phc.contains("Marie@2026Secure"));
        assert!(!format!("{:?}", comptes[0]).contains("Marie@2026Secure"));
    }

    #[tokio::test]
    async fn security_un_echec_d_envoi_ne_defait_pas_l_inscription() {
        struct BoiteEnPanne;
        impl EnvoiCourriel for BoiteEnPanne {
            async fn envoyer_securite(
                &self,
                _: &Email,
                _: Locale,
                _: crate::ports::courriel::CourrielSecurite,
            ) -> Result<(), crate::ports::courriel::ErreurEnvoi> {
                unreachable!("hors du périmètre de ce cas d'usage")
            }

            async fn envoyer_inscription(
                &self,
                _: &Email,
                _: Locale,
                _: CourrielInscription,
            ) -> Result<(), crate::ports::courriel::ErreurEnvoi> {
                Err(crate::ports::courriel::ErreurEnvoi(
                    "relais injoignable".into(),
                ))
            }
        }
        let depot = DepotMemoire::default();
        let journal = JournalMemoire::default();
        let r = inscrire(
            &depot,
            &BoiteEnPanne,
            &journal,
            &HorlogeFigee(instant()),
            P,
            CommandeInscription {
                email: "marie@example.eu".into(),
                mot_de_passe: "Marie@2026Secure".into(),
                locale: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(r, ResultatInscription::CompteCree);
        assert_eq!(depot.comptes.borrow().len(), 1);
    }
}
