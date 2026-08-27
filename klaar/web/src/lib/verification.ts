/**
 * Vérification d'adresse (Story 1.2, FR-001).
 *
 * Le lien du courriel ouvre cette page, qui présente ensuite le jeton par un
 * `POST`. L'ouverture de la page ne consomme rien : les passerelles de
 * messagerie d'entreprise visitent les liens avant leur destinataire, et un
 * `GET` consommant le jeton serait consommé par l'antivirus.
 */
import { ApiError, OfflineError, request } from "./api";
import type { LocaleKlaar } from "./inscription";

export type CodeVerification = "EMAIL_VERIFIED" | "EMAIL_ALREADY_VERIFIED";

export interface ReponseVerification {
  code: CodeVerification;
}

export type CodeErreurVerification =
  | "TOKEN_MISSING"
  | "TOKEN_INVALID"
  | "TOKEN_EXPIRED"
  | "SERVICE_UNAVAILABLE"
  | "INCONNU"
  | "HORS_LIGNE";

const SUCCES: Record<LocaleKlaar, Record<CodeVerification, string>> = {
  fr: {
    EMAIL_VERIFIED: "Votre adresse est confirmée. Votre compte est actif.",
    // Formulation identique dans son effet : la personne veut savoir si son
    // compte marche, pas combien de fois elle a cliqué.
    EMAIL_ALREADY_VERIFIED: "Votre adresse était déjà confirmée. Votre compte est actif.",
  },
  nl: {
    EMAIL_VERIFIED: "Uw adres is bevestigd. Uw account is actief.",
    EMAIL_ALREADY_VERIFIED: "Uw adres was al bevestigd. Uw account is actief.",
  },
  en: {
    EMAIL_VERIFIED: "Your address is confirmed. Your account is active.",
    EMAIL_ALREADY_VERIFIED: "Your address was already confirmed. Your account is active.",
  },
};

const ERREURS: Record<LocaleKlaar, Record<CodeErreurVerification, string>> = {
  fr: {
    TOKEN_MISSING: "Ce lien est incomplet. Rouvrez celui reçu par courriel.",
    TOKEN_INVALID: "Ce lien n'est pas valide. Rouvrez celui reçu par courriel.",
    TOKEN_EXPIRED:
      "Ce lien a dépassé son heure de validité. Recommencez l'inscription pour en recevoir un nouveau.",
    SERVICE_UNAVAILABLE: "Le service est momentanément indisponible. Réessayez.",
    INCONNU: "La confirmation n'a pas abouti. Réessayez.",
    HORS_LIGNE: "Aucune connexion. Rouvrez ce lien une fois le réseau revenu.",
  },
  nl: {
    TOKEN_MISSING: "Deze link is onvolledig. Open opnieuw de link uit uw e-mail.",
    TOKEN_INVALID: "Deze link is niet geldig. Open opnieuw de link uit uw e-mail.",
    TOKEN_EXPIRED:
      "Deze link is vervallen. Registreer opnieuw om een nieuwe link te ontvangen.",
    SERVICE_UNAVAILABLE: "De dienst is tijdelijk niet beschikbaar. Probeer opnieuw.",
    INCONNU: "De bevestiging is mislukt. Probeer opnieuw.",
    HORS_LIGNE: "Geen verbinding. Open deze link opnieuw zodra u online bent.",
  },
  en: {
    TOKEN_MISSING: "This link is incomplete. Open the one from your email again.",
    TOKEN_INVALID: "This link is not valid. Open the one from your email again.",
    TOKEN_EXPIRED: "This link has expired. Sign up again to receive a new one.",
    SERVICE_UNAVAILABLE: "The service is temporarily unavailable. Please retry.",
    INCONNU: "Confirmation did not go through. Please retry.",
    HORS_LIGNE: "No connection. Open this link again once you are back online.",
  },
};

export function messageSucces(locale: LocaleKlaar, code: CodeVerification): string {
  return SUCCES[locale][code] ?? SUCCES[locale].EMAIL_VERIFIED;
}

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = ERREURS[locale];
  return table[code as CodeErreurVerification] ?? table.INCONNU;
}

export function codeDepuisErreur(erreur: unknown): CodeErreurVerification {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurVerification;
  } catch {
    // Réponse d'une passerelle plutôt que de l'API.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

/**
 * Lit le jeton dans l'URL.
 *
 * Astro produit un site statique : il n'y a pas de route paramétrée côté
 * serveur, la valeur voyage donc en chaîne de requête et se lit ici. Le jeton
 * n'est jamais réécrit dans le DOM ni conservé.
 */
export function jetonDepuisUrl(url: string): string {
  try {
    return new URL(url).searchParams.get("jeton")?.trim() ?? "";
  } catch {
    return "";
  }
}

export async function verifier(jeton: string): Promise<ReponseVerification> {
  return request<ReponseVerification>("/auth/verify-email", {
    method: "POST",
    body: { jeton },
  });
}
