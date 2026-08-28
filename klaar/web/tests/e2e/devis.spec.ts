/**
 * Story 4.1 — le devis dans un vrai navigateur (FR-016).
 *
 * **Ce que ces cas vérifient, et que les tests unitaires ne peuvent pas voir :**
 * ce qui part réellement sur le réseau quand quelqu'un tape un prix, et ce qui
 * s'affiche quand le serveur refuse. Le corps de la requête est capturé et
 * comparé au centime : c'est la seule manière de prouver qu'aucune couche ne
 * retouche le montant entre le clavier et l'API.
 */
import { test, expect, type Page, type Route } from "@playwright/test";

const SESSION = { jeton_acces: "jwt.de.test", expire_dans: 3600 };
const MISSION_ID = "22222222-2222-4222-8222-222222222222";

const DISPONIBILITE = {
  provider_id: "11111111-1111-4111-8111-111111111111",
  statut: "ACTIVE",
  disponible: true,
  rayon_intervention_metres: 20000,
  occupe: true,
  sollicitable: false,
};

const PROPOSEE = {
  id: "33333333-3333-4333-8333-333333333333",
  secteur: "electricite",
  description: "Plus de courant dans la cuisine",
  urgence: "HIGH",
  distance_metres: 800,
  secondes_restantes: 25,
};

function mission(champs: Record<string, unknown> = {}) {
  return {
    id: MISSION_ID,
    statut: "ON_SITE",
    secteur: "electricite",
    description: "Plus de courant dans la cuisine",
    urgence: "HIGH",
    latitude: 50.8676,
    longitude: 4.3737,
    suites: ["COMPLETED", "CANCELLED"],
    devis: null,
    devis_restants: 3,
    ...champs,
  };
}

function devis(champs: Record<string, unknown> = {}) {
  return {
    id: "44444444-4444-4444-8444-444444444444",
    montant_htva_cents: 18000,
    taux_tva_bp: 2100,
    tva_cents: 3780,
    total_ttc_cents: 21780,
    delai_minutes: 45,
    note: "Remplacement du différentiel",
    statut: "SENT",
    secondes_restantes: 3600,
    echu: false,
    ...champs,
  };
}

/**
 * Installe l'espace prestataire avec une intervention en cours.
 *
 * `etats` est consommé dans l'ordre : le premier appel à la lecture de Mission
 * rend le premier état, le suivant le deuxième. C'est ce qui permet de simuler
 * l'écran **après** l'envoi sans que le test ait à deviner quand la relecture a
 * lieu.
 */
async function espacePrestataire(
  page: Page,
  etats: Array<Record<string, unknown>>,
  envois: Array<Record<string, unknown>> = [],
  reponseDevis: { status: number; body: unknown } = { status: 201, body: devis() },
) {
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(SESSION) }),
  );
  await page.route("**/api/v1/providers/me/availability", (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(DISPONIBILITE),
    }),
  );
  await page.route("**/api/v1/providers/me/requests", (route: Route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify([PROPOSEE]),
    }),
  );
  await page.route(`**/api/v1/requests/${PROPOSEE.id}/accept`, (route: Route) =>
    route.fulfill({
      status: 201,
      contentType: "application/json",
      body: JSON.stringify({
        id: MISSION_ID,
        demande_id: PROPOSEE.id,
        statut: "ACCEPTED",
        code: "REQUEST_MATCHED",
        autres_prevenus: 0,
      }),
    }),
  );

  let rang = 0;
  await page.route(`**/api/v1/missions/${MISSION_ID}`, (route: Route) => {
    const etat = etats[Math.min(rang, etats.length - 1)];
    rang += 1;
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(etat) });
  });

  await page.route(`**/api/v1/missions/${MISSION_ID}/quote`, (route: Route) => {
    envois.push(JSON.parse(route.request().postData() ?? "{}"));
    route.fulfill({
      status: reponseDevis.status,
      contentType: "application/json",
      body: JSON.stringify(reponseDevis.body),
    });
  });
}

/** Ouvre l'espace et prend la Demande, pour arriver sur l'écran d'intervention. */
async function surIntervention(page: Page) {
  await page.goto("/prestataire");
  await page.waitForSelector('[data-demandes="liste"]');
  await page.click('[data-action="accepter"]');
  await page.waitForSelector("[data-mission-statut]");
}

test("@happy le montant saisi part en centimes, exactement", async ({ page }) => {
  const envois: Array<Record<string, unknown>> = [];
  await espacePrestataire(page, [mission(), mission({ devis: devis() })], envois);
  await surIntervention(page);

  await page.fill('input[name="montant"]', "180");
  await page.fill('input[name="delai"]', "45");
  await page.fill('input[name="note"]', "Remplacement du différentiel");
  await page.click('[data-action="envoyer-devis"]');
  await page.waitForSelector("[data-devis-total]");

  expect(envois).toHaveLength(1);
  expect(envois[0]).toMatchObject({
    montant_htva_cents: 18000,
    taux_tva_bp: 2100,
    delai_minutes: 45,
    note: "Remplacement du différentiel",
  });
  await expect(page.locator("[data-devis-total]")).toContainText("217,80");
});

