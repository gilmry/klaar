/**
 * Connexion (Story 1.3, FR-004).
 *
 * **Où vit le jeton d'accès.** En mémoire du module, et nulle part ailleurs.
 * `localStorage` et `sessionStorage` sont lisibles par tout script de la page :
 * une seule faille XSS y prend le jeton et le garde. En mémoire, il disparaît
 * au rechargement — et c'est le refresh, en cookie `HttpOnly` donc hors de
 * portée de JavaScript, qui permet d'en réobtenir un (`restaurerSession`).
 */
import { ApiError, OfflineError, request } from "./api";
import type { LocaleKlaar } from "./inscription";

export interface DemandeConnexion {
  email: string;
  mot_de_passe: string;
}

export interface SessionOuverte {
  jeton_acces: string;
  expire_dans: number;
}

/**
 * Marge avant expiration à laquelle le renouvellement est déclenché.
 *
 * Assez tôt pour qu'une requête en vol ne parte pas avec un jeton qui expire
 * entre son émission et son arrivée, assez tard pour ne pas multiplier les
 * rotations : chacune consomme un refresh, et une boucle trop serrée finirait
 * par ressembler à un rejeu.
 */
export const MARGE_RENOUVELLEMENT_SECONDES = 60;

export type CodeErreurConnexion =
  | "EMAIL_EMPTY"
  | "EMAIL_MALFORMED"
  | "PASSWORD_EMPTY"
  | "PASSWORD_TOO_SHORT"
  | "PASSWORD_TOO_LONG"
  | "INVALID_CREDENTIALS"
  | "ACCOUNT_NOT_VERIFIED"
  | "ACCOUNT_LOCKED"
  | "RATE_LIMIT_EXCEEDED"
  // Refus de rafraîchissement (Story 1.4). Rarement affichés : la reprise de
  // session échoue en silence. `REFRESH_REUSED` fait exception — il signifie
  // qu'un vol a été détecté, et la personne doit savoir pourquoi elle a été
  // déconnectée.
  | "REFRESH_MISSING"
  | "REFRESH_INVALID"
  | "REFRESH_EXPIRED"
  | "REFRESH_REVOKED"
  | "REFRESH_REUSED"
  | "SERVICE_UNAVAILABLE"
  | "INCONNU"
  | "HORS_LIGNE";

