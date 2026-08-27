/**
 * Abonnement aux notifications (Story 0.12, ADR-010).
 *
 * Deux limites à connaître avant de lire ce code :
 *
 * - **iOS** ne délivre les notifications qu'aux PWA **ajoutées à l'écran
 *   d'accueil** (iOS ≥ 16.4). Dans Safari onglet, `PushManager` est absent et
 *   l'invitation ne doit tout simplement pas être proposée.
 * - `Notification.requestPermission()` ne peut être appelée qu'en réponse à un
 *   geste de l'utilisateur. La déclencher au chargement fait refuser
 *   définitivement la permission par certains navigateurs, sans dialogue.
 */
import { API_BASE } from "./api";

export type EtatPush =
  | "non-supporte"
  | "refuse"
  | "non-configure"
  | "inactif"
  | "actif";

/**
 * Convertit une clé VAPID base64url en octets, seule forme acceptée par
 * `PushManager.subscribe`.
 *
 * Le type de retour est annoté `Uint8Array<ArrayBuffer>` et le tampon alloué
 * explicitement : depuis TypeScript 5.7, `Uint8Array` est générique sur son
 * tampon, et `BufferSource` exclut `SharedArrayBuffer`. `Uint8Array.from`
 * produit le type large, que la signature de `subscribe` refuse.
 */
export function decoderCleVapid(base64url: string): Uint8Array<ArrayBuffer> {
  const remplissage = "=".repeat((4 - (base64url.length % 4)) % 4);
  const base64 = (base64url + remplissage).replace(/-/g, "+").replace(/_/g, "/");
  const brut = atob(base64);
  const octets = new Uint8Array(new ArrayBuffer(brut.length));
  for (let i = 0; i < brut.length; i += 1) octets[i] = brut.charCodeAt(i);
  return octets;
}

export function pushDisponible(): boolean {
  return (
    typeof navigator !== "undefined" &&
    "serviceWorker" in navigator &&
    typeof window !== "undefined" &&
    "PushManager" in window &&
    "Notification" in window
  );
}

export async function etatActuel(): Promise<EtatPush> {
  if (!pushDisponible()) return "non-supporte";
  if (Notification.permission === "denied") return "refuse";
  const registration = await navigator.serviceWorker.getRegistration("/");
  const abonnement = await registration?.pushManager.getSubscription();
  return abonnement ? "actif" : "inactif";
}

async function clePubliqueVapid(): Promise<string | null> {
  const reponse = await fetch(`${API_BASE}/push/cle-publique`);
  // 503 : le déploiement tourne sans notifications. Ce n'est pas une panne,
  // et l'interface doit masquer l'invitation plutôt qu'afficher une erreur.
  if (reponse.status === 503) return null;
  if (!reponse.ok) throw new Error(`clé VAPID indisponible (${reponse.status})`);
  return (await reponse.json()).cle as string;
}

/**
 * Demande la permission puis enregistre l'abonnement.
 * À n'appeler que depuis un gestionnaire d'évènement utilisateur.
 */
export async function activer(): Promise<EtatPush> {
  if (!pushDisponible()) return "non-supporte";

  const cle = await clePubliqueVapid();
  if (cle === null) return "non-configure";

  const permission = await Notification.requestPermission();
  if (permission !== "granted") return "refuse";

  const registration = await navigator.serviceWorker.ready;
  // `userVisibleOnly: true` est obligatoire : les navigateurs refusent un
  // abonnement qui permettrait de réveiller le service worker sans que
  // l'utilisateur voie quoi que ce soit.
  const abonnement = await registration.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: decoderCleVapid(cle),
  });

  const reponse = await fetch(`${API_BASE}/push/abonnements`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(abonnement.toJSON()),
    credentials: "include",
  });
  if (!reponse.ok) {
    // Le serveur n'a pas retenu l'abonnement : le défaire côté navigateur
    // évite un état où l'appareil se croit abonné et ne reçoit rien.
    await abonnement.unsubscribe();
    throw new Error(`enregistrement refusé (${reponse.status})`);
  }
  return "actif";
}

export async function desactiver(): Promise<EtatPush> {
  const registration = await navigator.serviceWorker.getRegistration("/");
  const abonnement = await registration?.pushManager.getSubscription();
  if (!abonnement) return "inactif";

  // Prévenir le serveur d'abord : s'il n'est pas joignable, mieux vaut rester
  // abonné que devenir injoignable tout en laissant une ligne derrière soi.
  await fetch(`${API_BASE}/push/abonnements`, {
    method: "DELETE",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ endpoint: abonnement.endpoint }),
    credentials: "include",
  });
  await abonnement.unsubscribe();
  return "inactif";
}
