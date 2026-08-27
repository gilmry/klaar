use serde::{Deserialize, Serialize};

/// Coordonnée géographique validée : lat ∈ [-90, 90], lon ∈ [-180, 180].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Geo {
    lat: f64,
    lon: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoError {
    LatitudeOutOfRange,
    LongitudeOutOfRange,
    NotFinite,
}

impl Geo {
    pub fn new(lat: f64, lon: f64) -> Result<Self, GeoError> {
        if !lat.is_finite() || !lon.is_finite() {
            return Err(GeoError::NotFinite);
        }
        if !(-90.0..=90.0).contains(&lat) {
            return Err(GeoError::LatitudeOutOfRange);
        }
        if !(-180.0..=180.0).contains(&lon) {
            return Err(GeoError::LongitudeOutOfRange);
        }
        Ok(Self { lat, lon })
    }

    pub fn lat(&self) -> f64 {
        self.lat
    }

    pub fn lon(&self) -> f64 {
        self.lon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_accepts_brussels_coordinates() {
        let brussels = Geo::new(50.8503, 4.3517).unwrap();
        assert_eq!(brussels.lat(), 50.8503);
        assert_eq!(brussels.lon(), 4.3517);
    }

    #[test]
    fn negative_rejects_latitude_over_90() {
        assert_eq!(Geo::new(91.0, 0.0), Err(GeoError::LatitudeOutOfRange));
    }

    #[test]
    fn edge_accepts_boundary_values() {
        assert!(Geo::new(90.0, 180.0).is_ok());
        assert!(Geo::new(-90.0, -180.0).is_ok());
    }

    #[test]
    fn security_rejects_nan_and_infinite_inputs() {
        assert_eq!(Geo::new(f64::NAN, 0.0), Err(GeoError::NotFinite));
        assert_eq!(Geo::new(0.0, f64::INFINITY), Err(GeoError::NotFinite));
    }
}
