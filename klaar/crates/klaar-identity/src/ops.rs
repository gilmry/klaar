//! Comptes d'exploitation et leurs droits (FR-041, Story 8.4).
//!
//! **Un compte d'exploitation n'est pas un compte d'utilisateur.** Il n'a ni
//! Demande, ni Mission, ni notation ; il regarde celles des autres. Les mêler
//! dans une seule table aurait donné à chaque requête de matching une colonne
//! « rôle » à ignorer, et à chaque revue de sécurité une question de plus.
//!
//! **Les droits sont attachés au rôle, pas au compte.** Quatre rôles, une
//! matrice explicite : c'est ce qui permet de répondre à « qui peut voir quoi »
//! en lisant vingt lignes, plutôt qu'en parcourant une table de permissions
//! dont personne ne connaît le contenu réel.
//!
//! **Le moindre privilège est le défaut.** Un rôle inconnu ne donne rien, et
//! `permet` est un `match` exhaustif : ajouter une permission sans dire qui y a
//! droit ne compile pas.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::EmpreinteMotDePasse;

/// Jours d'inactivité au-delà desquels un compte est désactivé (FR-041 `@edge`).
///
/// Quatre-vingt-dix. Un compte d'exploitation oublié est un compte dont
/// personne ne surveille l'usage, et c'est exactement celui qu'on retrouve dans
/// les rapports d'incident.
pub const INACTIVITE_MAX_JOURS: i64 = 90;

/// Ce qu'un compte d'exploitation peut faire.
///
/// **Une permission par geste réel**, et non par écran : un écran change, un
/// geste engage. « Trancher un litige » et « lever une libération » sont deux
/// décisions distinctes qui touchent à l'argent de gens différents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    /// Consulter le journal d'audit (FR-042).
    LireAudit,
    /// En exporter une période signée.
    ExporterAudit,
    /// Valider ou refuser un contrôle d'entreprise (FR-038).
    ReviserKyc,
    /// Trancher un litige (FR-036).
    TrancherLitige,
    /// Autoriser une libération au-delà du seuil des quatre yeux (FR-021).
    LeverLiberation,
    /// Créer, désactiver et réactiver des comptes d'exploitation.
    GererOps,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LireAudit => "AUDIT_READ",
            Self::ExporterAudit => "AUDIT_EXPORT",
            Self::ReviserKyc => "KYC_REVIEW",
            Self::TrancherLitige => "DISPUTE_RESOLVE",
            Self::LeverLiberation => "RELEASE_APPROVE",
            Self::GererOps => "OPS_MANAGE",
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Les rôles d'exploitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleOps {
    /// Tout, y compris la gestion des comptes d'exploitation.
    SuperAdmin,
    /// Contrôle des entreprises, et rien d'autre.
    ReviseurKyc,
    /// Litiges et libérations : les décisions qui touchent à l'argent.
    Mediateur,
    /// Lecture du journal, sans aucun pouvoir de décision.
    ///
    /// **Ce rôle existe pour que la curiosité légitime n'oblige pas à donner
    /// autre chose.** Un juriste qui prépare une réponse à l'APD a besoin de
    /// lire, pas de trancher.
    Lecteur,
}

