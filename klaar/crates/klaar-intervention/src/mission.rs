//! Agrégat `Mission` et sa machine à états (FR-013, FR-018).
//!
//! **La machine à états est une fonction totale, pas une suite de `if`.**
//! `transitions_possibles` énumère ce qui est permis depuis chaque statut, et
//! `transiter` s'y réfère. Ajouter un statut sans dire ce qu'on peut en faire
//! ne compile pas : c'est le `match` exhaustif qui l'impose, et c'est ce qui
//! évite qu'un état apparaisse un jour sans que personne ne se demande d'où on
//! y entre ni comment on en sort.
//!
//! **Une Mission terminée ou annulée ne bouge plus.** Autoriser un retour en
//! arrière permettrait de rouvrir une intervention validée, donc de rejouer ce
//! qui en dépend — paiement, notation, litige.

use chrono::{DateTime, Duration, Utc};
use klaar_shared_kernel::{dans_le_perimetre, Geo};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Écart maximal toléré entre l'horodatage annoncé par le client et l'heure du
/// serveur (FR-018 `@edge`).
///
/// Cinq minutes. Le client peut légitimement dater un changement d'état
/// survenu hors connexion, et refuser sa date obligerait à réécrire l'histoire
/// au moment de la synchronisation. Au-delà, ce n'est plus un décalage de
/// synchronisation mais une date choisie, et une intervention pourrait se
/// prétendre commencée une heure plus tôt.
pub const DERIVE_HORODATAGE_MAX_MINUTES: i64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatutMission {
    /// Attribuée au prestataire, pas encore commencée (FR-013).
    Acceptee,
    /// Le prestataire est en route.
    EnRoute,
    /// Le prestataire est sur place.
    SurPlace,
    /// L'intervention est terminée. La validation par le demandeur est une
    /// étape distincte (FR-021), pas encore livrée.
    Terminee,
    /// Annulée. Les pénalités relèvent de FR-022, pas encore livrées.
    Annulee,
}

impl StatutMission {
    /// Vocabulaire de FR-018, repris tel quel.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Acceptee => "ACCEPTED",
            Self::EnRoute => "PROVIDER_EN_ROUTE",
            Self::SurPlace => "ON_SITE",
            Self::Terminee => "COMPLETED",
            Self::Annulee => "CANCELLED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "ACCEPTED" => Some(Self::Acceptee),
            "PROVIDER_EN_ROUTE" => Some(Self::EnRoute),
            "ON_SITE" => Some(Self::SurPlace),
            "COMPLETED" => Some(Self::Terminee),
            "CANCELLED" => Some(Self::Annulee),
            _ => None,
        }
    }

    /// Statuts atteignables depuis celui-ci.
    ///
    /// Le `match` est exhaustif à dessein : ajouter un statut sans dire ce
    /// qu'on peut en faire ne compilera pas.
    pub fn transitions_possibles(&self) -> &'static [StatutMission] {
        match self {
            // L'annulation est permise par le domaine à chaque étape non
            // terminale ; la route qui l'expose, avec ses pénalités, relève de
            // FR-022 et n'existe pas encore.
            Self::Acceptee => &[Self::EnRoute, Self::Annulee],
            Self::EnRoute => &[Self::SurPlace, Self::Annulee],
            Self::SurPlace => &[Self::Terminee, Self::Annulee],
            // Terminaux. Un retour en arrière rouvrirait une intervention dont
            // dépendent le paiement, la notation et d'éventuels litiges.
            Self::Terminee => &[],
            Self::Annulee => &[],
        }
    }

    /// Vrai si la Mission occupe encore le prestataire.
    ///
    /// C'est cette notion, et non le statut lui-même, qu'interroge la règle
    /// « une Mission à la fois » (FR-013 `@edge`). Elle est ici pour que
    /// l'ajout d'un statut passe par cette fonction plutôt que par une liste
    /// recopiée dans une migration.
    pub fn occupe_le_prestataire(&self) -> bool {
        match self {
            Self::Acceptee | Self::EnRoute | Self::SurPlace => true,
            Self::Terminee | Self::Annulee => false,
        }
    }

    pub fn est_terminal(&self) -> bool {
        self.transitions_possibles().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionError {
    /// Transition non prévue par la machine à états (FR-018 `@negative`).
    TransitionInterdite {
        depuis: StatutMission,
        vers: StatutMission,
    },
    /// Horodatage client trop éloigné de l'heure du serveur (FR-018 `@edge`).
    HorodatageInvraisemblable { derive_secondes: i64 },
    /// Position hors de la Région pendant l'intervention (FR-018 `@edge`).
    HorsZone,
}

impl MissionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::TransitionInterdite { .. } => "INVALID_TRANSITION",
            Self::HorodatageInvraisemblable { .. } => "TIMESTAMP_IMPLAUSIBLE",
            Self::HorsZone => "OUT_OF_ZONE",
        }
    }
}

