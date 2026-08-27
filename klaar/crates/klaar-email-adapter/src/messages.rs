//! Contenu des courriels, en FR, NL et EN (DoD Story 1.1, FR-043).
//!
//! Les textes sont en dur plutôt que dans des fichiers de traduction : trois
//! langues et deux messages ne justifient pas encore une chaîne d'extraction,
//! et le compilateur garantit ici qu'aucune combinaison n'est oubliée, ce
//! qu'un fichier `.po` incomplet ne ferait pas.

use klaar_application::ports::courriel::{CourrielInscription, CourrielSecurite};
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

pub fn sujet_securite(locale: Locale, contenu: &CourrielSecurite) -> String {
    match (locale, contenu) {
        (Locale::Fr, CourrielSecurite::CompteVerrouille { .. }) => {
            "Votre compte Klaar a été temporairement verrouillé"
        }
        (Locale::Nl, CourrielSecurite::CompteVerrouille { .. }) => {
            "Uw Klaar-account is tijdelijk vergrendeld"
        }
        (Locale::En, CourrielSecurite::CompteVerrouille { .. }) => {
            "Your Klaar account has been temporarily locked"
        }
    }
    .to_string()
}

/// Corps de l'alerte de sécurité.
///
/// Aucun lien, ici non plus : ce message part à quelqu'un qui n'a rien demandé,
/// et un lien y ferait des tentatives ratées un moyen de lui expédier une
/// action à cliquer. Il dit ce qui s'est passé, combien de temps cela dure, et
/// quoi faire — rien de plus.
pub fn corps_securite(locale: Locale, contenu: &CourrielSecurite) -> String {
    let CourrielSecurite::CompteVerrouille { minutes } = contenu;
    match locale {
        Locale::Fr => format!(
            concat!(
                "Bonjour,\n\n",
                "Plusieurs tentatives de connexion à votre compte Klaar ont échoué. ",
                "Par précaution, il est verrouillé pendant {} minutes ; il se rouvrira ",
                "ensuite tout seul, sans démarche de votre part.\n\n",
                "Si ces tentatives venaient de vous, il n'y a rien à faire d'autre ",
                "qu'attendre. Sinon, changez votre mot de passe dès la réouverture : ",
                "quelqu'un connaît votre adresse et cherche le mot de passe qui va avec."
            ),
            minutes
        ),
        Locale::Nl => format!(
            concat!(
                "Hallo,\n\n",
                "Er zijn meerdere mislukte aanmeldpogingen op uw Klaar-account geweest. ",
                "Uit voorzorg is het {} minuten vergrendeld; daarna gaat het vanzelf ",
                "weer open.\n\n",
                "Kwamen die pogingen van u, dan hoeft u niets te doen. Zo niet, wijzig ",
                "dan uw wachtwoord zodra het account weer open is."
            ),
            minutes
        ),
        Locale::En => format!(
            concat!(
                "Hello,\n\n",
                "Several sign-in attempts on your Klaar account have failed. As a ",
                "precaution, it is locked for {} minutes; it will reopen on its own ",
                "afterwards.\n\n",
                "If those attempts were yours, there is nothing else to do but wait. ",
                "Otherwise, change your password as soon as it reopens: someone knows ",
                "your address and is looking for the password that goes with it."
            ),
            minutes
        ),
    }
}
