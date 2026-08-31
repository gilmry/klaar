/**
 * Navigation persistante, dans un vrai navigateur.
 *
 * Les tests unitaires (`tests/navigation.test.ts`) vérifient *ce que* le menu
 * doit contenir. Ici on vérifie qu'il est réellement rendu sur chaque page de
 * la coquille, qu'on peut circuler par des clics, et qu'il change au moment où
 * la session s'ouvre — la partie qu'aucune fonction pure ne peut prouver.
 */
import { test, expect, type Page, type Route } from "@playwright/test";

const SESSION = { jeton_acces: "jwt.de.test", expire_dans: 3600 };

/** Toutes les pages atteignables par un clic, plus les pages de destination. */
const PAGES = [
  "/",
  "/demande",
  "/catalogue",
  "/inscription",
  "/connexion",
  "/mon-compte",
  "/prestataire",
  "/ops",
  "/mentions-legales",
  "/hors-ligne",
  "/verifier-email",
];

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

test("@happy la navigation est présente sur toutes les pages", async ({ page }) => {
  await anonyme(page);
  for (const chemin of PAGES) {
    await page.goto(chemin);
    await expect(page.locator("[data-navigation]"), `menu absent sur ${chemin}`).toBeVisible();
    await expect(page.locator('.klaar-pied a[href="/mentions-legales"]')).toBeVisible();
  }
});

test("@happy chaque lien du menu mène à une page servie", async ({ page }) => {
  await anonyme(page);
  await page.goto("/");
  await expect(page.locator('[data-etat-navigation="visiteur"]')).toBeVisible();

  const href = await page.locator("[data-navigation] a").evaluateAll((liens) =>
    liens.map((l) => (l as HTMLAnchorElement).getAttribute("href") ?? ""),
  );
  expect(href.length).toBeGreaterThan(3);

  for (const cible of href) {
    // Une 404 ici veut dire un lien vers une page qui n'a jamais été
    // construite : exactement le défaut qu'un menu écrit à la main introduit.
    const reponse = await page.goto(cible);
    expect(reponse?.status(), `${cible} n'est pas servi`).toBeLessThan(400);
  }
});

test("@happy on va du catalogue à la demande en un clic", async ({ page }) => {
  // Avant la navigation persistante, il fallait revenir à l'accueil : aucune
  // page autre que l'accueil ne pointait vers `/demande`.
  await anonyme(page);
  await page.goto("/catalogue");
  await page.click('[data-navigation] a[data-lien="/demande"]');
  await expect(page).toHaveURL(/\/demande\/?$/);
});

test("@happy la console d'exploitation est atteignable depuis le pied de page", async ({
  page,
}) => {
  // Elle ne l'était par aucun lien du site auparavant.
  await anonyme(page);
  await page.goto("/");
  await page.click('.klaar-pied a[href="/ops"]');
  await expect(page).toHaveURL(/\/ops\/?$/);
  await expect(page.locator('[data-formulaire="ops"], form')).toBeVisible();
});

test("@negative un visiteur ne voit pas « mon compte » dans le menu", async ({ page }) => {
  await anonyme(page);
  await page.goto("/");
  await expect(page.locator('[data-etat-navigation="visiteur"]')).toBeVisible();
  await expect(page.locator('[data-navigation] a[data-lien="/mon-compte"]')).toHaveCount(0);
  await expect(page.locator('[data-navigation] a[data-lien="/connexion"]')).toBeVisible();
});

test("@happy un utilisateur connecté voit son compte et pas « créer un compte »", async ({
  page,
}) => {
  await connecte(page);
  await page.goto("/");
  await expect(page.locator('[data-etat-navigation="connecte"]')).toBeVisible();
  await expect(page.locator('[data-navigation] a[data-lien="/mon-compte"]')).toBeVisible();
  await expect(page.locator('[data-navigation] a[data-lien="/inscription"]')).toHaveCount(0);
});

test("@edge le menu bascule à la connexion, sans rechargement", async ({ page }) => {
  // Le formulaire de connexion ne redirige nulle part : la page reste la même
  // après l'envoi. Un menu qui ne lirait la session qu'au montage continuerait
  // d'afficher « Me connecter » à quelqu'un qui vient de se connecter.
  await anonyme(page);
  await page.route("**/api/v1/auth/login", (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(SESSION),
    }),
  );

  await page.goto("/connexion");
  await expect(page.locator('[data-etat-navigation="visiteur"]')).toBeVisible();

  await page.fill("#connexion-email", "personne@exemple.be");
  await page.fill("#connexion-mot-de-passe", "motdepassecorrect");
  await page.click('[data-action="connecter"]');

  await expect(page.locator('[data-etat-navigation="connecte"]')).toBeVisible();
  await expect(page.locator('[data-navigation] a[data-lien="/mon-compte"]')).toBeVisible();
});

test("@edge la page courante est annoncée aux lecteurs d'écran", async ({ page }) => {
  await anonyme(page);
  await page.goto("/catalogue");
  await expect(page.locator('[data-navigation] a[aria-current="page"]')).toHaveAttribute(
    "href",
    "/catalogue",
  );
});