impl fmt::Display for MissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TransitionInterdite { depuis, vers } => write!(
                f,
                "transition {} → {} interdite",
                depuis.as_str(),
                vers.as_str()
            ),
            Self::HorodatageInvraisemblable { derive_secondes } => write!(
                f,
                "horodatage à {derive_secondes} s de l'heure du serveur, \
                 {DERIVE_HORODATAGE_MAX_MINUTES} min tolérées"
            ),
            Self::HorsZone => write!(f, "position hors de la Région de Bruxelles-Capitale"),
        }
    }
}

impl std::error::Error for MissionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mission {
    pub id: Uuid,
    /// La Demande dont elle est née. Une Demande donne au plus une Mission :
    /// c'est ce que garantit l'attribution atomique (FR-013).
    pub demande_id: Uuid,
    pub provider_id: Uuid,
    pub statut: StatutMission,
    pub cree_le: DateTime<Utc>,
}

/// Un changement d'état, tel qu'il sera consigné (FR-018 `@security`).
///
/// Immuable une fois écrit. La position et son caractère « hors zone » en font
/// partie : les reconstruire après coup depuis un flux de positions ne dirait
/// pas où le prestataire était **au moment** où il a déclaré être sur place.
// `PartialEq` sans `Eq` : cette structure porte une position, donc des `f64`,
// et `NaN` n'est égal à rien — pas même à lui-même.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionMission {
    pub mission_id: Uuid,
    pub provider_id: Uuid,
    pub statut: StatutMission,
    /// Quand le prestataire dit que c'est arrivé.
    ///
    /// Distinct de l'instant d'enregistrement : un changement d'état survenu
    /// hors connexion se synchronise plus tard, et écraser sa date réécrirait
    /// l'histoire.
    pub horodate_le: DateTime<Utc>,
    /// Quand le serveur l'a reçu.
    pub enregistre_le: DateTime<Utc>,
    /// Position déclarée, quand il y en a une.
    ///
    /// **Facultative.** L'exiger rendrait l'autorisation de géolocalisation de
    /// fait obligatoire, alors que quelqu'un sans GPS ou qui la refuse doit
    /// pouvoir dire qu'il est arrivé. Son absence est consignée comme telle.
    pub position: Option<Geo>,
    /// Vrai si la position déclarée sort de la Région (FR-018 `@edge`).
    pub hors_zone: bool,
}

