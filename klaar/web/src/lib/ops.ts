/**
 * Console d'exploitation : session et tableau de bord (Story 8.3, FR-040).
 *
 * **Le jeton d'exploitation ne quitte jamais la mémoire de la page.** Ni
 * `localStorage`, ni `sessionStorage`, ni cookie : un jeton d'exploitation
 * survivrait alors à la fermeture de l'onglet, resterait lisible par n'importe
 * quel script injecté, et donnerait accès aux Demandes, aux litiges et aux
 * montants de tout le monde. Fermer l'onglet ferme la session, et c'est le
 * comportement voulu.
 *
 * **Les identifiants ne circulent qu'une fois**, dans le corps du `login`. La
 * première version de cette API les reprenait en paramètres d'URL à chaque
 * requête : un mot de passe dans la barre d'adresse, l'historique et les
 * journaux d'accès.
 */
import { ApiError, request } from "./api";

export interface SessionOps {
  id: string;
  role: RoleOps;
  jeton: string;
  expire_le: string;
}

export type RoleOps = "SUPER_ADMIN" | "KYC_REVIEWER" | "MEDIATOR" | "READER";

export interface TableauBord {
  depuis: string;
  fenetre_jours: number;
  comptes_actifs: number;
  demandes: number;
  demandes_attribuees: number;
  /** `null` quand aucune Demande : l'absence de mesure n'est pas un échec. */
  taux_remplissage: number | null;
  gmv_htva_cents: number;
  commission_htva_cents: number;
  litiges_ouverts: number;
  notes: number;
  /** Moyenne sur cinq. Ce n'est **pas** le NPS ; voir le service. */
  note_moyenne: number | null;
  sorties_de_zone: number;
  kyc_en_attente: number;
}

/** Jeton courant. Jamais écrit ailleurs qu'ici. */
let jetonCourant: string | null = null;
let echeanceCourante: number | null = null;

export function jetonOps(): string | null {
  return jetonCourant;
}

export function oublierJetonOps(): void {
  jetonCourant = null;
  echeanceCourante = null;
}

/**
 * Millisecondes avant l'échéance, ou `null` hors session.
 *
 * Exposé pour que l'écran puisse prévenir **avant** de couper. Une console qui
 * répond 401 au milieu d'une médiation fait perdre ce qui était en train d'être
 * écrit.
 */
export function resteAvantEcheance(): number | null {
  if (echeanceCourante === null) return null;
  return Math.max(0, echeanceCourante - Date.now());
}

/** Ouvre une session d'exploitation (mot de passe **et** code TOTP). */
export async function connexionOps(
  email: string,
  motDePasse: string,
  code: string,
): Promise<SessionOps> {
  const session = await request<SessionOps>("/ops/login", {
    method: "POST",
    body: { email, mot_de_passe: motDePasse, code },
  });
  jetonCourant = session.jeton;
  echeanceCourante = Date.parse(session.expire_le);
  return session;
}

/**
 * Ferme la session.
 *
 * **Le jeton local est oublié même si l'appel échoue.** Laisser un jeton en
 * mémoire parce que le réseau a coupé donnerait un écran qui se croit
 * déconnecté et ne l'est pas.
 */
export async function deconnexionOps(): Promise<void> {
  const jeton = jetonCourant;
  oublierJetonOps();
  if (!jeton) return;
  try {
    await request<void>("/ops/logout", {
      method: "POST",
      headers: { Authorization: `Bearer ${jeton}` },
    });
  } catch {
    // Une session non fermée côté serveur expire d'elle-même en trente
    // minutes ; l'échec n'a pas à remonter à l'écran.
  }
}

/** Lit les indicateurs. */
export async function tableauDeBord(): Promise<TableauBord> {
  return request<TableauBord>("/ops/dashboard", { headers: autorisationOps() });
}

function autorisationOps(): Record<string, string> {
  return jetonCourant ? { Authorization: `Bearer ${jetonCourant}` } : {};
}

/** Vrai si l'erreur dit que la session est finie. */
export function sessionFinie(erreur: unknown): boolean {
  return erreur instanceof ApiError && erreur.status === 401;
}

/**
 * Un taux en pourcentage lisible, ou le texte qui dit pourquoi il n'y en a pas.
 *
 * **`null` ne devient pas « 0 % ».** Zéro pour cent se lit comme un échec de la
 * plateforme ; à J0, il n'y a pas d'échec, il n'y a rien à mesurer.
 */
export function pourcentage(taux: number | null): string {
  if (taux === null) return "pas encore mesurable";
  return `${(taux * 100).toFixed(0)} %`;
}

/** Un montant en centimes, en euros lisibles. */
export function montantOps(cents: number): string {
  // La console d'exploitation reste en français (voir Story 9.1) ; le format
  // belge est donc celui qui convient à ses lecteurs.
  return `${(cents / 100).toLocaleString("fr-BE", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })} €`;
}

/** La note moyenne, ou le texte qui dit qu'il n'y en a pas encore. */
export function noteLisible(moyenne: number | null, nombre: number): string {
  if (moyenne === null) return "aucune note";
  return `${moyenne.toFixed(1)} / 5 sur ${nombre} note${nombre > 1 ? "s" : ""}`;
}

