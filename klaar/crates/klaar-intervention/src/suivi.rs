//! Suivi de position pendant l'intervention (FR-019, Story 4.4).
//!
//! **Le suivi hors intervention doit être impossible, pas interdit**
//! (invariant §10.5). Une position n'est acceptée que pour une Mission en
//! route, avec un consentement donné pour *cette* intervention : il n'existe
//! aucun chemin qui écrive une position sans ces deux conditions, et c'est ce
//! qui distingue une garantie d'une consigne.
//!
//! **La précision est volontairement dégradée à cinquante mètres.** Le RGPD
//! demande la minimisation (art. 5.1.c) : savoir que le plombier est à deux rues
//! suffit à préparer sa venue, et connaître sa position au mètre près dirait
//! devant quelle porte il s'est arrêté. La dégradation est faite **à
//! l'écriture** — dégrader à l'affichage laisserait la donnée fine en base,
//! c'est-à-dire exactement là où une fuite la prendrait.
//!
//! **Le suivi s'arrête à l'arrivée.** Une fois sur place, il n'apprend plus rien
//! au demandeur et ne fait que suivre quelqu'un chez lui.

use chrono::{DateTime, Duration, Utc};
use klaar_shared_kernel::{dans_le_perimetre, Geo};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::mission::StatutMission;

/// Précision maximale conservée, en mètres (FR-019 `@security`).
pub const PRECISION_METRES: f64 = 50.0;

/// Pas de la grille en latitude.
///
/// Un degré de latitude fait environ 111 320 m partout ; cinquante mètres en
/// font donc 0,000449°.
pub const PAS_LATITUDE: f64 = PRECISION_METRES / 111_320.0;

/// Pas de la grille en longitude, à la latitude de Bruxelles.
///
/// Un degré de longitude rétrécit avec le cosinus de la latitude : à 50,85°, il
/// vaut environ 70 300 m. Utiliser le même pas qu'en latitude donnerait des
/// cellules de trente mètres de large, soit une précision plus fine que celle
/// qu'on a décidé de garder — l'inverse du but.
pub const PAS_LONGITUDE: f64 = PRECISION_METRES / 70_300.0;

/// Silence au-delà duquel la position est déclarée perdue (FR-019 `@edge`).
pub const PERTE_POSITION_SECONDES: i64 = 30;

/// Délai de purge des positions après la fin de l'intervention, en heures.
///
/// Vingt-quatre. Au-delà, la trace des déplacements de quelqu'un n'a plus de
/// finalité : l'intervention est faite, et ce qui sert aux statistiques —
/// distance et durée — se calcule avant de supprimer.
pub const PURGE_HEURES: i64 = 24;

/// Ce que le demandeur voit du trajet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EtatSuivi {
    /// Position fraîche, le prestataire approche.
    EnRoute,
    /// Plus de position depuis trente secondes (FR-019 `@edge`).
    ///
    /// La dernière connue reste affichée : dire « on ne sait plus » vaut mieux
    /// que d'effacer, et mieux que de laisser croire qu'il n'a pas bougé.
    PositionPerdue,
    /// Le prestataire est sorti de la Région (FR-019 `@edge`).
    HorsZone,
    /// Le suivi est terminé, ou n'a jamais commencé.
    Arrete,
}

impl EtatSuivi {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EnRoute => "EN_ROUTE",
            Self::PositionPerdue => "POSITION_LOST",
            Self::HorsZone => "OUT_OF_ZONE",
            Self::Arrete => "STOPPED",
        }
    }

    pub fn parse(valeur: &str) -> Option<Self> {
        match valeur {
            "EN_ROUTE" => Some(Self::EnRoute),
            "POSITION_LOST" => Some(Self::PositionPerdue),
            "OUT_OF_ZONE" => Some(Self::HorsZone),
            "STOPPED" => Some(Self::Arrete),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuiviError {
    /// L'intervention n'est pas en route : le suivi n'a pas lieu d'être.
    PasEnRoute,
    /// Le prestataire n'a pas consenti au partage pour cette intervention.
    SansConsentement,
}

impl SuiviError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PasEnRoute => "TRACKING_NOT_ACTIVE",
            Self::SansConsentement => "TRACKING_NOT_CONSENTED",
        }
    }
}

