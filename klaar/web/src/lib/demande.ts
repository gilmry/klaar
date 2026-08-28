/**
 * Soumission d'une Demande (Story 3.1, FR-011).
 */
import { ApiError, OfflineError, request } from "./api";
import { jetonAcces } from "./connexion";
import type { LocaleKlaar } from "./inscription";

export type UrgenceKlaar = "LOW" | "NORMAL" | "HIGH";

export interface DemandeASoumettre {
  secteur: string;
  description: string;
  latitude: number;
  longitude: number;
  urgence: UrgenceKlaar;
}

export interface DemandeCreee {
  id: string;
  statut: string;
  code: "REQUEST_CREATED" | "REQUEST_DUPLICATE";
  /** Prestataires retenus pour notification. */
  candidats?: number;
  /** Appareils réellement joints. Distinct de `candidats`. */
  notifies?: number;
}

export type StatutDemande = "BROADCASTING" | "MATCHED" | "NO_MATCH" | "CANCELLED";

/** L'état d'une Demande, tel que son auteur le voit (Story 4.10). */
export interface SuiviDemande {
  id: string;
  secteur: string;
  description: string;
  urgence: UrgenceKlaar;
  statut: StatutDemande;
  rayon_metres: number;
  elargissements: number;
  /**
   * Le tour est écoulé bien que le statut dise encore « diffusion ».
   *
   * Le balayage passe périodiquement ; sans ce champ, quelqu'un attendrait
   * devant une Demande que plus personne ne peut accepter.
   */
  tour_ecoule: boolean;
  /** Nom de l'entreprise qui vient, une fois l'intervention attribuée. */
  prestataire: string | null;
  mission_id: string | null;
  mission_statut: string | null;
}

/** Motifs d'annulation, vocabulaire fermé (FR-014). */
export const MOTIFS_ANNULATION = [
  { code: "RESOLVED_ITSELF", libelle: "Le problème s'est réglé tout seul" },
  { code: "TOO_SLOW", libelle: "Trop long à venir" },
  { code: "FOUND_ELSEWHERE", libelle: "J'ai trouvé quelqu'un d'autre" },
  { code: "MISTAKE", libelle: "Je me suis trompé" },
  { code: "OTHER", libelle: "Autre" },
] as const;

/** Longueur maximale, alignée sur le domaine. */
export const DESCRIPTION_MAX = 2000;

export type CodeErreurDemande =
  | "SECTOR_NOT_FOUND"
  | "DESCRIPTION_EMPTY"
  | "DESCRIPTION_TOO_LONG"
  | "URGENCY_INVALID"
  | "GEO_OUTSIDE_RBC"
  | "GEO_INVALID"
  | "PAYMENT_METHOD_REQUIRED"
  | "RATE_LIMIT_EXCEEDED"
  | "AUTH_MISSING"
  | "AUTH_INVALID"
  | "SERVICE_UNAVAILABLE"
  | "POSITION_REFUSEE"
  | "REQUEST_NOT_FOUND"
  | "REQUEST_NOT_EXPIRED"
  | "REQUEST_CLOSED"
  | "MAX_RADIUS_REACHED"
  | "ALREADY_MATCHED"
  | "ALREADY_CANCELLED"
  | "INCONNU"
  | "HORS_LIGNE";