const MESSAGES: Record<LocaleKlaar, Record<CodeErreurConnexion, string>> = {
  fr: {
    EMAIL_EMPTY: "Indiquez votre adresse email.",
    EMAIL_MALFORMED: "Cette adresse email n'est pas valide.",
    PASSWORD_EMPTY: "Indiquez votre mot de passe.",
    PASSWORD_TOO_SHORT: "Adresse ou mot de passe incorrect.",
    PASSWORD_TOO_LONG: "Adresse ou mot de passe incorrect.",
    INVALID_CREDENTIALS: "Adresse ou mot de passe incorrect.",
    ACCOUNT_NOT_VERIFIED:
      "Votre adresse n'est pas encore confirmée. Ouvrez le lien reçu par courriel.",
    ACCOUNT_LOCKED:
      "Votre compte est temporairement verrouillé après plusieurs tentatives ratées. Réessayez dans un quart d'heure.",
    RATE_LIMIT_EXCEEDED: "Trop de tentatives depuis cette connexion. Réessayez dans une heure.",
    REFRESH_MISSING:
      "Votre session a expiré. Reconnectez-vous.",
    REFRESH_INVALID:
      "Votre session a expiré. Reconnectez-vous.",
    REFRESH_EXPIRED:
      "Votre session a expiré. Reconnectez-vous.",
    REFRESH_REVOKED:
      "Votre session a été fermée. Reconnectez-vous.",
    REFRESH_REUSED:
      "Votre session a été fermée par sécurité : un jeton déjà utilisé a été présenté. Reconnectez-vous, et changez votre mot de passe si vous n'êtes pas à l'origine de cette tentative.",
    SERVICE_UNAVAILABLE: "Le service est momentanément indisponible. Réessayez.",
    INCONNU: "La connexion n'a pas abouti. Réessayez.",
    HORS_LIGNE: "Aucune connexion. La connexion à votre compte a besoin du réseau.",
  },
  nl: {
    EMAIL_EMPTY: "Geef uw e-mailadres op.",
    EMAIL_MALFORMED: "Dit e-mailadres is niet geldig.",
    PASSWORD_EMPTY: "Geef uw wachtwoord op.",
    PASSWORD_TOO_SHORT: "Onjuist adres of wachtwoord.",
    PASSWORD_TOO_LONG: "Onjuist adres of wachtwoord.",
    INVALID_CREDENTIALS: "Onjuist adres of wachtwoord.",
    ACCOUNT_NOT_VERIFIED:
      "Uw adres is nog niet bevestigd. Open de link uit uw e-mail.",
    ACCOUNT_LOCKED:
      "Uw account is tijdelijk vergrendeld na meerdere mislukte pogingen. Probeer het over een kwartier opnieuw.",
    RATE_LIMIT_EXCEEDED:
      "Te veel pogingen vanaf deze verbinding. Probeer het over een uur opnieuw.",
    REFRESH_MISSING:
      "Uw sessie is verlopen. Meld u opnieuw aan.",
    REFRESH_INVALID:
      "Uw sessie is verlopen. Meld u opnieuw aan.",
    REFRESH_EXPIRED:
      "Uw sessie is verlopen. Meld u opnieuw aan.",
    REFRESH_REVOKED:
      "Uw sessie is beëindigd. Meld u opnieuw aan.",
    REFRESH_REUSED:
      "Uw sessie is uit veiligheid beëindigd: een reeds gebruikte token werd aangeboden. Meld u opnieuw aan en wijzig uw wachtwoord als u dit niet was.",
    SERVICE_UNAVAILABLE: "De dienst is tijdelijk niet beschikbaar. Probeer opnieuw.",
    INCONNU: "Aanmelden is mislukt. Probeer opnieuw.",
    HORS_LIGNE: "Geen verbinding. Aanmelden vereist het netwerk.",
  },
  en: {
    EMAIL_EMPTY: "Enter your email address.",
    EMAIL_MALFORMED: "This email address is not valid.",
    PASSWORD_EMPTY: "Enter your password.",
    PASSWORD_TOO_SHORT: "Incorrect address or password.",
    PASSWORD_TOO_LONG: "Incorrect address or password.",
    INVALID_CREDENTIALS: "Incorrect address or password.",
    ACCOUNT_NOT_VERIFIED:
      "Your address is not confirmed yet. Open the link from your email.",
    ACCOUNT_LOCKED:
      "Your account is temporarily locked after several failed attempts. Try again in fifteen minutes.",
    RATE_LIMIT_EXCEEDED: "Too many attempts from this connection. Try again in an hour.",
    REFRESH_MISSING:
      "Your session has expired. Sign in again.",
    REFRESH_INVALID:
      "Your session has expired. Sign in again.",
    REFRESH_EXPIRED:
      "Your session has expired. Sign in again.",
    REFRESH_REVOKED:
      "Your session was closed. Sign in again.",
    REFRESH_REUSED:
      "Your session was closed for security: an already-used token was presented. Sign in again, and change your password if this was not you.",
    SERVICE_UNAVAILABLE: "The service is temporarily unavailable. Please retry.",
    INCONNU: "Sign-in did not go through. Please retry.",
    HORS_LIGNE: "No connection. Signing in needs the network.",
  },
};

export function messageErreur(locale: LocaleKlaar, code: string): string {
  const table = MESSAGES[locale];
  return table[code as CodeErreurConnexion] ?? table.INCONNU;
}

export function codeDepuisErreur(erreur: unknown): CodeErreurConnexion {
  if (erreur instanceof OfflineError) return "HORS_LIGNE";
  if (!(erreur instanceof ApiError)) return "INCONNU";
  try {
    const corps = JSON.parse(erreur.body) as { code?: unknown };
    if (typeof corps.code === "string") return corps.code as CodeErreurConnexion;
  } catch {
    // Réponse d'une passerelle plutôt que de l'API.
  }
  return erreur.status >= 500 ? "SERVICE_UNAVAILABLE" : "INCONNU";
}

/** Jeton courant. Jamais écrit ailleurs qu'ici. */
let jetonCourant: string | null = null;
let minuterie: ReturnType<typeof setTimeout> | null = null;

export function jetonAcces(): string | null {
  return jetonCourant;
}

/**
 * Qui veut être prévenu quand la session s'ouvre ou se ferme.
 *
 * **Pourquoi une notification plutôt qu'une lecture ponctuelle.** La navigation
 * de la coquille est montée une fois par page et doit changer au moment où l'on
 * se connecte, sans rechargement : le formulaire de connexion ne redirige nulle
 * part, la page reste la même. Sans ce signal, le menu continuerait d'afficher
 * « Me connecter » à quelqu'un qui vient de se connecter — un mensonge visible
 * à l'écran.
 *
 * Le jeton lui-même ne circule pas : les observateurs ne reçoivent qu'un
 * booléen. Un menu n'a aucune raison de tenir un secret.
 */
type ObservateurSession = (connecte: boolean) => void;
const observateurs = new Set<ObservateurSession>();

