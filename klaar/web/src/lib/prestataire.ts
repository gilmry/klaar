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
  /** Dernier devis envoyé, quel que soit son statut (FR-016). */
  devis: Devis | null;
  /** Devis encore envoyables avant que le plafond n'annule la Mission. */
  devis_restants: number;
}

/**
 * Un devis, tel que le serveur le rend.
 *
 * **Tous les montants sont des entiers de centimes.** Les manipuler en euros
 * flottants ferait apparaître des 217,79999999999998 sur un document
 * contractuel ; la conversion n'a lieu qu'à l'affichage et à la saisie.
 */
export interface Devis {
  id: string;
  montant_htva_cents: number;
  taux_tva_bp: number;
  tva_cents: number;
  total_ttc_cents: number;
  delai_minutes: number;
  note: string | null;
  statut: StatutDevis;
  secondes_restantes: number;
  /** L'heure de validité est passée, même si le statut dit encore « envoyé ». */
  echu: boolean;
}

export type StatutDevis = "SENT" | "ACCEPTED" | "REFUSED" | "EXPIRED";

/** Ce que le prestataire propose. */
export interface Proposition {
  montant_htva_cents: number;
  taux_tva_bp: number;
  delai_minutes: number;
  note?: string;
  preuve_tva_reduite?: string;
}

export type StatutMission =
  | "ACCEPTED"
  | "PROVIDER_EN_ROUTE"
  | "ON_SITE"
  | "COMPLETED"
  /** Validée par le demandeur, ou par le délai de 72 h (FR-021). */
  | "VALIDATED"
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
      return "Terminée, en attente de validation du demandeur";
    case "VALIDATED":
      return "Validée par le demandeur";
    case "CANCELLED":
      return "Annulée";
    default:
      return statut;
  }
}

/** Ce que le statut d'un devis veut dire, en clair. */
export function libelleStatutDevis(devis: Devis): string {
  // L'échéance passe avant le statut : le balayage peut n'être pas encore venu,
  // et afficher « en attente » sur un devis mort ferait attendre pour rien.
  if (devis.statut === "SENT" && devis.echu) return "Expiré sans réponse";
  switch (devis.statut) {
    case "SENT":
      return "En attente de réponse";
    case "ACCEPTED":
      return "Accepté";
    case "REFUSED":
      return "Refusé";
    case "EXPIRED":
      return "Expiré sans réponse";
    default:
      return devis.statut;
  }
}

/**
 * Montant en centimes, rendu en euros.
 *
 * La division est faite ici et nulle part ailleurs : c'est le seul endroit du
 * front où un montant cesse d'être un entier, et il ne repart jamais dans
 * l'autre sens.
 */
