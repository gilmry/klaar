/**
 * Soumission d'une Demande (Story 3.1, FR-011).
 */
import { ApiError, OfflineError, request } from "./api";
import { jetonAcces } from "./connexion";
import type { LocaleKlaar } from "./inscription";
import { t } from "./i18n";

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
  /**
   * Dernier devis reçu, quel que soit son statut (FR-016).
   *
   * Un devis refusé ou expiré reste rendu : le faire disparaître laisserait
   * l'écran vide sans dire ce qui s'est passé.
   */
  devis: DevisRecu | null;
}

/**
 * Un devis, tel que le demandeur le voit.
 *
 * **Montants en centimes entiers**, comme partout : la conversion en euros n'a
 * lieu qu'à l'affichage. Le total TTC vient du serveur et n'est jamais
 * recalculé ici — il est conservé tel qu'il a été présenté, et le recalculer
 * après un changement de taux réécrirait un document contractuel.
 */
export interface DevisRecu {
  id: string;
  montant_htva_cents: number;
  taux_tva_bp: number;
  tva_cents: number;
  total_ttc_cents: number;
  delai_minutes: number;
  note: string | null;
  statut: "SENT" | "ACCEPTED" | "REFUSED" | "EXPIRED";
  secondes_restantes: number;
  /** L'heure de validité est passée, même si le statut dit encore « envoyé ». */
  echu: boolean;
}

/**
 * Ce que le devis attend du demandeur, en clair.
 *
 * L'échéance passe avant le statut : le balayage peut n'être pas encore venu,
 * et proposer de répondre à un devis mort ferait espérer pour rien.
 */
export function libelleDevis(devis: DevisRecu): string {
  if (devis.statut === "SENT" && devis.echu) return "Ce devis a expiré sans réponse.";
  switch (devis.statut) {
    case "SENT":
      return "Un devis vous attend.";
    case "ACCEPTED":
      return "Vous avez accepté ce devis.";
    case "REFUSED":
      return "Vous avez refusé ce devis.";
    case "EXPIRED":
      return "Ce devis a expiré sans réponse.";
    default:
      return devis.statut;
  }
}

