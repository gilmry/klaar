//! Port d'horloge.
//!
//! Les use cases ne lisent pas `Utc::now()` directement. Une expiration testée
//! contre l'horloge réelle se teste en attendant, donc ne se teste pas : le
//! test finit annoté « ignore », et l'expiration n'est plus vérifiée du tout.

use chrono::{DateTime, Utc};

pub trait Horloge: Send + Sync {
    fn maintenant(&self) -> DateTime<Utc>;
}

/// Horloge du système. La seule employée par les binaires.
pub struct HorlogeSysteme;

impl Horloge for HorlogeSysteme {
    fn maintenant(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Horloge figée, pour les tests.
pub struct HorlogeFigee(pub DateTime<Utc>);

impl Horloge for HorlogeFigee {
    fn maintenant(&self) -> DateTime<Utc> {
        self.0
    }
}
