/**
 * Story 3.7 — espace prestataire dans un vrai navigateur.
 */
import { test, expect, type Page, type Route } from "@playwright/test";

const SESSION = { jeton_acces: "jwt.de.test", expire_dans: 3600 };

const ETAT = {
  provider_id: "11111111-1111-4111-8111-111111111111",
  statut: "ACTIVE",
  disponible: true,
  rayon_intervention_metres: 20000,
  occupe: false,
  sollicitable: true,
};

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

/** Sert l'état, et enregistre les réglages envoyés. */
async function disponibilite(
  page: Page,
  etat: Record<string, unknown>,
  recues: Array<Record<string, unknown>> = [],
) {
  let courant = { ...etat };
  await page.route("**/api/v1/providers/me/availability", async (route: Route) => {
    if (route.request().method() === "PATCH") {
      const corps = JSON.parse(route.request().postData() ?? "{}");
      recues.push({ corps, autorisation: route.request().headers()["authorization"] });
      courant = { ...courant, ...corps };
      courant.sollicitable =
        courant.statut === "ACTIVE" && courant.disponible === true && !courant.occupe;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(courant),
    });
  });
}

test("@happy affiche qu'un prestataire en service reçoit les Demandes", async ({ page }) => {
  await connecte(page);
  await disponibilite(page, ETAT);

  await page.goto("/prestataire");
  await expect(page.locator('[data-sollicitable="true"]')).toContainText("recevez");
  await expect(page.locator('[data-action="basculer-disponibilite"]')).toContainText("pause");
});

test("@happy la mise en pause part en PATCH et change le libellé", async ({ page }) => {
  await connecte(page);
  const recues: Array<Record<string, unknown>> = [];
  await disponibilite(page, ETAT, recues);

  await page.goto("/prestataire");
  await page.click('[data-action="basculer-disponibilite"]');

  await expect(page.locator('[data-action="basculer-disponibilite"]')).toContainText(
    "remettre en service",
  );
  expect(recues).toHaveLength(1);
  expect(recues[0].corps).toEqual({ disponible: false });
  // Le jeton voyage en en-tête, jamais dans l'URL ni le corps.
  expect(recues[0].autorisation).toBe("Bearer jwt.de.test");
});

test("@happy le rayon part en mètres, pas en kilomètres", async ({ page }) => {
  // L'affichage est en kilomètres parce que personne ne pense en mètres pour un
  // déplacement ; l'API travaille en mètres.
  await connecte(page);
  const recues: Array<Record<string, unknown>> = [];
  await disponibilite(page, ETAT, recues);

  await page.goto("/prestataire");
  await page.fill('[data-champ="rayon"]', "5");
  await page.click('[data-action="enregistrer-rayon"]');

  expect(recues).toHaveLength(1);
  expect(recues[0].corps).toEqual({ rayon_intervention_metres: 5000 });
});

test("@negative explique la pause plutôt que de laisser un silence", async ({ page }) => {
  await connecte(page);
  await disponibilite(page, { ...ETAT, disponible: false, sollicitable: false });

  await page.goto("/prestataire");
  await expect(page.locator('[data-sollicitable="false"]')).toContainText("pause");
});

test("@negative un compte sans fiche prestataire est éconduit clairement", async ({ page }) => {
  await connecte(page);
  await page.route("**/api/v1/providers/me/availability", (route: Route) =>
    route.fulfill({
      status: 403,
      contentType: "application/json",
      body: '{"code":"NOT_A_PROVIDER"}',
    }),
  );

  await page.goto("/prestataire");
  await expect(page.locator('[data-etat-dispo="indisponible"]')).toContainText("prestataire");
});

test("@negative un rayon refusé ne laisse pas le curseur mentir", async ({ page }) => {
  // Laisser le curseur sur la valeur refusée ferait croire qu'elle a pris.
  await connecte(page);
  await page.route("**/api/v1/providers/me/availability", async (route: Route) => {
    if (route.request().method() === "PATCH") {
      await route.fulfill({
        status: 400,
        contentType: "application/json",
        body: '{"code":"SERVICE_RADIUS_OUT_OF_RANGE"}',
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(ETAT),
    });
  });

  await page.goto("/prestataire");
  await page.fill('[data-champ="rayon"]', "5");
  await page.click('[data-action="enregistrer-rayon"]');

  await expect(page.locator("[data-erreur-dispo]")).toBeVisible();
  await expect(page.locator('[data-champ="rayon"]')).toHaveValue("20");
});

test("@edge une intervention en cours est distinguée d'une pause", async ({ page }) => {
  // Confondre les deux enverrait le prestataire appuyer sur un interrupteur qui
  // n'y est pour rien.
  await connecte(page);
  await disponibilite(page, { ...ETAT, occupe: true, sollicitable: false });

  await page.goto("/prestataire");
  const message = page.locator('[data-sollicitable="false"]');
  await expect(message).toContainText("intervention");
  await expect(message).not.toContainText("pause");
  // Il reste « en service » : le bouton propose donc la pause, pas la reprise.
  await expect(page.locator('[data-action="basculer-disponibilite"]')).toContainText("pause");
});

test("@edge un compte suspendu est prévenu que l'interrupteur ne suffira pas", async ({
  page,
}) => {
  await connecte(page);
  await disponibilite(page, { ...ETAT, statut: "SUSPENDED", sollicitable: false });

  await page.goto("/prestataire");
  await expect(page.locator('[data-sollicitable="false"]')).toContainText("suspendu");
});

test("@security la page ne montre rien à un visiteur non connecté", async ({ page }) => {
  await anonyme(page);
  let appels = 0;
  await page.route("**/api/v1/providers/me/availability", async (route: Route) => {
    appels += 1;
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(ETAT) });
  });

  await page.goto("/prestataire");
  await expect(page.locator('[data-etat-dispo="anonyme"]')).toBeVisible();
  // Et rien n'est demandé à l'API : interroger d'abord et cacher ensuite
  // enverrait une requête au nom de personne.
  expect(appels).toBe(0);
});

test("@security le bouton d'enregistrement reste inerte tant que rien n'a changé", async ({
  page,
}) => {
  // Sans cela, chaque clic renverrait la même valeur et ferait du bruit sur une
  // route d'écriture pour rien.
  await connecte(page);
  const recues: Array<Record<string, unknown>> = [];
  await disponibilite(page, ETAT, recues);

  await page.goto("/prestataire");
  await expect(page.locator('[data-action="enregistrer-rayon"]')).toBeDisabled();
  expect(recues).toHaveLength(0);
});
