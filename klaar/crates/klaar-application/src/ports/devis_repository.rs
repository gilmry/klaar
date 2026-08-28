//! Port des Devis (FR-016, Story 4.1).
//!
//! **Deux règles de comptage, et la base les tient toutes les deux.** « Un seul
//! devis en attente par Mission » et « trois devis au maximum » se vérifient sur
//! des lignes que d'autres transactions écrivent au même moment : les lire puis
//! décider laisserait deux envois simultanés poser deux devis, et le demandeur
//! verrait deux prix sans savoir lequel l'engage. Le comptage est donc dans la
//! même instruction que l'insertion, et l'unicité dans un index partiel.

use chrono::{DateTime, Utc};
use klaar_payment::{Devis, StatutDevis};
use uuid::Uuid;

use super::erreurs::RepositoryError;

/// Issue d'une tentative d'émission.
///
/// Trois cas distincts et non un `Option` : « vous en avez déjà un en attente »
/// et « vous avez épuisé vos trois envois » appellent des réponses différentes,
/// et les confondre dirait au prestataire d'attendre alors que la Mission est
/// sur le point d'être annulée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultatEmission {
    Emis(Devis),
    /// Un devis attend encore une réponse pour cette Mission.
    DejaEnCours,
    /// Le plafond de FR-016 `@edge` est atteint.
    PlafondAtteint,
}

#[allow(async_fn_in_trait)]
pub trait DevisRepository {
    /// Écrit le devis, ou dit pourquoi il n'y avait pas de place pour lui.
    ///
    /// Le plafond est passé en paramètre plutôt que lu d'une constante par
    /// l'adaptateur : c'est une règle métier, elle appartient au domaine, et la
    /// recopier dans du SQL la ferait diverger le jour où elle change.
    async fn emettre(
        &self,
        devis: &Devis,
        plafond: usize,
    ) -> Result<ResultatEmission, RepositoryError>;

    /// Devis en attente de réponse pour cette Mission, s'il y en a un.
    async fn en_cours_pour_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<Devis>, RepositoryError>;

    /// Dernier devis émis pour cette Mission, quel que soit son statut.
    ///
    /// C'est ce que lit le suivi du demandeur : un devis expiré ou refusé doit
    /// rester visible, sans quoi l'écran redeviendrait vide sans explication.
    async fn dernier_pour_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<Option<Devis>, RepositoryError>;

    /// Nombre de devis déjà émis pour cette Mission, tous statuts confondus.
    async fn compter_pour_mission(&self, mission_id: Uuid) -> Result<usize, RepositoryError>;

    /// Écrit la réponse du demandeur, si le devis attend encore.
    ///
    /// **Compare-and-swap sur le statut**, comme partout où deux appelants
    /// peuvent arriver ensemble : le demandeur qui touche « accepter » deux
    /// fois, ou qui accepte à l'instant où le balayage expire son devis. Rend
    /// `false` quand le devis avait déjà bougé, et l'appelant traduit.
    async fn repondre(
        &self,
        devis_id: Uuid,
        reponse: StatutDevis,
        motif: Option<&str>,
    ) -> Result<bool, RepositoryError>;

    /// Devis lu par son identifiant.
    async fn par_id(&self, devis_id: Uuid) -> Result<Option<Devis>, RepositoryError>;

    /// Éteint les devis dont l'heure est passée et rend ceux qu'il vient
    /// d'éteindre — eux seuls.
    ///
    /// La sélection et l'extinction forment une seule opération : deux passages
    /// du balayage ne peuvent donc pas prévenir deux fois le même prestataire.
    async fn expirer_les_echus(
        &self,
        maintenant: DateTime<Utc>,
        limite: i64,
    ) -> Result<Vec<Devis>, RepositoryError>;
}
