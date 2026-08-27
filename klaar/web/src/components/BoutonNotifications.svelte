<script lang="ts">
  /**
   * Invitation à activer les notifications (Story 0.12).
   *
   * Le bouton n'apparaît que si le navigateur peut réellement les délivrer, et
   * l'appel à `activer()` part d'un clic : `Notification.requestPermission()`
   * hors geste utilisateur est refusée sans dialogue par plusieurs
   * navigateurs, et ce refus est définitif.
   */
  import { onMount } from "svelte";
  import { activer, desactiver, etatActuel, pushDisponible, type EtatPush } from "../lib/push";

  let etat = $state<EtatPush>("non-supporte");
  let occupe = $state(false);
  let erreur = $state<string | null>(null);

  onMount(async () => {
    if (!pushDisponible()) {
      etat = "non-supporte";
      return;
    }
    etat = await etatActuel();
  });

  async function basculer() {
    occupe = true;
    erreur = null;
    try {
      etat = etat === "actif" ? await desactiver() : await activer();
    } catch (e) {
      erreur = e instanceof Error ? e.message : String(e);
    } finally {
      occupe = false;
    }
  }
</script>

{#if etat === "non-supporte"}
  <p class="klaar-tempere" data-etat-push="non-supporte">
    Ce navigateur ne délivre pas de notifications. Sur iPhone, ajoutez d'abord
    Klaar à votre écran d'accueil : Safari ne les délivre qu'aux applications
    installées.
  </p>
{:else if etat === "non-configure"}
  <p class="klaar-tempere" data-etat-push="non-configure">
    Les notifications ne sont pas activées sur ce déploiement.
  </p>
{:else if etat === "refuse"}
  <p class="klaar-tempere" data-etat-push="refuse">
    Les notifications sont bloquées pour ce site. Le rétablir se fait dans les
    réglages du navigateur, pas depuis cette page.
  </p>
{:else}
  <button type="button" onclick={basculer} disabled={occupe} data-etat-push={etat}>
    {#if occupe}
      Un instant…
    {:else if etat === "actif"}
      Désactiver les notifications
    {:else}
      Recevoir les notifications
    {/if}
  </button>
{/if}

{#if erreur}
  <p role="alert" data-erreur-push>{erreur}</p>
{/if}

<style>
  button {
    font: inherit;
    padding: 0.5rem 0.9rem;
    border-radius: 8px;
    border: 1px solid var(--klaar-bord);
    background: var(--klaar-accent);
    color: #1b3a4b;
    cursor: pointer;
  }
  button:disabled { opacity: 0.6; cursor: progress; }
  p[role="alert"] { color: #c2543a; }
</style>
