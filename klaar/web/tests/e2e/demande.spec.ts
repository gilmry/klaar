/**
 * Story 3.1 — soumission d'une Demande dans un vrai navigateur.
 */
import { test, expect, type Page, type Route, type BrowserContext } from "@playwright/test";

const SESSION = { jeton_acces: "jwt.de.test", expire_dans: 3600 };
const CATALOGUE = {
  locale: "fr",
  secteurs: [
    { code: "plomberie", libelle: "Plomberie", skills: [] },
    { code: "serrurerie", libelle: "Serrurerie", skills: [] },
  ],
};
/** Grand-Place. */
const POSITION = { latitude: 50.8467, longitude: 4.3525 };

async function connecte(page: Page) {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(SESSION) }),
  );
  await page.route("**/api/v1/catalog/sectors*", (route: Route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(CATALOGUE) }),
  );
}

async function autoriserPosition(contexte: BrowserContext) {
  await contexte.grantPermissions(["geolocation"]);
  await contexte.setGeolocation(POSITION);
}

async function servirDemande(
  page: Page,
  statut: number,
  corps: unknown,
): Promise<Array<Record<string, unknown>>> {
  const recues: Array<Record<string, unknown>> = [];
  await page.route("**/api/v1/requests", async (route: Route) => {
    recues.push({
      corps: JSON.parse(route.request().postData() ?? "{}"),
      autorisation: route.request().headers()["authorization"],
    });
    await route.fulfill({
      status: statut,
      contentType: "application/json",
      body: JSON.stringify(corps),
    });
  });
  return recues;
}

async function remplir(page: Page) {
  await page.selectOption('[data-champ="secteur"]', "plomberie");
  await page.fill('[data-champ="description"]', "Fuite sous l'évier");
}

test("@happy une demande complète part avec la position et le jeton", async ({ page, context }) => {
  await connecte(page);
  await autoriserPosition(context);
  const recues = await servirDemande(page, 201, {
    id: "abc",
    statut: "BROADCASTING",
    code: "REQUEST_CREATED",
  });

  await page.goto("/demande");
  await remplir(page);
  await page.click('[data-action="envoyer-demande"]');

  await expect(page.locator('[data-demande="creee"]')).toContainText(/diffusée/i);
  expect(recues).toHaveLength(1);
  const corps = recues[0].corps as Record<string, unknown>;
  expect(corps.secteur).toBe("plomberie");
  expect(corps.urgence).toBe("NORMAL");
  expect(Number(corps.latitude)).toBeCloseTo(POSITION.latitude, 3);
  expect(Number(corps.longitude)).toBeCloseTo(POSITION.longitude, 3);
  // Le jeton voyage en en-tête, jamais dans le corps.
  expect(recues[0].autorisation).toBe("Bearer jwt.de.test");
  expect(JSON.stringify(corps)).not.toContain("jwt.de.test");
});

test("@happy un doublon est présenté comme une demande déjà en cours", async ({
  page,
  context,
}) => {
  await connecte(page);
  await autoriserPosition(context);
  await servirDemande(page, 200, {
    id: "abc",
    statut: "BROADCASTING",
    code: "REQUEST_DUPLICATE",
  });

  await page.goto("/demande");
  await remplir(page);
  await page.click('[data-action="envoyer-demande"]');

  // FR-011 `@edge` : l'utilisateur veut retrouver la sienne, pas apprendre
  // qu'il a cliqué deux fois.
  await expect(page.locator('[data-demande="creee"]')).toContainText(/déjà une demande/i);
});

test("@negative un visiteur non connecté est renvoyé vers la connexion", async ({ page }) => {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({ status: 401, contentType: "application/json", body: '{"code":"REFRESH_INVALID"}' }),
  );
  await page.route("**/api/v1/catalog/sectors*", (route: Route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(CATALOGUE) }),
  );

  await page.goto("/demande");
  await expect(page.locator('[data-etat-demande="anonyme"]')).toBeVisible();
  await expect(page.locator('[data-formulaire="demande"]')).toHaveCount(0);
});

