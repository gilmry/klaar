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
  import { restaurerLangue, t } from "../lib/i18n";
  import type { LocaleKlaar } from "../lib/inscription";

  let locale = $state<LocaleKlaar>("fr");

  let etat = $state<EtatPush>("non-supporte");
  let occupe = $state(false);
  let erreur = $state<string | null>(null);

  onMount(async () => {
    locale = restaurerLangue();
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
    {t(locale, "push.non_supporte")}
  </p>
{:else if etat === "non-configure"}
  <p class="klaar-tempere" data-etat-push="non-configure">
    {t(locale, "push.non_configure")}
  </p>
{:else if etat === "refuse"}
  <p class="klaar-tempere" data-etat-push="refuse">
    {t(locale, "push.refuse")}
  </p>
{:else}
  <button type="button" onclick={basculer} disabled={occupe} data-etat-push={etat}>
    {#if occupe}
      {t(locale, "commun.attendez")}
    {:else if etat === "actif"}
      {t(locale, "push.desactiver")}
    {:else}
      {t(locale, "push.activer")}
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
