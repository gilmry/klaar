/**
 * Compte de l'utilisateur connecté : effacement RGPD (Story 1.9, FR-005).
 */
import { ApiError, OfflineError, request } from "./api";
import { jetonAcces } from "./connexion";
import type { LocaleKlaar } from "./inscription";

/** Mot à reproduire pour confirmer, aligné sur le backend. */
export const MOT_DE_CONFIRMATION = "DELETE";

export interface EffacementProgramme {
  code: "ERASURE_SCHEDULED" | "ERASURE_ALREADY_SCHEDULED";
  dans_jours: number;
}

export type CodeErreurCompte =
  | "CONFIRMATION_REQUIRED"
  | "ACCOUNT_NOT_FOUND"
  | "NO_ERASURE_PENDING"
  | "AUTH_MISSING"
  | "AUTH_INVALID"
  | "SERVICE_UNAVAILABLE"
  | "INCONNU"
  | "HORS_LIGNE";

const MESSAGES: Record<LocaleKlaar, Record<CodeErreurCompte, string>> = {
  fr: {
    CONFIRMATION_REQUIRED: `Recopiez exactement ${MOT_DE_CONFIRMATION} pour confirmer.`,
    ACCOUNT_NOT_FOUND: "Ce compte n'existe plus.",
    NO_ERASURE_PENDING: "Aucun effacement n'est en attente.",
    AUTH_MISSING: "Votre session a expiré. Reconnectez-vous.",
    AUTH_INVALID: "Votre session a expiré. Reconnectez-vous.",
    SERVICE_UNAVAILABLE: "Le service est momentanément indisponible. Réessayez.",
    INCONNU: "L'opération n'a pas abouti. Réessayez.",
    HORS_LIGNE: "Aucune connexion. Cette opération a besoin du réseau.",
  },
  nl: {
    CONFIRMATION_REQUIRED: `Typ exact ${MOT_DE_CONFIRMATION} om te bevestigen.`,
    ACCOUNT_NOT_FOUND: "Dit account bestaat niet meer.",
    NO_ERASURE_PENDING: "Er is geen verwijdering in behandeling.",
    AUTH_MISSING: "Uw sessie is verlopen. Meld u opnieuw aan.",
    AUTH_INVALID: "Uw sessie is verlopen. Meld u opnieuw aan.",
    SERVICE_UNAVAILABLE: "De dienst is tijdelijk niet beschikbaar. Probeer opnieuw.",
    INCONNU: "De bewerking is mislukt. Probeer opnieuw.",
    HORS_LIGNE: "Geen verbinding. Deze bewerking vereist het netwerk.",
  },
  en: {
    CONFIRMATION_REQUIRED: `Type ${MOT_DE_CONFIRMATION} exactly to confirm.`,
    ACCOUNT_NOT_FOUND: "This account no longer exists.",
    NO_ERASURE_PENDING: "No erasure is pending.",
    AUTH_MISSING: "Your session has expired. Sign in again.",
    AUTH_INVALID: "Your session has expired. Sign in again.",
    SERVICE_UNAVAILABLE: "The service is temporarily unavailable. Please retry.",
    INCONNU: "The operation did not go through. Please retry.",
    HORS_LIGNE: "No connection. This operation needs the network.",
  },
};

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = MESSAGES[locale];
  return table[code as CodeErreurCompte] ?? table.INCONNU;
}

export function codeDepuisErreur(erreur: unknown): CodeErreurCompte {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurCompte;
  } catch {
    // Réponse d'une passerelle plutôt que de l'API.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

/**
 * En-tête d'autorisation.
 *
 * Le jeton est lu à l'appel et non conservé ici : une copie de plus serait une
 * copie de plus à oublier au moment de la déconnexion.
 */
function autorisation(): Record<string, string> {
  const jeton = jetonAcces();
  return jeton ? { Authorization: `Bearer ${jeton}` } : {};
}

export async function demanderEffacement(confirmation: string): Promise<EffacementProgramme> {
  return request<EffacementProgramme>("/me/erase", {
    method: "POST",
    body: { confirmation },
    headers: autorisation(),
  });
}

export async function annulerEffacement(): Promise<void> {
  return request<void>("/me/erase/cancel", { method: "POST", headers: autorisation() });
}
