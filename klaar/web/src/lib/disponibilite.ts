/**
 * Disponibilité du prestataire (Story 3.7).
 *
 * Trois notions distinctes que l'interface ne doit pas confondre : le statut
 * (contrôle d'entreprise), la disponibilité (« je suis en congé »), et
 * l'occupation (une Mission en cours). Seule la deuxième se règle ici ; les
 * deux autres s'affichent, parce qu'un prestataire en service et pourtant
 * jamais sollicité doit pouvoir comprendre pourquoi.
 */
import { ApiError, OfflineError, request } from "./api";
import { jetonAcces } from "./connexion";
import type { LocaleKlaar } from "./inscription";
import { t } from "./i18n";

/** Bornes du rayon d'intervention, alignées sur le domaine. */
export const RAYON_MIN_METRES = 1_000;
export const RAYON_MAX_METRES = 20_000;

export type StatutPrestataire = "PENDING_KYC" | "ACTIVE" | "SUSPENDED";

export interface Disponibilite {
  provider_id: string;
  statut: StatutPrestataire;
  disponible: boolean;
  rayon_intervention_metres: number;
  occupe: boolean;
  sollicitable: boolean;
}

export type CodeErreurDisponibilite =
  | "NOT_A_PROVIDER"
  | "SERVICE_RADIUS_OUT_OF_RANGE"
  | "AUTH_MISSING"
  | "AUTH_INVALID"
  | "SERVICE_UNAVAILABLE"
  | "INCONNU"
  | "HORS_LIGNE";

const MESSAGES: Record<LocaleKlaar, Record<CodeErreurDisponibilite, string>> = {
  fr: {
    NOT_A_PROVIDER: "Ce compte n'est pas un compte prestataire.",
    SERVICE_RADIUS_OUT_OF_RANGE: `Choisissez un rayon entre ${RAYON_MIN_METRES / 1000} et ${RAYON_MAX_METRES / 1000} km.`,
    AUTH_MISSING: "Votre session a expiré. Reconnectez-vous.",
    AUTH_INVALID: "Votre session a expiré. Reconnectez-vous.",
    SERVICE_UNAVAILABLE: "Le service est momentanément indisponible. Réessayez.",
    INCONNU: "L'opération n'a pas abouti. Réessayez.",
    HORS_LIGNE: "Aucune connexion. Cette opération a besoin du réseau.",
  },
  nl: {
    NOT_A_PROVIDER: "Dit account is geen vakman-account.",
    SERVICE_RADIUS_OUT_OF_RANGE: `Kies een straal tussen ${RAYON_MIN_METRES / 1000} en ${RAYON_MAX_METRES / 1000} km.`,
    AUTH_MISSING: "Uw sessie is verlopen. Meld u opnieuw aan.",
    AUTH_INVALID: "Uw sessie is verlopen. Meld u opnieuw aan.",
    SERVICE_UNAVAILABLE: "De dienst is tijdelijk niet beschikbaar. Probeer opnieuw.",
    INCONNU: "De bewerking is mislukt. Probeer opnieuw.",
    HORS_LIGNE: "Geen verbinding. Deze bewerking vereist het netwerk.",
  },
  en: {
    NOT_A_PROVIDER: "This account is not a provider account.",
    SERVICE_RADIUS_OUT_OF_RANGE: `Choose a radius between ${RAYON_MIN_METRES / 1000} and ${RAYON_MAX_METRES / 1000} km.`,
    AUTH_MISSING: "Your session has expired. Sign in again.",
    AUTH_INVALID: "Your session has expired. Sign in again.",
    SERVICE_UNAVAILABLE: "The service is temporarily unavailable. Please retry.",
    INCONNU: "The operation did not go through. Please retry.",
    HORS_LIGNE: "No connection. This operation needs the network.",
  },
};

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = MESSAGES[locale];
  return table[code as CodeErreurDisponibilite] ?? table.INCONNU;
}

export function codeDepuisErreur(erreur: unknown): CodeErreurDisponibilite {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurDisponibilite;
  } catch {
    // Réponse d'une passerelle plutôt que de l'API.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

/**
 * Pourquoi le prestataire ne reçoit rien, en une phrase.
 *
 * `null` quand il reçoit bien des Demandes. L'ordre des causes est celui sur
 * lequel il peut agir : sa pause d'abord, puisque c'est la seule qu'il lève
 * lui-même ; l'occupation ensuite, qui passera toute seule ; le statut en
 * dernier, qui ne dépend pas de lui.
 */
export function raisonDeSilence(
  etat: Disponibilite,
  locale: LocaleKlaar = "fr",
): string | null {
  if (etat.sollicitable) return null;
  if (etat.statut === "PENDING_KYC") return t(locale, "silence.kyc");
  if (etat.statut === "SUSPENDED") return t(locale, "silence.suspendu");
  if (!etat.disponible) return t(locale, "silence.pause");
  if (etat.occupe) return t(locale, "silence.occupe");
  return null;
}

function autorisation(): Record<string, string> {
  const jeton = jetonAcces();
  return jeton ? { Authorization: `Bearer ${jeton}` } : {};
}

export async function lireDisponibilite(): Promise<Disponibilite> {
  return request<Disponibilite>("/providers/me/availability", {
    headers: autorisation(),
  });
}

export async function reglerDisponibilite(reglage: {
  disponible?: boolean;
  rayon_intervention_metres?: number;
}): Promise<Disponibilite> {
  return request<Disponibilite>("/providers/me/availability", {
    method: "PATCH",
    body: reglage,
    headers: autorisation(),
  });
}
