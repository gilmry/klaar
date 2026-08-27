/**
 * Story 1.3 — page de connexion dans un vrai navigateur.
 */
import { test, expect, type Page, type Route } from "@playwright/test";

const ROUTE_LOGIN = "**/api/v1/auth/login";
const MDP = "Marie@2026Secure";

async function interceptrice(
  page: Page,
  reponse: { statut: number; corps: unknown; cookie?: string },
): Promise<Array<Record<string, unknown>>> {
  const recues: Array<Record<string, unknown>> = [];
  await page.route(ROUTE_LOGIN, async (route: Route) => {
    recues.push(JSON.parse(route.request().postData() ?? "{}"));
    await route.fulfill({
      status: reponse.statut,
      contentType: "application/json",
      headers: reponse.cookie ? { "Set-Cookie": reponse.cookie } : undefined,
      body: JSON.stringify(reponse.corps),
    });
  });
  return recues;
}

/**
 * Neutralise la reprise de session automatique.
 *
 * La page tente un `POST /auth/refresh` au montage. Sans cette interception,
 * les cas qui testent le formulaire verraient d'abord l'état « reprise », puis
 * un appel réseau réel dans le vide.
 */
async function sansReprise(page: Page) {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({ status: 401, contentType: "application/json", body: '{"code":"REFRESH_INVALID"}' }),
  );
}

async function remplir(page: Page, email: string, motDePasse: string) {
  await page.fill("#connexion-email", email);
  await page.fill("#connexion-mot-de-passe", motDePasse);
}

test("@happy une connexion valide affiche l'état connecté", async ({ page }) => {
  const recues = await interceptrice(page, {
    statut: 200,
    corps: { jeton_acces: "jwt.de.test", expire_dans: 3600 },
  });

  await sansReprise(page);
  await page.goto("/connexion");
  await remplir(page, "marie@example.eu", MDP);
  await page.click('[data-action="connecter"]');

  await expect(page.locator("[data-succes-connexion]")).toBeVisible();
  expect(recues).toEqual([{ email: "marie@example.eu", mot_de_passe: MDP }]);
});

