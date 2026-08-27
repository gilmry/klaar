/**
 * Connexion (Story 1.3, FR-004).
 *
 * **Où vit le jeton d'accès.** En mémoire du module, et nulle part ailleurs.
 * `localStorage` et `sessionStorage` sont lisibles par tout script de la page :
 * une seule faille XSS y prend le jeton et le garde. En mémoire, il disparaît
 * au rechargement — c'est le refresh, lui en cookie `HttpOnly` donc hors de
 * portée de JavaScript, qui permettra d'en réobtenir un (Story 1.4).
 *
 * Conséquence assumée en attendant la Story 1.4 : recharger la page déconnecte.
 */
import { ApiError, OfflineError, request } from "./api";
import type { LocaleKlaar } from "./inscription";

export interface DemandeConnexion {
  email: string;
  mot_de_passe: string;
}

export interface SessionOuverte {
  jeton_acces: string;
  expire_dans: number;
}

export type CodeErreurConnexion =
  | "EMAIL_EMPTY"
  | "EMAIL_MALFORMED"
  | "PASSWORD_EMPTY"
  | "PASSWORD_TOO_SHORT"
  | "PASSWORD_TOO_LONG"
  | "INVALID_CREDENTIALS"
  | "ACCOUNT_NOT_VERIFIED"
  | "RATE_LIMIT_EXCEEDED"
  | "SERVICE_UNAVAILABLE"
  | "INCONNU"
  | "HORS_LIGNE";

const MESSAGES: Record<LocaleKlaar, Record<CodeErreurConnexion, string>> = {
  fr: {
    EMAIL_EMPTY: "Indiquez votre adresse email.",
    EMAIL_MALFORMED: "Cette adresse email n'est pas valide.",
    PASSWORD_EMPTY: "Indiquez votre mot de passe.",
    PASSWORD_TOO_SHORT: "Adresse ou mot de passe incorrect.",
    PASSWORD_TOO_LONG: "Adresse ou mot de passe incorrect.",
    INVALID_CREDENTIALS: "Adresse ou mot de passe incorrect.",
    ACCOUNT_NOT_VERIFIED:
      "Votre adresse n'est pas encore confirmée. Ouvrez le lien reçu par courriel.",
    RATE_LIMIT_EXCEEDED: "Trop de tentatives depuis cette connexion. Réessayez dans une heure.",
    SERVICE_UNAVAILABLE: "Le service est momentanément indisponible. Réessayez.",
    INCONNU: "La connexion n'a pas abouti. Réessayez.",
    HORS_LIGNE: "Aucune connexion. La connexion à votre compte a besoin du réseau.",
  },
  nl: {
    EMAIL_EMPTY: "Geef uw e-mailadres op.",
    EMAIL_MALFORMED: "Dit e-mailadres is niet geldig.",
    PASSWORD_EMPTY: "Geef uw wachtwoord op.",
    PASSWORD_TOO_SHORT: "Onjuist adres of wachtwoord.",
    PASSWORD_TOO_LONG: "Onjuist adres of wachtwoord.",
    INVALID_CREDENTIALS: "Onjuist adres of wachtwoord.",
    ACCOUNT_NOT_VERIFIED:
      "Uw adres is nog niet bevestigd. Open de link uit uw e-mail.",
    RATE_LIMIT_EXCEEDED:
      "Te veel pogingen vanaf deze verbinding. Probeer het over een uur opnieuw.",
    SERVICE_UNAVAILABLE: "De dienst is tijdelijk niet beschikbaar. Probeer opnieuw.",
    INCONNU: "Aanmelden is mislukt. Probeer opnieuw.",
    HORS_LIGNE: "Geen verbinding. Aanmelden vereist het netwerk.",
  },
  en: {
    EMAIL_EMPTY: "Enter your email address.",
    EMAIL_MALFORMED: "This email address is not valid.",
    PASSWORD_EMPTY: "Enter your password.",
    PASSWORD_TOO_SHORT: "Incorrect address or password.",
    PASSWORD_TOO_LONG: "Incorrect address or password.",
    INVALID_CREDENTIALS: "Incorrect address or password.",
    ACCOUNT_NOT_VERIFIED:
      "Your address is not confirmed yet. Open the link from your email.",
    RATE_LIMIT_EXCEEDED: "Too many attempts from this connection. Try again in an hour.",
    SERVICE_UNAVAILABLE: "The service is temporarily unavailable. Please retry.",
    INCONNU: "Sign-in did not go through. Please retry.",
    HORS_LIGNE: "No connection. Signing in needs the network.",
  },
};

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = MESSAGES[locale];
  return table[code as CodeErreurConnexion] ?? table.INCONNU;
}

export function codeDepuisErreur(erreur: unknown): CodeErreurConnexion {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurConnexion;
  } catch {
    // Réponse d'une passerelle plutôt que de l'API.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

/** Jeton courant. Jamais écrit ailleurs qu'ici. */
let jetonCourant: string | null = null;

export function jetonAcces(): string | null {
  return jetonCourant;
}

export function oublierJeton(): void {
  jetonCourant = null;
}

export async function seConnecter(demande: DemandeConnexion): Promise<SessionOuverte> {
  const session = await request<SessionOuverte>("/auth/login", {
    method: "POST",
    body: demande,
  });
  jetonCourant = session.jeton_acces;
  return session;
}
