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

/**
 * Délai d'attente d'une notification livrée.
 *
 * **Dix secondes, et pas plus.** L'échec intermittent du cas `@negative` avait
 * été mis sur le compte d'un réveil de service worker trop lent, et ce délai
 * porté à trente secondes. Sans effet — et nuisible : trente secondes égalent
 * le délai de la *fonction de test* elle-même, donc l'attente ne pouvait plus
 * aboutir avant lui. Revenu à dix. Voir la note du cas concerné.
 */
const ATTENTE_NOTIFICATION_MS = 10_000;

const CLE_VAPID_FACTICE =
  "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";

/**
 * `expect.poll` et non `page.waitForFunction` : celui-ci voit la promesse
 * rendue par une fonction `async` — toujours vraie — plutôt que sa valeur, et
 * rendait donc la main avant que le service worker ne soit réellement actif.
 * Voir la note identique dans `pwa.spec.ts`.
 */
async function attendreServiceWorkerActif(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        page.evaluate(async () => {
          const reg = await navigator.serviceWorker.getRegistration("/");
          return Boolean(
            reg?.active?.state === "activated" && navigator.serviceWorker.controller,
          );
        }),
      { timeout: 20_000 },
    )
    .toBe(true);
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

/**
 * Collecte ce que le service worker dit du sort de chaque push.
 *
 * **Ce que cette écoute a permis d'établir.** Les échecs intermittents — quatre
 * sur onze exécutions — ne montraient qu'un tableau de notifications vide, qui
 * ne distingue pas « le service worker n'a rien reçu » de « il a reçu et n'a
 * pas pu afficher ». Le service worker diffuse maintenant une étape par push,
 * et la première reproduction locale a rendu son verdict :
 *
 *     ["recue:demande-M-1234", "affichee:demande-M-1234"]
 *
 * Le push est arrivé, `showNotification` a abouti — et `getNotifications()`
 * rendait pourtant une liste vide. Le défaut n'était donc **ni dans le service
 * worker ni dans l'application**, mais dans l'observable choisi : sans service
 * de notification de la plateforme, Chromium affiche puis oublie.
 *
 * Les cas s'appuient donc sur ce que le service worker déclare avoir fait, qui
 * est exactement la garantie qui compte — un push reçu produit toujours une
 * notification, faute de quoi Chrome retire au site le droit de notifier.
 */
interface EtapePush {
  etape: "recue" | "affichee" | "refusee";
  titre?: string;
  corps?: string;
  tag?: string;
}

async function suivreEtapesPush(page: Page): Promise<() => Promise<EtapePush[]>> {
  await page.evaluate(() => {
    (window as any).__etapesPush = [];
    navigator.serviceWorker.addEventListener("message", (e: MessageEvent) => {
      if (e.data?.type === "klaar:push") (window as any).__etapesPush.push(e.data);
    });
  });
  return () => page.evaluate(() => (window as any).__etapesPush as EtapePush[]);
}

/** Les notifications que le service worker déclare avoir affichées. */
function affichees(etapes: EtapePush[]) {
  return etapes
    .filter((e) => e.etape === "affichee")
    .map((e) => ({ titre: e.titre, corps: e.corps, tag: e.tag }));
}

/** Lit les notifications actuellement affichées par le service worker. */
async function notificationsAffichees(page: Page) {
  return page.evaluate(async () => {
    const reg = await navigator.serviceWorker.getRegistration("/");
    const notifs = (await reg?.getNotifications()) ?? [];
    return notifs.map((n) => ({ titre: n.title, corps: n.body, tag: n.tag }));
  });
}

// **Sans affichage, aucune notification n'est délivrable.** Chromium headless
// n'a pas le service de notification de la plateforme :
// `Notification.permission` rend « denied » alors que la permission a bien été
// accordée. Ces cas tournent donc dans le projet `chromium-notifications`, avec
// affichage ; `npm run test:e2e` en fournit un (`xvfb-run`). Quand il n'y en a
// pas du tout, on le dit et on saute, plutôt que d'échouer en laissant croire à
// une régression du code.
test.skip(
  process.env.KLAAR_SANS_AFFICHAGE === "1",
  "aucun affichage disponible : Chromium ne peut pas délivrer de notification (installer xvfb)",
);

