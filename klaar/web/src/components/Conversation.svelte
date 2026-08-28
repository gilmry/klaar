<script lang="ts">
  /**
   * Fil de conversation d'une intervention (Story 6.1, FR-030, FR-032).
   *
   * **Le même composant des deux côtés.** Le service déduit du jeton qui écrit
   * et à qui ; `de_moi` suffit à placer la bulle. Faire deux composants aurait
   * demandé de dupliquer la même logique pour une différence d'affichage.
   *
   * **Le refus de coordonnées explique au lieu d'accuser.** Quelqu'un qui donne
   * son numéro n'a pas forcément voulu contourner quoi que ce soit ; le texte
   * dit pourquoi la messagerie protège, et le compteur n'apparaît qu'à la
   * récidive.
   */
  import { onDestroy, onMount } from "svelte";
  import {
    envoyerMessage,
    lireConversation,
    messageRefus,
    refusCoordonnees,
    type MessageLu,
  } from "../lib/conversation";
  import { ouvrirFlux } from "../lib/tempsReel";
  import { etiquetteBcp47, restaurerLangue, t } from "../lib/i18n";
  import type { LocaleKlaar } from "../lib/inscription";

  interface Props {
    /** Identifiant de la Mission. Le fil n'existe que pour une intervention. */
    missionId: string;
  }
  let { missionId }: Props = $props();

  let locale = $state<LocaleKlaar>("fr");
  let fil = $state<MessageLu[]>([]);
  let saisie = $state("");
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let fermerFlux: (() => void) | null = null;

  onMount(async () => {
    locale = restaurerLangue();
    await rafraichir();
    // La socket sert ici comme ailleurs : elle dit qu'il s'est passé quelque
    // chose, et c'est la relecture qui dit quoi.
    fermerFlux = ouvrirFlux(missionId, { surEvenement: () => void rafraichir() });
  });

  onDestroy(() => fermerFlux?.());

  async function rafraichir() {
    try {
      fil = await lireConversation(missionId);
    } catch {
      // Un fil illisible n'a pas à masquer l'écran d'intervention : le prochain
      // geste le dira.
    }
  }

  async function envoyer(evenement: Event) {
    evenement.preventDefault();
    if (occupe || saisie.trim() === "") return;
    occupe = true;
    erreur = null;
    try {
      await envoyerMessage(missionId, saisie);
      saisie = "";
      await rafraichir();
    } catch (e) {
      const refus = refusCoordonnees(e);
      erreur = refus
        ? messageRefus(refus)
        : "Le message n'est pas parti. Réessayez.";
    } finally {
      occupe = false;
    }
  }

  /** Heure lisible, sans la date : le fil est court et récent. */
  function heure(iso: string): string {
    return new Date(iso).toLocaleTimeString(etiquetteBcp47(locale), {
      hour: "2-digit",
      minute: "2-digit",
    });
  }
</script>

<section data-conversation aria-label="Conversation">
  <h4>{t(locale, "conversation.titre")}</h4>

  {#if fil.length === 0}
    <p class="klaar-tempere" data-fil="vide">
      {t(locale, "conversation.vide")}
    </p>
  {:else}
    <ul data-fil="liste">
      {#each fil as message (message.id)}
        <li data-de-moi={message.de_moi}>
          <p>{message.corps}</p>
          <span class="klaar-tempere">{heure(message.envoye_le)}</span>
        </li>
      {/each}
    </ul>
  {/if}

  <form onsubmit={envoyer} data-formulaire="message">
    <label for="message-corps">{t(locale, "conversation.votre_message")}</label>
    <input id="message-corps" type="text" bind:value={saisie} data-champ="message" />
    <button type="submit" disabled={occupe || saisie.trim() === ""} data-action="envoyer-message">
      {occupe ? t(locale, "commun.attendez") : t(locale, "conversation.envoyer")}
    </button>
  </form>

  {#if erreur}
    <p role="alert" data-erreur-conversation>{erreur}</p>
  {/if}
</section>

<style>
  section {
    border: 1px solid var(--klaar-bord);
    border-radius: 8px;
    padding: 0.6rem 0.8rem;
    margin: 0.8rem 0;
  }
  h4 { margin: 0 0 0.4rem; }
  ul { list-style: none; padding: 0; margin: 0 0 0.6rem; }
  li {
    border-radius: 10px;
    padding: 0.4rem 0.7rem;
    margin: 0.3rem 0;
    background: #f2f4f5;
    max-width: 85%;
  }
  li[data-de-moi="true"] {
    background: var(--klaar-accent);
    margin-left: auto;
  }
  li p { margin: 0; }
  form { display: flex; gap: 0.4rem; align-items: end; flex-wrap: wrap; }
  label {
    /* Le libellé reste dans le document pour les lecteurs d'écran ; le champ
       se suffit visuellement à lui-même dans un fil. */
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  input {
    font: inherit;
    flex: 1 1 12rem;
    padding: 0.45rem 0.5rem;
    border: 1px solid var(--klaar-bord);
    border-radius: 6px;
  }
  button {
    font: inherit;
    padding: 0.5rem 0.9rem;
    border-radius: 8px;
    border: 1px solid var(--klaar-bord);
    background: var(--klaar-accent);
    color: #1b3a4b;
    cursor: pointer;
  }
  button:disabled { opacity: 0.6; cursor: not-allowed; }
  p[role="alert"] { color: #c2543a; }
</style>