impl RoleOps {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SuperAdmin => "SUPER_ADMIN",
            Self::ReviseurKyc => "KYC_REVIEWER",
            Self::Mediateur => "MEDIATOR",
            Self::Lecteur => "READER",
        }
    }

    /// Rend `None` pour un rôle inconnu (FR-041 `@negative`).
    ///
    /// Aucun repli : un rôle mal orthographié doit faire échouer la création,
    /// pas donner silencieusement le rôle le plus faible — ni, pire, le plus
    /// fort.
    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "SUPER_ADMIN" => Some(Self::SuperAdmin),
            "KYC_REVIEWER" => Some(Self::ReviseurKyc),
            "MEDIATOR" => Some(Self::Mediateur),
            "READER" => Some(Self::Lecteur),
            _ => None,
        }
    }

    /// La matrice des droits, en clair.
    ///
    /// Un `match` exhaustif dans les deux dimensions : ajouter un rôle ou une
    /// permission sans dire ce qu'il en est ne compile pas. C'est ce qui évite
    /// qu'une permission nouvelle soit accordée à tous par défaut, ou à
    /// personne sans que quiconque s'en aperçoive.
    pub fn permet(&self, permission: Permission) -> bool {
        match (self, permission) {
            // Le super-administrateur peut tout. C'est la définition du rôle, et
            // c'est aussi pourquoi il doit être rare.
            (Self::SuperAdmin, _) => true,

            (Self::ReviseurKyc, Permission::ReviserKyc) => true,
            (Self::ReviseurKyc, Permission::LireAudit) => true,
            (
                Self::ReviseurKyc,
                Permission::ExporterAudit
                | Permission::TrancherLitige
                | Permission::LeverLiberation
                | Permission::GererOps,
            ) => false,

            (Self::Mediateur, Permission::TrancherLitige | Permission::LeverLiberation) => true,
            (Self::Mediateur, Permission::LireAudit) => true,
            (
                Self::Mediateur,
                Permission::ExporterAudit | Permission::ReviserKyc | Permission::GererOps,
            ) => false,

            // Le lecteur lit, et exporte ce qu'il lit : un export est une
            // lecture mise en forme, pas un pouvoir de plus.
            (Self::Lecteur, Permission::LireAudit | Permission::ExporterAudit) => true,
            (
                Self::Lecteur,
                Permission::ReviserKyc
                | Permission::TrancherLitige
                | Permission::LeverLiberation
                | Permission::GererOps,
            ) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpsError {
    /// Rôle hors de la liste (FR-041 `@negative`).
    RoleInconnu,
    /// Le compte est désactivé, par révocation ou par inactivité.
    Desactive,
    /// L'authentification à deux facteurs n'est pas encore configurée
    /// (FR-041 `@security`).
    MfaAbsente,
}

impl OpsError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::RoleInconnu => "ROLE_UNKNOWN",
            Self::Desactive => "OPS_DISABLED",
            Self::MfaAbsente => "MFA_REQUIRED",
        }
    }
}

impl fmt::Display for OpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoleInconnu => write!(f, "rôle inconnu"),
            Self::Desactive => write!(f, "ce compte d'exploitation est désactivé"),
            Self::MfaAbsente => {
                write!(f, "l'authentification à deux facteurs doit être configurée")
            }
        }
    }
}

impl std::error::Error for OpsError {}

/// Un compte d'exploitation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompteOps {
    pub id: Uuid,
    /// Adresse professionnelle. Sert d'identifiant de connexion.
    pub email: klaar_shared_kernel::Email,
    pub empreinte_mot_de_passe: EmpreinteMotDePasse,
    pub role: RoleOps,
    /// Secret TOTP, une fois la seconde authentification configurée.
    ///
    /// `None` à la création : FR-041 `@security` exige que le premier accès
    /// serve à la configurer, et rien d'autre.
    pub secret_totp: Option<Vec<u8>>,
    pub actif: bool,
    /// Dernière fois que ce compte a fait quelque chose. Sert à la révocation
    /// par inactivité.
    pub derniere_activite: DateTime<Utc>,
    pub cree_le: DateTime<Utc>,
}

impl CompteOps {
    /// Crée un compte, sans seconde authentification.
    pub fn creer(
        email: klaar_shared_kernel::Email,
        empreinte_mot_de_passe: EmpreinteMotDePasse,
        role: &str,
        maintenant: DateTime<Utc>,
    ) -> Result<Self, OpsError> {
        Ok(Self {
            id: Uuid::new_v4(),
            email,
            empreinte_mot_de_passe,
            role: RoleOps::parse(role).ok_or(OpsError::RoleInconnu)?,
            secret_totp: None,
            actif: true,
            derniere_activite: maintenant,
            cree_le: maintenant,
        })
    }

    /// Vrai si ce compte peut agir maintenant.
    ///
    /// **Trois conditions, dans cet ordre** : être actif, avoir configuré sa
    /// seconde authentification, et ne pas être tombé en désuétude. L'ordre
    /// n'est pas cosmétique — il détermine le message d'erreur, et « votre
    /// compte est désactivé » est plus utile que « configurez votre MFA » à
    /// quelqu'un dont le compte a été révoqué.
    pub fn peut_agir(&self, maintenant: DateTime<Utc>) -> Result<(), OpsError> {
        if !self.actif {
            return Err(OpsError::Desactive);
        }
        if self.secret_totp.is_none() {
            return Err(OpsError::MfaAbsente);
        }
        if self.inactif_depuis_trop_longtemps(maintenant) {
            return Err(OpsError::Desactive);
        }
        Ok(())
    }