/** Un litige, tel que la console de médiation le voit (Story 7.4, FR-036). */
export interface DossierLitige {
  id: string;
  mission_id: string;
  partie: "USER" | "PROVIDER";
  motif: string;
  description: string;
  ouvert_le: string;
  age_jours: number;
  /** Ouvert depuis plus de trente jours (FR-036 `@edge`). */
  a_escalader: boolean;
  total_ttc_cents: number;
}

export type DecisionOps = "USER_FAVOR" | "PROVIDER_FAVOR" | "PARTIAL_REFUND" | "NO_FAULT";

export interface IssueLitige {
  statut: string;
  remboursement_cents: number;
  reversement_cents: number;
  /**
   * Faux tant que le séquestre n'est pas provisionné (Epic 5).
   *
   * **Rendu par le service et affiché tel quel.** Annoncer « remboursé » pour
   * un mouvement qui n'a pas lieu ferait attendre un virement qui ne viendra
   * pas, et c'est ce qui transforme un litige tranché en second litige.
   */
  execute: boolean;
}

/** Les litiges à trancher, du plus ancien au plus récent. */
export async function fileMediation(): Promise<DossierLitige[]> {
  const corps = await request<{ dossiers: DossierLitige[] }>("/ops/disputes", {
    headers: autorisationOps(),
  });
  return corps.dossiers;
}

/**
 * Tranche un litige.
 *
 * `partBp` n'est passé que pour `PARTIAL_REFUND` : le service refuse un taux
 * sur une décision qui n'en prend pas, plutôt que de l'ignorer.
 */
export async function trancherLitige(
  litigeId: string,
  decision: DecisionOps,
  partBp?: number,
): Promise<IssueLitige> {
  return request<IssueLitige>(`/ops/disputes/${litigeId}/resolve`, {
    method: "POST",
    body: decision === "PARTIAL_REFUND" ? { decision, part_bp: partBp } : { decision },
    headers: autorisationOps(),
  });
}

/** Le libellé français d'un motif de litige. */
export function libelleMotif(motif: string): string {
  switch (motif) {
    case "QUALITY":
      return "Travail mal fait";
    case "NOT_DONE":
      return "Travail non fait";
    case "AMOUNT_DISPUTED":
      return "Montant contesté";
    case "USER_NO_SHOW":
      return "Personne sur place";
    case "IMPOSSIBLE_CONDITIONS":
      return "Conditions d'intervention impossibles";
    case "OTHER":
      return "Autre";
    default:
      return motif;
  }
}

/** Les quatre décisions possibles, avec ce qu'elles veulent dire. */
export const DECISIONS: { code: DecisionOps; libelle: string }[] = [
  { code: "USER_FAVOR", libelle: "Rembourser le demandeur intégralement" },
  { code: "PARTIAL_REFUND", libelle: "Rembourser une part" },
  { code: "PROVIDER_FAVOR", libelle: "Payer le prestataire intégralement" },
  { code: "NO_FAULT", libelle: "Classer sans faute" },
];

/** Une entreprise en attente de contrôle (Story 8.1, FR-038). */
export interface DossierKyc {
  provider_id: string;
  numero_bce: string;
  raison_sociale: string;
  secteurs: string[];
  inscrit_le: string;
  attente_jours: number;
  attente_longue: boolean;
  refus_en_attente: RefusEnAttente | null;
}

export interface RefusEnAttente {
  motif: string;
  propose_le: string;
  /** Vrai si c'est vous qui l'avez proposé : on ne confirme pas son propre refus. */
  propose_par_moi: boolean;
}

export interface IssueRevue {
  code: string;
  statut: string | null;
  attend_confirmation: boolean;
  /** Faux : aucun courriel n'est parti. */
  notifie: boolean;
}

/** Les entreprises en attente de contrôle. */
export async function fileKyc(): Promise<DossierKyc[]> {
  const corps = await request<{ dossiers: DossierKyc[] }>("/ops/kyc/pending", {
    headers: autorisationOps(),
  });
  return corps.dossiers;
}

/**
 * Valide ou refuse une entreprise.
 *
 * Le motif n'accompagne qu'un refus : le service refuse un motif sur une
 * validation plutôt que de l'ignorer, parce qu'un motif ignoré laisserait
 * croire qu'il a été consigné.
 */
export async function reviserKyc(
  providerId: string,
  decision: "APPROVE" | "REJECT",
  motif?: string,
): Promise<IssueRevue> {
  return request<IssueRevue>(`/ops/kyc/${providerId}/review`, {
    method: "POST",
    body: decision === "REJECT" ? { decision, motif } : { decision },
    headers: autorisationOps(),
  });
}

/** L'entreprise retire sa demande d'inscription (FR-038 `@edge`). */
export async function retirerInscription(jeton: string): Promise<void> {
  return request<void>("/providers/me/registration", {
    method: "DELETE",
    headers: { Authorization: `Bearer ${jeton}` },
  });
}

/** Motif minimal exigé pour un refus, en caractères. */
export const MOTIF_KYC_MIN = 20;
