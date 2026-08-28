/**
 * Story 0.2 — queue d'écritures hors-ligne, quatre classes de test.
 *
 * IndexedDB est fourni par `fake-indexeddb` (voir `tests/setup.ts`) : c'est une
 * implémentation conforme, pas un bouchon, donc les transactions et l'ordre
 * d'insertion sont réellement exercés.
 */
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import { deleteDB } from "idb";
import {
  enqueue,
  flushQueue,
  pendingCount,
  deadCount,
  listDead,
  closeDb,
} from "../src/lib/offlineQueue";
import { oublierJeton } from "../src/lib/connexion";

interface AppelCapture {
  url: string;
  method: string;
  idempotencyKey: string | undefined;
  body: string | undefined;
}

let appels: AppelCapture[] = [];

/**
 * Installe un `fetch` scripté : une réponse (ou une panne) par appel.
 *
 * **La reprise de session est répondue à part et n'entre pas dans `appels`.**
 * Depuis que le rejeu porte un jeton, `flushQueue` commence par réobtenir un
 * accès quand il n'en a pas en mémoire ; le compter avec les écritures ferait
 * porter chaque assertion sur un appel qui n'est pas celui qu'elle vise.
 */
function scripterFetch(...reponses: Array<Response | "panne-reseau">) {
  let i = 0;
  vi.stubGlobal("fetch", async (url: string, init: RequestInit) => {
    if (String(url).includes("/auth/refresh")) {
      return new Response(JSON.stringify({ jeton_acces: "jwt-de-test", expire_dans: 3600 }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    const headers = (init.headers ?? {}) as Record<string, string>;
    appels.push({
      url,
      method: init.method ?? "GET",
      idempotencyKey: headers["Idempotency-Key"],
      body: init.body as string | undefined,
    });
    const r = reponses[Math.min(i, reponses.length - 1)];
    i += 1;
    if (r === "panne-reseau") throw new TypeError("Failed to fetch");
    return r.clone();
  });
}

const ok = () => new Response(JSON.stringify({ ok: true }), { status: 200 });
const erreur = (status: number, corps = "") => new Response(corps, { status });

beforeEach(async () => {
  appels = [];
  // Le jeton vit en mémoire du module : sans cet oubli, un cas laisserait au
  // suivant une session déjà ouverte et masquerait la reprise.
  oublierJeton();
  await closeDb();
  await deleteDB("klaar-offline");
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("@happy", () => {
  it("envoie les écritures en attente et vide la file", async () => {
    scripterFetch(ok(), ok());
    await enqueue("POST", "/demandes", { secteur: "plomberie" });
    await enqueue("POST", "/demandes", { secteur: "electricite" });
    expect(await pendingCount()).toBe(2);

    const rapport = await flushQueue();

    expect(rapport).toEqual({ sent: 2, dead: 0, interrupted: false });
    expect(await pendingCount()).toBe(0);
    expect(appels.map((a) => a.method)).toEqual(["POST", "POST"]);
    expect(JSON.parse(appels[0].body!)).toEqual({ secteur: "plomberie" });
  });

  it("joint une clé d'idempotence distincte à chaque écriture", async () => {
    scripterFetch(ok());
    await enqueue("POST", "/demandes", { n: 1 });
    await enqueue("POST", "/demandes", { n: 2 });
    await flushQueue();

    const cles = appels.map((a) => a.idempotencyKey);
    expect(cles.every((c) => typeof c === "string" && c.length > 0)).toBe(true);
    expect(new Set(cles).size).toBe(2);
  });

  it("rejoue dans l'ordre d'insertion", async () => {
    // « créer » puis « annuler » ne commutent pas : l'ordre est une propriété
    // du domaine, pas un détail d'implémentation.
    scripterFetch(ok());
    await enqueue("POST", "/demandes", { etape: "creation" });
    await enqueue("DELETE", "/demandes/1", null);
    await flushQueue();

    expect(appels.map((a) => a.url)).toEqual(["/api/v1/demandes", "/api/v1/demandes/1"]);
  });
});

describe("@negative", () => {
  it("écarte une écriture définitivement refusée au lieu de la rejouer sans fin", async () => {
    scripterFetch(erreur(422, "secteur inconnu"));
    await enqueue("POST", "/demandes", { secteur: "licorne" });

    const rapport = await flushQueue();

    expect(rapport.dead).toBe(1);
    expect(await pendingCount()).toBe(0);
    const morts = await listDead();
    expect(morts).toHaveLength(1);
    expect(morts[0].status).toBe(422);
    expect(morts[0].detail).toBe("secteur inconnu");
    // Le contenu est conservé : l'utilisateur doit pouvoir le récupérer.
    expect(morts[0].body).toEqual({ secteur: "licorne" });
  });

  it("garde en file une erreur serveur, qui est rejouable", async () => {
    scripterFetch(erreur(503));
    await enqueue("POST", "/demandes", { secteur: "plomberie" });

    const rapport = await flushQueue();

    expect(rapport).toEqual({ sent: 0, dead: 0, interrupted: true });
    expect(await pendingCount()).toBe(1);
    expect(await deadCount()).toBe(0);
  });

  it("traite 429 comme rejouable, malgré son code 4xx", async () => {
    scripterFetch(erreur(429));
    await enqueue("POST", "/demandes", {});
    await flushQueue();
    expect(await pendingCount()).toBe(1);
    expect(await deadCount()).toBe(0);
  });
});

describe("@edge", () => {
  it("s'arrête à la coupure réseau sans sauter l'élément bloquant", async () => {
    scripterFetch(ok(), "panne-reseau", ok());
    await enqueue("POST", "/a", { n: 1 });
    await enqueue("POST", "/b", { n: 2 });
    await enqueue("POST", "/c", { n: 3 });

    const rapport = await flushQueue();

    expect(rapport.sent).toBe(1);
    expect(rapport.interrupted).toBe(true);
    // /c n'a pas été tenté : le sauter aurait inversé son ordre avec /b.
    expect(appels.map((a) => a.url)).toEqual(["/api/v1/a", "/api/v1/b"]);
    expect(await pendingCount()).toBe(2);
  });

  it("ne lance pas deux rejeux concurrents", async () => {
    scripterFetch(ok());
    await enqueue("POST", "/demandes", {});
    const [premier, second] = await Promise.all([flushQueue(), flushQueue()]);
    expect(premier.sent + second.sent).toBe(1);
    expect(appels).toHaveLength(1);
  });

  it("ne fait rien, sans erreur, sur une file vide", async () => {
    scripterFetch(ok());
    expect(await flushQueue()).toEqual({ sent: 0, dead: 0, interrupted: false });
    expect(appels).toHaveLength(0);
  });
});

describe("@security", () => {
  it("rejoue une écriture avec la même clé d'idempotence après un échec", async () => {
    // C'est l'invariant qui protège du double effet : sans clé stable, un
    // rejeu après coupure créerait une seconde Demande, ou un second séquestre.
    scripterFetch("panne-reseau", ok());
    await enqueue("POST", "/devis/1/acceptation", { montant: 12000 });

    await flushQueue();
    await flushQueue();

    expect(appels).toHaveLength(2);
    expect(appels[0].idempotencyKey).toBe(appels[1].idempotencyKey);
  });

  it("borne ce qu'un serveur peut faire écrire dans le stockage local", async () => {
    scripterFetch(erreur(400, "x".repeat(5000)));
    await enqueue("POST", "/demandes", {});
    await flushQueue();

    const [mort] = await listDead();
    expect(mort.detail).toHaveLength(500);
  });

  it("n'envoie jamais une écriture sans clé d'idempotence", async () => {
    scripterFetch(ok(), ok(), ok());
    await enqueue("POST", "/a", {});
    await enqueue("PUT", "/b", {});
    await enqueue("DELETE", "/c", null);
    await flushQueue();

    expect(appels).toHaveLength(3);
    expect(appels.every((a) => Boolean(a.idempotencyKey))).toBe(true);
  });
});

describe("@security rejeu authentifié", () => {
  it("reprend la session puis porte le jeton sur l'écriture rejouée", async () => {
    // Le jeton d'accès vit en mémoire et ne survit pas au rechargement ; une
    // écriture mise en file hors connexion est rejouée après, donc sans jeton.
    // Sans reprise de session, elle recevrait un 401 et finirait dans les
    // refusées — c'est-à-dire perdue, alors que le refresh était valable.
    const { enqueue, flushQueue } = await import("../src/lib/offlineQueue");
    const { oublierJeton } = await import("../src/lib/connexion");
    oublierJeton();

    const appels: Array<{ url: string; entetes: Record<string, string> }> = [];
    const origine = globalThis.fetch;
    globalThis.fetch = (async (url: unknown, init: RequestInit) => {
      const chemin = String(url);
      appels.push({ url: chemin, entetes: { ...((init?.headers as Record<string, string>) ?? {}) } });
      if (chemin.includes("/auth/refresh")) {
        return new Response(JSON.stringify({ jeton_acces: "jwt-de-file", expire_dans: 3600 }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response("{}", { status: 200, headers: { "content-type": "application/json" } });
    }) as typeof fetch;

    try {
      await enqueue("POST", "/requests", { secteur: "plomberie" });
      const rapport = await flushQueue();
      expect(rapport.sent).toBe(1);

      const rejeu = appels.find((a) => a.url.includes("/requests"));
      expect(rejeu?.entetes.Authorization).toBe("Bearer jwt-de-file");
      // La clé d'idempotence voyage avec, pour que le service puisse un jour
      // s'en servir.
      expect(rejeu?.entetes["Idempotency-Key"]).toBeTruthy();
    } finally {
      globalThis.fetch = origine;
      oublierJeton();
    }
  });

  it("ne reprend pas la session quand il n'y a rien à rejouer", async () => {
    // Sans cette garde, la file ferait une rotation de refresh toutes les
    // trente secondes pour rien — et une rotation trop fréquente finit par
    // ressembler au rejeu d'un jeton volé.
    const { flushQueue } = await import("../src/lib/offlineQueue");
    let appels = 0;
    const origine = globalThis.fetch;
    globalThis.fetch = (async () => {
      appels += 1;
      return new Response("{}", { status: 200 });
    }) as typeof fetch;

    try {
      await flushQueue();
      expect(appels).toBe(0);
    } finally {
      globalThis.fetch = origine;
    }
  });
});
