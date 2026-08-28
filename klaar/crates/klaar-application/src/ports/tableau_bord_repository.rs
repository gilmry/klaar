//! Port des indicateurs d'exploitation (FR-040, Story 8.3).
//!
//! **Que des agrégats, jamais une ligne nominative.** Un tableau de bord qui
//! rendrait des identifiants deviendrait un moyen commode de consulter des
//! dossiers sans passer par les routes qui, elles, journalisent qui a regardé
//! quoi. Les types de ce port n'ont aucun champ qui désigne quelqu'un.

use chrono::{DateTime, Utc};

use super::erreurs::RepositoryError;

/// Les indicateurs, à un instant donné.
///
/// **Des numérateurs et des dénominateurs, pas des pourcentages.** Un taux
/// calculé côté base arriverait sans son assiette, et « 60 % » sur trois
/// Demandes se lit comme « 60 % » sur trois mille. Le calcul se fait là où
/// l'affichage peut aussi montrer le nombre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Indicateurs {
    /// Comptes ayant ouvert une session sur la fenêtre (MAU).
    pub comptes_actifs: i64,
    /// Demandes soumises sur la fenêtre.
    pub demandes: i64,
    /// Celles qui ont trouvé un prestataire. Avec `demandes`, c'est le
    /// *fill rate*.
    pub demandes_attribuees: i64,
    /// Somme HTVA des devis acceptés sur la fenêtre, en centimes (GMV).
    pub gmv_htva_cents: i64,
    /// Ce que la plateforme a prélevé dessus, en centimes.
    pub commission_htva_cents: i64,
    /// Litiges non tranchés, **toutes dates confondues** : un litige de six
    /// semaines compte encore, et c'est même celui qui compte le plus.
    pub litiges_ouverts: i64,
    /// Notes déposées sur la fenêtre, et leur somme. Le rapport donne la note
    /// moyenne, avec son assiette.
    pub notes: i64,
    pub somme_notes: i64,
    /// Sorties de zone consignées sur la fenêtre (FR-018).
    pub sorties_de_zone: i64,
    /// Contrôles d'entreprise en attente de décision (FR-038).
    pub kyc_en_attente: i64,
}

#[allow(async_fn_in_trait)]
pub trait TableauBordRepository {
    /// Calcule les indicateurs sur `[depuis, maintenant]`.
    async fn indicateurs(&self, depuis: DateTime<Utc>) -> Result<Indicateurs, RepositoryError>;
}