impl fmt::Display for SuiviError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasEnRoute => write!(f, "le suivi n'a lieu que pendant le trajet"),
            Self::SansConsentement => {
                write!(f, "le partage de position n'a pas été accepté")
            }
        }
    }
}

impl std::error::Error for SuiviError {}

/// Une position, telle qu'elle sera conservée.
// `PartialEq` sans `Eq` : la position porte des `f64`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionSuivie {
    pub mission_id: Uuid,
    /// Position **déjà dégradée** à la grille de cinquante mètres.
    pub position: Geo,
    pub hors_zone: bool,
    pub relevee_le: DateTime<Utc>,
}

/// Dégrade une position à la grille de cinquante mètres.
///
/// **Appliquée à l'écriture, pas à la lecture.** Dégrader à l'affichage
/// laisserait la donnée fine en base, c'est-à-dire là où une fuite la prendrait
/// et là où une réquisition la trouverait.
pub fn degrader(position: Geo) -> Geo {
    let lat = (position.lat() / PAS_LATITUDE).round() * PAS_LATITUDE;
    let lon = (position.lon() / PAS_LONGITUDE).round() * PAS_LONGITUDE;
    // La grille ramène toujours dans les bornes d'une position valide : un
    // arrondi ne fait pas sortir de la Terre. `unwrap_or` garde la position
    // d'origine plutôt que de perdre le relevé si un cas limite surgissait.
    Geo::new(lat, lon).unwrap_or(position)
}

/// Accepte un relevé, ou dit pourquoi il est refusé.
///
/// `consenti` vient du consentement donné pour **cette** intervention : le
/// passer en paramètre plutôt que de le lire ici est ce qui empêche d'oublier
/// de le vérifier — la fonction ne peut pas être appelée sans une réponse.
pub fn relever(
    mission_id: Uuid,
    statut: StatutMission,
    consenti: bool,
    position: Geo,
    maintenant: DateTime<Utc>,
) -> Result<PositionSuivie, SuiviError> {
    // **L'ordre compte.** Refuser d'abord sur l'état, ensuite sur le
    // consentement : un prestataire qui n'a pas consenti n'apprend pas au
    // passage dans quel état est une intervention qui n'est pas la sienne.
    if statut != StatutMission::EnRoute {
        return Err(SuiviError::PasEnRoute);
    }
    if !consenti {
        return Err(SuiviError::SansConsentement);
    }

    let degradee = degrader(position);
    Ok(PositionSuivie {
        mission_id,
        position: degradee,
        // Le hors-zone est constaté sur la position **dégradée** : c'est elle
        // qui sera conservée, et juger sur une donnée qu'on ne garde pas
        // rendrait le verdict invérifiable.
        hors_zone: !dans_le_perimetre(degradee),
        relevee_le: maintenant,
    })
}

/// L'état à afficher, d'après le dernier relevé.
pub fn etat(
    derniere: Option<&PositionSuivie>,
    statut: StatutMission,
    maintenant: DateTime<Utc>,
) -> EtatSuivi {
    if statut != StatutMission::EnRoute {
        return EtatSuivi::Arrete;
    }
    let Some(derniere) = derniere else {
        return EtatSuivi::PositionPerdue;
    };
    if derniere.hors_zone {
        return EtatSuivi::HorsZone;
    }
    if maintenant >= derniere.relevee_le + Duration::seconds(PERTE_POSITION_SECONDES) {
        return EtatSuivi::PositionPerdue;
    }
    EtatSuivi::EnRoute
}

