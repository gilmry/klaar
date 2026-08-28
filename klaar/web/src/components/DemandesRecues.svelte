<script lang="ts">
  /**
   * Demandes reçues et intervention en cours (Story 4.10, FR-013, FR-018).
   *
   * **Ce qui est montré avant d'accepter, et ce qui l'est après.** Avant : le
   * secteur, la description, l'urgence, une distance. Pas l'adresse. Après :
   * l'adresse, parce qu'il faut s'y rendre. La règle est appliquée par l'API ;
   * cet écran la reflète, il ne l'invente pas.
   *
   * **Les boutons d'étape viennent du serveur.** `suites` porte les statuts
   * atteignables : recopier la machine à états ici la ferait diverger, et
   * l'interface proposerait un bouton que le domaine refuse.
   */
  import { onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerSession } from "../lib/connexion";
  import {
    accepter,
    avancerMission,
    codeDepuisErreur,
    demandesRecues,
    distanceLisible,
    libelleStatut,
    libelleTransition,
    libelleUrgence,
    lireMission,
    messageErreur,
    type Mission,
    type Proposee,
    type StatutMission,
  } from "../lib/prestataire";

  let connecte = $state(false);
  let reprise = $state(true);
  let demandes = $state<Proposee[]>([]);
  let mission = $state<Mission | null>(null);
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let locale = $state<LocaleKlaar>("fr");

  onMount(async () => {
    locale = localeAffichee();
    connecte = await restaurerSession();
    if (connecte) await rafraichir();
    reprise = false;
  });

  async function rafraichir() {
    erreur = null;
    try {
      demandes = await demandesRecues();
    } catch (e) {
      // Un compte non prestataire n'est pas une erreur à crier : la page le
      // dira une fois, et la liste reste vide.
      erreur = messageErreur(locale, codeDepuisErreur(e));
      demandes = [];
    }
  }

  async function prendre(id: string) {
    if (occupe) return;
    occupe = true;
    erreur = null;
    try {
      const attribuee = await accepter(id);
      mission = await lireMission(attribuee.id);
      demandes = [];
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
      // La Demande a peut-être été prise entre l'affichage et le clic : on
      // recharge plutôt que de laisser une ligne qui ne mène nulle part.
      await rafraichir();
    } finally {
      occupe = false;
    }
  }

  async function avancer(statut: StatutMission) {
    if (occupe || !mission) return;
    occupe = true;
    erreur = null;
    try {
      await avancerMission(mission.id, statut);
      mission = await lireMission(mission.id);
      if (statut === "COMPLETED" || statut === "CANCELLED") {
        // L'intervention est close : le prestataire redevient disponible, et
        // la liste des Demandes reprend son sens.
        await rafraichir();
      }
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
    }
  }
</script>

{#if reprise}
  <p data-etat-demandes="reprise">Reprise de session…</p>
{:else if !connecte}
  <p role="status" data-etat-demandes="anonyme">
    Cette page demande d'être connecté. <a href="/connexion">Me connecter</a>
  </p>
{:else if mission}
  <section data-mission={mission.statut}>
    <h3>Intervention en cours — {mission.secteur}</h3>
    <p data-mission-statut>{libelleStatut(mission.statut)}</p>
    <p>{mission.description}</p>
    <p class="klaar-tempere">
      Urgence : {libelleUrgence(mission.urgence)} · Adresse :
      <span data-mission-position>{mission.latitude.toFixed(5)}, {mission.longitude.toFixed(5)}</span>
    </p>

    {#if mission.suites.length === 0}
      <p role="status" data-mission-close>Cette intervention est close.</p>
      <button type="button" onclick={() => (mission = null)} data-action="revenir">
        Revenir aux Demandes
      </button>
    {:else}
      {#each mission.suites.filter((s) => s !== "CANCELLED") as suite}
        <button
          type="button"
          onclick={() => avancer(suite)}
          disabled={occupe}
          data-action="avancer"
          data-vers={suite}
        >
          {occupe ? "Un instant…" : libelleTransition(suite)}
        </button>
      {/each}
    {/if}
  </section>
{:else}
  <button type="button" onclick={rafraichir} disabled={occupe} data-action="rafraichir">
    Rafraîchir
  </button>

  {#if demandes.length === 0}
    <p role="status" data-demandes="aucune">
      Aucune Demande en attente. Elles apparaissent ici pendant les trente
      secondes qui suivent leur diffusion.
    </p>
  {:else}
    <ul data-demandes="liste">
      {#each demandes as d (d.id)}
        <li data-demande-id={d.id}>
          <h3>{d.secteur} · {distanceLisible(d.distance_metres)}</h3>
          <p>{d.description}</p>
          <p class="klaar-tempere">
            Urgence : {libelleUrgence(d.urgence)} · Encore {d.secondes_restantes} s
          </p>
          <p class="klaar-tempere">
            L'adresse exacte vous sera donnée si vous prenez cette intervention.
          </p>
          <button
            type="button"
            onclick={() => prendre(d.id)}
            disabled={occupe}
            data-action="accepter"
          >
            {occupe ? "Un instant…" : "Je prends"}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
{/if}

{#if erreur}
  <p role="alert" data-erreur-demandes>{erreur}</p>
{/if}

<style>
  ul { list-style: none; padding: 0; }
  li {
    border: 1px solid var(--klaar-bord);
    border-radius: 10px;
    padding: 0.8rem 1rem;
    margin: 0.8rem 0;
  }
  h3 { margin: 0 0 0.3rem; }
  section {
    border: 2px solid var(--klaar-accent);
    border-radius: 10px;
    padding: 0.8rem 1rem;
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
