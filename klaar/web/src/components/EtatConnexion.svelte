<script lang="ts">
  /**
   * Indicateur d'état réseau et de file d'attente.
   *
   * Il montre ce qui est en attente d'envoi, parce qu'une PWA qui accepte une
   * saisie hors ligne sans le dire laisse croire que c'est parti.
   *
   * Tout le câblage vit dans la fonction de nettoyage retournée par `onMount`,
   * et non dans `onDestroy` : Astro rend ce composant une fois côté serveur au
   * moment du build, où `onDestroy` s'exécute alors que `removeEventListener`
   * n'existe pas. `onMount`, lui, ne s'exécute jamais côté serveur, donc son
   * nettoyage non plus.
   *
   * L'état affiché vient d'une sonde réseau, pas de `navigator.onLine`, qui
   * reste à `true` quand le serveur est injoignable (voir `sonderReseau`).
   */
  import { onMount } from "svelte";
  import {
    pendingCount,
    deadCount,
    flushQueue,
    startAutoSync,
    stopAutoSync,
  } from "../lib/offlineQueue";
  import { sonderReseau } from "../lib/api";

  /**
   * `null` tant que rien n'a été vérifié.
   *
   * Partir de « en ligne » revenait à l'affirmer avant de l'avoir constaté :
   * sur une page dont les scripts n'ont pas pu être chargés — premier passage
   * hors ligne, chunk absent du cache — l'îlot ne s'hydrate pas et la pastille
   * restait bloquée sur « En ligne » alors que le réseau était coupé. Un
   * indicateur qui ment sur l'état du réseau est pire que pas d'indicateur.
   */
  let enLigne = $state<boolean | null>(null);
  let enAttente = $state(0);
  let echecs = $state(0);

  async function rafraichir() {
    enAttente = await pendingCount();
    echecs = await deadCount();
  }

  async function reevaluer() {
    enLigne = await sonderReseau();
    if (enLigne) {
      // Le rejeu fait autorité sur la sonde : s'il s'interrompt, c'est que la
      // connexion est retombée entre les deux.
      const rapport = await flushQueue();
      if (rapport.interrupted) enLigne = false;
    }
    await rafraichir();
  }

  onMount(() => {
    const surChangementReseau = () => void reevaluer();
    window.addEventListener("online", surChangementReseau);
    window.addEventListener("offline", surChangementReseau);
    startAutoSync();
    void reevaluer();
    const minuteur = setInterval(() => void reevaluer(), 5_000);

    return () => {
      window.removeEventListener("online", surChangementReseau);
      window.removeEventListener("offline", surChangementReseau);
      stopAutoSync();
      clearInterval(minuteur);
    };
  });
</script>

<p class="klaar-etat">
  <span
    class="klaar-pastille"
    data-etat={enLigne === null ? "inconnu" : enLigne ? "en-ligne" : "hors-ligne"}
  ></span>
  {#if enLigne === null}
    Vérification de la connexion…
  {:else if enLigne}
    En ligne
  {:else}
    Hors ligne, vos saisies sont conservées
  {/if}
  {#if enAttente > 0}
    · {enAttente} en attente d'envoi
  {/if}
  {#if echecs > 0}
    · <strong>{echecs} refusée{echecs > 1 ? "s" : ""}</strong>
  {/if}
</p>