/// Instant à partir duquel les positions d'une intervention finie se purgent.
pub fn echeance_purge(terminee_le: DateTime<Utc>) -> DateTime<Utc> {
    terminee_le + Duration::hours(PURGE_HEURES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Grand-Place.
    const CENTRE: (f64, f64) = (50.8467, 4.3525);

    fn t0() -> DateTime<Utc> {
        Utc.timestamp_opt(1_780_000_000, 0).unwrap()
    }

    fn geo(lat: f64, lon: f64) -> Geo {
        Geo::new(lat, lon).unwrap()
    }

    /// Distance approchée entre deux points, en mètres.
    fn distance(a: Geo, b: Geo) -> f64 {
        let dlat = (a.lat() - b.lat()) * 111_320.0;
        let dlon = (a.lon() - b.lon()) * 70_300.0;
        (dlat * dlat + dlon * dlon).sqrt()
    }

    // === @happy ===

    #[test]
    fn happy_un_releve_en_route_avec_consentement_est_accepte() {
        let p = relever(
            Uuid::new_v4(),
            StatutMission::EnRoute,
            true,
            geo(CENTRE.0, CENTRE.1),
            t0(),
        )
        .unwrap();
        assert!(!p.hors_zone);
        assert_eq!(p.relevee_le, t0());
    }

    #[test]
    fn happy_une_position_fraiche_affiche_en_route() {
        let p = relever(
            Uuid::new_v4(),
            StatutMission::EnRoute,
            true,
            geo(CENTRE.0, CENTRE.1),
            t0(),
        )
        .unwrap();
        assert_eq!(
            etat(Some(&p), StatutMission::EnRoute, t0()),
            EtatSuivi::EnRoute
        );
    }

    // === @negative ===

    #[test]
    fn negative_sans_consentement_rien_n_est_releve() {
        // FR-019 `@negative` : le demandeur voit « position non partagée ».
        assert_eq!(
            relever(
                Uuid::new_v4(),
                StatutMission::EnRoute,
                false,
                geo(CENTRE.0, CENTRE.1),
                t0()
            ),
            Err(SuiviError::SansConsentement)
        );
    }

    #[test]
    fn negative_hors_du_trajet_rien_n_est_releve() {
        // FR-019 `@happy` : « le tracking s'arrête immédiatement » à `ON_SITE`.
        for statut in [
            StatutMission::Acceptee,
            StatutMission::SurPlace,
            StatutMission::Terminee,
            StatutMission::Validee,
            StatutMission::Annulee,
        ] {
            assert_eq!(
                relever(Uuid::new_v4(), statut, true, geo(CENTRE.0, CENTRE.1), t0()),
                Err(SuiviError::PasEnRoute),
                "{}",
                statut.as_str()
            );
        }
    }

    // === @edge ===

    #[test]
    fn edge_un_silence_de_trente_secondes_perd_la_position() {
        // FR-019 `@edge`. La dernière connue reste affichée : dire « on ne sait
        // plus » vaut mieux que d'effacer.
        let p = relever(
            Uuid::new_v4(),
            StatutMission::EnRoute,
            true,
            geo(CENTRE.0, CENTRE.1),
            t0(),
        )
        .unwrap();

        let avant = t0() + Duration::seconds(PERTE_POSITION_SECONDES - 1);
        assert_eq!(
            etat(Some(&p), StatutMission::EnRoute, avant),
            EtatSuivi::EnRoute
        );

        let apres = t0() + Duration::seconds(PERTE_POSITION_SECONDES);
        assert_eq!(
            etat(Some(&p), StatutMission::EnRoute, apres),
            EtatSuivi::PositionPerdue
        );
    }

    #[test]
    fn edge_sortir_de_la_region_se_signale() {
        // FR-019 `@edge`. Le relevé est **accepté** : un prestataire qui coupe
        // par le ring reste en intervention, et refuser sa position ferait
        // disparaître le suivi au lieu de l'expliquer.
        let ailleurs = geo(50.4, 4.0);
        let p = relever(Uuid::new_v4(), StatutMission::EnRoute, true, ailleurs, t0()).unwrap();
        assert!(p.hors_zone);
        assert_eq!(
            etat(Some(&p), StatutMission::EnRoute, t0()),
            EtatSuivi::HorsZone
        );
    }

    #[test]
    fn edge_sans_aucun_releve_la_position_est_perdue() {
        assert_eq!(
            etat(None, StatutMission::EnRoute, t0()),
            EtatSuivi::PositionPerdue
        );
    }

    #[test]
    fn edge_hors_trajet_l_etat_est_arrete() {
        assert_eq!(etat(None, StatutMission::SurPlace, t0()), EtatSuivi::Arrete);
    }

    #[test]
    fn edge_la_purge_tombe_a_vingt_quatre_heures() {
        assert_eq!((echeance_purge(t0()) - t0()).num_hours(), PURGE_HEURES);
    }

    // === @security ===

    #[test]
    fn security_la_precision_conservee_ne_depasse_jamais_cinquante_metres() {
        // **C'est la minimisation de l'article 5.1.c.** Savoir que le plombier
        // est à deux rues suffit ; sa position au mètre près dirait devant
        // quelle porte il s'est arrêté.
        let mut ecart_max: f64 = 0.0;
        for i in 0..200 {
            let lat = CENTRE.0 + f64::from(i) * 0.000_137;
            let lon = CENTRE.1 + f64::from(i) * 0.000_211;
            let fine = geo(lat, lon);
            let ecart = distance(fine, degrader(fine));
            ecart_max = ecart_max.max(ecart);
        }
        // La grille garantit au plus un demi-pas d'écart sur chaque axe, soit
        // moins de trente-six mètres en diagonale.
        assert!(
            ecart_max <= PRECISION_METRES,
            "écart maximal de {ecart_max} m"
        );
        assert!(ecart_max > 0.0, "la dégradation doit réellement dégrader");
    }

    #[test]
    fn security_la_degradation_est_appliquee_avant_conservation() {
        // Dégrader à l'affichage laisserait la donnée fine en base, c'est-à-dire
        // là où une fuite la prendrait. Le relevé rendu est déjà sur la grille.
        let fine = geo(50.846_712_3, 4.352_598_7);
        let p = relever(Uuid::new_v4(), StatutMission::EnRoute, true, fine, t0()).unwrap();
        assert_eq!(p.position, degrader(fine));
        assert_ne!(p.position, fine);
    }

    #[test]
    fn security_deux_positions_voisines_deviennent_indistinguables() {
        // C'est ce que la minimisation veut dire concrètement : deux portes de
        // la même rue ne doivent pas se distinguer.
        let a = geo(CENTRE.0, CENTRE.1);
        let b = geo(CENTRE.0 + 0.000_05, CENTRE.1 + 0.000_05);
        assert!(distance(a, b) < PRECISION_METRES / 2.0);
        assert_eq!(degrader(a), degrader(b));
    }

    #[test]
    fn security_le_suivi_hors_intervention_est_impossible_a_ecrire() {
        // Invariant §10.5. Ce n'est pas une consigne : `relever` ne rend un
        // relevé que pour une Mission en route et consentie, et il n'existe
        // aucun autre chemin qui produise un `PositionSuivie`.
        let interdits = [
            (StatutMission::Acceptee, true),
            (StatutMission::SurPlace, true),
            (StatutMission::EnRoute, false),
            (StatutMission::Terminee, true),
        ];
        for (statut, consenti) in interdits {
            assert!(
                relever(
                    Uuid::new_v4(),
                    statut,
                    consenti,
                    geo(CENTRE.0, CENTRE.1),
                    t0()
                )
                .is_err(),
                "{} consenti={consenti}",
                statut.as_str()
            );
        }
    }

    #[test]
    fn security_l_etat_ne_fuit_rien_hors_du_trajet() {
        // Une fois arrivé, l'état ne dit plus où il est — même si un relevé
        // ancien traîne.
        let p = relever(
            Uuid::new_v4(),
            StatutMission::EnRoute,
            true,
            geo(CENTRE.0, CENTRE.1),
            t0(),
        )
        .unwrap();
        for statut in [
            StatutMission::SurPlace,
            StatutMission::Terminee,
            StatutMission::Validee,
            StatutMission::Annulee,
        ] {
            assert_eq!(
                etat(Some(&p), statut, t0()),
                EtatSuivi::Arrete,
                "{}",
                statut.as_str()
            );
        }
    }

    #[test]
    fn security_le_vocabulaire_est_ferme() {
        for e in [
            EtatSuivi::EnRoute,
            EtatSuivi::PositionPerdue,
            EtatSuivi::HorsZone,
            EtatSuivi::Arrete,
        ] {
            assert_eq!(EtatSuivi::parse(e.as_str()), Some(e));
        }
        assert_eq!(EtatSuivi::parse("TRACKING"), None);
    }
}
