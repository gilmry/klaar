/**
 * Story 0.2 — la PWA, vérifiée dans un vrai navigateur.
 *
 * Ces cas existent parce que rien de ce qu'ils couvrent ne se prouve hors
 * navigateur : un service worker qui s'enregistre, un manifeste que le
 * navigateur accepte, une navigation qui aboutit sans réseau. Les tester à
 * l'unité reviendrait à tester des bouchons.
 */
import { test, expect, type Page } from "@playwright/test";

/** Attend que le service worker soit réellement actif, pas seulement inscrit. */
async function attendreServiceWorkerActif(page: Page): Promise<void> {
  await page.waitForFunction(
    async () => {
      const reg = await navigator.serviceWorker.getRegistration("/");
      return Boolean(reg?.active && navigator.serviceWorker.controller);
    },
    undefined,
    { timeout: 20_000 },
  );
}

test.describe("@happy", () => {
  test("sert l'accueil et enregistre son service worker", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Klaar", level: 1 })).toBeVisible();

    await attendreServiceWorkerActif(page);
    const scope = await page.evaluate(async () => {
      const reg = await navigator.serviceWorker.getRegistration("/");
      return reg?.scope ?? null;
    });
    expect(scope).toContain("127.0.0.1:4321");
  });

  test("déclare un manifeste installable dont les icônes existent", async ({ page, request }) => {
    await page.goto("/");
    const href = await page.locator('link[rel="manifest"]').getAttribute("href");
    expect(href).toBe("/manifest.webmanifest");

    const manifeste = await (await request.get(href!)).json();
    // Les trois champs sans lesquels aucun navigateur ne propose l'installation.
    expect(manifeste.name).toBeTruthy();
    expect(manifeste.start_url).toBeTruthy();
    expect(["standalone", "fullscreen", "minimal-ui"]).toContain(manifeste.display);

    // Une icône maskable est ce qui évite le rendu en vignette rognée sur
    // Android. Elle est facile à déclarer et facile à oublier de fournir.
    const maskable = manifeste.icons.filter((i: { purpose?: string }) =>
      i.purpose?.includes("maskable"),
    );
    expect(maskable.length).toBeGreaterThan(0);

    for (const icone of manifeste.icons) {
      const reponse = await request.get(icone.src);
      expect(reponse.status(), `icône déclarée mais absente : ${icone.src}`).toBe(200);
      expect(reponse.headers()["content-type"]).toContain("image/png");
    }
  });

  test("affiche l'état de connexion", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".klaar-etat")).toContainText("En ligne");
  });
});

test.describe("@negative", () => {
  test("sert la page hors-ligne pour une adresse jamais visitée", async ({ page, context }) => {
    await page.goto("/");
    await attendreServiceWorkerActif(page);

    await context.setOffline(true);
    // Sans service worker, cette navigation donnerait l'écran d'erreur du
    // navigateur. Avec, elle doit donner notre page.
    await page.goto("/une-adresse-jamais-visitee");
    await expect(page.getByRole("heading", { name: "Pas de réseau" })).toBeVisible();
    await context.setOffline(false);
  });
});

test.describe("@edge", () => {
  test("rouvre une page déjà visitée sans réseau et signale l'état", async ({ page, context }) => {
    await page.goto("/");
    await attendreServiceWorkerActif(page);
    await page.reload(); // première réponse mise en cache par le service worker

    await context.setOffline(true);
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Klaar", level: 1 })).toBeVisible();
    await expect(page.locator(".klaar-etat")).toContainText("Hors ligne");
    await context.setOffline(false);
  });
});

test.describe("@security", () => {
  test("ne contacte aucune origine tierce", async ({ page }) => {
    // Une PWA qui charge une police ou un script depuis un CDN fait fuiter
    // l'adresse IP de chaque visiteur vers ce tiers, ce qui est un traitement
    // au sens du RGPD que personne n'a documenté.
    const tierces: string[] = [];
    page.on("request", (req) => {
      const origine = new URL(req.url()).origin;
      if (origine !== "http://127.0.0.1:4321") tierces.push(req.url());
    });

    await page.goto("/");
    await attendreServiceWorkerActif(page);
    expect(tierces, `requêtes vers une origine tierce : ${tierces.join(", ")}`).toEqual([]);
  });

  test("ne met jamais de réponse d'API en cache", async ({ page }) => {
    await page.goto("/");
    await attendreServiceWorkerActif(page);

    // /api/ porte des données personnelles et le Cache Storage n'est pas
    // chiffré : le service worker doit laisser passer ces requêtes sans les
    // retenir.
    await page.evaluate(() => fetch("/api/v1/health").catch(() => null));
    const misesEnCache = await page.evaluate(async () => {
      const noms = await caches.keys();
      const urls: string[] = [];
      for (const nom of noms) {
        const cache = await caches.open(nom);
        for (const req of await cache.keys()) urls.push(req.url);
      }
      return urls;
    });
    expect(misesEnCache.filter((u) => u.includes("/api/"))).toEqual([]);
  });
});

test.describe("@security indicateur de connexion", () => {
  test("n'affirme pas « en ligne » avant d'avoir vérifié", async ({ page }) => {
    // Le rendu statique est servi avant que l'îlot ne s'hydrate, et il peut
    // n'être jamais hydraté — chunk absent du cache lors d'un premier passage
    // hors ligne. Partir de « en ligne » faisait alors mentir la pastille
    // précisément quand le réseau manquait. Trouvé en filmant un parcours.
    const reponse = await page.request.get("/");
    const html = await reponse.text();
    expect(html).not.toContain('data-etat="en-ligne"');
    expect(html).toContain('data-etat="inconnu"');
  });
});
