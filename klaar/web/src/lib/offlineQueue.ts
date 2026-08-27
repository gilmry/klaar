/**
 * Queue d'écritures hors-ligne (Story 0.2, ADR-010).
 *
 * Une écriture faite sans réseau est mise en file dans IndexedDB puis rejouée
 * à la reconnexion. Trois choix méritent d'être justifiés, parce qu'ils
 * s'écartent de la queue d'Elevia dont ce module reprend la forme.
 *
 * 1. **Clé d'idempotence obligatoire, tirée à la mise en file.** Elevia peut
 *    rejouer sans risque : ses écritures sont des upserts serveur, idempotents
 *    par construction. Les écritures de Klaar ne le sont pas — rejouer
 *    « créer une Demande » créerait un doublon, et « accepter un Devis »
 *    déclencherait deux séquestres. La clé est tirée une fois et voyage avec
 *    l'élément : c'est ce qui rend le rejeu sûr, pas la bonne volonté.
 *
 * 2. **Un refus définitif ne se réessaye pas indéfiniment.** Une requête qui
 *    reçoit un 4xx non rejouable est déplacée en `dead`, pas laissée en file.
 *    Elevia journalise et réessaye pour toujours, ce qui transforme une erreur
 *    permanente en boucle silencieuse.
 *
 * 3. **Rien n'est supprimé sans avoir abouti ou été constaté.** Un élément
 *    quitte `queue` soit vers le serveur, soit vers `dead` où l'interface peut
 *    le montrer. Le travail de l'utilisateur n'est jamais jeté en silence.
 */
import { openDB, type DBSchema, type IDBPDatabase } from "idb";
import { ApiError, OfflineError, request } from "./api";

export interface QueuedWrite {
  id?: number;
  method: "POST" | "PUT" | "PATCH" | "DELETE";
  path: string;
  body: unknown;
  idempotencyKey: string;
  queuedAt: string;
  attempts: number;
}

export interface DeadWrite extends QueuedWrite {
  failedAt: string;
  status: number;
  detail: string;
}

interface KlaarDB extends DBSchema {
  queue: { key: number; value: QueuedWrite };
  dead: { key: number; value: DeadWrite };
}

const DB_NAME = "klaar-offline";
const DB_VERSION = 1;

let dbPromise: Promise<IDBPDatabase<KlaarDB>> | null = null;

function getDb(): Promise<IDBPDatabase<KlaarDB>> {
  if (!dbPromise) {
    dbPromise = openDB<KlaarDB>(DB_NAME, DB_VERSION, {
      upgrade(db) {
        if (!db.objectStoreNames.contains("queue")) {
          db.createObjectStore("queue", { keyPath: "id", autoIncrement: true });
        }
        if (!db.objectStoreNames.contains("dead")) {
          db.createObjectStore("dead", { keyPath: "id", autoIncrement: true });
        }
      },
    });
  }
  return dbPromise;
}

/**
 * Ferme la connexion et oublie le handle mémorisé.
 *
 * Réservé aux tests : `deleteDB` reste bloqué tant qu'une connexion est
 * ouverte, et attend indéfiniment plutôt que d'échouer. Fermer explicitement
 * est la seule façon de repartir d'une base vide entre deux cas.
 */
export async function closeDb(): Promise<void> {
  if (dbPromise === null) return;
  const db = await dbPromise;
  dbPromise = null;
  db.close();
}

function newIdempotencyKey(): string {
  const c = globalThis.crypto;
  if (c && typeof c.randomUUID === "function") return c.randomUUID();
  // Repli pour les contextes non sécurisés, où `crypto.randomUUID` est absent.
  // Suffisant pour distinguer des écritures d'un même appareil ; la garantie
  // d'unicité globale ne repose de toute façon pas sur le client seul.
  return `k-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Met une écriture en file et retourne sa clé d'idempotence, pour que
 * l'appelant puisse la corréler à ce qu'il affiche.
 */
export async function enqueue(
  method: QueuedWrite["method"],
  path: string,
  body: unknown,
): Promise<string> {
  const idempotencyKey = newIdempotencyKey();
  const db = await getDb();
  await db.add("queue", {
    method,
    path,
    body,
    idempotencyKey,
    queuedAt: new Date().toISOString(),
    attempts: 0,
  });
  return idempotencyKey;
}

export async function pendingCount(): Promise<number> {
  return (await getDb()).count("queue");
}

export async function deadCount(): Promise<number> {
  return (await getDb()).count("dead");
}

export async function listDead(): Promise<DeadWrite[]> {
  return (await getDb()).getAll("dead");
}

/** Retire un élément mort après que l'utilisateur en a pris connaissance. */
export async function discardDead(id: number): Promise<void> {
  await (await getDb()).delete("dead", id);
}

export interface FlushReport {
  sent: number;
  dead: number;
  /** Vrai si le rejeu s'est arrêté sur une coupure réseau, laissant du reste. */
  interrupted: boolean;
}

let flushing = false;

/**
 * Rejoue la file dans l'ordre d'insertion.
 *
 * L'ordre compte : « créer une Demande » puis « l'annuler » ne commutent pas.
 * Le rejeu s'arrête donc à la première coupure réseau plutôt que de sauter
 * l'élément bloquant, quitte à retarder les suivants.
 */
export async function flushQueue(): Promise<FlushReport> {
  const report: FlushReport = { sent: 0, dead: 0, interrupted: false };
  if (flushing) return report;
  flushing = true;
  try {
    const db = await getDb();
    for (const item of await db.getAll("queue")) {
      if (item.id === undefined) continue;
      try {
        await request(item.path, {
          method: item.method,
          body: item.body,
          idempotencyKey: item.idempotencyKey,
        });
        await db.delete("queue", item.id);
        report.sent += 1;
      } catch (err) {
        if (err instanceof OfflineError) {
          report.interrupted = true;
          break;
        }
        if (err instanceof ApiError && !err.retryable) {
          const { id, ...rest } = item;
          await db.add("dead", {
            ...rest,
            failedAt: new Date().toISOString(),
            status: err.status,
            detail: err.body.slice(0, 500),
          });
          await db.delete("queue", id);
          report.dead += 1;
          continue;
        }
        // Erreur serveur rejouable : on la laisse en file, on incrémente le
        // compteur de tentatives et on arrête là. Insister immédiatement
        // ajouterait de la charge à un serveur qui vient de dire qu'il en a
        // trop.
        await db.put("queue", { ...item, attempts: item.attempts + 1 });
        report.interrupted = true;
        break;
      }
    }
    return report;
  } finally {
    flushing = false;
  }
}

let autoSyncTimer: ReturnType<typeof setInterval> | null = null;

/** Rejoue à la reconnexion, puis toutes les 30 s tant que la PWA est ouverte. */
export function startAutoSync(intervalMs = 30_000): void {
  if (autoSyncTimer !== null) return;
  globalThis.addEventListener?.("online", () => void flushQueue());
  autoSyncTimer = setInterval(() => void flushQueue(), intervalMs);
  void flushQueue();
}

export function stopAutoSync(): void {
  if (autoSyncTimer !== null) {
    clearInterval(autoSyncTimer);
    autoSyncTimer = null;
  }
}
