/**
 * Story 2.2 — page catalogue dans un vrai navigateur.
 */
import { test, expect, type Page, type Route } from "@playwright/test";

const CATALOGUE = {
  locale: "fr",
  secteurs: [
    {
      code: "plomberie",
      libelle: "Plomberie",
      skills: [
        { code: "fuite-eau", libelle: "Fuite d'eau" },
        { code: "debouchage", libelle: "Débouchage" },
      ],
    },
    { code: "auto", libelle: "Auto", skills: [] },
  ],
};

async function servir(page: Page, statut: number, corps: unknown): Promise<string[]> {
  const urls: string[] = [];
  await page.route("**/api/v1/catalog/sectors*", async (route: Route) => {
    urls.push(route.request().url());
    await route.fulfill({
      status: statut,
      contentType: "application/json",
      body: JSON.stringify(corps),
    });
  });
  return urls;
}

test("@happy affiche les secteurs et leurs compétences", async ({ page }) => {
  const urls = await servir(page, 200, CATALOGUE);

  await page.goto("/catalogue");

  await expect(page.locator('[data-secteur="plomberie"]')).toContainText("Plomberie");
  await expect(page.locator('[data-skill="fuite-eau"]')).toContainText("Fuite d'eau");
  // La langue de la page voyage dans la requête : messages, courriels et
  // catalogue parlent la même.
  expect(urls[0]).toContain("locale=fr");
});

test("@edge un secteur sans compétence s'affiche quand même", async ({ page }) => {
  // C'est un secteur ouvert dont les compétences ne sont pas encore décrites,
  // pas une anomalie à masquer.
  await servir(page, 200, CATALOGUE);
  await page.goto("/catalogue");
  await expect(page.locator('[data-secteur="auto"]')).toContainText("Auto");
});

test("@edge un catalogue vide le dit sans crier à l'erreur", async ({ page }) => {
  // FR-008 `@edge` : 200 avec liste vide. Un état de démarrage légitime.
  await servir(page, 200, { locale: "fr", secteurs: [] });
  await page.goto("/catalogue");
  await expect(page.locator('[data-etat-catalogue="vide"]')).toBeVisible();
  await expect(page.locator("[data-erreur-catalogue]")).toHaveCount(0);
});

test("@negative la maintenance est annoncée comme telle", async ({ page }) => {
  await servir(page, 503, { code: "CATALOG_MAINTENANCE" });
  await page.goto("/catalogue");
  await expect(page.locator("[data-erreur-catalogue]")).toContainText(/mise à jour/i);
});

test("@negative une coupure réseau est distinguée d'une panne du service", async ({ page }) => {
  await page.route("**/api/v1/catalog/sectors*", (route: Route) => route.abort("failed"));
  await page.goto("/catalogue");
  await expect(page.locator("[data-erreur-catalogue]")).toContainText(/connexion/i);
});

test("@security la page ne demande aucune authentification", async ({ page }) => {
  // Le catalogue est public : y exiger un jeton empêcherait un visiteur de
  // savoir ce que fait le service avant de créer un compte.
  let autorisation: string | undefined;
  await page.route("**/api/v1/catalog/sectors*", async (route: Route) => {
    autorisation = route.request().headers()["authorization"];
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(CATALOGUE),
    });
  });

  await page.goto("/catalogue");
  await expect(page.locator('[data-secteur="plomberie"]')).toBeVisible();
  expect(autorisation).toBeUndefined();
});

test("@security la réponse d'API n'est pas mise en cache par le service worker", async ({
  page,
}) => {
  // Le service worker ne met jamais `/api/` en cache : une réponse figée là
  // survivrait à la mise à jour du catalogue sans que rien ne l'invalide.
  await servir(page, 200, CATALOGUE);
  await page.goto("/catalogue");
  await expect(page.locator('[data-secteur="plomberie"]')).toBeVisible();

  const enCache = await page.evaluate(async () => {
    const noms = await caches.keys();
    for (const nom of noms) {
      const cache = await caches.open(nom);
      const clefs = await cache.keys();
      if (clefs.some((r) => r.url.includes("/api/"))) return true;
    }
    return false;
  });
  expect(enCache).toBe(false);
});