test("@negative un refus n'expose pas si l'adresse existe", async ({ page }) => {
  await interceptrice(page, { statut: 401, corps: { code: "INVALID_CREDENTIALS" } });

  await sansReprise(page);
  await page.goto("/connexion");
  await remplir(page, "marie@example.eu", MDP);
  await page.click('[data-action="connecter"]');

  const alerte = page.locator("[data-erreur-connexion]");
  await expect(alerte).toBeVisible();
  await expect(alerte).not.toContainText(/inconnu|n'existe/i);
  await expect(page.locator("[data-succes-connexion]")).toHaveCount(0);
});

test("@negative un compte non vérifié renvoie vers le courriel", async ({ page }) => {
  await interceptrice(page, { statut: 403, corps: { code: "ACCOUNT_NOT_VERIFIED" } });
  await sansReprise(page);
  await page.goto("/connexion");
  await remplir(page, "marie@example.eu", MDP);
  await page.click('[data-action="connecter"]');
  await expect(page.locator("[data-erreur-connexion]")).toContainText(/courriel/i);
});

test("@edge la limitation de débit est annoncée pour ce qu'elle est", async ({ page }) => {
  await interceptrice(page, { statut: 429, corps: { code: "RATE_LIMIT_EXCEEDED" } });
  await sansReprise(page);
  await page.goto("/connexion");
  await remplir(page, "marie@example.eu", MDP);
  await page.click('[data-action="connecter"]');
  await expect(page.locator("[data-erreur-connexion]")).toContainText(/tentatives/i);
});

test("@edge une coupure réseau est distinguée d'un refus", async ({ page }) => {
  await page.route(ROUTE_LOGIN, (route: Route) => route.abort("failed"));
  await sansReprise(page);
  await page.goto("/connexion");
  await remplir(page, "marie@example.eu", MDP);
  await page.click('[data-action="connecter"]');
  await expect(page.locator("[data-erreur-connexion]")).toContainText(/connexion/i);
});

test("@security le jeton n'est écrit ni dans localStorage ni dans sessionStorage", async ({
  page,
}) => {
  // La raison d'être du jeton en mémoire : ces deux stockages sont lisibles par
  // tout script de la page, donc par une faille XSS.
  await interceptrice(page, {
    statut: 200,
    corps: { jeton_acces: "JETON-RECONNAISSABLE", expire_dans: 3600 },
  });

  await sansReprise(page);
  await page.goto("/connexion");
  await remplir(page, "marie@example.eu", MDP);
  await page.click('[data-action="connecter"]');
  await expect(page.locator("[data-succes-connexion]")).toBeVisible();

  const stockage = await page.evaluate(() => ({
    local: JSON.stringify(window.localStorage),
    session: JSON.stringify(window.sessionStorage),
  }));
  expect(stockage.local).not.toContain("JETON-RECONNAISSABLE");
  expect(stockage.session).not.toContain("JETON-RECONNAISSABLE");
});

test("@security le cookie de refresh reste invisible à JavaScript", async ({ page }) => {
  // FR-004 `@security` : `HttpOnly`. Le cookie posé par le serveur ne doit pas
  // apparaître dans `document.cookie`.
  await interceptrice(page, {
    statut: 200,
    corps: { jeton_acces: "jwt.de.test", expire_dans: 3600 },
    cookie: "klaar_refresh=secret-refresh; Path=/api/v1/auth; HttpOnly; SameSite=Lax",
  });

  await sansReprise(page);
  await page.goto("/connexion");
  await remplir(page, "marie@example.eu", MDP);
  await page.click('[data-action="connecter"]');
  await expect(page.locator("[data-succes-connexion]")).toBeVisible();

  const lisibles = await page.evaluate(() => document.cookie);
  expect(lisibles).not.toContain("secret-refresh");
  expect(lisibles).not.toContain("klaar_refresh");
});

test("@security le mot de passe est vidé et absent du DOM après connexion", async ({ page }) => {
  await interceptrice(page, {
    statut: 200,
    corps: { jeton_acces: "jwt.de.test", expire_dans: 3600 },
  });

  await sansReprise(page);
  await page.goto("/connexion");
  await remplir(page, "marie@example.eu", "MotDePasseTresParticulier@2026");
  await page.click('[data-action="connecter"]');
  await expect(page.locator("[data-succes-connexion]")).toBeVisible();

  expect(await page.content()).not.toContain("MotDePasseTresParticulier@2026");
  expect(page.url()).not.toContain("MotDePasse");
});

test("@security le champ mot de passe est masqué et annoncé comme existant", async ({ page }) => {
  await sansReprise(page);
  await page.goto("/connexion");
  const champ = page.locator("#connexion-mot-de-passe");
  await expect(champ).toHaveAttribute("type", "password");
  // `current-password` ici, `new-password` à l'inscription : c'est ce qui fait
  // proposer le mot de passe enregistré plutôt qu'une suggestion de création.
  await expect(champ).toHaveAttribute("autocomplete", "current-password");
});

test("@happy la session est reprise au chargement sans ressaisie", async ({ page }) => {
  // Ce que la Story 1.4 change pour l'utilisateur : recharger ne déconnecte
  // plus. Le refresh voyage dans son cookie, invisible à ce code.
  let appels = 0;
  await page.route("**/api/v1/auth/refresh", async (route: Route) => {
    appels += 1;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ jeton_acces: "jwt.repris", expire_dans: 3600 }),
    });
  });

  await page.goto("/connexion");

  await expect(page.locator("[data-succes-connexion]")).toBeVisible();
  await expect(page.locator('[data-formulaire="connexion"]')).toHaveCount(0);
  expect(appels).toBe(1);
});

test("@edge une reprise impossible affiche le formulaire sans erreur", async ({ page }) => {
  // Arriver sur la page sans être connecté est l'état normal d'un visiteur.
  await sansReprise(page);
  await page.goto("/connexion");

  await expect(page.locator('[data-formulaire="connexion"]')).toBeVisible();
  await expect(page.locator("[data-erreur-connexion]")).toHaveCount(0);
});

test("@happy se déconnecter ferme la session et rend le formulaire", async ({ page }) => {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ jeton_acces: "jwt.repris", expire_dans: 3600 }),
    }),
  );
  let deconnexions = 0;
  await page.route("**/api/v1/auth/logout", async (route: Route) => {
    deconnexions += 1;
    await route.fulfill({ status: 204, body: "" });
  });

  await page.goto("/connexion");
  await expect(page.locator("[data-succes-connexion]")).toBeVisible();

  await page.click('[data-action="deconnecter"]');

  await expect(page.locator('[data-formulaire="connexion"]')).toBeVisible();
  expect(deconnexions).toBe(1);
});

test("@security la déconnexion oublie le jeton même si le serveur échoue", async ({ page }) => {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ jeton_acces: "jwt.repris", expire_dans: 3600 }),
    }),
  );
  await page.route("**/api/v1/auth/logout", (route: Route) => route.abort("failed"));

  await page.goto("/connexion");
  await expect(page.locator("[data-succes-connexion]")).toBeVisible();

  await page.click('[data-action="deconnecter"]');

  // Laisser la session ouverte parce que le réseau a lâché serait le pire des
  // deux mondes : l'utilisateur croit être déconnecté et ne l'est pas.
  await expect(page.locator('[data-formulaire="connexion"]')).toBeVisible();
});
