/**
 * Inscription (Story 1.1, FR-001) : appel HTTP et traduction des codes.
 *
 * Le backend ne renvoie que des codes, jamais de prose. C'est délibéré : un
 * message côté serveur serait dans une seule langue, et les codes ont l'autre
 * avantage de rendre la réponse d'inscription identique quelle que soit
 * l'issue, ce dont dépend l'anti-énumération.
 */
import { ApiError, OfflineError, request } from "./api";

export type LocaleKlaar = "fr" | "nl" | "en";

export interface DemandeInscription {
  email: string;
  mot_de_passe: string;
  locale?: LocaleKlaar;
}

export interface ReponseInscription {
  code: "SIGNUP_ACCEPTED";
}

/**
 * Codes de refus que l'API peut renvoyer. La liste est fermée : un code
 * inconnu doit produire un message générique plutôt qu'une page vide.
 */
export type CodeErreurInscription =
  | "EMAIL_EMPTY"
  | "EMAIL_MALFORMED"
  | "PASSWORD_EMPTY"
  | "PASSWORD_TOO_SHORT"
  | "PASSWORD_TOO_LONG"
  | "RATE_LIMIT_EXCEEDED"
  | "SERVICE_UNAVAILABLE"
  | "INCONNU"
  | "HORS_LIGNE";

/** Longueur minimale, alignée sur le domaine (NIST SP 800-63B). */
export const LONGUEUR_MIN_MOT_DE_PASSE = 12;

const MESSAGES: Record<LocaleKlaar, Record<CodeErreurInscription, string>> = {
  fr: {
    EMAIL_EMPTY: "Indiquez votre adresse email.",
    EMAIL_MALFORMED: "Cette adresse email n'est pas valide.",
    PASSWORD_EMPTY: "Choisissez un mot de passe.",
    PASSWORD_TOO_SHORT: `Votre mot de passe doit faire au moins ${LONGUEUR_MIN_MOT_DE_PASSE} caractères.`,
    PASSWORD_TOO_LONG: "Ce mot de passe est trop long.",
    RATE_LIMIT_EXCEEDED:
      "Trop de tentatives depuis cette connexion. Réessayez dans une heure.",
    SERVICE_UNAVAILABLE: "Le service est momentanément indisponible. Réessayez.",
    INCONNU: "L'inscription n'a pas abouti. Réessayez.",
    HORS_LIGNE:
      "Aucune connexion. L'inscription a besoin du réseau et n'est pas mise en attente.",
  },
  nl: {
    EMAIL_EMPTY: "Geef uw e-mailadres op.",
    EMAIL_MALFORMED: "Dit e-mailadres is niet geldig.",
    PASSWORD_EMPTY: "Kies een wachtwoord.",
    PASSWORD_TOO_SHORT: `Uw wachtwoord moet minstens ${LONGUEUR_MIN_MOT_DE_PASSE} tekens bevatten.`,
    PASSWORD_TOO_LONG: "Dit wachtwoord is te lang.",
    RATE_LIMIT_EXCEEDED:
      "Te veel pogingen vanaf deze verbinding. Probeer het over een uur opnieuw.",
    SERVICE_UNAVAILABLE: "De dienst is tijdelijk niet beschikbaar. Probeer opnieuw.",
    INCONNU: "De registratie is mislukt. Probeer opnieuw.",
    HORS_LIGNE:
      "Geen verbinding. Registreren vereist het netwerk en wordt niet in wachtrij gezet.",
  },
  en: {
    EMAIL_EMPTY: "Enter your email address.",
    EMAIL_MALFORMED: "This email address is not valid.",
    PASSWORD_EMPTY: "Choose a password.",
    PASSWORD_TOO_SHORT: `Your password must be at least ${LONGUEUR_MIN_MOT_DE_PASSE} characters.`,
    PASSWORD_TOO_LONG: "This password is too long.",
    RATE_LIMIT_EXCEEDED: "Too many attempts from this connection. Try again in an hour.",
    SERVICE_UNAVAILABLE: "The service is temporarily unavailable. Please retry.",
    INCONNU: "Sign-up did not go through. Please retry.",
    HORS_LIGNE: "No connection. Sign-up needs the network and is not queued.",
  },
};

/**
 * Message de succès. Volontairement ambigu sur l'existence du compte.
 *
 * Écrire « compte créé, vérifiez vos emails » dirait à un visiteur qu'aucun
 * compte n'existait sur cette adresse, et ruinerait côté interface ce que le
 * backend prend soin de ne pas révéler.
 */
const SUCCES: Record<LocaleKlaar, string> = {
  fr: "C'est noté. Si cette adresse peut être utilisée, un courriel vient d'y être envoyé. Le lien qu'il contient est valable une heure.",
  nl: "Genoteerd. Als dit adres gebruikt kan worden, is er zojuist een e-mail verstuurd. De link is een uur geldig.",
  en: "Noted. If this address can be used, an email has just been sent to it. The link is valid for one hour.",
};

export function messageSucces(locale: LocaleKlaar): string {
  return SUCCES[locale];
}

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = MESSAGES[locale];
  return table[code as CodeErreurInscription] ?? table.INCONNU;
}

/** Extrait le code d'une réponse d'erreur, sans faire confiance à sa forme. */
export function codeDepuisErreur(erreur: unknown): CodeErreurInscription {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurInscription;
  } catch {
    // Corps non JSON : un proxy ou une passerelle a répondu à la place de
    // l'API. Le message générique vaut mieux que d'afficher du HTML brut.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

/** Réduit une étiquette de langue (`fr-BE`, `NL`) à une locale Klaar. */
function normaliser(etiquette: string | undefined | null): LocaleKlaar | null {
  const brut = (etiquette ?? "").slice(0, 2).toLowerCase();
  return brut === "fr" || brut === "nl" || brut === "en" ? brut : null;
}

/**
 * Langue de la page, et donc des messages et du courriel.
 *
 * Lue sur `<html lang>` et **non** sur `navigator.language`. La version
 * précédente suivait le navigateur, ce qui affichait un refus en anglais au
 * milieu d'une page écrite en français — constaté en exécutant les tests
 * Playwright, dont le Chromium annonce `en-US`.
 *
 * Le repli sur le navigateur ne sert que si la page ne déclare rien, ce que le
 * gabarit fait toujours. Limite connue : il n'y a pas encore de sélecteur de
 * langue, la coquille Astro est en français, donc `fr` en pratique. Les trois
 * traductions existent et suivront le jour où la coquille sera traduite
 * (FR-043), plutôt que d'être écrites en catastrophe ce jour-là.
 */
export function localeAffichee(): LocaleKlaar {
  if (typeof document !== "undefined") {
    const declaree = normaliser(document.documentElement?.lang);
    if (declaree) return declaree;
  }
  if (typeof navigator !== "undefined") {
    const preferee = normaliser(navigator.language);
    if (preferee) return preferee;
  }
  return "fr";
}

/**
 * Envoie la demande d'inscription.
 *
 * Ne passe **pas** par la queue hors-ligne, contrairement aux autres écritures.
 * Une inscription rejouée une heure plus tard aboutirait pendant que
 * l'utilisateur a depuis longtemps quitté la page, sans jamais voir le
 * courriel de vérification arriver dans l'heure qui suit. Mieux vaut le dire
 * franchement que mettre en file une action qui expire.
 */
export async function inscrire(demande: DemandeInscription): Promise<ReponseInscription> {
  return request<ReponseInscription>("/auth/signup", {
    method: "POST",
    body: demande,
  });
}
