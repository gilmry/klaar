//! Périmètre géographique du service (FR-011, `GEO_OUTSIDE_RBC`).
//!
//! **Ici et non dans un bounded context.** La zone servie n'appartient ni au
//! matching ni à l'intervention : les deux s'y réfèrent, une Demande pour être
//! acceptée (FR-011) et une Mission pour signaler une sortie de zone
//! (FR-018 `@edge`). La dupliquer ferait diverger deux définitions de la même
//! frontière, et un prestataire serait alors « hors zone » selon des bornes que
//! le demandeur n'a jamais connues.
//!
//! **Ce contrôle est une approximation, et il faut le savoir.** La Région de
//! Bruxelles-Capitale est un polygone de dix-neuf communes aux contours
//! irréguliers ; ce module la ramène à un rectangle englobant. Un rectangle
//! **sur-accepte** : il laisse passer des points situés en Brabant flamand,
//! juste au-delà de la frontière régionale — Kraainem, Drogenbos, Zaventem.
//!
//! Ce choix est délibéré et son sens compte. Sur-accepter fait entrer quelques
//! Demandes hors périmètre, qu'un prestataire refusera ; sous-accepter
//! refuserait des Bruxellois chez eux, ce qui est bien pire. Le rectangle est
//! donc dessiné large.
//!
//! Le contour réel viendra avec les données OpenStreetMap de la Story 0.11,
//! aujourd'hui bloquée faute d'hébergement pour le tile-server. À remplacer
//! avant toute mise en service — c'est écrit dans `COMPLIANCE.md`.

use crate::Geo;

/// Rectangle englobant la Région de Bruxelles-Capitale.
///
/// Bornes prises avec une marge sur les extrêmes de la région : au nord
/// Neder-Over-Heembeek, au sud Uccle, à l'ouest Berchem-Sainte-Agathe, à l'est
/// Woluwe-Saint-Pierre.
pub const LAT_MIN: f64 = 50.76;
pub const LAT_MAX: f64 = 50.92;
pub const LON_MIN: f64 = 4.24;
pub const LON_MAX: f64 = 4.49;

/// Vrai si le point tombe dans le périmètre servi.
pub fn dans_le_perimetre(position: Geo) -> bool {
    (LAT_MIN..=LAT_MAX).contains(&position.lat()) && (LON_MIN..=LON_MAX).contains(&position.lon())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> Geo {
        Geo::new(lat, lon).expect("coordonnée valide")
    }

    #[test]
    fn happy_accepte_le_centre_de_bruxelles() {
        // Grand-Place.
        assert!(dans_le_perimetre(point(50.8467, 4.3525)));
    }

    #[test]
    fn happy_accepte_les_communes_peripheriques_de_la_region() {
        let communes = [
            ("Uccle", 50.8003, 4.3383),
            ("Neder-Over-Heembeek", 50.8994, 4.3856),
            ("Berchem-Sainte-Agathe", 50.8644, 4.2939),
            ("Woluwe-Saint-Pierre", 50.8317, 4.4372),
            ("Anderlecht", 50.8367, 4.3097),
        ];
        for (nom, lat, lon) in communes {
            assert!(dans_le_perimetre(point(lat, lon)), "{nom} est dans la RBC");
        }
    }

    #[test]
    fn negative_refuse_une_autre_ville_belge() {
        let ailleurs = [
            ("Anvers", 51.2194, 4.4025),
            ("Liège", 50.6326, 5.5797),
            ("Gand", 51.0543, 3.7174),
            ("Charleroi", 50.4108, 4.4446),
            ("Namur", 50.4674, 4.8720),
        ];
        for (nom, lat, lon) in ailleurs {
            assert!(!dans_le_perimetre(point(lat, lon)), "{nom} est hors RBC");
        }
    }

    #[test]
    fn negative_refuse_l_autre_bout_du_monde() {
        for (lat, lon) in [(0.0, 0.0), (35.68, 139.69), (-33.87, 151.21), (90.0, 0.0)] {
            assert!(!dans_le_perimetre(point(lat, lon)));
        }
    }

    #[test]
    fn edge_les_bornes_elles_memes_sont_acceptees() {
        // Un point exactement sur la limite est dedans : refuser au micro-degré
        // près produirait des refus incompréhensibles pour qui habite au bord.
        assert!(dans_le_perimetre(point(LAT_MIN, LON_MIN)));
        assert!(dans_le_perimetre(point(LAT_MAX, LON_MAX)));
    }

    #[test]
    fn edge_juste_au_dela_d_une_borne_est_refuse() {
        assert!(!dans_le_perimetre(point(LAT_MIN - 0.001, LON_MIN)));
        assert!(!dans_le_perimetre(point(LAT_MAX + 0.001, LON_MAX)));
        assert!(!dans_le_perimetre(point(LAT_MIN, LON_MIN - 0.001)));
        assert!(!dans_le_perimetre(point(LAT_MAX, LON_MAX + 0.001)));
    }

    #[test]
    fn security_le_rectangle_sur_accepte_et_c_est_documente() {
        // Ce test **constate** la limite plutôt que de la masquer. Kraainem et
        // Drogenbos sont en Brabant flamand, hors de la Région, et tombent
        // pourtant dans le rectangle.
        //
        // Sur-accepter fait entrer quelques Demandes hors périmètre, qu'un
        // prestataire refusera ; sous-accepter refuserait des Bruxellois chez
        // eux, ce qui est pire. Le jour où le contour réel remplace ce
        // rectangle (Story 0.11), ce test doit être inversé.
        assert!(
            dans_le_perimetre(point(50.8583, 4.4667)),
            "Kraainem : hors RBC mais dans le rectangle"
        );
        assert!(
            dans_le_perimetre(point(50.7842, 4.3128)),
            "Drogenbos : hors RBC mais dans le rectangle"
        );
    }

    #[test]
    fn security_le_perimetre_ne_couvre_pas_tout_le_pays() {
        // Le rectangle est large, pas ouvert : il doit rester un filtre.
        // Sa diagonale fait une vingtaine de kilomètres, pas la Belgique.
        // `const { }` : ces bornes sont des constantes, et l'assertion se
        // vérifie donc à la compilation plutôt qu'à l'exécution. Élargir le
        // rectangle au-delà de ces limites ne compilera plus.
        const { assert!(LAT_MAX - LAT_MIN < 0.25) };
        const { assert!(LON_MAX - LON_MIN < 0.35) };
    }
}
