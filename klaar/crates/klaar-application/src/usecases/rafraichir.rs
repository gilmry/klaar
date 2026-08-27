//! Cas d'usage « rafraîchir la session » et « se déconnecter » (FR-004,
//! Story 1.4).
//!
//! **La rotation est ce qui rend le vol détectable.** Chaque présentation
//! consomme le refresh et en rend un neuf. Le porteur légitime a donc toujours
//! le dernier ; présenter un refresh déjà consommé signifie qu'il en circule
//! une copie. On ne sait pas laquelle des deux est la bonne — c'est pourquoi la
//! **famille entière** est coupée, et non le seul jeton rejoué : couper l'autre
//! laisserait le voleur en place, et il n'y a aucun moyen de les distinguer.
//! Le coût est une reconnexion pour le porteur légitime, ce qui est bien
//! moindre qu'une session volée qui dure trente jours.
//!
//! **Ce que le contexte apporte, et ce qu'il n'apporte pas.** FR-004
//! `@security` demande un lien entre le refresh et « UA + IP + device ».
//!
//! - L'**agent utilisateur** est lié, sous forme d'empreinte. Un changement
//!   lève `SESSION_CONTEXT_CHANGED` dans le journal d'audit **sans couper la
//!   session** : les navigateurs modifient leur agent utilisateur à chaque mise
//!   à jour, toutes les quelques semaines, et bloquer là-dessus déconnecterait
//!   tout le monde en même temps sans qu'aucun vol n'ait eu lieu.
//! - L'**adresse IP** n'est délibérément pas liée. Un téléphone change d'IP en
//!   passant du wifi aux données mobiles, plusieurs fois par trajet ; y lier la
//!   session revient à déconnecter les utilisateurs mobiles en permanence. La
//!   protection réelle contre le vol de refresh est la rotation ci-dessus, qui
//!   ne dépend d'aucune de ces heuristiques.
//! - Le **challenge itsme** que FR-004 prévoit en réponse à l'anomalie n'est
//!   pas fourni : il demande un contrat itsme, hors périmètre. L'anomalie est
//!   donc consignée sans remédiation automatique.

use chrono::{DateTime, Utc};
use klaar_identity::{EmpreinteJeton, JetonVerification};
use std::fmt;
use uuid::Uuid;

use crate::ports::audit::{CodeAudit, EntreeAudit, JournalAudit};
use crate::ports::erreurs::RepositoryError;
use crate::ports::horloge::Horloge;
use crate::ports::jeton_acces::{ClaimsAcces, EmetteurJetonAcces, VALIDITE_ACCES_SECONDES};
use crate::ports::session_repository::{
    ResultatRotation, SessionRepository, VALIDITE_REFRESH_JOURS,
};
use crate::usecases::connecter::Session;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurRafraichissement {
    /// Aucun refresh présenté.
    Absent,
    /// Refresh inconnu.
    Invalide,
    Expire,
    Revoque,
    /// Refresh déjà consommé : la famille vient d'être coupée.
    Rejeu,
    Indisponible(String),
}

impl ErreurRafraichissement {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Absent => "REFRESH_MISSING",
            Self::Invalide => "REFRESH_INVALID",
            Self::Expire => "REFRESH_EXPIRED",
            Self::Revoque => "REFRESH_REVOKED",
            Self::Rejeu => "REFRESH_REUSED",
            Self::Indisponible(_) => "SERVICE_UNAVAILABLE",
        }
    }
}

impl fmt::Display for ErreurRafraichissement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => write!(f, "aucun refresh présenté"),
            Self::Invalide => write!(f, "refresh inconnu"),
            Self::Expire => write!(f, "refresh expiré"),
            Self::Revoque => write!(f, "refresh révoqué"),
            Self::Rejeu => write!(f, "refresh rejoué : famille coupée"),
            Self::Indisponible(d) => write!(f, "service indisponible : {d}"),
        }
    }
}

impl std::error::Error for ErreurRafraichissement {}

impl From<RepositoryError> for ErreurRafraichissement {
    fn from(e: RepositoryError) -> Self {
        Self::Indisponible(e.to_string())
    }
}

