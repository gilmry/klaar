/**
 * Enregistre le service worker.
 *
 * Ne fait rien, sans erreur, quand l'API est absente : un navigateur sans
 * service worker doit voir une application en ligne qui fonctionne, pas une
 * page cassée (`@edge` de la Story 0.2).
 */
export function registerServiceWorker(): void {
  if (typeof navigator === "undefined" || !("serviceWorker" in navigator)) return;
  window.addEventListener("load", () => {
    navigator.serviceWorker
      .register("/service-worker.js", { scope: "/" })
      .catch((err) => console.warn("service worker non enregistré", err));
  });
}
