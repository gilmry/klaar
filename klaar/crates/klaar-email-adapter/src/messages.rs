//! Contenu des courriels, en FR, NL et EN (DoD Story 1.1, FR-043).
//!
//! Les textes sont en dur plutôt que dans des fichiers de traduction : trois
//! langues et deux messages ne justifient pas encore une chaîne d'extraction,
//! et le compilateur garantit ici qu'aucune combinaison n'est oubliée, ce
//! qu'un fichier `.po` incomplet ne ferait pas.

use klaar_application::ports::courriel::CourrielInscription;
use klaar_shared_kernel::Locale;

pub struct MessageCourriel {
    pub sujet: String,
    pub corps: String,
}

pub fn sujet_inscription(locale: Locale, contenu: &CourrielInscription) -> String {
    match (locale, contenu) {
        (Locale::Fr, CourrielInscription::Verification { .. }) => "Confirmez votre adresse Klaar",
        (Locale::Nl, CourrielInscription::Verification { .. }) => "Bevestig uw Klaar-adres",
        (Locale::En, CourrielInscription::Verification { .. }) => "Confirm your Klaar address",
        (Locale::Fr, CourrielInscription::CompteDejaExistant) => {
            "Une inscription a été tentée avec votre adresse"
        }
        (Locale::Nl, CourrielInscription::CompteDejaExistant) => {
            "Er is een registratie geprobeerd met uw adres"
        }
        (Locale::En, CourrielInscription::CompteDejaExistant) => {
            "Someone tried to sign up with your address"
        }
    }
    .to_string()
}

/// Corps du message.
///
/// `lien` n'est utilisé que par la variante `Verification`. Le message de
/// compte déjà existant n'en porte aucun, délibérément : il part à quelqu'un
/// qui n'a rien demandé, et tout lien qu'il contiendrait ferait de
/// l'inscription sur l'adresse d'autrui un moyen de lui expédier un jeton.
pub fn corps_inscription(
    locale: Locale,
    contenu: &CourrielInscription,
    lien: Option<&str>,
) -> String {
    match contenu {
        CourrielInscription::Verification { .. } => {
            let invitation = match locale {
                Locale::Fr => concat!(
                    "Bonjour,\n\n",
                    "Vous venez de créer un compte Klaar. Confirmez votre adresse ",
                    "dans l'heure qui vient pour l'activer.",
                ),
                Locale::Nl => concat!(
                    "Hallo,\n\n",
                    "U hebt zojuist een Klaar-account aangemaakt. Bevestig uw adres ",
                    "binnen het uur om het te activeren.",
                ),
                Locale::En => concat!(
                    "Hello,\n\n",
                    "You have just created a Klaar account. Confirm your address ",
                    "within the hour to activate it.",
                ),
            };
            let clore = match locale {
                Locale::Fr => concat!(
                    "\n\nSi vous n'êtes pas à l'origine de cette demande, ignorez ce ",
                    "message : le compte restera inactif et sera effacé.",
                ),
                Locale::Nl => concat!(
                    "\n\nHebt u dit niet aangevraagd? Negeer dit bericht: het account ",
                    "blijft inactief en wordt verwijderd.",
                ),
                Locale::En => concat!(
                    "\n\nIf you did not request this, ignore this message: the account ",
                    "will stay inactive and be deleted.",
                ),
            };
            match lien {
                Some(lien) => format!("{invitation}\n\n{lien}{clore}"),
                None => format!("{invitation}{clore}"),
            }
        }
        CourrielInscription::CompteDejaExistant => match locale {
            Locale::Fr => concat!(
                "Bonjour,\n\n",
                "Quelqu'un vient de tenter de créer un compte Klaar avec votre ",
                "adresse. Vous en avez déjà un, aucun nouveau compte n'a donc été ",
                "créé et rien n'a changé.\n\n",
                "Si ce n'était pas vous, il n'y a rien à faire. Si vous avez oublié ",
                "votre mot de passe, utilisez la réinitialisation depuis la page de ",
                "connexion.",
            ),
            Locale::Nl => concat!(
                "Hallo,\n\n",
                "Iemand heeft zojuist geprobeerd een Klaar-account aan te maken met ",
                "uw adres. U hebt er al een, dus er is geen nieuw account aangemaakt ",
                "en er is niets gewijzigd.\n\n",
                "Was u dit niet, dan hoeft u niets te doen. Bent u uw wachtwoord ",
                "vergeten, gebruik dan het herstel op de inlogpagina.",
            ),
            Locale::En => concat!(
                "Hello,\n\n",
                "Someone has just tried to create a Klaar account with your address. ",
                "You already have one, so no new account was created and nothing has ",
                "changed.\n\n",
                "If this was not you, there is nothing to do. If you have forgotten ",
                "your password, use the reset link on the sign-in page.",
            ),
        }
        .to_string(),
    }
}