const MESSAGES: Record<LocaleKlaar, Record<CodeErreurDemande, string>> = {
  fr: {
    REQUEST_NOT_FOUND: "Cette Demande n'existe pas.",
    REQUEST_NOT_EXPIRED: "Votre Demande est encore diffusée. Laissez-lui trente secondes.",
    REQUEST_CLOSED: "Cette Demande est déjà attribuée ou annulée.",
    MAX_RADIUS_REACHED: "La zone a déjà été élargie trois fois. Votre Demande a été annulée.",
    ALREADY_MATCHED: "Un prestataire a déjà accepté : c'est l'intervention qu'il faut annuler.",
    ALREADY_CANCELLED: "Cette Demande est déjà annulée.",
    SECTOR_NOT_FOUND: "Choisissez un secteur dans la liste.",
    DESCRIPTION_EMPTY: "Décrivez le problème en quelques mots.",
    DESCRIPTION_TOO_LONG: `Votre description dépasse ${DESCRIPTION_MAX} caractères.`,
    URGENCY_INVALID: "Choisissez un niveau d'urgence.",
    GEO_OUTSIDE_RBC:
      "Klaar n'intervient pour l'instant qu'en Région de Bruxelles-Capitale.",
    GEO_INVALID: "Votre position n'a pas pu être lue. Réessayez.",
    PAYMENT_METHOD_REQUIRED:
      "Enregistrez un moyen de paiement avant de faire une demande.",
    RATE_LIMIT_EXCEEDED: "Vous avez atteint la limite de demandes pour cette heure.",
    AUTH_MISSING: "Votre session a expiré. Reconnectez-vous.",
    AUTH_INVALID: "Votre session a expiré. Reconnectez-vous.",
    SERVICE_UNAVAILABLE: "Le service est momentanément indisponible. Réessayez.",
    POSITION_REFUSEE:
      "Sans votre position, aucun prestataire ne peut être averti. Autorisez la localisation pour continuer.",
    INCONNU: "La demande n'a pas abouti. Réessayez.",
    HORS_LIGNE: "Aucune connexion. Une demande de dépannage a besoin du réseau.",
  },
  nl: {
    REQUEST_NOT_FOUND: "Deze aanvraag bestaat niet.",
    REQUEST_NOT_EXPIRED: "Uw aanvraag loopt nog. Geef ze dertig seconden.",
    REQUEST_CLOSED: "Deze aanvraag is al toegewezen of geannuleerd.",
    MAX_RADIUS_REACHED: "De zone is al drie keer vergroot. Uw aanvraag is geannuleerd.",
    ALREADY_MATCHED: "Een vakman heeft al aanvaard: annuleer de interventie.",
    ALREADY_CANCELLED: "Deze aanvraag is al geannuleerd.",
    SECTOR_NOT_FOUND: "Kies een sector uit de lijst.",
    DESCRIPTION_EMPTY: "Beschrijf het probleem in enkele woorden.",
    DESCRIPTION_TOO_LONG: `Uw beschrijving overschrijdt ${DESCRIPTION_MAX} tekens.`,
    URGENCY_INVALID: "Kies een dringendheidsniveau.",
    GEO_OUTSIDE_RBC: "Klaar werkt voorlopig enkel in het Brussels Hoofdstedelijk Gewest.",
    GEO_INVALID: "Uw locatie kon niet worden gelezen. Probeer opnieuw.",
    PAYMENT_METHOD_REQUIRED: "Registreer een betaalmiddel voor u een aanvraag doet.",
    RATE_LIMIT_EXCEEDED: "U hebt de limiet aan aanvragen voor dit uur bereikt.",
    AUTH_MISSING: "Uw sessie is verlopen. Meld u opnieuw aan.",
    AUTH_INVALID: "Uw sessie is verlopen. Meld u opnieuw aan.",
    SERVICE_UNAVAILABLE: "De dienst is tijdelijk niet beschikbaar. Probeer opnieuw.",
    POSITION_REFUSEE:
      "Zonder uw locatie kan geen enkele dienstverlener verwittigd worden. Sta locatie toe om verder te gaan.",
    INCONNU: "De aanvraag is mislukt. Probeer opnieuw.",
    HORS_LIGNE: "Geen verbinding. Een pechverhelping vereist het netwerk.",
  },
  en: {
    REQUEST_NOT_FOUND: "This request does not exist.",
    REQUEST_NOT_EXPIRED: "Your request is still open. Give it thirty seconds.",
    REQUEST_CLOSED: "This request is already assigned or cancelled.",
    MAX_RADIUS_REACHED: "The area was already widened three times. Your request was cancelled.",
    ALREADY_MATCHED: "A provider already accepted: cancel the job instead.",
    ALREADY_CANCELLED: "This request is already cancelled.",
    SECTOR_NOT_FOUND: "Choose a sector from the list.",
    DESCRIPTION_EMPTY: "Describe the problem in a few words.",
    DESCRIPTION_TOO_LONG: `Your description exceeds ${DESCRIPTION_MAX} characters.`,
    URGENCY_INVALID: "Choose an urgency level.",
    GEO_OUTSIDE_RBC: "Klaar currently operates only in the Brussels-Capital Region.",
    GEO_INVALID: "Your location could not be read. Please retry.",
    PAYMENT_METHOD_REQUIRED: "Register a payment method before making a request.",
    RATE_LIMIT_EXCEEDED: "You have reached the request limit for this hour.",
    AUTH_MISSING: "Your session has expired. Sign in again.",
    AUTH_INVALID: "Your session has expired. Sign in again.",
    SERVICE_UNAVAILABLE: "The service is temporarily unavailable. Please retry.",
    POSITION_REFUSEE:
      "Without your location, no provider can be alerted. Allow location access to continue.",
    INCONNU: "The request did not go through. Please retry.",
    HORS_LIGNE: "No connection. A callout request needs the network.",
  },
};

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = MESSAGES[locale];
  return table[code as CodeErreurDemande] ?? table.INCONNU;
}