test("@happy l'aperçu montre le TTC pendant la saisie, sans rien décider", async ({ page }) => {
  await espacePrestataire(page, [mission()]);
  await surIntervention(page);

  // Rien avant la saisie : pas de zéro affiché, pas de montant suggéré.
  await expect(page.locator("[data-devis-apercu]")).toHaveCount(0);

  await page.fill('input[name="montant"]', "180");
  await expect(page.locator("[data-devis-apercu]")).toContainText("217,80");
});

test("@negative un refus du serveur s'affiche tel qu'il est", async ({ page }) => {
  await espacePrestataire(page, [mission()], [], {
    status: 422,
    body: { code: "DELAY_TOO_LONG" },
  });
  await surIntervention(page);

  await page.fill('input[name="montant"]', "180");
  await page.fill('input[name="delai"]', "1500");
  await page.click('[data-action="envoyer-devis"]');

  await expect(page.locator("[data-erreur-demandes]")).toContainText("24 h");
});

test("@negative une saisie vide ne part pas au serveur", async ({ page }) => {
  const envois: Array<Record<string, unknown>> = [];
  await espacePrestataire(page, [mission()], envois);
  await surIntervention(page);

  // Le champ est requis : le navigateur bloque avant nous. Ce cas vérifie que
  // rien ne part quand même, parce qu'un `null` envoyé produirait un 400 que
  // l'utilisateur ne comprendrait pas.
  await page.click('[data-action="envoyer-devis"]');
  await page.waitForTimeout(300);
  expect(envois).toHaveLength(0);
});

test("@edge le formulaire disparaît tant qu'un devis attend une réponse", async ({ page }) => {
  await espacePrestataire(page, [mission({ devis: devis(), devis_restants: 2 })]);
  await surIntervention(page);

  await expect(page.locator('[data-formulaire="devis"]')).toHaveCount(0);
  await expect(page.locator("[data-devis-statut]")).toContainText("attente");
});

test("@edge un devis échu rouvre le formulaire et se dit expiré", async ({ page }) => {
  // Le statut stocké dit encore « envoyé » : le balayage n'est pas passé. Le
  // croire ferait attendre une réponse qui ne peut plus venir.
  await espacePrestataire(page, [
    mission({ devis: devis({ echu: true, secondes_restantes: 0 }), devis_restants: 2 }),
  ]);
  await surIntervention(page);

  await expect(page.locator("[data-devis-statut]")).toContainText("Expiré");
  await expect(page.locator('[data-formulaire="devis"]')).toHaveCount(1);
});

test("@edge le plafond atteint remplace le formulaire par un avertissement", async ({ page }) => {
  await espacePrestataire(page, [mission({ devis: devis({ statut: "REFUSED" }), devis_restants: 0 })]);
  await surIntervention(page);

  await expect(page.locator('[data-formulaire="devis"]')).toHaveCount(0);
  await expect(page.locator('[data-devis="plafond"]')).toContainText("Trois devis");
});

test("@security le formulaire ne propose aucun montant", async ({ page }) => {
  // L'invariant §10.2 à l'écran. Une valeur par défaut serait une suggestion de
  // prix, et une suggestion de prix est une fixation de prix douce.
  await espacePrestataire(page, [mission()]);
  await surIntervention(page);

  await expect(page.locator('input[name="montant"]')).toHaveValue("");
  await expect(page.locator('input[name="delai"]')).toHaveValue("");
  // Le taux, lui, a bien une valeur : c'est la loi qui le fixe, pas nous.
  await expect(page.locator('select[name="taux"]')).toHaveValue("2100");
});

test("@security le devis part avec le jeton de session", async ({ page }) => {
  let autorisation: string | undefined;
  await page.route("**/api/v1/auth/refresh", (route: Route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(SESSION) }),
  );
  await espacePrestataire(page, [mission(), mission({ devis: devis() })]);
  page.on("request", (requete) => {
    if (requete.url().includes("/quote")) autorisation = requete.headers()["authorization"];
  });
  await surIntervention(page);

  await page.fill('input[name="montant"]', "180");
  await page.fill('input[name="delai"]', "45");
  await page.click('[data-action="envoyer-devis"]');
  await page.waitForSelector("[data-devis-total]");

  expect(autorisation).toBe(`Bearer ${SESSION.jeton_acces}`);
});

test("@security un taux réduit exige la preuve avant l'envoi", async ({ page }) => {
  const envois: Array<Record<string, unknown>> = [];
  await espacePrestataire(page, [mission(), mission({ devis: devis({ taux_tva_bp: 600 }) })], envois);
  await surIntervention(page);

  await page.selectOption('select[name="taux"]', "600");
  // Le champ n'apparaît qu'au taux réduit, et il est requis : sans lui, un
  // devis à 6 % partirait sans justification, et c'est nous qui aurions
  // documenté la fraude.
  await expect(page.locator('input[name="preuve"]')).toHaveCount(1);

  await page.fill('input[name="montant"]', "180");
  await page.fill('input[name="delai"]', "45");
  await page.click('[data-action="envoyer-devis"]');
  await page.waitForTimeout(300);
  expect(envois).toHaveLength(0);

  await page.fill('input[name="preuve"]', "Logement de 1974");
  await page.click('[data-action="envoyer-devis"]');
  await page.waitForSelector("[data-devis-total]");
  expect(envois[0]).toMatchObject({ taux_tva_bp: 600, preuve_tva_reduite: "Logement de 1974" });
});
