/**
 * Client HTTP de klaar-api.
 *
 * Les types de requête et de réponse viendront de `@klaar/client`, généré
 * depuis l'OpenAPI servi par le backend (Story 0.6, ADR-004). Tant que le
 * contrat ne sert que `/api/v1/health`, ce module ne connaît que le transport ;
 * il sera typé endpoint par endpoint au fur et à mesure des epics.
 */

/** Levée quand la requête n'a pas atteint le serveur (réseau coupé, DNS, TLS). */
export class OfflineError extends Error {
  constructor(cause?: unknown) {
    super("requête non transmise : réseau indisponible");
    this.name = "OfflineError";
    this.cause = cause;
  }
}

/** Levée quand le serveur a répondu autre chose qu'un succès. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly body: string,
  ) {
    super(`klaar-api a répondu ${status}`);
    this.name = "ApiError";
  }

  /**
   * Vrai si réessayer la même requête a une chance d'aboutir.
   *
   * Un 4xx signifie que la requête est mauvaise : la rejouer produira le même
   * refus. Deux exceptions, où le serveur demande explicitement d'attendre :
   * 408 (Request Timeout) et 429 (Too Many Requests).
   */
  get retryable(): boolean {
    if (this.status === 408 || this.status === 429) return true;
    return this.status >= 500;
  }
}

export const API_BASE =
  (import.meta.env?.PUBLIC_KLAAR_API_BASE as string | undefined) ?? "/api/v1";

export interface RequestOptions {
  method?: string;
  body?: unknown;
  /**
   * Rejoué à l'identique, l'en-tête `Idempotency-Key` garantit que le serveur
   * n'exécute l'effet qu'une fois. Obligatoire pour toute écriture passant par
   * la queue hors-ligne (voir `offlineQueue.ts`).
   */
  idempotencyKey?: string;
  /**
   * En-têtes supplémentaires, `Authorization` en pratique.
   *
   * Fusionnés après les en-têtes calculés ici : une route qui a besoin de
   * porter un jeton le dit à l'appel, plutôt que ce module n'aille chercher un
   * état d'authentification dont il n'a pas à connaître l'existence.
   */
  headers?: Record<string, string>;
  signal?: AbortSignal;
}

export async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { method = "GET", body, idempotencyKey, signal } = options;

  const headers: Record<string, string> = { Accept: "application/json" };
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (idempotencyKey) headers["Idempotency-Key"] = idempotencyKey;
  Object.assign(headers, options.headers ?? {});

  let response: Response;
  try {
    response = await fetch(`${API_BASE}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: "include",
      signal,
    });
  } catch (err) {
    // fetch ne rejette que sur un échec de transport. Un 500 est une réponse,
    // pas une exception : le distinguer est ce qui permet à la queue de savoir
    // s'il faut réessayer ou renoncer.
    throw new OfflineError(err);
  }

  if (!response.ok) {
    throw new ApiError(response.status, await response.text().catch(() => ""));
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export const api = {
  health: () => request<{ status: string }>("/health"),
};

/**
 * Détermine si le serveur est réellement joignable.
 *
 * `navigator.onLine` ne répond pas à cette question : il indique la présence
 * d'une interface réseau, pas l'accessibilité d'un serveur. Vérifié dans
 * Chrome avec le réseau coupé — il reste à `true` pendant que toute requête
 * échoue. S'y fier fait afficher « En ligne » à un utilisateur qui ne l'est
 * pas, et qui ne comprendra donc pas pourquoi rien ne part.
 *
 * La sonde vise une ressource statique assortie d'un paramètre unique : le
 * service worker ne peut pas la servir depuis son cache, elle atteint donc
 * vraiment le réseau. Elle évite `/api/`, dont l'indisponibilité signalerait
 * une panne du backend et non de la connexion.
 */
export async function sonderReseau(signal?: AbortSignal): Promise<boolean> {
  if (typeof navigator !== "undefined" && navigator.onLine === false) return false;
  try {
    await fetch(`/manifest.webmanifest?sonde=${Date.now()}`, { cache: "no-store", signal });
    return true;
  } catch {
    return false;
  }
}
