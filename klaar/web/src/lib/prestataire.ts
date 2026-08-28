/**
 * Espace prestataire : Demandes reçues, acceptation, suivi de Mission
 * (Story 4.10, FR-013, FR-018).
 *
 * **Ce que le prestataire voit avant d'accepter, et ce qu'il voit après.**
 * Avant : le secteur, la description, l'urgence, une distance. Pas l'adresse.
 * Après : l'adresse, parce qu'il doit s'y rendre. L'API applique cette règle ;
 * ce module ne fait que la refléter, et les deux types le disent — `Proposee`
 * n'a pas de champ de position.
 */
import { ApiError, OfflineError, request } from "./api";
import { jetonAcces } from "./connexion";
import type { LocaleKlaar } from "./inscription";

export interface Proposee {
  id: string;
  secteur: string;
  description: string;
  urgence: "LOW" | "NORMAL" | "HIGH";
  distance_metres: number;
  secondes_restantes: number;
}

export interface Mission {
  id: string;
  statut: StatutMission;
  secteur: string;
  description: string;
  urgence: "LOW" | "NORMAL" | "HIGH";
  latitude: number;
  longitude: number;
  /** Statuts atteignables, rendus par le serveur. */
  suites: StatutMission[];
}

export type StatutMission =
  | "ACCEPTED"
  | "PROVIDER_EN_ROUTE"
  | "ON_SITE"
  | "COMPLETED"
  | "CANCELLED";

export interface MissionAttribuee {
  id: string;
  demande_id: string;
  statut: StatutMission;
  code: string;
  autres_prevenus: number;
}

/** Libellé du bouton qui mène à ce statut. */
export function libelleTransition(statut: StatutMission): string {
  switch (statut) {
    case "PROVIDER_EN_ROUTE":
      return "Je pars";
    case "ON_SITE":
      return "Je suis arrivé";
    case "COMPLETED":
      return "L'intervention est terminée";
    case "CANCELLED":
      return "Annuler";
    default:
      return statut;
  }
}

/** Ce que le statut courant veut dire, en clair. */
export function libelleStatut(statut: StatutMission): string {
  switch (statut) {
    case "ACCEPTED":
      return "Acceptée, pas encore commencée";
    case "PROVIDER_EN_ROUTE":
      return "En route";
    case "ON_SITE":
      return "Sur place";
    case "COMPLETED":
      return "Terminée";
    case "CANCELLED":
      return "Annulée";
    default:
      return statut;
  }
}

export function libelleUrgence(urgence: string): string {
  switch (urgence) {
    case "HIGH":
      return "tout de suite";
    case "NORMAL":
      return "dans la journée";
    case "LOW":
      return "peut attendre";
    default:
      return urgence;
  }
}

/**
 * Distance arrondie, comme dans les notifications.
 *
 * À la centaine de mètres sous le kilomètre. Au mètre près, croisée avec la
 * position du prestataire, elle situerait le demandeur chez lui — et l'API
 * l'arrondit déjà, mais l'afficher brute donnerait une précision que la mesure
 * n'a pas.
 */
export function distanceLisible(metres: number): string {
  if (metres < 1000) return `${Math.round(metres / 100) * 100} m`;
  return `${(metres / 1000).toFixed(1)} km`;
}

export type CodeErreurPrestataire =
  | "NOT_A_PROVIDER"
  | "PROVIDER_NOT_ELIGIBLE"
  | "PROVIDER_BUSY"
  | "REQUEST_ALREADY_MATCHED"
  | "REQUEST_EXPIRED"
  | "REQUEST_CANCELLED"
  | "REQUEST_NOT_FOUND"
  | "MISSION_NOT_FOUND"
  | "INVALID_TRANSITION"
  | "TIMESTAMP_IMPLAUSIBLE"
  | "RATE_LIMIT_EXCEEDED"
  | "AUTH_MISSING"
  | "AUTH_INVALID"
  | "SERVICE_UNAVAILABLE"
  | "INCONNU"
  | "HORS_LIGNE";