    /// Vrai si l'inactivité dépasse le seuil (FR-041 `@edge`).
    pub fn inactif_depuis_trop_longtemps(&self, maintenant: DateTime<Utc>) -> bool {
        maintenant >= self.derniere_activite + Duration::days(INACTIVITE_MAX_JOURS)
    }

    /// Vrai si ce compte a le droit de faire ce geste.
    ///
    /// **La permission ne suffit pas** : un compte révoqué qui aurait le bon
    /// rôle ne doit rien pouvoir faire. Les deux contrôles sont ici, ensemble,
    /// pour qu'aucun appelant ne puisse en oublier un.
    pub fn autorise(&self, permission: Permission, maintenant: DateTime<Utc>) -> bool {
        self.peut_agir(maintenant).is_ok() && self.role.permet(permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MotDePasse, ParametresArgon2};
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn compte(role: &str) -> CompteOps {
        let empreinte = EmpreinteMotDePasse::calculer(
            &MotDePasse::parse("Ops@2026Securise").unwrap(),
            ParametresArgon2::tests(),
        )
        .unwrap();
        let mut c = CompteOps::creer(
            klaar_shared_kernel::Email::parse("ops@klaar.test").unwrap(),
            empreinte,
            role,
            t0(),
        )
        .expect("rôle connu");
        c.secret_totp = Some(vec![7; 32]);
        c
    }

    const TOUTES: [Permission; 6] = [
        Permission::LireAudit,
        Permission::ExporterAudit,
        Permission::ReviserKyc,
        Permission::TrancherLitige,
        Permission::LeverLiberation,
        Permission::GererOps,
    ];

    // === @happy ===

    #[test]
    fn happy_le_super_admin_peut_tout() {
        let c = compte("SUPER_ADMIN");
        for p in TOUTES {
            assert!(c.autorise(p, t0()), "{p}");
        }
    }

    #[test]
    fn happy_le_reviseur_kyc_ne_fait_que_du_kyc() {
        // FR-041 `@happy` : « le nouvel ops peut uniquement valider KYC ».
        let c = compte("KYC_REVIEWER");
        assert!(c.autorise(Permission::ReviserKyc, t0()));
        for p in [
            Permission::TrancherLitige,
            Permission::LeverLiberation,
            Permission::GererOps,
            Permission::ExporterAudit,
        ] {
            assert!(!c.autorise(p, t0()), "{p} ne doit pas être permis");
        }
    }

    #[test]
    fn happy_le_mediateur_tranche_et_libere() {
        let c = compte("MEDIATOR");
        assert!(c.autorise(Permission::TrancherLitige, t0()));
        assert!(c.autorise(Permission::LeverLiberation, t0()));
        assert!(!c.autorise(Permission::GererOps, t0()));
    }

    // === @negative ===

    #[test]
    fn negative_un_role_inconnu_est_refuse() {
        // FR-041 `@negative` : ni repli silencieux vers le rôle le plus faible,
        // ni — pire — vers le plus fort.
        let empreinte = EmpreinteMotDePasse::calculer(
            &MotDePasse::parse("Ops@2026Securise").unwrap(),
            ParametresArgon2::tests(),
        )
        .unwrap();
        for role in ["super_root", "ADMIN", "", "super_admin"] {
            assert_eq!(
                CompteOps::creer(
                    klaar_shared_kernel::Email::parse("ops@klaar.test").unwrap(),
                    empreinte.clone(),
                    role,
                    t0()
                )
                .map(|_| ()),
                Err(OpsError::RoleInconnu),
                "{role}"
            );
        }
    }

    #[test]
    fn negative_un_compte_desactive_ne_peut_rien() {
        let mut c = compte("SUPER_ADMIN");
        c.actif = false;
        assert_eq!(c.peut_agir(t0()), Err(OpsError::Desactive));
        for p in TOUTES {
            assert!(!c.autorise(p, t0()), "{p}");
        }
    }

    // === @edge ===

    #[test]
    fn edge_un_compte_inactif_depuis_quatre_vingt_dix_jours_est_hors_jeu() {
        // FR-041 `@edge`. Un compte d'exploitation oublié est un compte dont
        // personne ne surveille l'usage.
        let c = compte("SUPER_ADMIN");
        let veille = t0() + Duration::days(INACTIVITE_MAX_JOURS) - Duration::seconds(1);
        assert!(c.peut_agir(veille).is_ok());

        let echu = t0() + Duration::days(INACTIVITE_MAX_JOURS);
        assert_eq!(c.peut_agir(echu), Err(OpsError::Desactive));
    }

    #[test]
    fn edge_un_compte_neuf_doit_d_abord_configurer_sa_seconde_authentification() {
        // FR-041 `@security` : sans MFA, accès bloqué.
        let empreinte = EmpreinteMotDePasse::calculer(
            &MotDePasse::parse("Ops@2026Securise").unwrap(),
            ParametresArgon2::tests(),
        )
        .unwrap();
        let neuf = CompteOps::creer(
            klaar_shared_kernel::Email::parse("ops@klaar.test").unwrap(),
            empreinte,
            "SUPER_ADMIN",
            t0(),
        )
        .unwrap();
        assert_eq!(neuf.peut_agir(t0()), Err(OpsError::MfaAbsente));
        assert!(!neuf.autorise(Permission::LireAudit, t0()));
    }

    #[test]
    fn edge_le_message_de_refus_dit_la_cause_la_plus_utile() {
        // Un compte révoqué **et** sans MFA doit s'entendre dire qu'il est
        // révoqué : lui demander de configurer sa MFA l'enverrait perdre son
        // temps.
        let empreinte = EmpreinteMotDePasse::calculer(
            &MotDePasse::parse("Ops@2026Securise").unwrap(),
            ParametresArgon2::tests(),
        )
        .unwrap();
        let mut c = CompteOps::creer(
            klaar_shared_kernel::Email::parse("ops@klaar.test").unwrap(),
            empreinte,
            "READER",
            t0(),
        )
        .unwrap();
        c.actif = false;
        assert_eq!(c.peut_agir(t0()), Err(OpsError::Desactive));
    }

    // === @security ===

    #[test]
    fn security_le_lecteur_ne_decide_de_rien() {
        // Ce rôle existe pour que la curiosité légitime n'oblige pas à donner
        // autre chose : un juriste qui prépare une réponse à l'APD a besoin de
        // lire, pas de trancher.
        let c = compte("READER");
        assert!(c.autorise(Permission::LireAudit, t0()));
        assert!(c.autorise(Permission::ExporterAudit, t0()));
        for p in [
            Permission::ReviserKyc,
            Permission::TrancherLitige,
            Permission::LeverLiberation,
            Permission::GererOps,
        ] {
            assert!(!c.autorise(p, t0()), "{p} ne doit pas être permis");
        }
    }

    #[test]
    fn security_un_seul_role_gere_les_comptes_d_exploitation() {
        // Donner ce droit à deux rôles doublerait la surface d'escalade : qui
        // peut créer un compte peut se créer un super-administrateur.
        let porteurs: Vec<&str> = [
            RoleOps::SuperAdmin,
            RoleOps::ReviseurKyc,
            RoleOps::Mediateur,
            RoleOps::Lecteur,
        ]
        .iter()
        .filter(|r| r.permet(Permission::GererOps))
        .map(|r| r.as_str())
        .collect();
        assert_eq!(porteurs, ["SUPER_ADMIN"]);
    }

    #[test]
    fn security_la_permission_seule_ne_suffit_pas() {
        // Un compte révoqué qui aurait le bon rôle ne doit rien pouvoir faire.
        // Les deux contrôles vivent ensemble pour qu'aucun appelant n'en oublie
        // un.
        let mut c = compte("MEDIATOR");
        assert!(c.role.permet(Permission::TrancherLitige));
        c.actif = false;
        assert!(!c.autorise(Permission::TrancherLitige, t0()));
    }

    #[test]
    fn security_le_vocabulaire_des_roles_est_stable() {
        for role in [
            RoleOps::SuperAdmin,
            RoleOps::ReviseurKyc,
            RoleOps::Mediateur,
            RoleOps::Lecteur,
        ] {
            assert_eq!(RoleOps::parse(role.as_str()), Some(role));
        }
    }

    #[test]
    fn security_aucun_role_ne_recoit_une_permission_par_defaut() {
        // La matrice est un `match` exhaustif : ce test fixe la conséquence,
        // pour qu'une permission ajoutée sans décision explicite se voie.
        let sans_kyc = [RoleOps::Mediateur, RoleOps::Lecteur];
        for role in sans_kyc {
            assert!(!role.permet(Permission::ReviserKyc), "{}", role.as_str());
        }
    }
}
