/**
 * Story 1.9 — page d'effacement de compte dans un vrai navigateur.
 */
import { test, expect, type Page, type Route } from "@playwright/test";

const SESSION = { jeton_acces: "jwt.de.test", expire_dans: 3600 };

async function connecte(page: Page) {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(SESSION),
    }),
  );
}

async function anonyme(page: Page) {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({
      status: 401,
      contentType: "application/json",
      body: '{"code":"REFRESH_INVALID"}',
    }),
  );
}

test("@happy la confirmation exacte programme l'effacement", async ({ page }) => {
  await connecte(page);
  const recues: Array<Record<string, unknown>> = [];
  await page.route("**/api/v1/me/erase", async (route: Route) => {
    recues.push({
      corps: JSON.parse(route.request().postData() ?? "{}"),
      autorisation: route.request().headers()["authorization"],
    });
    await route.fulfill({
      status: 202,
      contentType: "application/json",
      body: JSON.stringify({ code: "ERASURE_SCHEDULED", dans_jours: 30 }),
    });
  });

  await page.goto("/mon-compte");
  await page.click('[data-action="ouvrir-effacement"]');
  await page.fill('[data-champ="confirmation"]', "DELETE");
  await page.click('[data-action="confirmer-effacement"]');

  await expect(page.locator('[data-effacement="programme"]')).toContainText("30");
  expect(recues).toHaveLength(1);
  expect(recues[0].corps).toEqual({ confirmation: "DELETE" });
  // Le jeton voyage en en-tête, jamais dans l'URL ni le corps.
  expect(recues[0].autorisation).toBe("Bearer jwt.de.test");
});

test("@happy l'annulation est proposée et ramène à l'état initial", async ({ page }) => {
  await connecte(page);
  await page.route("**/api/v1/me/erase", (route: Route) =>
    route.fulfill({
      status: 202,
      contentType: "application/json",
      body: JSON.stringify({ code: "ERASURE_SCHEDULED", dans_jours: 30 }),
    }),
  );
  let annulations = 0;
  await page.route("**/api/v1/me/erase/cancel", async (route: Route) => {
    annulations += 1;
    await route.fulfill({ status: 204, body: "" });
  });

  await page.goto("/mon-compte");
  await page.click('[data-action="ouvrir-effacement"]');
  await page.fill('[data-champ="confirmation"]', "DELETE");
  await page.click('[data-action="confirmer-effacement"]');
  await expect(page.locator('[data-effacement="programme"]')).toBeVisible();

  await page.click('[data-action="annuler-effacement"]');
  await expect(page.locator('[data-action="ouvrir-effacement"]')).toBeVisible();
  expect(annulations).toBe(1);
});

test("@negative un visiteur non connecté est renvoyé vers la connexion", async ({ page }) => {
  await anonyme(page);
  await page.goto("/mon-compte");
  await expect(page.locator('[data-etat-compte="anonyme"]')).toBeVisible();
  await expect(page.locator('a[href="/connexion"]')).toBeVisible();
  await expect(page.locator('[data-action="ouvrir-effacement"]')).toHaveCount(0);
});

test("@negative un refus du serveur est traduit", async ({ page }) => {
  await connecte(page);
  await page.route("**/api/v1/me/erase", (route: Route) =>
    route.fulfill({
      status: 400,
      contentType: "application/json",
      body: '{"code":"CONFIRMATION_REQUIRED"}',
    }),
  );

  await page.goto("/mon-compte");
  await page.click('[data-action="ouvrir-effacement"]');
  await page.fill('[data-champ="confirmation"]', "DELETE");
  await page.click('[data-action="confirmer-effacement"]');
  await expect(page.locator("[data-erreur-compte]")).toContainText("DELETE");
});

test("@edge l'effacement est replié derrière un bouton et une confirmation", async ({ page }) => {
  // Deux gestes délibérés avant l'irréversible : ouvrir, puis recopier un mot.
  await connecte(page);
  let appels = 0;
  await page.route("**/api/v1/me/erase", async (route: Route) => {
    appels += 1;
    await route.fulfill({ status: 202, body: '{"code":"ERASURE_SCHEDULED","dans_jours":30}' });
  });

  await page.goto("/mon-compte");
  await expect(page.locator('[data-champ="confirmation"]')).toHaveCount(0);

  await page.click('[data-action="ouvrir-effacement"]');
  await expect(page.locator('[data-action="confirmer-effacement"]')).toBeDisabled();

  await page.fill('[data-champ="confirmation"]', "delete");
  await expect(page.locator('[data-action="confirmer-effacement"]')).toBeDisabled();

  await page.fill('[data-champ="confirmation"]', "DELETE");
  await expect(page.locator('[data-action="confirmer-effacement"]')).toBeEnabled();
  expect(appels, "aucun appel avant le clic final").toBe(0);
});

test("@edge renoncer referme sans rien envoyer", async ({ page }) => {
  await connecte(page);
  let appels = 0;
  await page.route("**/api/v1/me/erase", async (route: Route) => {
    appels += 1;
    await route.fulfill({ status: 202, body: '{"code":"ERASURE_SCHEDULED","dans_jours":30}' });
  });

  await page.goto("/mon-compte");
  await page.click('[data-action="ouvrir-effacement"]');
  await page.fill('[data-champ="confirmation"]', "DELETE");
  await page.click('[data-action="renoncer"]');

  await expect(page.locator('[data-action="ouvrir-effacement"]')).toBeVisible();
  expect(appels).toBe(0);
});

test("@security le jeton n'apparaît ni dans l'URL ni dans le DOM", async ({ page }) => {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ jeton_acces: "JETON-RECONNAISSABLE", expire_dans: 3600 }),
    }),
  );
  await page.route("**/api/v1/me/erase", (route: Route) =>
    route.fulfill({ status: 202, body: '{"code":"ERASURE_SCHEDULED","dans_jours":30}' }),
  );

  await page.goto("/mon-compte");
  await page.click('[data-action="ouvrir-effacement"]');
  await page.fill('[data-champ="confirmation"]', "DELETE");
  await page.click('[data-action="confirmer-effacement"]');
  await expect(page.locator('[data-effacement="programme"]')).toBeVisible();

  expect(page.url()).not.toContain("JETON-RECONNAISSABLE");
  expect(await page.content()).not.toContain("JETON-RECONNAISSABLE");
});