const MESSAGES: Record<LocaleKlaar, Record<CodeErreurPrestataire, string>> = {
  fr: {
    NOT_A_PROVIDER: "Ce compte n'est pas un compte prestataire.",
    PROVIDER_NOT_ELIGIBLE:
      "Vous ne pouvez pas prendre cette Demande : compte suspendu, en attente de contrôle, ou secteur non couvert.",
    PROVIDER_BUSY: "Vous avez déjà une intervention en cours. Terminez-la d'abord.",
    REQUEST_ALREADY_MATCHED: "Un autre prestataire a été plus rapide.",
    REQUEST_EXPIRED: "Cette Demande n'est plus ouverte.",
    REQUEST_CANCELLED: "Le demandeur a retiré sa Demande.",
    REQUEST_NOT_FOUND: "Cette Demande n'existe pas.",
    MISSION_NOT_FOUND: "Cette intervention n'existe pas.",
    INVALID_TRANSITION: "Cette étape n'est pas possible depuis l'état actuel.",
    TIMESTAMP_IMPLAUSIBLE: "L'heure déclarée est trop éloignée de l'heure du serveur.",
    RATE_LIMIT_EXCEEDED: "Trop de tentatives. Patientez un instant.",
    AUTH_MISSING: "Votre session a expiré. Reconnectez-vous.",
    AUTH_INVALID: "Votre session a expiré. Reconnectez-vous.",
    SERVICE_UNAVAILABLE: "Le service est momentanément indisponible. Réessayez.",
    INCONNU: "L'opération n'a pas abouti. Réessayez.",
    HORS_LIGNE: "Aucune connexion. Cette opération a besoin du réseau.",
  },
  nl: {
    NOT_A_PROVIDER: "Dit account is geen vakman-account.",
    PROVIDER_NOT_ELIGIBLE:
      "U kunt deze aanvraag niet aannemen: account geschorst, in afwachting van controle, of sector niet gedekt.",
    PROVIDER_BUSY: "U heeft al een lopende interventie. Rond die eerst af.",
    REQUEST_ALREADY_MATCHED: "Een andere vakman was sneller.",
    REQUEST_EXPIRED: "Deze aanvraag is niet meer open.",
    REQUEST_CANCELLED: "De aanvrager heeft de aanvraag ingetrokken.",
    REQUEST_NOT_FOUND: "Deze aanvraag bestaat niet.",
    MISSION_NOT_FOUND: "Deze interventie bestaat niet.",
    INVALID_TRANSITION: "Deze stap is niet mogelijk vanuit de huidige toestand.",
    TIMESTAMP_IMPLAUSIBLE: "Het opgegeven tijdstip ligt te ver van de servertijd.",
    RATE_LIMIT_EXCEEDED: "Te veel pogingen. Even geduld.",
    AUTH_MISSING: "Uw sessie is verlopen. Meld u opnieuw aan.",
    AUTH_INVALID: "Uw sessie is verlopen. Meld u opnieuw aan.",
    SERVICE_UNAVAILABLE: "De dienst is tijdelijk niet beschikbaar. Probeer opnieuw.",
    INCONNU: "De bewerking is mislukt. Probeer opnieuw.",
    HORS_LIGNE: "Geen verbinding. Deze bewerking vereist het netwerk.",
  },
  en: {
    NOT_A_PROVIDER: "This account is not a provider account.",
    PROVIDER_NOT_ELIGIBLE:
      "You cannot take this request: account suspended, awaiting checks, or sector not covered.",
    PROVIDER_BUSY: "You already have a job in progress. Finish it first.",
    REQUEST_ALREADY_MATCHED: "Another provider was faster.",
    REQUEST_EXPIRED: "This request is no longer open.",
    REQUEST_CANCELLED: "The requester withdrew it.",
    REQUEST_NOT_FOUND: "This request does not exist.",
    MISSION_NOT_FOUND: "This job does not exist.",
    INVALID_TRANSITION: "That step is not possible from the current state.",
    TIMESTAMP_IMPLAUSIBLE: "The declared time is too far from server time.",
    RATE_LIMIT_EXCEEDED: "Too many attempts. Please wait a moment.",
    AUTH_MISSING: "Your session has expired. Sign in again.",
    AUTH_INVALID: "Your session has expired. Sign in again.",
    SERVICE_UNAVAILABLE: "The service is temporarily unavailable. Please retry.",
    INCONNU: "The operation did not go through. Please retry.",
    HORS_LIGNE: "No connection. This operation needs the network.",
  },
};

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = MESSAGES[locale];
  return table[code as CodeErreurPrestataire] ?? table.INCONNU;
}

export function codeDepuisErreur(erreur: unknown): CodeErreurPrestataire {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurPrestataire;
  } catch {
    // Réponse d'une passerelle plutôt que de l'API.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

function autorisation(): Record<string, string> {
  const jeton = jetonAcces();
  return jeton ? { Authorization: `Bearer ${jeton}` } : {};
}

export async function demandesRecues(): Promise<Proposee[]> {
  return request<Proposee[]>("/providers/me/requests", { headers: autorisation() });
}

export async function accepter(demandeId: string): Promise<MissionAttribuee> {
  return request<MissionAttribuee>(`/requests/${demandeId}/accept`, {
    method: "POST",
    headers: autorisation(),
  });
}

export async function lireMission(missionId: string): Promise<Mission> {
  return request<Mission>(`/missions/${missionId}`, { headers: autorisation() });
}

export async function avancerMission(
  missionId: string,
  statut: StatutMission,
  position?: { latitude: number; longitude: number },
): Promise<{ statut: StatutMission; hors_zone: boolean }> {
  return request(`/missions/${missionId}/status`, {
    method: "PATCH",
    body: { statut, ...(position ?? {}) },
    headers: autorisation(),
  });
}
