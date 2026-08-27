/**
 * Service worker de Klaar (Story 0.2, ADR-010).
 *
 * Deux stratégies, choisies pour ce que fait l'application :
 *
 * - **Navigations : réseau d'abord, cache en repli.** Les pages contiennent
 *   l'état d'une Mission ; servir une version en cache à un utilisateur en
 *   ligne lui montrerait un dépanneur qui n'est plus en route. Le cache ne sert
 *   que quand le réseau ne répond pas.
 * - **Ressources : cache d'abord.** Les fichiers construits par Astro portent
 *   une empreinte dans leur nom, donc un contenu changé change d'URL. Les
 *   servir depuis le cache est sûr et évite un aller-retour.
 *
 * Ce qui n'est délibérément pas fait : mettre en cache les réponses de
 * `/api/`. Elles contiennent des données personnelles (positions, adresses,
 * identités) et le Cache Storage n'est pas chiffré. Les écritures hors-ligne
 * passent par la queue IndexedDB de `src/lib/offlineQueue.ts`, pas par ici.
 */

const CACHE = "klaar-shell-v1";
const OFFLINE_URL = "/hors-ligne";
const APP_SHELL = ["/", OFFLINE_URL, "/manifest.webmanifest", "/icons/icon-192.png"];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE).then((cache) =>
      // addAll est tout-ou-rien : une seule 404 ferait échouer l'installation
      // entière et laisserait la PWA sans coquille. On tolère les manquants.
      Promise.all(
        APP_SHELL.map((url) =>
          cache.add(url).catch((err) => console.warn("pré-cache ignoré", url, err)),
        ),
      ),
    ),
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  const url = new URL(request.url);

  if (request.method !== "GET" || url.origin !== self.location.origin) return;
  if (url.pathname.startsWith("/api/")) return; // jamais en cache, voir l'en-tête

  if (request.mode === "navigate") {
    event.respondWith(
      fetch(request)
        .then((response) => {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
          return response;
        })
        .catch(async () => {
          const cache = await caches.open(CACHE);
          return (
            (await cache.match(request)) ??
            (await cache.match(OFFLINE_URL)) ??
            new Response("Hors ligne", { status: 503, headers: { "Content-Type": "text/plain" } })
          );
        }),
    );
    return;
  }

  event.respondWith(
    caches.match(request).then(
      (cached) =>
        cached ??
        fetch(request).then((response) => {
          if (response.ok && response.type === "basic") {
            const copy = response.clone();
            caches.open(CACHE).then((cache) => cache.put(request, copy));
          }
          return response;
        }),
    ),
  );
});

/* ------------------------------------------------------------------------ *
 * Story 0.12 — Web Push (ADR-010).
 *
 * Le contenu arrive déjà déchiffré : le navigateur applique RFC 8291 avant de
 * livrer l'évènement. Ce que ce code doit garantir, c'est qu'une notification
 * s'affiche dans tous les cas — un service worker qui reçoit un push sans en
 * afficher fait perdre au site son autorisation de notifier dans Chrome.
 * ------------------------------------------------------------------------ */

const NOTIFICATION_PAR_DEFAUT = {
  titre: "Klaar",
  corps: "Vous avez une nouvelle notification.",
  url: "/",
};

function lireCharge(event) {
  if (!event.data) return NOTIFICATION_PAR_DEFAUT;
  try {
    const charge = event.data.json();
    return {
      titre: charge.titre || NOTIFICATION_PAR_DEFAUT.titre,
      corps: charge.corps || NOTIFICATION_PAR_DEFAUT.corps,
      url: charge.url || NOTIFICATION_PAR_DEFAUT.url,
      tag: charge.tag,
    };
  } catch (err) {
    // Charge illisible : on affiche quand même le message générique plutôt
    // que de ne rien afficher, ce qui coûterait l'autorisation de notifier.
    console.warn("charge push illisible", err);
    return NOTIFICATION_PAR_DEFAUT;
  }
}

self.addEventListener("push", (event) => {
  const charge = lireCharge(event);
  event.waitUntil(
    self.registration.showNotification(charge.titre, {
      body: charge.corps,
      icon: "/icons/icon-192.png",
      badge: "/icons/icon-192.png",
      // `tag` fait qu'une notification remplace la précédente de même
      // étiquette, au lieu d'empiler dix alertes pour une même Mission.
      tag: charge.tag,
      data: { url: charge.url },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const cible = new URL(event.notification.data?.url || "/", self.location.origin);

  event.waitUntil(
    (async () => {
      const fenetres = await self.clients.matchAll({
        type: "window",
        includeUncontrolled: true,
      });
      // Réutiliser un onglet déjà ouvert plutôt qu'en ouvrir un de plus :
      // sinon un utilisateur qui reçoit trois alertes se retrouve avec trois
      // onglets Klaar.
      for (const fenetre of fenetres) {
        if (new URL(fenetre.url).origin === self.location.origin) {
          await fenetre.focus();
          if ("navigate" in fenetre) await fenetre.navigate(cible.href);
          return;
        }
      }
      await self.clients.openWindow(cible.href);
    })(),
  );
});

self.addEventListener("pushsubscriptionchange", (event) => {
  // Le navigateur peut renouveler un abonnement de son propre chef. Sans ce
  // ré-enregistrement, l'appareil cesse silencieusement de recevoir.
  event.waitUntil(
    (async () => {
      const cle = await fetch("/api/v1/push/cle-publique")
        .then((r) => (r.ok ? r.json() : null))
        .catch(() => null);
      if (!cle) return;
      const abonnement = await self.registration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: cle.cle,
      });
      await fetch("/api/v1/push/abonnements", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(abonnement.toJSON()),
      });
    })(),
  );
});
