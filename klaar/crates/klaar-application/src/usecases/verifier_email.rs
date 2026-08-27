//! Cas d'usage « vérifier l'adresse email » (FR-001, Story 1.2).
//!
//! **Écart tracé avec le PRD.** Le tableau des endpoints du PRD annonce
//! `GET /api/v1/auth/verify-email?token=…`. Le lien envoyé par courriel ne
//! pointe pas cet endpoint : il ouvre la page `/verifier-email` de la PWA, qui
//! présente ensuite le jeton par un `POST`.
//!
//! La raison n'est pas stylistique. Les passerelles de messagerie
//! d'entreprise, Outlook et Gmail en tête, visitent les liens des courriels
//! avant leur destinataire pour les analyser. Un `GET` qui consomme le jeton
//! est donc consommé par l'antivirus, et l'utilisateur trouve un lien déjà
//! utilisé au moment où il clique. Ouvrir une page statique n'a aucun effet ;
//! seul le `POST` déclenché par le navigateur en a un.

use chrono::{DateTime, Utc};
use klaar_identity::{EmpreinteJeton, JetonVerification};
use std::fmt;
use uuid::Uuid;

use crate::ports::audit::{CodeAudit, EntreeAudit, JournalAudit};
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::utilisateur_repository::{ResultatJeton, UtilisateurRepository};

/// Issue exposable de la vérification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultatVerification {
    /// Le compte vient de passer en `ACTIVE`.
    Verifie,
    /// Le compte l'était déjà. Un rechargement de page, pas une erreur.
    DejaVerifie,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurVerification {
    /// Jeton absent de la requête.
    JetonManquant,
    /// Jeton inconnu, ou dont le compte a disparu.
    JetonInvalide,
    /// Jeton connu mais passé sa validité d'une heure.
    JetonExpire,
    Indisponible(String),
}

