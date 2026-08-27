<script lang="ts">
  /**
   * Effacement du compte (Story 1.9, FR-005, RGPD art. 17).
   *
   * Deux protections contre le clic malheureux, et aucune de plus :
   * l'action est repliée derrière un bouton qui l'ouvre, et il faut recopier
   * un mot. Un effacement n'a pas à être facile ; il n'a pas non plus à être
   * un parcours du combattant, c'est un droit.
   *
   * L'annulation est proposée tant que le délai court. Le backend la permet
   * pour la même raison : trente jours pendant lesquels on ne pourrait rien
   * annuler seraient trente jours d'attente pour rien.
   */
  import { onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerSession } from "../lib/connexion";
  import {
    annulerEffacement,
    codeDepuisErreur,
    demanderEffacement,
    messageErreur,
    MOT_DE_CONFIRMATION,
  } from "../lib/compte";

  let connecte = $state(false);
  let reprise = $state(true);
  let deplie = $state(false);
  let confirmation = $state("");
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let programme = $state<number | null>(null);
  let locale = $state<LocaleKlaar>("fr");

  onMount(async () => {
    locale = localeAffichee();
    connecte = await restaurerSession();
    reprise = false;
  });

  const confirmationValide = $derived(confirmation === MOT_DE_CONFIRMATION);

  async function effacer() {
    if (occupe || !confirmationValide) return;
    occupe = true;
    erreur = null;
    try {
      const reponse = await demanderEffacement(confirmation);
      programme = reponse.dans_jours;
      confirmation = "";
      deplie = false;
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
    }
  }

  async function annuler() {
    occupe = true;
    erreur = null;
    try {
      await annulerEffacement();
      programme = null;
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
    }
  }
</script>

{#if reprise}
  <p data-etat-compte="reprise">Reprise de session…</p>
{:else if !connecte}
  <p role="status" data-etat-compte="anonyme">
    Cette page demande d'être connecté. <a href="/connexion">Me connecter</a>
  </p>
{:else if programme !== null}
  <p role="status" data-effacement="programme">
    L'effacement de votre compte est programmé dans {programme} jours. Vos données
    personnelles seront supprimées à cette échéance. Vous pouvez encore changer
    d'avis jusque-là.
  </p>
  <button type="button" onclick={annuler} disabled={occupe} data-action="annuler-effacement">
    {occupe ? "Un instant…" : "Annuler l'effacement"}
  </button>
{:else if !deplie}
  <button type="button" onclick={() => (deplie = true)} data-action="ouvrir-effacement">
    Effacer mon compte
  </button>
{:else}
  <p>
    Cette demande supprimera votre adresse, votre mot de passe, vos sessions et
    vos abonnements aux notifications, après un délai de trente jours pendant
    lequel vous pourrez encore l'annuler.
  </p>
  <p class="klaar-tempere">
    Le journal d'audit, lui, est conservé : il ne porte ni votre adresse ni
    aucun contenu, seulement la trace horodatée que ce droit a été exercé.
  </p>

  <label for="effacement-confirmation">
    Recopiez <code>{MOT_DE_CONFIRMATION}</code> pour confirmer
  </label>
  <input
    id="effacement-confirmation"
    type="text"
    autocomplete="off"
    bind:value={confirmation}
    data-champ="confirmation"
  />

  <button
    type="button"
    onclick={effacer}
    disabled={occupe || !confirmationValide}
    data-action="confirmer-effacement"
  >
    {occupe ? "Un instant…" : "Effacer définitivement"}
  </button>
  <button type="button" onclick={() => (deplie = false)} data-action="renoncer">
    Renoncer
  </button>
{/if}

{#if erreur}
  <p role="alert" data-erreur-compte>{erreur}</p>
{/if}

<style>
  label { display: block; font-weight: 600; margin-top: 0.6rem; }
  input {
    font: inherit;
    padding: 0.5rem;
    border: 1px solid var(--klaar-bord);
    border-radius: 8px;
  }
  button {
    font: inherit;
    margin: 0.6rem 0.4rem 0 0;
    padding: 0.55rem 0.9rem;
    border-radius: 8px;
    border: 1px solid var(--klaar-bord);
    background: var(--klaar-accent);
    color: #1b3a4b;
    cursor: pointer;
  }
  button:disabled { opacity: 0.6; cursor: not-allowed; }
  p[role="alert"] { color: #c2543a; }
</style>