test("@negative une position hors région est expliquée", async ({ page, context }) => {
  await connecte(page);
  await autoriserPosition(context);
  await servirDemande(page, 400, { code: "GEO_OUTSIDE_RBC" });

  await page.goto("/demande");
  await remplir(page);
  await page.click('[data-action="envoyer-demande"]');

  await expect(page.locator("[data-erreur-demande]")).toContainText(/Bruxelles/i);
});

test("@negative un refus de géolocalisation dit ce qu'il empêche", async ({ page, context }) => {
  // Ailleurs la géolocalisation est un confort ; ici, c'est la donnée sans
  // laquelle personne ne peut être envoyé.
  await connecte(page);
  await context.clearPermissions();
  let appels = 0;
  await page.route("**/api/v1/requests", async (route: Route) => {
    appels += 1;
    await route.fulfill({ status: 201, body: "{}" });
  });

  await page.goto("/demande");
  await remplir(page);
  await page.click('[data-action="envoyer-demande"]');

  await expect(page.locator("[data-erreur-demande]")).toContainText(/position/i);
  expect(appels, "rien ne part sans position").toBe(0);
});

test("@edge le formulaire refuse de partir incomplet", async ({ page, context }) => {
  await connecte(page);
  await autoriserPosition(context);
  let appels = 0;
  await page.route("**/api/v1/requests", async (route: Route) => {
    appels += 1;
    await route.fulfill({ status: 201, body: "{}" });
  });

  await page.goto("/demande");
  await expect(page.locator('[data-action="envoyer-demande"]')).toBeDisabled();

  await page.selectOption('[data-champ="secteur"]', "plomberie");
  await expect(page.locator('[data-action="envoyer-demande"]')).toBeDisabled();

  await page.fill('[data-champ="description"]', "Fuite");
  await expect(page.locator('[data-action="envoyer-demande"]')).toBeEnabled();
  expect(appels).toBe(0);
});

test("@edge le compteur de caractères suit la saisie", async ({ page, context }) => {
  await connecte(page);
  await autoriserPosition(context);
  await page.goto("/demande");

  await expect(page.locator("[data-restant]")).toContainText("2000");
  await page.fill('[data-champ="description"]', "Fuite");
  await expect(page.locator("[data-restant]")).toContainText("1995");
});

test("@security la description hostile n'est pas interprétée par la page", async ({
  page,
  context,
}) => {
  await connecte(page);
  await autoriserPosition(context);
  const recues = await servirDemande(page, 201, {
    id: "abc",
    statut: "BROADCASTING",
    code: "REQUEST_CREATED",
  });

  await page.goto("/demande");
  await page.selectOption('[data-champ="secteur"]', "plomberie");
  await page.fill('[data-champ="description"]', "<img src=x onerror=alert(1)>");
  await page.click('[data-action="envoyer-demande"]');

  await expect(page.locator('[data-demande="creee"]')).toBeVisible();
  // Le texte part tel quel vers l'API — c'est du texte, pas du balisage — et
  // aucune balise n'a été injectée dans la page.
  expect((recues[0].corps as Record<string, string>).description).toBe(
    "<img src=x onerror=alert(1)>",
  );
  // Viser l'image injectée, pas toutes les images : l'en-tête en porte une,
  // légitime. Compter `img` tout court ferait échouer ce test pour le logo.
  expect(await page.locator('img[src="x"]').count()).toBe(0);
});

test("@security la position n'est demandée qu'à l'envoi", async ({ page, context }) => {
  // Une invite de géolocalisation à l'arrivée, avant que le visiteur n'ait rien
  // demandé, est refusée par réflexe — et ce refus est définitif dans plusieurs
  // navigateurs.
  await connecte(page);
  await autoriserPosition(context);
  await page.goto("/demande");

  const demandee = await page.evaluate(() => {
    let appelee = false;
    const original = navigator.geolocation.getCurrentPosition.bind(navigator.geolocation);
    (window as unknown as { __geoAppelee: () => boolean }).__geoAppelee = () => appelee;
    navigator.geolocation.getCurrentPosition = ((...args: unknown[]) => {
      appelee = true;
      return (original as (...a: unknown[]) => void)(...args);
    }) as typeof navigator.geolocation.getCurrentPosition;
    return appelee;
  });
  expect(demandee).toBe(false);

  await expect(page.locator('[data-formulaire="demande"]')).toBeVisible();
});
