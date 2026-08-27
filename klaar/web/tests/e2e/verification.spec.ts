/**
 * Story 1.2 — page de confirmation d'adresse dans un vrai navigateur.
 *
 * L'API est interceptée : le contrat est couvert côté Rust par
 * `crates/klaar-api/tests/verification_routes.rs`, contre un vrai PostgreSQL.
 * Ce qui se vérifie ici est le comportement de la page — quand elle appelle,
 * ce qu'elle affiche, et ce qu'elle laisse derrière elle dans l'URL.
 */
import { test, expect, type Page, type Route } from "@playwright/test";

const ROUTE_VERIFY = "**/api/v1/auth/verify-email";
const JETON = "JETON-DE-TEST-RECONNAISSABLE";

async function interceptrice(
  page: Page,
  reponse: { statut: number; corps: unknown },
): Promise<Array<Record<string, unknown>>> {
  const recues: Array<Record<string, unknown>> = [];
  await page.route(ROUTE_VERIFY, async (route: Route) => {
    recues.push(JSON.parse(route.request().postData() ?? "{}"));
    await route.fulfill({
      status: reponse.statut,
      contentType: "application/json",
      body: JSON.stringify(reponse.corps),
    });
  });
  return recues;
}

test("@happy un jeton valide confirme le compte sans clic supplémentaire", async ({ page }) => {
  const recues = await interceptrice(page, { statut: 200, corps: { code: "EMAIL_VERIFIED" } });

  await page.goto(`/verifier-email?jeton=${JETON}`);

  await expect(page.locator('[data-etat-verification="confirme"]')).toContainText(/actif/i);
  expect(recues).toEqual([{ jeton: JETON }]);
});

test("@happy le second passage reste un succès", async ({ page }) => {
  await interceptrice(page, { statut: 200, corps: { code: "EMAIL_ALREADY_VERIFIED" } });
  await page.goto(`/verifier-email?jeton=${JETON}`);
  await expect(page.locator('[data-etat-verification="confirme"]')).toContainText(/actif/i);
  await expect(page.locator('[data-etat-verification="echec"]')).toHaveCount(0);
});

test("@negative un jeton expiré dit quoi faire ensuite", async ({ page }) => {
  await interceptrice(page, { statut: 410, corps: { code: "TOKEN_EXPIRED" } });

  await page.goto(`/verifier-email?jeton=${JETON}`);

  const alerte = page.locator('[data-etat-verification="echec"]');
  await expect(alerte).toContainText(/inscription/i);
  // Constater l'échec ne suffit pas : la page doit offrir la sortie.
  await expect(page.locator('a[href="/inscription"]')).toBeVisible();
});

test("@negative un jeton inconnu est refusé", async ({ page }) => {
  await interceptrice(page, { statut: 404, corps: { code: "TOKEN_INVALID" } });
  await page.goto(`/verifier-email?jeton=${JETON}`);
  await expect(page.locator('[data-etat-verification="echec"]')).toContainText(/pas valide/i);
});

test("@edge une URL sans jeton appelle quand même et affiche le bon refus", async ({ page }) => {
  // Le serveur reste seul juge : la page ne décide pas à sa place qu'un jeton
  // vide est invalide, elle le lui présente et traduit sa réponse.
  const recues = await interceptrice(page, { statut: 400, corps: { code: "TOKEN_MISSING" } });

  await page.goto("/verifier-email");

  await expect(page.locator('[data-etat-verification="echec"]')).toContainText(/incomplet/i);
  expect(recues).toEqual([{ jeton: "" }]);
});

test("@edge une coupure réseau invite à rouvrir le lien plus tard", async ({ page }) => {
  await page.route(ROUTE_VERIFY, (route: Route) => route.abort("failed"));
  await page.goto(`/verifier-email?jeton=${JETON}`);
  await expect(page.locator('[data-etat-verification="echec"]')).toContainText(/connexion/i);
});

test("@security le jeton disparaît de la barre d'adresse", async ({ page }) => {
  // Sans cela, il reste dans l'historique, dans les captures d'écran et dans
  // l'en-tête `Referer` de tout lien suivi depuis cette page.
  await interceptrice(page, { statut: 200, corps: { code: "EMAIL_VERIFIED" } });

  await page.goto(`/verifier-email?jeton=${JETON}`);
  await expect(page.locator('[data-etat-verification="confirme"]')).toBeVisible();

  expect(page.url()).not.toContain(JETON);
  expect(page.url()).not.toContain("jeton=");
});

test("@security le jeton n'est jamais écrit dans la page", async ({ page }) => {
  await interceptrice(page, { statut: 404, corps: { code: "TOKEN_INVALID" } });
  await page.goto(`/verifier-email?jeton=${JETON}`);
  await expect(page.locator('[data-etat-verification="echec"]')).toBeVisible();
  expect(await page.content()).not.toContain(JETON);
});

test("@security ouvrir la page ne consomme rien sans JavaScript", async ({ browser }) => {
  // C'est la raison d'être du POST : un analyseur de liens de messagerie
  // charge la page mais n'exécute pas son script. Simulé en désactivant
  // JavaScript, comme le ferait un tel analyseur.
  const contexte = await browser.newContext({ javaScriptEnabled: false });
  const page = await contexte.newPage();
  let appels = 0;
  await page.route(ROUTE_VERIFY, async (route: Route) => {
    appels += 1;
    await route.fulfill({ status: 200, body: '{"code":"EMAIL_VERIFIED"}' });
  });

  await page.goto(`/verifier-email?jeton=${JETON}`);
  await page.waitForTimeout(500);

  expect(appels, "aucun appel ne doit partir du simple chargement de la page").toBe(0);
  await contexte.close();
});
