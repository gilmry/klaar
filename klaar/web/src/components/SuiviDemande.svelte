<script lang="ts">
  /**
   * Suivi d'une Demande par son auteur (Story 4.10, FR-014, FR-015, FR-018).
   *
   * C'est la page que les notifications ouvrent : `/demande?id=…`. Sans elle,
   * un « personne n'a répondu » menait à un formulaire vierge.
   *
   * **Le statut stocké ne suffit pas à dire ce qui se passe.** Une Demande peut
   * être « en diffusion » et son tour écoulé, parce que le balayage passe
   * périodiquement. Afficher « recherche en cours » dans ce cas ferait attendre
   * pour rien : `tour_ecoule` tranche, et c'est le serveur qui le calcule.
   */
  import { onMount, onDestroy } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerSession } from "../lib/connexion";
  import {
    annulerDemande,
    codeDepuisErreur,
    elargirZone,
    libelleMission,
    libelleStatutDemande,
    messageErreur,
    MOTIFS_ANNULATION,
    peutAnnuler,
    peutElargir,
    suivreDemande,
    type SuiviDemande,
  } from "../lib/demande";

  // L'identifiant est lu dans la barre d'adresse plutôt que reçu en propriété :
  // le site est généré statiquement, la page n'a donc rien à transmettre, et
  // faire transiter la valeur par un événement personnalisé ajouterait un
  // aller-retour pour la même information.
  let id = $state("");

  let connecte = $state(false);
  let reprise = $state(true);
  let suivi = $state<SuiviDemande | null>(null);
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let motif = $state("");
  let locale = $state<LocaleKlaar>("fr");
  let minuterie: ReturnType<typeof setInterval> | null = null;

  onMount(async () => {
    locale = localeAffichee();
    id = new URLSearchParams(location.search).get("id") ?? "";
    if (!id) {
      reprise = false;
      return;
    }
    connecte = await restaurerSession();
    if (connecte) {
      await rafraichir();
      // Cinq secondes : assez pour voir arriver une acceptation sans marteler
      // le serveur. Le temps réel viendra avec le WebSocket (FR-018) ; d'ici
      // là, un sondage court est honnête et se voit dans les journaux.
      minuterie = setInterval(rafraichir, 5000);
    }
    reprise = false;
  });

  onDestroy(() => {
    if (minuterie) clearInterval(minuterie);
  });

  const etat = $derived(suivi ? libelleStatutDemande(suivi) : null);
  const intervention = $derived(suivi ? libelleMission(suivi.mission_statut) : null);

  async function rafraichir() {
    try {
      suivi = await suivreDemande(id);
      // Une Demande close ne bouge plus : arrêter le sondage évite de
      // continuer à interroger le serveur pour rien.
      if (suivi.statut === "MATCHED" && suivi.mission_statut === "COMPLETED") arreter();
      if (suivi.statut === "CANCELLED") arreter();
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
      arreter();
    }
  }

  function arreter() {
    if (minuterie) clearInterval(minuterie);
    minuterie = null;
  }

  async function elargir() {
    if (occupe) return;
    occupe = true;
    erreur = null;
    try {
      suivi = await elargirZone(id);
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
      await rafraichir();
    } finally {
      occupe = false;
    }
  }

  async function annuler() {
    if (occupe) return;
    occupe = true;
    erreur = null;
    try {
      await annulerDemande(id, motif || undefined);
      await rafraichir();
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
    }
  }
</script>

{#if reprise}
  <p data-etat-suivi="reprise">Reprise de session…</p>
{:else if !connecte}
  <p role="status" data-etat-suivi="anonyme">
    Cette page demande d'être connecté. <a href="/connexion">Me connecter</a>
  </p>
{:else if suivi === null}
  <p role="alert" data-etat-suivi="introuvable">{erreur ?? "Demande introuvable."}</p>
{:else}
  <section data-suivi={suivi.statut}>
    <p role="status" data-suivi-etat>{etat}</p>
    {#if intervention}
      <p data-suivi-intervention>{intervention}</p>
    {/if}
    <p class="klaar-tempere">
      {suivi.secteur} · zone de {(suivi.rayon_metres / 1000).toFixed(0)} km{#if suivi.elargissements > 0}, élargie {suivi.elargissements} fois sur 3{/if}
    </p>
    <p>{suivi.description}</p>

    {#if peutElargir(suivi)}
      <button type="button" onclick={elargir} disabled={occupe} data-action="elargir">
        {occupe ? "Un instant…" : "Élargir la zone de recherche"}
      </button>
    {/if}

    {#if peutAnnuler(suivi)}
      <div data-bloc="annulation">
        <label for="motif-annulation">Pourquoi (facultatif)</label>
        <select id="motif-annulation" bind:value={motif} data-champ="motif">
          <option value="">Sans motif</option>
          {#each MOTIFS_ANNULATION as m}
            <option value={m.code}>{m.libelle}</option>
          {/each}
        </select>
        <button type="button" onclick={annuler} disabled={occupe} data-action="annuler">
          {occupe ? "Un instant…" : "Retirer ma demande"}
        </button>
      </div>
    {/if}
  </section>
{/if}

{#if erreur && suivi !== null}
  <p role="alert" data-erreur-suivi>{erreur}</p>
{/if}

<style>
  section {
    border: 1px solid var(--klaar-bord);
    border-radius: 10px;
    padding: 0.8rem 1rem;
  }
  p[data-suivi-etat] { font-weight: 600; font-size: 1.1rem; margin-top: 0; }
  label { display: block; font-weight: 600; margin-top: 0.6rem; }
  select { font: inherit; padding: 0.45rem; border-radius: 8px; border: 1px solid var(--klaar-bord); }
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