export function codeDepuisErreur(erreur: unknown): CodeErreurDemande {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurDemande;
  } catch {
    // Réponse d'une passerelle plutôt que de l'API.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

/**
 * Position du navigateur.
 *
 * **Bloquante, contrairement au reste de l'application.** Ailleurs, la
 * géolocalisation est un confort dont l'absence se contourne ; ici, elle est la
 * donnée sans laquelle personne ne peut être envoyé. Mieux vaut le dire que
 * soumettre une demande que rien n'atteindra.
 */
export function positionActuelle(): Promise<GeolocationPosition> {
  return new Promise((resolve, reject) => {
    if (typeof navigator === "undefined" || !navigator.geolocation) {
      reject(new Error("POSITION_REFUSEE"));
      return;
    }
    navigator.geolocation.getCurrentPosition(resolve, () => reject(new Error("POSITION_REFUSEE")), {
      enableHighAccuracy: true,
      // Dix secondes : au-delà, l'utilisateur croit que l'application a planté.
      timeout: 10_000,
      // Une position vieille d'une minute reste bonne pour un dépannage ; la
      // refuser ferait rallumer le GPS pour rien.
      maximumAge: 60_000,
    });
  });
}

/**
 * Ne passe **pas** par la file hors-ligne.
 *
 * Une demande de dépannage rejouée deux heures plus tard enverrait un
 * prestataire chez quelqu'un dont la fuite est réparée depuis longtemps. Le
 * refus franc vaut mieux que la promesse tenue trop tard.
 */
export async function soumettreDemande(demande: DemandeASoumettre): Promise<DemandeCreee> {
  const jeton = jetonAcces();
  return request<DemandeCreee>("/requests", {
    method: "POST",
    body: demande,
    headers: jeton ? { Authorization: `Bearer ${jeton}` } : {},
  });
}

/** Ce que le statut veut dire, pour celui qui attend. */
export function libelleStatutDemande(suivi: SuiviDemande): string {
  if (suivi.statut === "MATCHED") {
    return suivi.prestataire
      ? `${suivi.prestataire} a pris votre demande`
      : "Un prestataire a pris votre demande";
  }
  if (suivi.statut === "CANCELLED") return "Demande annulée";
  if (suivi.statut === "NO_MATCH") return "Personne n'a répondu";
  // `BROADCASTING` recouvre deux situations que le demandeur ne doit pas
  // confondre : le tour court encore, ou il est écoulé et le balayage n'est
  // pas passé. Dire « en cours » dans le second cas ferait attendre pour rien.
  return suivi.tour_ecoule
    ? "Personne n'a répondu"
    : "Recherche d'un prestataire en cours…";
}

/** Ce que l'intervention en est, une fois attribuée. */
export function libelleMission(statut: string | null): string | null {
  switch (statut) {
    case "ACCEPTED":
      return "Acceptée, le prestataire va partir";
    case "PROVIDER_EN_ROUTE":
      return "Le prestataire est en route";
    case "ON_SITE":
      return "Le prestataire est arrivé";
    case "COMPLETED":
      return "Intervention terminée";
    case "CANCELLED":
      return "Intervention annulée";
    default:
      return null;
  }
}

/** Vrai si le demandeur peut encore élargir la zone (FR-015). */
export function peutElargir(suivi: SuiviDemande): boolean {
  const attend = suivi.statut === "NO_MATCH" || (suivi.statut === "BROADCASTING" && suivi.tour_ecoule);
  return attend && suivi.elargissements < 3;
}

/** Vrai si le demandeur peut encore retirer sa Demande (FR-014). */
export function peutAnnuler(suivi: SuiviDemande): boolean {
  return suivi.statut === "BROADCASTING" || suivi.statut === "NO_MATCH";
}

function autorisationSuivi(): Record<string, string> {
  const jeton = jetonAcces();
  return jeton ? { Authorization: `Bearer ${jeton}` } : {};
}

export async function suivreDemande(id: string): Promise<SuiviDemande> {
  return request<SuiviDemande>(`/requests/${id}`, { headers: autorisationSuivi() });
}

export async function elargirZone(id: string): Promise<SuiviDemande> {
  await request(`/requests/${id}/expand-radius`, {
    method: "POST",
    headers: autorisationSuivi(),
  });
  return suivreDemande(id);
}

export async function annulerDemande(id: string, motif?: string): Promise<void> {
  const suffixe = motif ? `?motif=${encodeURIComponent(motif)}` : "";
  await request(`/requests/${id}${suffixe}`, {
    method: "DELETE",
    headers: autorisationSuivi(),
  });
}