impl Mission {
    /// Crée la Mission née d'une acceptation (FR-013).
    ///
    /// Aucun paramètre ne permet de la créer dans un autre état : une Mission
    /// naît acceptée, comme une Demande naît diffusée.
    pub fn attribuer(demande_id: Uuid, provider_id: Uuid, maintenant: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            demande_id,
            provider_id,
            statut: StatutMission::Acceptee,
            cree_le: maintenant,
        }
    }

    /// Vrai si ce prestataire est celui à qui la Mission est attribuée.
    pub fn appartient_a(&self, provider_id: Uuid) -> bool {
        self.provider_id == provider_id
    }

    /// Fait avancer la Mission, et produit l'entrée à consigner.
    ///
    /// L'ordre des contrôles est délibéré : la transition d'abord, puisqu'une
    /// transition interdite ne mérite ni horodatage ni position ; l'horodatage
    /// ensuite, parce qu'il détermine l'instant de l'événement ; la position en
    /// dernier, qui ne fait jamais échouer — sortir de la Région est un fait à
    /// signaler, pas un refus.
    pub fn transiter(
        &mut self,
        vers: StatutMission,
        horodate_le: Option<DateTime<Utc>>,
        position: Option<Geo>,
        maintenant: DateTime<Utc>,
    ) -> Result<TransitionMission, MissionError> {
        if !self.statut.transitions_possibles().contains(&vers) {
            return Err(MissionError::TransitionInterdite {
                depuis: self.statut,
                vers,
            });
        }

        let horodate_le = match horodate_le {
            None => maintenant,
            Some(annonce) => {
                let derive = (annonce - maintenant).num_seconds().abs();
                if derive > Duration::minutes(DERIVE_HORODATAGE_MAX_MINUTES).num_seconds() {
                    return Err(MissionError::HorodatageInvraisemblable {
                        derive_secondes: derive,
                    });
                }
                annonce
            }
        };

        // Hors zone se **consigne**, ne refuse pas. Un prestataire coincé sur
        // un ring qui sort de la Région trois cents mètres reste en
        // intervention ; c'est à l'exploitation d'y regarder, pas au domaine de
        // bloquer.
        let hors_zone = position.is_some_and(|p| !dans_le_perimetre(p));

        self.statut = vers;
        Ok(TransitionMission {
            mission_id: self.id,
            provider_id: self.provider_id,
            statut: vers,
            horodate_le,
            enregistre_le: maintenant,
            position,
            hors_zone,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn instant() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn bruxelles() -> Geo {
        Geo::new(50.8467, 4.3525).unwrap()
    }

    fn mission() -> Mission {
        Mission::attribuer(Uuid::new_v4(), Uuid::new_v4(), instant())
    }

    fn au_statut(statut: StatutMission) -> Mission {
        let mut m = mission();
        m.statut = statut;
        m
    }

    const TOUS: [StatutMission; 5] = [
        StatutMission::Acceptee,
        StatutMission::EnRoute,
        StatutMission::SurPlace,
        StatutMission::Terminee,
        StatutMission::Annulee,
    ];

    #[test]
    fn happy_une_mission_nait_acceptee() {
        let m = mission();
        assert_eq!(m.statut.as_str(), "ACCEPTED");
        assert_eq!(m.cree_le, instant());
    }

    #[test]
    fn happy_les_cinq_statuts_font_l_aller_retour() {
        for statut in TOUS {
            assert_eq!(StatutMission::parse(statut.as_str()), Some(statut));
        }
    }

    #[test]
    fn happy_le_parcours_nominal_va_jusqu_au_bout() {
        // FR-018 `@happy` : ACCEPTED → PROVIDER_EN_ROUTE → ON_SITE → COMPLETED.
        let mut m = mission();
        for vers in [
            StatutMission::EnRoute,
            StatutMission::SurPlace,
            StatutMission::Terminee,
        ] {
            let entree = m
                .transiter(vers, None, Some(bruxelles()), instant())
                .unwrap();
            assert_eq!(entree.statut, vers);
            assert_eq!(m.statut, vers);
            assert!(!entree.hors_zone);
        }
    }

    #[test]
    fn happy_l_entree_consignee_porte_ce_que_le_fr_demande() {
        // FR-018 `@security` : status, ts (UTC), geo, provider_id.
        let mut m = mission();
        let entree = m
            .transiter(StatutMission::EnRoute, None, Some(bruxelles()), instant())
            .unwrap();
        assert_eq!(entree.mission_id, m.id);
        assert_eq!(entree.provider_id, m.provider_id);
        assert_eq!(entree.statut, StatutMission::EnRoute);
        assert_eq!(entree.horodate_le, instant());
        assert_eq!(entree.enregistre_le, instant());
        assert_eq!(entree.position, Some(bruxelles()));
    }

    #[test]
    fn negative_les_transitions_interdites_du_fr_sont_refusees() {
        // FR-018 `@negative`, repris tel quel.
        for (depuis, vers) in [
            (StatutMission::Terminee, StatutMission::EnRoute),
            (StatutMission::SurPlace, StatutMission::Acceptee),
            (StatutMission::Annulee, StatutMission::SurPlace),
        ] {
            let e = au_statut(depuis)
                .transiter(vers, None, None, instant())
                .unwrap_err();
            assert_eq!(e.code(), "INVALID_TRANSITION", "{depuis:?} → {vers:?}");
        }
    }

    #[test]
    fn negative_une_transition_refusee_ne_change_pas_le_statut() {
        let mut m = au_statut(StatutMission::Terminee);
        let _ = m.transiter(StatutMission::EnRoute, None, None, instant());
        assert_eq!(m.statut, StatutMission::Terminee);
    }

    #[test]
    fn negative_un_horodatage_trop_ancien_est_refuse() {
        // Au-delà de la tolérance, ce n'est plus un décalage de synchronisation
        // mais une date choisie : une intervention pourrait se prétendre
        // commencée une heure plus tôt.
        let mut m = mission();
        let e = m
            .transiter(
                StatutMission::EnRoute,
                Some(instant() - Duration::hours(1)),
                None,
                instant(),
            )
            .unwrap_err();
        assert_eq!(e.code(), "TIMESTAMP_IMPLAUSIBLE");
        assert_eq!(m.statut, StatutMission::Acceptee);
    }

    #[test]
    fn negative_un_horodatage_dans_le_futur_est_refuse_aussi() {
        // La dérive se mesure en valeur absolue : dater dans le futur est le
        // même mensonge dans l'autre sens.
        let mut m = mission();
        assert!(m
            .transiter(
                StatutMission::EnRoute,
                Some(instant() + Duration::hours(1)),
                None,
                instant(),
            )
            .is_err());
    }

    #[test]
    fn edge_un_horodatage_dans_la_tolerance_est_conserve_tel_quel() {
        // C'est tout l'intérêt : un changement survenu hors connexion garde sa
        // date, et l'écraser réécrirait l'histoire.
        let mut m = mission();
        let annonce = instant() - Duration::minutes(4);
        let entree = m
            .transiter(StatutMission::EnRoute, Some(annonce), None, instant())
            .unwrap();
        assert_eq!(entree.horodate_le, annonce);
        assert_eq!(entree.enregistre_le, instant());
    }

    #[test]
    fn edge_sans_horodatage_client_le_serveur_fait_foi() {
        let mut m = mission();
        let entree = m
            .transiter(StatutMission::EnRoute, None, None, instant())
            .unwrap();
        assert_eq!(entree.horodate_le, entree.enregistre_le);
    }

    #[test]
    fn edge_une_position_hors_region_est_consignee_sans_bloquer() {
        // Un prestataire coincé sur un ring qui sort de la Région trois cents
        // mètres reste en intervention : c'est à l'exploitation d'y regarder.
        let anvers = Geo::new(51.2194, 4.4025).unwrap();
        let mut m = mission();
        let entree = m
            .transiter(StatutMission::EnRoute, None, Some(anvers), instant())
            .unwrap();
        assert!(entree.hors_zone);
        assert_eq!(m.statut, StatutMission::EnRoute);
    }

    #[test]
    fn edge_sans_position_rien_n_est_declare_hors_zone() {
        // Absence de position n'est pas présomption de sortie : quelqu'un sans
        // GPS ne doit pas déclencher d'alerte.
        let mut m = mission();
        let entree = m
            .transiter(StatutMission::EnRoute, None, None, instant())
            .unwrap();
        assert_eq!(entree.position, None);
        assert!(!entree.hors_zone);
    }

    #[test]
    fn edge_les_statuts_terminaux_n_ont_aucune_suite() {
        for statut in [StatutMission::Terminee, StatutMission::Annulee] {
            assert!(statut.est_terminal(), "{statut:?}");
            assert!(statut.transitions_possibles().is_empty());
        }
    }

    #[test]
    fn edge_seuls_les_statuts_non_terminaux_occupent_le_prestataire() {
        // C'est ce qui libère quelqu'un quand son intervention se termine.
        for statut in TOUS {
            assert_eq!(
                statut.occupe_le_prestataire(),
                !statut.est_terminal(),
                "{statut:?}"
            );
        }
    }

    #[test]
    fn security_aucun_chemin_ne_ramene_en_arriere() {
        // Rouvrir une intervention terminée permettrait de rejouer ce qui en
        // dépend : paiement, notation, litige.
        for depuis in TOUS {
            for vers in depuis.transitions_possibles() {
                assert!(
                    !vers.transitions_possibles().contains(&depuis),
                    "{depuis:?} → {vers:?} → {depuis:?} formerait un cycle"
                );
            }
        }
    }

    #[test]
    fn security_aucune_transition_ne_part_d_un_statut_terminal() {
        for statut in TOUS.iter().filter(|s| s.est_terminal()) {
            for vers in TOUS {
                assert!(
                    au_statut(*statut)
                        .transiter(vers, None, None, instant())
                        .is_err(),
                    "{statut:?} → {vers:?}"
                );
            }
        }
    }

    #[test]
    fn security_une_mission_ne_se_transite_pas_vers_elle_meme() {
        // Sans quoi une entrée de plus serait consignée à chaque clic, et
        // l'historique cesserait de raconter ce qui s'est passé.
        for statut in TOUS {
            assert!(
                !statut.transitions_possibles().contains(&statut),
                "{statut:?}"
            );
        }
    }

    #[test]
    fn security_l_appartenance_ne_se_devine_pas() {
        let m = mission();
        assert!(m.appartient_a(m.provider_id));
        assert!(!m.appartient_a(Uuid::new_v4()));
    }

    #[test]
    fn security_aucun_chemin_ne_cree_une_mission_deja_terminee() {
        // Ce test attrape l'ajout d'un paramètre `statut` à `attribuer`, qui
        // laisserait fabriquer une intervention réputée faite sans que personne
        // ne se soit déplacé.
        assert_eq!(mission().statut, StatutMission::Acceptee);
    }

    #[test]
    fn security_deux_missions_ne_partagent_pas_d_identifiant() {
        let demande = Uuid::new_v4();
        let a = Mission::attribuer(demande, Uuid::new_v4(), instant());
        let b = Mission::attribuer(demande, Uuid::new_v4(), instant());
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn negative_un_statut_inconnu_ne_se_relit_pas() {
        for inconnu in ["accepted", "ASSIGNED", "EN_ROUTE", "DONE", ""] {
            assert_eq!(StatutMission::parse(inconnu), None, "statut {inconnu}");
        }
    }
}
