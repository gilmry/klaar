/**
 * Story 1.1 — parcours d'inscription dans un vrai navigateur.
 *
 * L'API est interceptée plutôt que lancée : ce qui est vérifié ici est le
 * comportement de la page — ce qu'elle envoie, ce qu'elle affiche, ce qu'elle
 * ne dit pas. Le contrat de l'API, lui, est couvert côté Rust par
 * `crates/klaar-api/tests/auth_routes.rs`, contre un vrai PostgreSQL.
 */
import { test, expect, type Page, type Route } from "@playwright/test";

const ROUTE_SIGNUP = "**/api/v1/auth/signup";

/** Fait répondre l'API sans backend, et rend les charges reçues. */
async function interceptrice(
  page: Page,
  reponse: { statut: number; corps: unknown },
): Promise<Array<Record<string, unknown>>> {
  const recues: Array<Record<string, unknown>> = [];
  await page.route(ROUTE_SIGNUP, async (route: Route) => {
    recues.push(JSON.parse(route.request().postData() ?? "{}"));
    await route.fulfill({
      status: reponse.statut,
      contentType: "application/json",
      body: JSON.stringify(reponse.corps),
    });
  });
  return recues;
}

async function remplir(page: Page, email: string, motDePasse: string) {
  await page.fill("#inscription-email", email);
  await page.fill("#inscription-mot-de-passe", motDePasse);
}

test("@happy une inscription valide affiche une confirmation prudente", async ({ page }) => {
  const recues = await interceptrice(page, {
    statut: 202,
    corps: { code: "SIGNUP_ACCEPTED" },
  });

  await page.goto("/inscription");
  await remplir(page, "marie@example.eu", "Marie@2026Secure");
  await page.click('[data-action="inscrire"]');

  const confirmation = page.locator("[data-succes-inscription]");
  await expect(confirmation).toBeVisible();
  await expect(confirmation).toContainText(/valable une heure/i);

  expect(recues).toHaveLength(1);
  expect(recues[0].email).toBe("marie@example.eu");
  expect(recues[0].mot_de_passe).toBe("Marie@2026Secure");
});

test("@happy le mot de passe est vidé après un envoi réussi", async ({ page }) => {
  // La page peut rester ouverte longtemps sur un appareil partagé ou une
  // borne : laisser le secret dans le champ le donne au suivant.
  await interceptrice(page, { statut: 202, corps: { code: "SIGNUP_ACCEPTED" } });
  await page.goto("/inscription");
  await remplir(page, "marie@example.eu", "Marie@2026Secure");
  await page.click('[data-action="inscrire"]');
  await expect(page.locator("[data-succes-inscription]")).toBeVisible();
  await expect(page.locator("#inscription-mot-de-passe")).toHaveValue("");
});

test("@negative un refus du serveur est traduit et affiché", async ({ page }) => {
  await interceptrice(page, { statut: 400, corps: { code: "PASSWORD_TOO_SHORT" } });

  await page.goto("/inscription");
  await remplir(page, "marie@example.eu", "trop-court");
  await page.click('[data-action="inscrire"]');

  const alerte = page.locator("[data-erreur-inscription]");
  await expect(alerte).toBeVisible();
  await expect(alerte).toContainText("12");
  // Un refus n'est pas un succès : les deux ne doivent jamais coexister.
  await expect(page.locator("[data-succes-inscription]")).toHaveCount(0);
});

test("@negative la limitation de débit est annoncée pour ce qu'elle est", async ({ page }) => {
  await interceptrice(page, { statut: 429, corps: { code: "RATE_LIMIT_EXCEEDED" } });
  await page.goto("/inscription");
  await remplir(page, "marie@example.eu", "Marie@2026Secure");
  await page.click('[data-action="inscrire"]');
  await expect(page.locator("[data-erreur-inscription]")).toContainText(/tentatives/i);
});

test("@edge la saisie courte est signalée avant tout aller-retour", async ({ page }) => {
  let appels = 0;
  await page.route(ROUTE_SIGNUP, async (route: Route) => {
    appels += 1;
    await route.fulfill({ status: 202, body: '{"code":"SIGNUP_ACCEPTED"}' });
  });

  await page.goto("/inscription");
  await page.fill("#inscription-mot-de-passe", "court");
  await expect(page.locator('[data-avertissement="mot-de-passe-court"]')).toBeVisible();
  expect(appels, "aucun appel ne doit partir pendant la frappe").toBe(0);
});

test("@edge une coupure réseau le dit sans mettre l'inscription en file", async ({ page }) => {
  // Contrairement aux autres écritures, l'inscription n'est pas mise en file :
  // rejouée une heure plus tard, son courriel de vérification arriverait bien
  // après le départ de l'utilisateur.
  await page.route(ROUTE_SIGNUP, (route: Route) => route.abort("failed"));

  await page.goto("/inscription");
  await remplir(page, "marie@example.eu", "Marie@2026Secure");
  await page.click('[data-action="inscrire"]');

  await expect(page.locator("[data-erreur-inscription]")).toContainText(/connexion/i);
});

test("@security la confirmation ne révèle pas si un compte a été créé", async ({ page }) => {
  // Le coeur de l'arbitrage FR-001 vu depuis l'interface : deux tentatives sur
  // la même adresse, deux fois le même écran.
  await interceptrice(page, { statut: 202, corps: { code: "SIGNUP_ACCEPTED" } });

  await page.goto("/inscription");
  const vus: string[] = [];
  for (let i = 0; i < 2; i += 1) {
    await remplir(page, "marie@example.eu", "Marie@2026Secure");
    await page.click('[data-action="inscrire"]');
    const texte = await page.locator("[data-succes-inscription]").innerText();
    vus.push(texte);
  }
  expect(vus[0]).toBe(vus[1]);
  expect(vus[0]).not.toMatch(/compte créé|déjà (pris|existant)/i);
});

test("@security le mot de passe n'est ni dans l'URL ni relu depuis le DOM", async ({ page }) => {
  await interceptrice(page, { statut: 202, corps: { code: "SIGNUP_ACCEPTED" } });

  await page.goto("/inscription");
  await remplir(page, "marie@example.eu", "MotDePasseTresParticulier@2026");
  await page.click('[data-action="inscrire"]');
  await expect(page.locator("[data-succes-inscription]")).toBeVisible();

  expect(page.url()).not.toContain("MotDePasse");
  const html = await page.content();
  expect(html).not.toContain("MotDePasseTresParticulier@2026");
});

test("@security le champ mot de passe est masqué et non auto-rempli à l'ancienne", async ({
  page,
}) => {
  await page.goto("/inscription");
  const champ = page.locator("#inscription-mot-de-passe");
  await expect(champ).toHaveAttribute("type", "password");
  // `new-password` et non `current-password` : le second ferait proposer au
  // gestionnaire de mots de passe celui d'un compte existant, à l'écran même
  // qui est censé en créer un.
  await expect(champ).toHaveAttribute("autocomplete", "new-password");
});