/**
 * S'abonne aux changements d'état de session. Rend la fonction de
 * désabonnement, que l'appelant doit appeler à sa destruction — un composant
 * démonté qui reste abonné garde une référence vivante sur sa fermeture.
 */
export function observerSession(observateur: ObservateurSession): () => void {
  observateurs.add(observateur);
  return () => observateurs.delete(observateur);
}

function annoncerSession(connecte: boolean): void {
  for (const observateur of observateurs) {
    try {
      observateur(connecte);
    } catch {
      // Un observateur qui échoue ne doit pas empêcher les autres d'être
      // prévenus, ni faire échouer la connexion elle-même.
    }
  }
}

export function oublierJeton(): void {
  const avait = jetonCourant !== null;
  jetonCourant = null;
  if (minuterie !== null) {
    clearTimeout(minuterie);
    minuterie = null;
  }
  if (avait) annoncerSession(false);
}

/**
 * Programme le renouvellement avant expiration.
 *
 * Sans lui, le jeton meurt en silence et la première action de l'utilisateur
 * échoue sans qu'il comprenne — alors que le refresh était valable.
 */
function programmerRenouvellement(expireDans: number): void {
  if (minuterie !== null) clearTimeout(minuterie);
  const delai = Math.max(expireDans - MARGE_RENOUVELLEMENT_SECONDES, 1) * 1000;
  minuterie = setTimeout(() => {
    // Un échec ici n'a personne à qui parler : la session est simplement
    // perdue, et la prochaine action affichera le refus.
    rafraichir().catch(() => oublierJeton());
  }, delai);
}

function retenir(session: SessionOuverte): SessionOuverte {
  const avait = jetonCourant !== null;
  jetonCourant = session.jeton_acces;
  programmerRenouvellement(session.expire_dans);
  // Un renouvellement n'est pas une ouverture de session : ne prévenir qu'au
  // passage de « pas connecté » à « connecté » évite de faire clignoter tout ce
  // qui écoute, toutes les heures, sans que rien n'ait changé pour personne.
  if (!avait) annoncerSession(true);
  return session;
}

export async function seConnecter(demande: DemandeConnexion): Promise<SessionOuverte> {
  return retenir(
    await request<SessionOuverte>("/auth/login", { method: "POST", body: demande }),
  );
}

/**
 * Rafraîchissement en vol, partagé par tous les appelants.
 *
 * **Sans cela, deux composants sur la même page se déconnectent mutuellement.**
 * Le refresh est à usage unique et sa rotation détecte le rejeu : deux appels
 * simultanés présentent le même jeton, le second passe pour un vol, et la
 * famille entière est révoquée — comportement voulu côté serveur (FR-004), et
 * catastrophique quand c'est notre propre page qui le déclenche.
 *
 * Le cas s'est produit sur l'espace prestataire, où deux îlots Svelte appellent
 * chacun `restaurerSession()` au montage. Il n'a été vu qu'en filmant un
 * parcours contre le vrai service : les tests de vérification simulent
 * `/auth/refresh` et ne peuvent pas le reproduire.
 */
let enVol: Promise<SessionOuverte> | null = null;

/**
 * Échange le refresh contre un accès neuf.
 *
 * Aucun corps : le refresh voyage dans son cookie `HttpOnly`, que ce code ne
 * peut de toute façon pas lire.
 *
 * Les appels concurrents partagent une seule requête. C'est une nécessité, pas
 * une optimisation : voir `enVol` ci-dessus.
 */
export async function rafraichir(): Promise<SessionOuverte> {
  if (enVol) return enVol;
  enVol = (async () => {
    try {
      return retenir(await request<SessionOuverte>("/auth/refresh", { method: "POST" }));
    } finally {
      // Libéré dans tous les cas : garder une promesse rejetée ferait échouer
      // pour toujours tout rafraîchissement ultérieur.
      enVol = null;
    }
  })();
  return enVol;
}

/**
 * Tente de reprendre une session au chargement de la page.
 *
 * Rend `false` sans bruit si aucune session ne peut être reprise : arriver sur
 * une page en n'étant pas connecté est l'état normal d'un visiteur, pas une
 * erreur à afficher.
 */
export async function restaurerSession(): Promise<boolean> {
  try {
    await rafraichir();
    return true;
  } catch {
    oublierJeton();
    return false;
  }
}

/**
 * Ferme la session, côté serveur comme côté client.
 *
 * Le jeton local est oublié **même si l'appel échoue** : laisser un jeton en
 * mémoire après un clic sur « me déconnecter » est le pire des deux mondes.
 */
export async function seDeconnecter(): Promise<void> {
  try {
    await request<void>("/auth/logout", { method: "POST" });
  } finally {
    oublierJeton();
  }
}