/// Rafraîchit une session.
///
/// `nouveau_refresh` est tiré par l'appelant, comme à la connexion : le cas
/// d'usage n'a pas à connaître le générateur.
#[allow(clippy::too_many_arguments)]
pub async fn rafraichir<S, J, H, E>(
    sessions: &S,
    journal: &J,
    horloge: &H,
    emetteur: &E,
    refresh_presente: &str,
    nouveau_refresh: &str,
    contexte: Option<&str>,
) -> Result<Session, ErreurRafraichissement>
where
    S: SessionRepository,
    J: JournalAudit,
    H: Horloge,
    E: EmetteurJetonAcces + ?Sized,
{
    if refresh_presente.trim().is_empty() {
        return Err(ErreurRafraichissement::Absent);
    }

    let presentee = empreinte(refresh_presente);
    let nouvelle = empreinte(nouveau_refresh);
    let empreinte_contexte = contexte.map(empreinte);
    let maintenant = horloge.maintenant();
    let expire_le = maintenant + chrono::Duration::days(VALIDITE_REFRESH_JOURS);

    let resultat = sessions
        .rotationner(
            &presentee,
            &nouvelle,
            empreinte_contexte.as_ref(),
            expire_le,
            maintenant,
        )
        .await?;

    match resultat {
        ResultatRotation::Rotationne {
            utilisateur_id,
            contexte_change,
            ..
        } => {
            if contexte_change {
                consigner(
                    journal,
                    CodeAudit::SessionContextChanged,
                    Some(utilisateur_id),
                    maintenant,
                )
                .await?;
            }
            let jeton_acces = emetteur
                .emettre(&ClaimsAcces {
                    utilisateur_id,
                    emis_le: maintenant,
                    expire_le: maintenant + chrono::Duration::seconds(VALIDITE_ACCES_SECONDES),
                })
                .map_err(|e| ErreurRafraichissement::Indisponible(e.to_string()))?;

            consigner(
                journal,
                CodeAudit::SessionRefreshed,
                Some(utilisateur_id),
                maintenant,
            )
            .await?;

            Ok(Session {
                utilisateur_id,
                jeton_acces,
                expire_dans_secondes: VALIDITE_ACCES_SECONDES,
                refresh: nouveau_refresh.to_string(),
                refresh_expire_dans_secondes: VALIDITE_REFRESH_JOURS * 86_400,
            })
        }
        ResultatRotation::Rejeu {
            famille_id,
            utilisateur_id,
        } => {
            // Couper avant de répondre, et non après : une réponse d'erreur
            // laissant la famille vivante donnerait au voleur le temps de
            // réessayer avec le refresh courant.
            sessions.revoquer_famille(famille_id, maintenant).await?;
            consigner(
                journal,
                CodeAudit::SessionReuseDetected,
                Some(utilisateur_id),
                maintenant,
            )
            .await?;
            tracing::warn!(
                code = "ANOMALY_REFRESH",
                "refresh rejoué : famille de sessions coupée"
            );
            Err(ErreurRafraichissement::Rejeu)
        }
        ResultatRotation::Expire => Err(ErreurRafraichissement::Expire),
        ResultatRotation::Revoque => Err(ErreurRafraichissement::Revoque),
        ResultatRotation::Inconnu => Err(ErreurRafraichissement::Invalide),
    }
}

/// Coupe la session courante et toute sa famille.
///
/// Ne dit jamais si le refresh présenté existait : une déconnexion qui échoue
/// n'a rien à apprendre à personne, et le client doit de toute façon jeter son
/// cookie.
pub async fn deconnecter<S, J, H>(
    sessions: &S,
    journal: &J,
    horloge: &H,
    refresh_presente: &str,
) -> Result<(), ErreurRafraichissement>
where
    S: SessionRepository,
    J: JournalAudit,
    H: Horloge,
{
    if refresh_presente.trim().is_empty() {
        return Ok(());
    }
    let maintenant = horloge.maintenant();
    let presentee = empreinte(refresh_presente);

    if let Some(famille_id) = sessions.famille_de(&presentee).await? {
        // Toute la famille, et pas seulement le refresh courant : une
        // déconnexion qui laisserait vivants les maillons précédents ne
        // déconnecterait rien.
        let coupees = sessions.revoquer_famille(famille_id, maintenant).await?;
        if coupees > 0 {
            consigner(journal, CodeAudit::UserLogout, None, maintenant).await?;
        }
    }
    Ok(())
}

fn empreinte(valeur: &str) -> EmpreinteJeton {
    JetonVerification::depuis_chaine(valeur).empreinte()
}

