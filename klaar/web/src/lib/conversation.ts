/**
 * Conversation entre le demandeur et le prestataire (Story 6.1, FR-030).
 *
 * **Le même module pour les deux côtés.** Le service déduit du jeton qui écrit
 * et à qui ; le client n'a rien à savoir de plus, et `de_moi` lui dit de quel
 * côté afficher la bulle sans qu'aucun identifiant de compte ne traverse.
 *
 * **Les coordonnées sont refusées, et le service dit combien de fois** (FR-032).
 * L'afficher vaut mieux que de laisser quelqu'un découvrir la sanction au
 * troisième essai.
 */
import { ApiError, request } from "./api";
import { jetonAcces } from "./connexion";

export interface MessageLu {
  id: string;
  /** Vrai si c'est vous qui l'avez écrit. */
  de_moi: boolean;
  corps: string;
  /** RFC 3339. */
  envoye_le: string;
}

/** Refus pour coordonnées, avec le compteur de récidive. */
export interface RefusCoordonnees {
  code: "CONTACT_INFO_FORBIDDEN";
  tentatives: number;
  signale: boolean;
}

function autorisation(): Record<string, string> {
  const jeton = jetonAcces();
  return jeton ? { Authorization: `Bearer ${jeton}` } : {};
}

export async function lireConversation(missionId: string): Promise<MessageLu[]> {
  const fil = await request<{ messages: MessageLu[] }>(`/missions/${missionId}/messages`, {
    headers: autorisation(),
  });
  return fil.messages;
}

/**
 * Envoie un message.
 *
 * **Pas de mise en file hors-ligne.** Un message rejoué une heure plus tard
 * arriverait après l'intervention qu'il concernait, et « vous êtes où ? » posé
 * une fois le plombier reparti ne rend service à personne.
 */
export async function envoyerMessage(missionId: string, corps: string): Promise<MessageLu> {
  return request(`/missions/${missionId}/messages`, {
    method: "POST",
    body: { corps },
    headers: autorisation(),
  });
}

/**
 * Extrait le compteur d'un refus pour coordonnées.
 *
 * Rend `null` quand ce n'est pas ce refus-là : l'appelant retombe alors sur son
 * message d'erreur habituel.
 */
export function refusCoordonnees(erreur: unknown): RefusCoordonnees | null {
  if (!(erreur instanceof ApiError)) return null;
  try {
    const corps = JSON.parse(erreur.body) as Partial<RefusCoordonnees>;
    if (corps.code !== "CONTACT_INFO_FORBIDDEN") return null;
    return {
      code: "CONTACT_INFO_FORBIDDEN",
      tentatives: typeof corps.tentatives === "number" ? corps.tentatives : 0,
      signale: corps.signale === true,
    };
  } catch {
    return null;
  }
}

/**
 * Ce qu'on affiche à qui vient de tenter d'échanger ses coordonnées.
 *
 * Le ton compte : la personne n'a pas forcément voulu contourner quoi que ce
 * soit, et la formulation doit expliquer plutôt qu'accuser.
 */
export function messageRefus(refus: RefusCoordonnees): string {
  if (refus.signale) {
    return (
      "Les numéros et adresses ne s'échangent pas ici. Plusieurs tentatives ont " +
      "été relevées sur votre compte ; passer par le service est ce qui vous " +
      "protège en cas de litige."
    );
  }
  return (
    "Les numéros et adresses ne s'échangent pas dans la messagerie. Tout ce qui " +
    "concerne l'intervention peut s'écrire ici, et c'est ce qui fait preuve en " +
    "cas de désaccord."
  );
}
