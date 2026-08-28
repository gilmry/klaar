/**
 * Flux temps réel d'une Mission (Story 4.9).
 *
 * **Ce que la socket apporte, et ce qu'elle ne remplace pas.** Elle dit qu'il
 * s'est passé quelque chose ; c'est l'appel HTTP qui dit quoi. Le sondage reste
 * donc en place, simplement ralenti quand la socket vit : une socket peut être
 * coupée par un proxy sans que rien ne le signale, et un écran de suivi qui
 * cesse de bouger sans le dire est pire qu'un écran lent.
 *
 * **Deux requêtes pour ouvrir.** Un navigateur ne peut pas poser d'en-tête
 * `Authorization` sur une WebSocket ; le service émet donc un billet à usage
 * unique, valable trente secondes, qui voyage dans l'URL. Un jeton d'accès y
 * finirait dans les journaux du serveur, du proxy et dans l'historique du
 * navigateur.
 */
import { API_BASE, request } from "./api";
import { jetonAcces } from "./connexion";

/** Ce que la socket délivre. */
export interface EvenementMission {
  mission_id?: string;
  genre: "MISSION_STATUS" | "QUOTE_SENT" | "QUOTE_EXPIRED" | "RESYNC";
  statut?: string;
  survenu_le?: string;
}

interface Billet {
  billet: string;
  expire_dans: number;
}

/** Attentes successives avant de retenter, en millisecondes. */
export const REPRISES_MS = [1_000, 2_000, 5_000, 10_000, 30_000];

/**
 * Attente avant la n-ième tentative.
 *
 * Progressive et **plafonnée** : un service qui redémarre ne doit pas recevoir
 * une reconnexion par seconde de la part de chaque onglet ouvert. Le palier
 * final est gardé indéfiniment plutôt que d'abandonner — quelqu'un qui laisse
 * son suivi ouvert pendant une panne doit le voir repartir tout seul.
 */
export function attenteReprise(tentative: number): number {
  const rang = Math.min(tentative, REPRISES_MS.length - 1);
  return REPRISES_MS[Math.max(rang, 0)];
}

/**
 * URL de la socket, dérivée de celle de l'API.
 *
 * `https` devient `wss`, `http` devient `ws`. Dérivée et non configurée à part :
 * deux réglages finiraient par diverger, et une socket en clair sous une page
 * en TLS est refusée par le navigateur — au bon moment pour nous, au mauvais
 * moment pour l'utilisateur.
 */
export function urlSocket(missionId: string, billet: string, origine = location): string {
  const base = API_BASE.startsWith("http")
    ? new URL(API_BASE)
    : new URL(API_BASE, origine.origin);
  const protocole = base.protocol === "https:" ? "wss:" : "ws:";
  const chemin = `${base.pathname.replace(/\/$/, "")}/missions/${missionId}/events`;
  return `${protocole}//${base.host}${chemin}?billet=${encodeURIComponent(billet)}`;
}

export interface FluxOptions {
  /** Appelé à chaque événement reçu. */
  surEvenement: (evenement: EvenementMission) => void;
  /** Appelé quand la socket s'ouvre ou se ferme, pour ajuster le sondage. */
  surEtat?: (ouverte: boolean) => void;
}

/**
 * Ouvre le flux d'une Mission et le maintient ouvert.
 *
 * Rend une fonction de fermeture. L'appeler arrête les reconnexions : sans
 * cela, quitter la page laisserait une boucle tenter de rouvrir une socket dont
 * plus personne n'attend rien.
 *
 * **Aucune erreur n'est propagée.** Le temps réel est un accélérateur ; son
 * échec ne doit pas casser un écran qui fonctionne par sondage. Ce qui échoue
 * est simplement réessayé.
 */
export function ouvrirFlux(missionId: string, options: FluxOptions): () => void {
  let socket: WebSocket | null = null;
  let minuterie: ReturnType<typeof setTimeout> | null = null;
  let tentative = 0;
  let ferme = false;

  const annoncer = (ouverte: boolean) => options.surEtat?.(ouverte);

  async function connecter() {
    if (ferme) return;
    // Sans session, rien à ouvrir : le billet demande un jeton. Réessayer plus
    // tard plutôt qu'abandonner — la session peut revenir d'un rafraîchissement
    // en cours.
    if (!jetonAcces()) return reprendre();

    let billet: Billet;
    try {
      billet = await request<Billet>("/realtime/ticket", {
        method: "POST",
        headers: { Authorization: `Bearer ${jetonAcces()}` },
      });
    } catch {
      return reprendre();
    }
    if (ferme) return;

    try {
      socket = new WebSocket(urlSocket(missionId, billet.billet));
    } catch {
      return reprendre();
    }

    socket.onopen = () => {
      tentative = 0;
      annoncer(true);
    };
    socket.onmessage = (message) => {
      try {
        options.surEvenement(JSON.parse(String(message.data)) as EvenementMission);
      } catch {
        // Un message illisible ne doit pas fermer le flux : le sondage
        // rattrapera ce qu'il annonçait.
      }
    };
    socket.onclose = () => {
      annoncer(false);
      socket = null;
      reprendre();
    };
    // `onerror` est toujours suivi de `onclose` : tout est fait là, sinon la
    // reprise serait programmée deux fois.
    socket.onerror = () => {};
  }

  function reprendre() {
    if (ferme || minuterie !== null) return;
    const attente = attenteReprise(tentative);
    tentative += 1;
    minuterie = setTimeout(() => {
      minuterie = null;
      void connecter();
    }, attente);
  }

  void connecter();

  return () => {
    ferme = true;
    if (minuterie !== null) clearTimeout(minuterie);
    minuterie = null;
    // `onclose` est retiré avant de fermer : sans cela, la fermeture volontaire
    // déclencherait une reconnexion.
    if (socket) {
      socket.onclose = null;
      socket.close();
      socket = null;
    }
    annoncer(false);
  };
}
