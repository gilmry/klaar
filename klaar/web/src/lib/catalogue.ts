/**
 * Catalogue des secteurs et Skills (Story 2.2, FR-008).
 *
 * Lecture publique : aucun jeton, et une réponse mise en cache cinq minutes par
 * le navigateur comme par tout intermédiaire. Ce module ne cache rien de son
 * côté — le faire dupliquerait une politique déjà portée par les en-têtes HTTP,
 * et les deux finiraient par diverger.
 */
import { ApiError, OfflineError, request } from "./api";
import type { LocaleKlaar } from "./inscription";

export interface SkillCatalogue {
  code: string;
  libelle: string;
}

export interface FourchettePrix {
  min_cents: number;
  max_cents: number;
}

export interface SecteurCatalogue {
  code: string;
  libelle: string;
  /** Absente tant que l'historique ne permet pas d'en publier une (FR-009). */
  fourchette?: FourchettePrix;
  skills: SkillCatalogue[];
}

export interface Catalogue {
  locale: LocaleKlaar;
  /** `LOCALE_FALLBACK` quand la langue demandée n'est pas prise en charge. */
  avertissement?: string;
  secteurs: SecteurCatalogue[];
}

export type CodeErreurCatalogue =
  | "CATALOG_MAINTENANCE"
  | "RATE_LIMIT_EXCEEDED"
  | "SERVICE_UNAVAILABLE"
  | "INCONNU"
  | "HORS_LIGNE";

const MESSAGES: Record<LocaleKlaar, Record<CodeErreurCatalogue, string>> = {
  fr: {
    CATALOG_MAINTENANCE: "Le catalogue est en cours de mise à jour. Réessayez dans une minute.",
    RATE_LIMIT_EXCEEDED: "Trop de chargements depuis cette connexion. Patientez un instant.",
    SERVICE_UNAVAILABLE: "Le catalogue est momentanément indisponible. Réessayez.",
    INCONNU: "Le catalogue n'a pas pu être chargé.",
    HORS_LIGNE: "Aucune connexion. Le catalogue s'affichera au retour du réseau.",
  },
  nl: {
    CATALOG_MAINTENANCE: "De catalogus wordt bijgewerkt. Probeer het over een minuut opnieuw.",
    RATE_LIMIT_EXCEEDED: "Te veel aanvragen vanaf deze verbinding. Even geduld.",
    SERVICE_UNAVAILABLE: "De catalogus is tijdelijk niet beschikbaar. Probeer opnieuw.",
    INCONNU: "De catalogus kon niet worden geladen.",
    HORS_LIGNE: "Geen verbinding. De catalogus verschijnt zodra u online bent.",
  },
  en: {
    CATALOG_MAINTENANCE: "The catalogue is being updated. Try again in a minute.",
    RATE_LIMIT_EXCEEDED: "Too many loads from this connection. Please wait a moment.",
    SERVICE_UNAVAILABLE: "The catalogue is temporarily unavailable. Please retry.",
    INCONNU: "The catalogue could not be loaded.",
    HORS_LIGNE: "No connection. The catalogue will appear once you are back online.",
  },
};

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = MESSAGES[locale];
  return table[code as CodeErreurCatalogue] ?? table.INCONNU;
}

export function codeDepuisErreur(erreur: unknown): CodeErreurCatalogue {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurCatalogue;
  } catch {
    // Réponse d'une passerelle plutôt que de l'API.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

export async function chargerCatalogue(locale: LocaleKlaar): Promise<Catalogue> {
  return request<Catalogue>(`/catalog/sectors?locale=${encodeURIComponent(locale)}`);
}

/**
 * Met en forme une fourchette, ou dit qu'il n'y en a pas.
 *
 * L'absence se traduit par « prix sur devis » et non par un blanc : un blanc
 * laisse croire à un défaut d'affichage, alors que l'absence de fourchette est
 * une information — il n'y a pas encore assez d'interventions pour en publier
 * une (FR-009 `@negative`).
 */
export function formaterFourchette(
  locale: LocaleKlaar,
  fourchette: FourchettePrix | undefined,
): string {
  if (!fourchette) {
    return { fr: "Prix sur devis", nl: "Prijs op aanvraag", en: "Price on request" }[locale];
  }
  const euros = (cents: number) =>
    new Intl.NumberFormat(`${locale}-BE`, {
      style: "currency",
      currency: "EUR",
      // Les tarifs de dépannage se donnent en euros ronds ; afficher
      // « 80,00 € – 200,00 € » suggère une précision que la fourchette n'a pas.
      maximumFractionDigits: 0,
    }).format(cents / 100);
  return `${euros(fourchette.min_cents)} – ${euros(fourchette.max_cents)}`;
}

/**
 * Avertissement qui doit accompagner toute fourchette (FR-009 `@happy`).
 *
 * Non facultatif : sans lui, une fourchette indicative se lit comme un devis,
 * et l'écart avec le prix réellement facturé devient un litige.
 */
export function mentionPrix(locale: LocaleKlaar): string {
  return {
    fr: "Prix indicatif. Le prix final est fixé par le prestataire.",
    nl: "Richtprijs. De uiteindelijke prijs wordt door de dienstverlener bepaald.",
    en: "Indicative price. The final price is set by the provider.",
  }[locale];
}