impl ErreurVerification {
    pub fn code(&self) -> &'static str {
        match self {
            Self::JetonManquant => "TOKEN_MISSING",
            Self::JetonInvalide => "TOKEN_INVALID",
            Self::JetonExpire => "TOKEN_EXPIRED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }

    pub fn est_saisie_invalide(&self) -> bool {
        !matches!(self, Self::Indisponible(_))
    }
}

impl fmt::Display for ErreurVerification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JetonManquant => write!(f, "jeton absent"),
            Self::JetonInvalide => write!(f, "jeton inconnu"),
            Self::JetonExpire => write!(f, "jeton expiré"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurVerification {}

impl From<RepositoryError> for ErreurVerification {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

pub async fn verifier_email<R, J, H>(
    depot: &R,
    journal: &J,
    horloge: &H,
    jeton_presente: &str,
) -> Result<ResultatVerification, ErreurVerification>
where
    R: UtilisateurRepository,
    J: JournalAudit,
    H: Horloge,
{
    if jeton_presente.trim().is_empty() {
        return Err(ErreurVerification::JetonManquant);
    }

    // Le jeton présenté est haché avant d'atteindre la base : la requête porte
    // l'empreinte, jamais la valeur. Une requête lente enregistrée par la base
    // ne révèle donc pas de jeton utilisable.
    let empreinte: EmpreinteJeton = JetonVerification::depuis_chaine(jeton_presente).empreinte();
    let maintenant = horloge.maintenant();

    let resultat = depot
        .consommer_jeton_verification(&empreinte, maintenant)
        .await?;

    match resultat {
        ResultatJeton::Consomme { utilisateur_id } => {
            consigner(journal, utilisateur_id, maintenant).await?;
            Ok(ResultatVerification::Verifie)
        }
        // Pas de seconde entrée d'audit : l'événement « adresse vérifiée » a
        // déjà eu lieu, et le journaliser à chaque rechargement de page rendrait
        // le décompte des vérifications faux.
        ResultatJeton::DejaConsomme { .. } => Ok(ResultatVerification::DejaVerifie),
        ResultatJeton::Expire => Err(ErreurVerification::JetonExpire),
        ResultatJeton::Inconnu => Err(ErreurVerification::JetonInvalide),
    }
}

async fn consigner<J: JournalAudit>(
    journal: &J,
    utilisateur_id: Uuid,
    horodatage: DateTime<Utc>,
) -> Result<(), RepositoryError> {
    journal
        .consigner(EntreeAudit {
            code: CodeAudit::UserEmailVerified,
            sujet_id: Some(utilisateur_id),
            horodatage,
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::utilisateur_repository::JetonAConserver;
    use chrono::{Duration, TimeZone};
    use klaar_identity::Utilisateur;
    use klaar_shared_kernel::Email;
    use std::cell::RefCell;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    struct Ligne {
        empreinte: EmpreinteJeton,
        utilisateur_id: Uuid,
        expire_le: DateTime<Utc>,
        consomme: bool,
    }

    #[derive(Default)]
    struct DepotMemoire {
        lignes: RefCell<Vec<Ligne>>,
        actifs: RefCell<Vec<Uuid>>,
        en_panne: bool,
    }

    impl DepotMemoire {
        fn avec_jeton(jeton: &JetonVerification, expire_le: DateTime<Utc>) -> (Self, Uuid) {
            let id = Uuid::new_v4();
            let depot = Self::default();
            depot.lignes.borrow_mut().push(Ligne {
                empreinte: jeton.empreinte(),
                utilisateur_id: id,
                expire_le,
                consomme: false,
            });
            (depot, id)
        }
    }

    impl UtilisateurRepository for DepotMemoire {
        async fn creer_si_absent(
            &self,
            _: &Utilisateur,
            _: &JetonAConserver,
        ) -> Result<bool, RepositoryError> {
            unreachable!("hors du périmètre de ce cas d'usage")
        }

        async fn consommer_jeton_verification(
            &self,
            empreinte: &EmpreinteJeton,
            maintenant: DateTime<Utc>,
        ) -> Result<ResultatJeton, RepositoryError> {
            if self.en_panne {
                return Err(RepositoryError::Indisponible("test".into()));
            }
            let mut lignes = self.lignes.borrow_mut();
            let Some(ligne) = lignes.iter_mut().find(|l| &l.empreinte == empreinte) else {
                return Ok(ResultatJeton::Inconnu);
            };
            if ligne.consomme {
                return Ok(ResultatJeton::DejaConsomme {
                    utilisateur_id: ligne.utilisateur_id,
                });
            }
            if ligne.expire_le <= maintenant {
                return Ok(ResultatJeton::Expire);
            }
            ligne.consomme = true;
            self.actifs.borrow_mut().push(ligne.utilisateur_id);
            Ok(ResultatJeton::Consomme {
                utilisateur_id: ligne.utilisateur_id,
            })
        }

        async fn mettre_a_jour_verrouillage(
            &self,
            _: Uuid,
            _: &klaar_identity::Verrouillage,
        ) -> Result<(), RepositoryError> {
            unreachable!("hors du périmètre de ce cas d'usage")
        }

        async fn par_email(&self, _: &Email) -> Result<Option<Utilisateur>, RepositoryError> {
            Ok(None)
        }

        async fn par_id(&self, _: Uuid) -> Result<Option<Utilisateur>, RepositoryError> {
            Ok(None)
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

    #[tokio::test]
    async fn happy_un_jeton_valide_active_le_compte_et_est_audite() {
        let jeton = JetonVerification::tirer();
        let (depot, id) = DepotMemoire::avec_jeton(&jeton, instant() + Duration::hours(1));
        let journal = JournalMemoire::default();

        let r = verifier_email(&depot, &journal, &HorlogeFigee(instant()), jeton.expose())
            .await
            .unwrap();

        assert_eq!(r, ResultatVerification::Verifie);
        assert_eq!(depot.actifs.borrow().as_slice(), &[id]);
        let entrees = journal.entrees.borrow();
        assert_eq!(entrees.len(), 1);
        assert_eq!(entrees[0].code, CodeAudit::UserEmailVerified);
        assert_eq!(entrees[0].sujet_id, Some(id));
    }

    #[tokio::test]
    async fn negative_un_jeton_inconnu_est_refuse() {
        let jeton = JetonVerification::tirer();
        let (depot, _) = DepotMemoire::avec_jeton(&jeton, instant() + Duration::hours(1));
        let journal = JournalMemoire::default();

        let e = verifier_email(
            &depot,
            &journal,
            &HorlogeFigee(instant()),
            JetonVerification::tirer().expose(),
        )
        .await
        .unwrap_err();

        assert_eq!(e.code(), "TOKEN_INVALID");
        assert!(depot.actifs.borrow().is_empty());
        assert!(journal.entrees.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_un_jeton_expire_est_refuse_sans_activer() {
        let jeton = JetonVerification::tirer();
        let (depot, _) = DepotMemoire::avec_jeton(&jeton, instant() + Duration::hours(1));
        let journal = JournalMemoire::default();

        // Deux heures plus tard : au-delà de l'heure de validité de FR-001.
        let e = verifier_email(
            &depot,
            &journal,
            &HorlogeFigee(instant() + Duration::hours(2)),
            jeton.expose(),
        )
        .await
        .unwrap_err();

        assert_eq!(e.code(), "TOKEN_EXPIRED");
        assert!(depot.actifs.borrow().is_empty());
    }

    #[tokio::test]
    async fn negative_un_jeton_absent_ne_touche_pas_la_base() {
        let depot = DepotMemoire {
            en_panne: true,
            ..Default::default()
        };
        let journal = JournalMemoire::default();
        for vide in ["", "   "] {
            let e = verifier_email(&depot, &journal, &HorlogeFigee(instant()), vide)
                .await
                .unwrap_err();
            // `TOKEN_MISSING` et non `SERVICE_UNAVAILABLE` : la requête est
            // rejetée avant tout appel, comme le prouve le dépôt en panne.
            assert_eq!(e.code(), "TOKEN_MISSING");
        }
    }

    #[tokio::test]
    async fn negative_une_panne_ne_passe_pas_pour_un_jeton_invalide() {
        let depot = DepotMemoire {
            en_panne: true,
            ..Default::default()
        };
        let journal = JournalMemoire::default();
        let e = verifier_email(
            &depot,
            &journal,
            &HorlogeFigee(instant()),
            JetonVerification::tirer().expose(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code(), "SERVICE_UNAVAILABLE");
        assert!(!e.est_saisie_invalide());
    }

    #[tokio::test]
    async fn edge_le_second_clic_ne_produit_pas_d_erreur() {
        // Un lien ouvert deux fois, ou une page rechargée, est le cas le plus
        // banal du parcours. Y répondre par une erreur ferait croire à un échec
        // à quelqu'un dont le compte vient d'être activé.
        let jeton = JetonVerification::tirer();
        let (depot, _) = DepotMemoire::avec_jeton(&jeton, instant() + Duration::hours(1));
        let journal = JournalMemoire::default();
        let horloge = HorlogeFigee(instant());

        assert_eq!(
            verifier_email(&depot, &journal, &horloge, jeton.expose())
                .await
                .unwrap(),
            ResultatVerification::Verifie
        );
        assert_eq!(
            verifier_email(&depot, &journal, &horloge, jeton.expose())
                .await
                .unwrap(),
            ResultatVerification::DejaVerifie
        );
    }

    #[tokio::test]
    async fn edge_le_second_clic_n_ajoute_pas_d_entree_d_audit() {
        let jeton = JetonVerification::tirer();
        let (depot, _) = DepotMemoire::avec_jeton(&jeton, instant() + Duration::hours(1));
        let journal = JournalMemoire::default();
        let horloge = HorlogeFigee(instant());

        for _ in 0..3 {
            let _ = verifier_email(&depot, &journal, &horloge, jeton.expose()).await;
        }
        assert_eq!(
            journal.entrees.borrow().len(),
            1,
            "une adresse n'est vérifiée qu'une fois, quel que soit le nombre de clics"
        );
    }

    #[tokio::test]
    async fn edge_un_jeton_expirant_a_la_seconde_pres_est_refuse() {
        // Borne exacte : `expire_le <= maintenant` refuse. Un jeton valable
        // « jusqu'à » son instant d'expiration inclus étendrait la fenêtre d'une
        // seconde, ce qui n'est pas grave mais rend la règle floue.
        let jeton = JetonVerification::tirer();
        let expiration = instant() + Duration::hours(1);
        let (depot, _) = DepotMemoire::avec_jeton(&jeton, expiration);
        let journal = JournalMemoire::default();

        let e = verifier_email(&depot, &journal, &HorlogeFigee(expiration), jeton.expose())
            .await
            .unwrap_err();
        assert_eq!(e.code(), "TOKEN_EXPIRED");
    }

    #[tokio::test]
    async fn security_le_jeton_expire_ne_devient_pas_valable_en_le_representant() {
        let jeton = JetonVerification::tirer();
        let (depot, _) = DepotMemoire::avec_jeton(&jeton, instant() + Duration::hours(1));
        let journal = JournalMemoire::default();
        for _ in 0..5 {
            assert!(verifier_email(
                &depot,
                &journal,
                &HorlogeFigee(instant() + Duration::hours(2)),
                jeton.expose()
            )
            .await
            .is_err());
        }
        assert!(depot.actifs.borrow().is_empty());
    }

    #[tokio::test]
    async fn security_un_jeton_voisin_d_un_caractere_ne_passe_pas() {
        // Le jeton est comparé par son empreinte : une différence d'un
        // caractère produit une empreinte sans rapport, il n'y a pas de
        // comparaison partielle exploitable.
        let jeton = JetonVerification::tirer();
        let (depot, _) = DepotMemoire::avec_jeton(&jeton, instant() + Duration::hours(1));
        let journal = JournalMemoire::default();

        let mut altere = jeton.expose().to_string();
        altere.pop();
        altere.push(if jeton.expose().ends_with('A') {
            'B'
        } else {
            'A'
        });

        let e = verifier_email(&depot, &journal, &HorlogeFigee(instant()), &altere)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "TOKEN_INVALID");
    }

    #[tokio::test]
    async fn security_l_erreur_ne_renvoie_jamais_le_jeton_presente() {
        // Un message d'erreur qui répète le jeton le fait entrer dans les
        // journaux du client, les rapports d'anomalie et les captures d'écran.
        let depot = DepotMemoire::default();
        let journal = JournalMemoire::default();
        let jeton = JetonVerification::tirer();
        let e = verifier_email(&depot, &journal, &HorlogeFigee(instant()), jeton.expose())
            .await
            .unwrap_err();
        assert!(!e.to_string().contains(jeton.expose()));
        assert!(!format!("{e:?}").contains(jeton.expose()));
    }
}