export function montantLisible(cents: number): string {
  const euros = (cents / 100).toLocaleString("fr-BE", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  return `${euros} €`;
}

/** Délai en minutes, rendu en heures et minutes. */
export function delaiLisible(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const reste = minutes % 60;
  return reste === 0 ? `${h} h` : `${h} h ${reste}`;
}

/**
 * Convertit une saisie en euros en centimes entiers.
 *
 * **La chaîne est découpée, jamais multipliée.** `Math.round(1.005 * 100)`
 * rend 100 et non 101, parce que 1,005 n'existe pas en binaire : il vaut
 * 1,00499999999999989. Passer par le flottant introduirait donc une erreur
 * d'un centime sur des montants parfaitement ordinaires, dans un module dont
 * toute la discipline est de n'avoir que des entiers.
 *
 * Rend `null` sur une saisie qui n'est pas un montant : laisser passer `NaN`
 * enverrait `null` au serveur et produirait un 400 que l'utilisateur ne
 * comprendrait pas. La virgule est acceptée, c'est ce qu'on tape en Belgique.
 *
 * Un montant négatif est converti et transmis tel quel : c'est le serveur qui
 * le refuse, avec `AMOUNT_NEGATIVE`, et le refuser ici priverait l'utilisateur
 * de l'explication.
 */
export function centimesDepuisEuros(saisie: string): number | null {
  const normalisee = saisie.trim().replace(",", ".");
  // Un chiffre au moins, d'un côté ou de l'autre du séparateur.
  if (!/^-?(\d+(\.\d*)?|\.\d+)$/.test(normalisee)) return null;

  const negatif = normalisee.startsWith("-");
  const [entier, decimales = ""] = normalisee.replace("-", "").split(".");
  const euros = entier === "" ? 0 : Number(entier);
  const centimes = Number(`${decimales}00`.slice(0, 2));
  // Au-delà du centime, arrondi commercial sur la première décimale ignorée.
  const reste = decimales.slice(2);
  const arrondi = reste !== "" && Number(reste[0]) >= 5 ? 1 : 0;

  const total = euros * 100 + centimes + arrondi;
  if (!Number.isSafeInteger(total)) return null;
  return negatif ? -total : total;
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
  | "MISSION_CLOSED"
  | "RESERVED_TO_USER"
  | "MISSION_NOT_COMPLETED"
  | "ALREADY_RELEASED"
  | "QUOTE_NOT_ACCEPTED"
  | "QUOTE_ALREADY_PENDING"
  | "MAX_QUOTES_REACHED"
  | "AMOUNT_ZERO"
  | "AMOUNT_NEGATIVE"
  | "AMOUNT_TOO_HIGH"
  | "DELAY_INVALID"
  | "DELAY_TOO_LONG"
  | "NOTE_TOO_LONG"
  | "VAT_RATE_UNKNOWN"
  | "VAT_PROOF_REQUIRED"
  | "VAT_PROOF_TOO_LONG"
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
    MISSION_CLOSED: "Cette intervention est close : elle ne peut plus être chiffrée.",
    RESERVED_TO_USER: "C'est au demandeur de valider la fin de l'intervention.",
    MISSION_NOT_COMPLETED: "L'intervention n'est pas déclarée terminée.",
    ALREADY_RELEASED: "Cette intervention a déjà été validée.",
    QUOTE_NOT_ACCEPTED: "Aucun devis accepté : il n'y a rien à libérer.",
    QUOTE_ALREADY_PENDING: "Un devis attend déjà une réponse pour cette intervention.",
    MAX_QUOTES_REACHED:
      "Trois devis ont déjà été envoyés. L'intervention a été annulée, le demandeur doit relancer.",
    AMOUNT_ZERO: "Indiquez un montant.",
    AMOUNT_NEGATIVE: "Un montant ne peut pas être négatif.",
    AMOUNT_TOO_HIGH: "Ce montant dépasse ce qu'un dépannage peut chiffrer ici.",
    DELAY_INVALID: "Indiquez un délai d'intervention.",
    DELAY_TOO_LONG: "Le délai ne peut pas dépasser 24 h.",
    NOTE_TOO_LONG: "Votre note est trop longue.",
    VAT_RATE_UNKNOWN: "Ce taux de TVA n'est pas applicable.",
    VAT_PROOF_REQUIRED: "Un taux réduit demande une preuve : indiquez-la.",
    VAT_PROOF_TOO_LONG: "La référence de la preuve est trop longue.",
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
    MISSION_CLOSED: "Deze interventie is afgesloten: er kan geen offerte meer bij.",
    RESERVED_TO_USER: "Het is aan de aanvrager om het einde te bevestigen.",
    MISSION_NOT_COMPLETED: "De interventie is niet als afgerond gemeld.",
    ALREADY_RELEASED: "Deze interventie is al bevestigd.",
    QUOTE_NOT_ACCEPTED: "Geen aanvaarde offerte: er valt niets vrij te geven.",
    QUOTE_ALREADY_PENDING: "Er wacht al een offerte op antwoord voor deze interventie.",
    MAX_QUOTES_REACHED:
      "Er zijn al drie offertes verstuurd. De interventie is geannuleerd; de aanvrager moet opnieuw starten.",
    AMOUNT_ZERO: "Geef een bedrag op.",
    AMOUNT_NEGATIVE: "Een bedrag kan niet negatief zijn.",
    AMOUNT_TOO_HIGH: "Dit bedrag overstijgt wat hier als herstelling kan worden geoffreerd.",
    DELAY_INVALID: "Geef een interventietermijn op.",
    DELAY_TOO_LONG: "De termijn mag niet meer dan 24 u bedragen.",
    NOTE_TOO_LONG: "Uw nota is te lang.",
    VAT_RATE_UNKNOWN: "Dit btw-tarief is niet van toepassing.",
    VAT_PROOF_REQUIRED: "Een verlaagd tarief vereist een bewijs: vermeld het.",
    VAT_PROOF_TOO_LONG: "De verwijzing naar het bewijs is te lang.",
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
    MISSION_CLOSED: "This job is closed: it can no longer be quoted.",
    RESERVED_TO_USER: "Validating the end of the job is the requester's to do.",
    MISSION_NOT_COMPLETED: "The job has not been reported as finished.",
    ALREADY_RELEASED: "This job has already been validated.",
    QUOTE_NOT_ACCEPTED: "No accepted quote: there is nothing to release.",
    QUOTE_ALREADY_PENDING: "A quote is already awaiting an answer for this job.",
    MAX_QUOTES_REACHED:
      "Three quotes have already been sent. The job was cancelled; the requester must start again.",
    AMOUNT_ZERO: "Enter an amount.",
    AMOUNT_NEGATIVE: "An amount cannot be negative.",
    AMOUNT_TOO_HIGH: "This amount is beyond what a callout can be quoted at here.",
    DELAY_INVALID: "Enter a response time.",
    DELAY_TOO_LONG: "The delay cannot exceed 24 h.",
    NOTE_TOO_LONG: "Your note is too long.",
    VAT_RATE_UNKNOWN: "This VAT rate does not apply.",
    VAT_PROOF_REQUIRED: "A reduced rate requires proof: state it.",
    VAT_PROOF_TOO_LONG: "The proof reference is too long.",
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

/** Motifs d'annulation d'une intervention, vocabulaire fermé (FR-022). */
export const MOTIFS_ANNULATION_MISSION = [
  { code: "UNAVAILABLE", libelle: "Je ne peux plus venir" },
  { code: "NO_ACCESS", libelle: "Impossible d'accéder au lieu" },
  { code: "DISAGREEMENT", libelle: "Désaccord sur le travail à faire" },
  { code: "OTHER", libelle: "Autre" },
] as const;

/**
 * Annule une intervention en cours (FR-022).
 *
 * La même route pour les deux parties : le service déduit du jeton qui annule,
 * et c'est cela qui détermine ce que l'annulation coûte.
 */
export async function annulerMission(
  missionId: string,
  motif?: string,
): Promise<{ auteur: string; prestataire_suspendu: boolean }> {
  return request(`/missions/${missionId}/cancel`, {
    method: "POST",
    body: motif ? { motif } : {},
    headers: autorisation(),
  });
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

/**
 * Envoie un devis pour une Mission (FR-016).
 *
 * **Pas de mise en file hors-ligne.** Un devis rejoué une heure plus tard
 * porterait un prix décidé devant une fuite qu'on n'a plus sous les yeux, et le
 * délai annoncé serait déjà faux. Mieux vaut un refus immédiat qu'un document
 * contractuel envoyé à retardement.
 */
export async function envoyerDevis(
  missionId: string,
  proposition: Proposition,
): Promise<Devis & { code: string }> {
  return request(`/missions/${missionId}/quote`, {
    method: "POST",
    body: proposition,
    headers: autorisation(),
  });
}
