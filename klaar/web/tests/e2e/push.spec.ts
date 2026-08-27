/**
 * Story 0.12 — Web Push côté navigateur.
 *
 * Le message est livré au service worker par le protocole DevTools
 * (`ServiceWorker.deliverPushMessage`), c'est-à-dire par le même chemin qu'un
 * push réel une fois déchiffré. Ce qui n'est pas couvert ici : la livraison
 * depuis un service de push distant, qui demande un appareil et un compte,
 * et le comportement d'iOS. Le protocole lui-même est vérifié côté Rust
 * contre les vecteurs du RFC 8291.
 */
import { test, expect, type Page, type CDPSession } from "@playwright/test";

const CLE_VAPID_FACTICE =
  "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";

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

/** Ouvre une session CDP et récupère l'identifiant d'enregistrement du SW. */
async function sessionServiceWorker(
  page: Page,
): Promise<{ cdp: CDPSession; registrationId: string }> {
  const cdp = await page.context().newCDPSession(page);
  const identifiant = new Promise<string>((resolve) => {
    cdp.on("ServiceWorker.workerRegistrationUpdated", (evt: any) => {
      const inscription = evt.registrations?.find((r: any) => !r.isDeleted);
      if (inscription) resolve(inscription.registrationId);
    });
  });
  await cdp.send("ServiceWorker.enable");
  return { cdp, registrationId: await identifiant };
}

/** Lit les notifications actuellement affichées par le service worker. */
async function notificationsAffichees(page: Page) {
  return page.evaluate(async () => {
    const reg = await navigator.serviceWorker.getRegistration("/");
    const notifs = (await reg?.getNotifications()) ?? [];
    return notifs.map((n) => ({ titre: n.title, corps: n.body, tag: n.tag }));
  });
}

test.beforeEach(async ({ context }) => {
  await context.grantPermissions(["notifications"], { origin: "http://127.0.0.1:4321" });
});

test.describe("@happy", () => {
  test("affiche la notification portée par un push", async ({ page }) => {
    await page.goto("/");
    await attendreServiceWorkerActif(page);
    const { cdp, registrationId } = await sessionServiceWorker(page);

    await cdp.send("ServiceWorker.deliverPushMessage", {
      origin: "http://127.0.0.1:4321",
      registrationId,
      data: JSON.stringify({
        titre: "Nouvelle Demande",
        corps: "Plomberie, Saint-Gilles",
        url: "/missions/M-1234",
        tag: "demande-M-1234",
      }),
    });

    await expect
      .poll(() => notificationsAffichees(page), { timeout: 10_000 })
      .toContainEqual({
        titre: "Nouvelle Demande",
        corps: "Plomberie, Saint-Gilles",
        tag: "demande-M-1234",
      });
  });

  test("deux messages de même étiquette n'en affichent qu'un", async ({ page }) => {
    // Sinon dix mises à jour d'une Mission empilent dix notifications.
    await page.goto("/");
    await attendreServiceWorkerActif(page);
    const { cdp, registrationId } = await sessionServiceWorker(page);

    for (const corps of ["Le dépanneur est en route", "Le dépanneur arrive"]) {
      await cdp.send("ServiceWorker.deliverPushMessage", {
        origin: "http://127.0.0.1:4321",
        registrationId,
        data: JSON.stringify({ titre: "Mission", corps, url: "/", tag: "mission-1" }),
      });
    }

    await expect
      .poll(() => notificationsAffichees(page).then((n) => n.filter((x) => x.tag === "mission-1")), {
        timeout: 10_000,
      })
      .toEqual([{ titre: "Mission", corps: "Le dépanneur arrive", tag: "mission-1" }]);
  });
});

test.describe("@negative", () => {
  test("affiche un message générique quand la charge est illisible", async ({ page }) => {
    // Un service worker qui reçoit un push sans rien afficher fait perdre au
    // site son autorisation de notifier : mieux vaut un message vague que rien.
    await page.goto("/");
    await attendreServiceWorkerActif(page);
    const { cdp, registrationId } = await sessionServiceWorker(page);

    await cdp.send("ServiceWorker.deliverPushMessage", {
      origin: "http://127.0.0.1:4321",
      registrationId,
      data: "ceci n'est pas du JSON",
    });

    await expect
      .poll(() => notificationsAffichees(page).then((n) => n.map((x) => x.titre)), {
        timeout: 10_000,
      })
      .toContain("Klaar");
  });
});

test.describe("@edge", () => {
  test("masque l'invitation quand le serveur n'a pas de push configuré", async ({ page }) => {
    // 503 signifie « non configuré », pas « en panne » : l'interface doit le
    // dire, pas afficher une erreur.
    await page.route("**/api/v1/push/cle-publique", (route) =>
      route.fulfill({ status: 503, body: '{"erreur":"non configuré"}' }),
    );
    await page.goto("/");
    await page.getByRole("button", { name: "Recevoir les notifications" }).click();
    await expect(page.locator('[data-etat-push="non-configure"]')).toBeVisible();
  });
});

test.describe("@security", () => {
  test("n'enregistre l'abonnement qu'après accord explicite", async ({ page, context }) => {
    // La permission ne doit jamais être demandée au chargement : plusieurs
    // navigateurs refusent définitivement une demande hors geste utilisateur.
    await context.clearPermissions();
    const appels: string[] = [];
    await page.route("**/api/v1/push/**", (route) => {
      appels.push(route.request().url());
      return route.fulfill({ status: 200, body: JSON.stringify({ cle: CLE_VAPID_FACTICE }) });
    });

    await page.goto("/");
    await attendreServiceWorkerActif(page);
    await page.waitForTimeout(500);

    expect(
      appels.filter((u) => u.includes("/abonnements")),
      "aucun abonnement ne doit être enregistré sans clic",
    ).toEqual([]);
  });

  test("défait l'abonnement navigateur si le serveur le refuse", async ({ page }) => {
    // Sans ça, l'appareil se croit abonné et ne reçoit rien : une panne
    // invisible, donc la pire.
    await page.route("**/api/v1/push/cle-publique", (route) =>
      route.fulfill({ status: 200, body: JSON.stringify({ cle: CLE_VAPID_FACTICE }) }),
    );
    await page.route("**/api/v1/push/abonnements", (route) =>
      route.fulfill({ status: 503, body: '{"erreur":"dépôt indisponible"}' }),
    );

    await page.goto("/");
    await attendreServiceWorkerActif(page);
    await page.getByRole("button", { name: "Recevoir les notifications" }).click();

    await expect(page.locator("[data-erreur-push]")).toBeVisible();
    const abonne = await page.evaluate(async () => {
      const reg = await navigator.serviceWorker.getRegistration("/");
      return Boolean(await reg?.pushManager.getSubscription());
    });
    expect(abonne, "l'abonnement navigateur doit avoir été défait").toBe(false);
  });
});