/** Montant en centimes, rendu en euros. La seule division du module. */
export function montantLisible(cents: number): string {
  const euros = (cents / 100).toLocaleString("fr-BE", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  return `${euros} €`;
}

/** Motifs de refus d'un devis, vocabulaire fermé (FR-017). */
export const MOTIFS_REFUS = [
  { code: "TOO_EXPENSIVE", libelle: "Trop cher" },
  { code: "DELAY_TOO_LONG", libelle: "Trop long à venir" },
  { code: "NO_LONGER_NEEDED", libelle: "Je n'en ai plus besoin" },
  { code: "OTHER", libelle: "Autre" },
] as const;

/**
 * Vrai si l'intervention peut encore être notée (FR-033).
 *
 * Quatorze jours après la validation, la fenêtre se ferme : une note écrite
 * trois mois plus tard ne dit plus rien de l'intervention.
 */
export function peutNoter(suivi: SuiviDemande): boolean {
  return suivi.mission_statut === "VALIDATED";
}

/** Motifs de litige ouverts au demandeur, vocabulaire fermé (FR-034). */
export const MOTIFS_LITIGE = [
  { code: "QUALITY", libelle: "Le travail est mal fait" },
  { code: "NOT_DONE", libelle: "Rien n'a été fait" },
  { code: "AMOUNT_DISPUTED", libelle: "Le montant ne correspond pas à ce qui était convenu" },
  { code: "OTHER", libelle: "Autre" },
] as const;

/** Caractères minimaux du récit : « pas content » ne permet pas de trancher. */
export const RECIT_MIN_CARACTERES = 20;

/**
 * Vrai si l'intervention peut encore être contestée (FR-034).
 *
 * Une intervention faite ne s'annule pas — elle a eu lieu — mais elle peut être
 * contestée. Sans ce recours, le seul geste possible après un travail mal fait
 * serait une mauvaise note, ce qui ne rend l'argent à personne.
 */
export function peutContester(suivi: SuiviDemande): boolean {
  return suivi.mission_statut === "COMPLETED" || suivi.mission_statut === "VALIDATED";
}

/** Motifs d'annulation d'une intervention en cours, vocabulaire fermé (FR-022). */
export const MOTIFS_ANNULATION_MISSION = [
  { code: "NO_LONGER_NEEDED", libelle: "Je n'en ai plus besoin" },
  { code: "NO_ACCESS", libelle: "Personne ne peut ouvrir" },
  { code: "DISAGREEMENT", libelle: "Désaccord sur le travail à faire" },
  { code: "OTHER", libelle: "Autre" },
] as const;

/**
 * Vrai si l'intervention est en cours et peut encore être annulée (FR-022).
 *
 * Une intervention faite ne s'annule pas : elle se conteste, et le litige n'est
 * pas encore livré. Offrir le bouton ferait cliquer pour recevoir un refus.
 */
export function peutAnnulerMission(suivi: SuiviDemande): boolean {
  return (
    suivi.mission_statut === "ACCEPTED" ||
    suivi.mission_statut === "PROVIDER_EN_ROUTE" ||
    suivi.mission_statut === "ON_SITE"
  );
}

/**
 * Vrai si le demandeur peut valider la fin de l'intervention (FR-021).
 *
 * Le prestataire déclare avoir terminé ; c'est une autre personne qui dit que
 * c'est fait. Sans réponse, le service valide de lui-même au bout de
 * soixante-douze heures — l'écran le dit, pour que le silence ne passe pas pour
 * un blocage.
 */
export function peutValider(suivi: SuiviDemande): boolean {
  return suivi.mission_statut === "COMPLETED";
}

/** Vrai si ce devis attend encore une réponse du demandeur. */
export function attendUneReponse(devis: DevisRecu): boolean {
  return devis.statut === "SENT" && !devis.echu;
}

/** Délai en minutes, rendu en heures et minutes. */
export function delaiLisible(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const reste = minutes % 60;
  return reste === 0 ? `${h} h` : `${h} h ${reste}`;
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
  | "MISSION_NOT_COMPLETED"
  | "ALREADY_RELEASED"
  | "QUOTE_NOT_ACCEPTED"
  | "QUOTE_EXPIRED"
  | "QUOTE_ALREADY_ANSWERED"
  | "QUOTE_NOT_FOUND"
  | "REASON_UNKNOWN"
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
    MISSION_NOT_COMPLETED: "L'intervention n'est pas encore déclarée terminée.",
    ALREADY_RELEASED: "Cette intervention a déjà été validée.",
    QUOTE_NOT_ACCEPTED: "Aucun devis accepté : il n'y a rien à valider.",
    QUOTE_EXPIRED: "Ce devis a expiré. Demandez-en un nouveau au prestataire.",
    QUOTE_ALREADY_ANSWERED: "Ce devis a déjà reçu une réponse.",
    QUOTE_NOT_FOUND: "Aucun devis en attente pour cette intervention.",
    REASON_UNKNOWN: "Ce motif n'est pas reconnu.",
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
    MISSION_NOT_COMPLETED: "De interventie is nog niet als afgerond gemeld.",
    ALREADY_RELEASED: "Deze interventie is al bevestigd.",
    QUOTE_NOT_ACCEPTED: "Geen aanvaarde offerte: er valt niets te bevestigen.",
    QUOTE_EXPIRED: "Deze offerte is vervallen. Vraag de vakman om een nieuwe.",
    QUOTE_ALREADY_ANSWERED: "Deze offerte heeft al een antwoord gekregen.",
    QUOTE_NOT_FOUND: "Geen offerte in behandeling voor deze interventie.",
    REASON_UNKNOWN: "Deze reden wordt niet herkend.",
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
    MISSION_NOT_COMPLETED: "The job has not been reported as finished yet.",
    ALREADY_RELEASED: "This job has already been validated.",
    QUOTE_NOT_ACCEPTED: "No accepted quote: there is nothing to validate.",
    QUOTE_EXPIRED: "This quote expired. Ask the provider for a new one.",
    QUOTE_ALREADY_ANSWERED: "This quote already has an answer.",
    QUOTE_NOT_FOUND: "No pending quote for this job.",
    REASON_UNKNOWN: "This reason is not recognised.",
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
    case "VALIDATED":
      return "Intervention validée";
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

/**
 * Accepte le devis en attente d'une Mission (FR-017).
 *
 * **Pas de mise en file hors-ligne.** Un accord rejoué une heure plus tard
 * porterait sur un devis probablement expiré, et le refus arriverait sans que
 * personne comprenne pourquoi. Mieux vaut demander à quelqu'un de réessayer
 * quand il a du réseau.
 */
export async function accepterDevis(missionId: string): Promise<{ statut: string }> {
  return request(`/missions/${missionId}/accept-quote`, {
    method: "POST",
    headers: autorisationSuivi(),
  });
}

/** Refuse le devis en attente, avec ou sans motif. */
export async function refuserDevis(
  missionId: string,
  motif?: string,
): Promise<{ statut: string }> {
  return request(`/missions/${missionId}/refuse-quote`, {
    method: "POST",
    body: motif ? { motif } : {},
    headers: autorisationSuivi(),
  });
}

/**
 * Valide la fin de l'intervention (FR-021).
 *
 * **Pas de mise en file hors-ligne.** Une validation rejouée plus tard porterait
 * sur une Mission peut-être déjà validée par le délai, et le refus arriverait
 * sans que personne comprenne pourquoi.
 */
export async function validerMission(missionId: string): Promise<{ statut: string }> {
  return request(`/missions/${missionId}/validate`, {
    method: "POST",
    headers: autorisationSuivi(),
  });
}

/** Annule l'intervention en cours (FR-022). */
export async function annulerMissionEnCours(
  missionId: string,
  motif?: string,
): Promise<{ forfait_deplacement_cents: number; remboursement_cents: number }> {
  return request(`/missions/${missionId}/cancel`, {
    method: "POST",
    body: motif ? { motif } : {},
    headers: autorisationSuivi(),
  });
}

/** Note l'autre partie après une intervention validée (FR-033). */
export async function noterIntervention(
  missionId: string,
  note: number,
  commentaire?: string,
): Promise<{ publiee: boolean }> {
  return request(`/missions/${missionId}/rating`, {
    method: "POST",
    body: commentaire ? { note, commentaire } : { note },
    headers: autorisationSuivi(),
  });
}

/** Ouvre un litige sur une intervention terminée (FR-034). */
export async function ouvrirLitige(
  missionId: string,
  motif: string,
  description: string,
): Promise<{ id: string; statut: string }> {
  return request(`/missions/${missionId}/dispute`, {
    method: "POST",
    body: { motif, description },
    headers: autorisationSuivi(),
  });
}

/**
 * État du trajet, tel que le demandeur le voit (Story 4.4, FR-019).
 *
 * `POSITION_LOST` ne veut pas dire « panne » : le prestataire peut n'avoir pas
 * consenti au partage, ou traverser un tunnel. L'écran doit distinguer les deux
 * pour ne pas faire croire à un problème là où il n'y a qu'un droit exercé.
 */
export type EtatSuivi = "EN_ROUTE" | "POSITION_LOST" | "OUT_OF_ZONE" | "STOPPED";

export interface TrajetSuivi {
  etat: EtatSuivi;
  /**
   * Dernière position connue, **déjà dégradée à cinquante mètres par le
   * serveur**. Le front n'a rien à arrondir : la maille est appliquée à
   * l'écriture, pas à l'affichage.
   */
  position: { lat: number; lon: number } | null;
  relevee_le: string | null;
  /** Délai au-delà duquel le serveur déclare la position perdue. */
  perte_apres_secondes: number;
}

/** Lit où en est le prestataire pendant le trajet (FR-019). */
export async function suivreTrajet(missionId: string): Promise<TrajetSuivi> {
  return request(`/missions/${missionId}/tracking`, {
    headers: autorisationSuivi(),
  });
}

/**
 * Ce que l'écran dit du trajet (FR-019).
 *
 * **`POSITION_LOST` ne dit pas « panne ».** Le prestataire peut n'avoir pas
 * consenti au partage, ou traverser un tunnel. Annoncer une erreur dans ce cas
 * ferait douter d'une intervention qui se déroule normalement, et pousserait à
 * appeler pour rien.
 */
export function libelleTrajet(etat: EtatSuivi, locale: LocaleKlaar = "fr"): string {
  switch (etat) {
    case "EN_ROUTE":
      return t(locale, "trajet.en_route");
    case "OUT_OF_ZONE":
      return t(locale, "trajet.hors_zone");
    case "POSITION_LOST":
      return t(locale, "trajet.perdue");
    case "STOPPED":
      return t(locale, "trajet.arrete");
  }
}