test.beforeEach(async ({ context }) => {
  await context.grantPermissions(["notifications"], { origin: "http://127.0.0.1:4321" });
});

test.describe("@happy", () => {
  test("affiche la notification portée par un push", async ({ page }) => {
    await page.goto("/");
    await attendreServiceWorkerActif(page);
    const etapes = await suivreEtapesPush(page);
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
      .poll(async () => affichees(await etapes()), { timeout: ATTENTE_NOTIFICATION_MS })
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
    const etapes = await suivreEtapesPush(page);
    const { cdp, registrationId } = await sessionServiceWorker(page);

    for (const corps of ["Le dépanneur est en route", "Le dépanneur arrive"]) {
      await cdp.send("ServiceWorker.deliverPushMessage", {
        origin: "http://127.0.0.1:4321",
        registrationId,
        data: JSON.stringify({ titre: "Mission", corps, url: "/", tag: "mission-1" }),
      });
    }

    // Les deux messages doivent être affichés, et **avec la même étiquette** :
    // c'est ce que fait notre code, et c'est tout ce qu'il peut faire. Le
    // remplacement lui-même appartient au navigateur.
    await expect
      .poll(async () => affichees(await etapes()), { timeout: ATTENTE_NOTIFICATION_MS })
      .toEqual([
        { titre: "Mission", corps: "Le dépanneur est en route", tag: "mission-1" },
        { titre: "Mission", corps: "Le dépanneur arrive", tag: "mission-1" },
      ]);

    // Et le navigateur n'en garde qu'une. **« Au plus une » et non « exactement
    // une »** : sans service de notification de la plateforme, Chromium affiche
    // puis oublie, et `getNotifications()` rend une liste vide sans que rien
    // n'ait dysfonctionné. Ce qui serait un défaut, c'est d'en trouver deux —
    // dix mises à jour d'une Mission empileraient alors dix alertes.
    const gardees = (await notificationsAffichees(page)).filter((n) => n.tag === "mission-1");
    expect(gardees.length).toBeLessThanOrEqual(1);
    if (gardees.length === 1) {
      expect(gardees[0]).toEqual({
        titre: "Mission",
        corps: "Le dépanneur arrive",
        tag: "mission-1",
      });
    }
  });
});

test.describe("@negative", () => {
  // **Ce cas est intermittent en intégration continue, et la cause n'est pas
  // trouvée.** Sur trois publications de la vitrine, il a échoué deux fois et
  // réussi la troisième, sans qu'aucun changement ne le concerne : la
  // notification n'apparaît pas du tout, ou apparaît normalement. Les cinq
  // autres cas de ce fichier passent chaque fois, sur le même navigateur et le
  // même affichage virtuel, et celui-ci passe systématiquement en local. La
  // seule différence tient à la charge livrée : une chaîne qui n'est pas du
  // JSON, là où les autres en envoient.
  //
  // Deux hypothèses ont été essayées et démenties : un délai d'attente trop
  // court, puis un échec systématique. La seconde a tenu deux exécutions avant
  // que la troisième ne la contredise — c'est écrit ici pour que personne ne
  // reparte de cette conclusion-là.
  //
  // Il n'est **pas** neutralisé : ce qu'il vérifie compte — un service worker
  // qui reçoit un push sans rien afficher fait perdre au site son autorisation
  // de notifier. Un test qui rougit une fois sur trois en disant vrai vaut
  // mieux qu'un test vert qui ne vérifie rien, et le rapport publié à côté des
  // parcours filmés le montre plutôt que de le cacher.
  test("affiche un message générique quand la charge est illisible", async ({ page }) => {
    // Un service worker qui reçoit un push sans rien afficher fait perdre au
    // site son autorisation de notifier : mieux vaut un message vague que rien.
    await page.goto("/");
    await attendreServiceWorkerActif(page);
    const etapes = await suivreEtapesPush(page);
    const { cdp, registrationId } = await sessionServiceWorker(page);

    await cdp.send("ServiceWorker.deliverPushMessage", {
      origin: "http://127.0.0.1:4321",
      registrationId,
      data: "ceci n'est pas du JSON",
    });

    await expect
      .poll(async () => affichees(await etapes()).map((n) => n.titre), {
        timeout: ATTENTE_NOTIFICATION_MS,
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
