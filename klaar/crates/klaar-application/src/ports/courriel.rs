//! Port d'envoi de courriel.

use klaar_identity::JetonVerification;
use klaar_shared_kernel::{Email, Locale};
use std::fmt;

#[derive(Debug)]
pub struct ErreurEnvoi(pub String);

impl fmt::Display for ErreurEnvoi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "envoi de courriel impossible : {}", self.0)
    }
}

impl std::error::Error for ErreurEnvoi {}

/// Message envoyé à l'issue d'une tentative d'inscription.
///
/// Les deux variantes existent parce que l'anti-énumération l'impose. Si
/// l'inscription sur une adresse déjà prise n'envoyait rien, l'attaquant
/// n'aurait pas besoin de lire la réponse : il lui suffirait de mesurer le
/// temps de traitement, un envoi de courriel se voyant sans ambiguïté. En
/// envoyant dans les deux cas, le coût est le même, et le titulaire légitime
/// apprend au passage que quelqu'un a tenté de s'inscrire avec son adresse.
///
/// C'est un écart avec la lettre de FR-001 (« aucun email n'est envoyé »), mais
/// la conséquence directe de son propre scénario `@security`, qui exige une
/// réponse indistinguable « en timing et en payload ».
#[derive(Debug)]
pub enum CourrielInscription {
    /// Compte créé : lien de vérification à suivre dans l'heure.
    Verification { jeton: JetonVerification },
    /// Adresse déjà rattachée à un compte. Aucun lien, aucune action possible
    /// depuis ce message : il informe, il n'autorise rien.
    CompteDejaExistant,
}

/// Alerte de sécurité adressée au titulaire d'un compte.
#[derive(Debug, Clone, Copy)]
pub enum CourrielSecurite {
    /// Compte verrouillé après des échecs répétés (FR-007).
    ///
    /// Ne porte aucun lien : ce message part à quelqu'un qui n'a probablement
    /// rien demandé, et tout lien qu'il contiendrait ferait des tentatives
    /// ratées un moyen de lui expédier une action à cliquer.
    CompteVerrouille { minutes: i64 },
}

#[allow(async_fn_in_trait)]
pub trait EnvoiCourriel {
    async fn envoyer_securite(
        &self,
        destinataire: &Email,
        locale: Locale,
        contenu: CourrielSecurite,
    ) -> Result<(), ErreurEnvoi>;

    async fn envoyer_inscription(
        &self,
        destinataire: &Email,
        locale: Locale,
        contenu: CourrielInscription,
    ) -> Result<(), ErreurEnvoi>;
}
