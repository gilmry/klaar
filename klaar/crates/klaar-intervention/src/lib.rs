//! Bounded context Intervention : Mission, machine à états (FR-013, FR-018 à
//! FR-023).
//!
//! La Mission naît de l'acceptation d'une Demande (FR-013, Story 3.4). Sa
//! machine à états — en route, sur place, terminée, validée, annulée — relève
//! de FR-018 et suivants, et n'est pas encore écrite : voir
//! docs/bmad-livrables/04-Epics-Stories.md.

mod mission;

pub use mission::{Mission, StatutMission};