async fn consigner<J: JournalAudit>(
    journal: &J,
    code: CodeAudit,
    sujet_id: Option<Uuid>,
    horodatage: DateTime<Utc>,
) -> Result<(), RepositoryError> {
    journal
        .consigner(EntreeAudit {
            code,
            sujet_id,
            horodatage,
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::horloge::HorlogeFigee;
    use crate::ports::jeton_acces::ErreurJeton;
    use crate::ports::session_repository::SessionAConserver;
    use chrono::TimeZone;
    use std::cell::RefCell;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    #[derive(Clone)]
    struct Ligne {
        empreinte: EmpreinteJeton,
        utilisateur_id: Uuid,
        famille_id: Uuid,
        expire_le: DateTime<Utc>,
        consomme: bool,
        revoque: bool,
        contexte: Option<EmpreinteJeton>,
    }

    #[derive(Default)]
    struct SessionsMemoire {
        lignes: RefCell<Vec<Ligne>>,
        en_panne: bool,
    }

    impl SessionsMemoire {
        fn avec(refresh: &str, contexte: Option<&str>) -> (Self, Uuid, Uuid) {
            let utilisateur_id = Uuid::new_v4();
            let famille_id = Uuid::new_v4();
            let depot = Self::default();
            depot.lignes.borrow_mut().push(Ligne {
                empreinte: empreinte(refresh),
                utilisateur_id,
                famille_id,
                expire_le: instant() + chrono::Duration::days(30),
                consomme: false,
                revoque: false,
                contexte: contexte.map(empreinte),
            });
            (depot, utilisateur_id, famille_id)
        }

        fn vivantes(&self, famille_id: Uuid) -> usize {
            self.lignes
                .borrow()
                .iter()
                .filter(|l| l.famille_id == famille_id && !l.revoque)
                .count()
        }
    }

    impl SessionRepository for SessionsMemoire {
        async fn ouvrir(&self, session: &SessionAConserver) -> Result<(), RepositoryError> {
            self.lignes.borrow_mut().push(Ligne {
                empreinte: session.empreinte.clone(),
                utilisateur_id: session.utilisateur_id,
                famille_id: session.famille_id,
                expire_le: session.expire_le,
                consomme: false,
                revoque: false,
                contexte: session.empreinte_contexte.clone(),
            });
            Ok(())
        }

        async fn rotationner(
            &self,
            presentee: &EmpreinteJeton,
            nouvelle: &EmpreinteJeton,
            contexte: Option<&EmpreinteJeton>,
            expire_le: DateTime<Utc>,
            maintenant: DateTime<Utc>,
        ) -> Result<ResultatRotation, RepositoryError> {
            if self.en_panne {
                return Err(RepositoryError::Indisponible("test".into()));
            }
            let mut lignes = self.lignes.borrow_mut();
            let Some(index) = lignes.iter().position(|l| &l.empreinte == presentee) else {
                return Ok(ResultatRotation::Inconnu);
            };
            let ligne = lignes[index].clone();
            if ligne.consomme {
                return Ok(ResultatRotation::Rejeu {
                    famille_id: ligne.famille_id,
                    utilisateur_id: ligne.utilisateur_id,
                });
            }
            if ligne.revoque {
                return Ok(ResultatRotation::Revoque);
            }
            if ligne.expire_le <= maintenant {
                return Ok(ResultatRotation::Expire);
            }
            lignes[index].consomme = true;
            let contexte_change = match (&ligne.contexte, contexte) {
                (Some(attendu), Some(recu)) => attendu != recu,
                _ => false,
            };
            lignes.push(Ligne {
                empreinte: nouvelle.clone(),
                utilisateur_id: ligne.utilisateur_id,
                famille_id: ligne.famille_id,
                expire_le,
                consomme: false,
                revoque: false,
                contexte: ligne.contexte.clone(),
            });
            Ok(ResultatRotation::Rotationne {
                utilisateur_id: ligne.utilisateur_id,
                famille_id: ligne.famille_id,
                contexte_change,
            })
        }

        async fn famille_de(
            &self,
            empreinte: &EmpreinteJeton,
        ) -> Result<Option<Uuid>, RepositoryError> {
            Ok(self
                .lignes
                .borrow()
                .iter()
                .find(|l| &l.empreinte == empreinte)
                .map(|l| l.famille_id))
        }

        async fn revoquer_famille(
            &self,
            famille_id: Uuid,
            _: DateTime<Utc>,
        ) -> Result<u64, RepositoryError> {
            let mut coupees = 0;
            for ligne in self.lignes.borrow_mut().iter_mut() {
                if ligne.famille_id == famille_id && !ligne.revoque {
                    ligne.revoque = true;
                    coupees += 1;
                }
            }
            Ok(coupees)
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

    struct EmetteurFactice;

    impl EmetteurJetonAcces for EmetteurFactice {
        fn emettre(&self, claims: &ClaimsAcces) -> Result<String, ErreurJeton> {
            Ok(format!("acces:{}", claims.utilisateur_id))
        }
        fn verifier(&self, _: &str) -> Result<ClaimsAcces, ErreurJeton> {
            unreachable!()
        }
    }

    async fn tourner(
        sessions: &SessionsMemoire,
        journal: &JournalMemoire,
        presente: &str,
        nouveau: &str,
        contexte: Option<&str>,
    ) -> Result<Session, ErreurRafraichissement> {
        rafraichir(
            sessions,
            journal,
            &HorlogeFigee(instant()),
            &EmetteurFactice,
            presente,
            nouveau,
            contexte,
        )
        .await
    }

    #[tokio::test]
    async fn happy_la_rotation_rend_un_acces_et_un_refresh_neufs() {
        let (sessions, id, _) = SessionsMemoire::avec("R1", Some("Firefox/120"));
        let journal = JournalMemoire::default();

        let session = tourner(&sessions, &journal, "R1", "R2", Some("Firefox/120"))
            .await
            .unwrap();

        assert_eq!(session.utilisateur_id, id);
        assert_eq!(session.refresh, "R2");
        assert_eq!(session.expire_dans_secondes, 3600);
        assert_eq!(journal.codes(), vec![CodeAudit::SessionRefreshed]);
    }

    #[tokio::test]
    async fn happy_le_nouveau_refresh_sert_a_la_rotation_suivante() {
        let (sessions, _, _) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();

        tourner(&sessions, &journal, "R1", "R2", None)
            .await
            .unwrap();
        let seconde = tourner(&sessions, &journal, "R2", "R3", None)
            .await
            .unwrap();
        assert_eq!(seconde.refresh, "R3");
    }

    #[tokio::test]
    async fn negative_un_refresh_inconnu_est_refuse() {
        let (sessions, _, _) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();
        let e = tourner(&sessions, &journal, "jamais-vu", "R2", None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "REFRESH_INVALID");
    }

    #[tokio::test]
    async fn negative_un_refresh_absent_est_refuse_sans_toucher_au_depot() {
        let sessions = SessionsMemoire {
            en_panne: true,
            ..Default::default()
        };
        let journal = JournalMemoire::default();
        for vide in ["", "   "] {
            let e = tourner(&sessions, &journal, vide, "R2", None)
                .await
                .unwrap_err();
            assert_eq!(e.code(), "REFRESH_MISSING");
        }
    }

    #[tokio::test]
    async fn negative_un_refresh_expire_est_refuse() {
        let (sessions, _, _) = SessionsMemoire::avec("R1", None);
        sessions.lignes.borrow_mut()[0].expire_le = instant() - chrono::Duration::days(1);
        let journal = JournalMemoire::default();
        let e = tourner(&sessions, &journal, "R1", "R2", None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "REFRESH_EXPIRED");
    }

    #[tokio::test]
    async fn negative_un_refresh_revoque_est_refuse() {
        let (sessions, _, famille) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();
        sessions.revoquer_famille(famille, instant()).await.unwrap();

        let e = tourner(&sessions, &journal, "R1", "R2", None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "REFRESH_REVOKED");
    }

    #[tokio::test]
    async fn edge_un_changement_de_contexte_est_signale_sans_couper() {
        // Les navigateurs modifient leur agent utilisateur à chaque mise à
        // jour : bloquer là-dessus déconnecterait tout le monde toutes les
        // quelques semaines, sans qu'aucun vol n'ait eu lieu.
        let (sessions, id, famille) = SessionsMemoire::avec("R1", Some("Firefox/120"));
        let journal = JournalMemoire::default();

        let session = tourner(&sessions, &journal, "R1", "R2", Some("curl/8"))
            .await
            .expect("la session doit survivre à un changement de contexte");

        assert_eq!(session.refresh, "R2");
        assert_eq!(
            journal.codes(),
            vec![
                CodeAudit::SessionContextChanged,
                CodeAudit::SessionRefreshed
            ]
        );
        assert_eq!(journal.entrees.borrow()[0].sujet_id, Some(id));
        assert!(sessions.vivantes(famille) > 0);
    }

    #[tokio::test]
    async fn edge_une_session_sans_contexte_connu_n_est_pas_une_anomalie() {
        let (sessions, _, _) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();
        tourner(&sessions, &journal, "R1", "R2", Some("Firefox/120"))
            .await
            .unwrap();
        assert_eq!(journal.codes(), vec![CodeAudit::SessionRefreshed]);
    }

    #[tokio::test]
    async fn edge_deconnecter_sans_refresh_ne_fait_rien_et_ne_se_plaint_pas() {
        let sessions = SessionsMemoire::default();
        let journal = JournalMemoire::default();
        assert!(
            deconnecter(&sessions, &journal, &HorlogeFigee(instant()), "")
                .await
                .is_ok()
        );
        assert!(journal.entrees.borrow().is_empty());
    }

    #[tokio::test]
    async fn edge_deconnecter_deux_fois_ne_consigne_qu_une_fois() {
        let (sessions, _, _) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();
        let horloge = HorlogeFigee(instant());

        deconnecter(&sessions, &journal, &horloge, "R1")
            .await
            .unwrap();
        deconnecter(&sessions, &journal, &horloge, "R1")
            .await
            .unwrap();
        assert_eq!(journal.codes(), vec![CodeAudit::UserLogout]);
    }

    #[tokio::test]
    async fn security_rejouer_un_refresh_consomme_coupe_toute_la_famille() {
        // Le coeur de la story. Le porteur légitime a reçu R2 ; présenter R1
        // signifie qu'une copie circule. On ne sait pas laquelle des deux mains
        // est la bonne, donc les deux sont coupées.
        let (sessions, id, famille) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();

        tourner(&sessions, &journal, "R1", "R2", None)
            .await
            .unwrap();
        assert!(sessions.vivantes(famille) > 0);

        let e = tourner(&sessions, &journal, "R1", "R3", None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "REFRESH_REUSED");
        assert_eq!(sessions.vivantes(famille), 0, "la famille doit être coupée");

        let entrees = journal.entrees.borrow();
        let derniere = entrees.last().unwrap();
        assert_eq!(derniere.code, CodeAudit::SessionReuseDetected);
        assert_eq!(derniere.sujet_id, Some(id));
    }

    #[tokio::test]
    async fn security_apres_un_rejeu_le_refresh_courant_ne_marche_plus() {
        // Sans cela, couper « la famille » ne servirait à rien : le voleur
        // garderait le jeton en cours de validité.
        let (sessions, _, _) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();

        tourner(&sessions, &journal, "R1", "R2", None)
            .await
            .unwrap();
        let _ = tourner(&sessions, &journal, "R1", "R3", None).await;

        let e = tourner(&sessions, &journal, "R2", "R4", None)
            .await
            .unwrap_err();
        assert_eq!(e.code(), "REFRESH_REVOKED");
    }

    #[tokio::test]
    async fn security_un_refresh_consomme_ne_redevient_jamais_valable() {
        let (sessions, _, _) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();
        tourner(&sessions, &journal, "R1", "R2", None)
            .await
            .unwrap();
        for _ in 0..3 {
            assert!(tourner(&sessions, &journal, "R1", "RX", None)
                .await
                .is_err());
        }
    }

    #[tokio::test]
    async fn security_la_deconnexion_coupe_toute_la_famille_pas_le_seul_jeton() {
        let (sessions, _, famille) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();
        let horloge = HorlogeFigee(instant());

        tourner(&sessions, &journal, "R1", "R2", None)
            .await
            .unwrap();
        deconnecter(&sessions, &journal, &horloge, "R2")
            .await
            .unwrap();

        assert_eq!(sessions.vivantes(famille), 0);
        // Y compris le maillon consommé : le laisser vivant rendrait un rejeu
        // ultérieur indétectable puisque la famille serait déjà partiellement
        // révoquée.
        assert!(tourner(&sessions, &journal, "R1", "R3", None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn security_l_erreur_ne_repete_jamais_le_refresh_presente() {
        let (sessions, _, _) = SessionsMemoire::avec("R1", None);
        let journal = JournalMemoire::default();
        let e = tourner(
            &sessions,
            &journal,
            "REFRESH-TRES-RECONNAISSABLE",
            "R2",
            None,
        )
        .await
        .unwrap_err();
        assert!(!e.to_string().contains("RECONNAISSABLE"));
        assert!(!format!("{e:?}").contains("RECONNAISSABLE"));
    }
}
