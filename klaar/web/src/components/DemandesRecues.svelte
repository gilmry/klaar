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
   *
   * **Le prix est saisi, jamais suggéré (FR-016, invariant §10.2).** Le champ
   * est vide au départ et le reste : aucune valeur par défaut, aucun montant
   * « conseillé », aucun rappel de ce qu'on a facturé la dernière fois. Une
   * suggestion serait une fixation de prix douce, et c'est précisément ce que
   * la loi sur le travail de plateforme regarde.
   */
  import { onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerSession } from "../lib/connexion";
  import {
    accepter,
    avancerMission,
    centimesDepuisEuros,
    codeDepuisErreur,
    delaiLisible,
    demandesRecues,
    distanceLisible,
    envoyerDevis,
    libelleStatut,
    libelleStatutDevis,
    libelleTransition,
    libelleUrgence,
    lireMission,
    messageErreur,
    montantLisible,
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

  // Champs du devis. Vides à l'ouverture, et jamais préremplis : voir l'en-tête.
  let montantSaisi = $state("");
  let delaiSaisi = $state("");
  let noteSaisie = $state("");
  let tauxSaisi = $state("2100");
  let preuveSaisie = $state("");

  /** Un devis attend encore une réponse : le formulaire n'a rien à faire là. */
  let devisEnAttente = $derived(
    mission?.devis != null && mission.devis.statut === "SENT" && !mission.devis.echu,
  );

  /**
   * Total TTC calculé pendant la saisie.
   *
   * **C'est un aperçu, pas le devis.** Le montant qui fait foi est celui que le
   * serveur calcule et conserve ; le recopier ici servirait à voir ce qu'on
   * envoie, jamais à le décider. Rendu `null` tant que la saisie n'est pas un
   * montant.
   */
  let apercuTtc = $derived.by(() => {
    const cents = centimesDepuisEuros(montantSaisi);
    if (cents === null || cents <= 0) return null;
    const bp = Number(tauxSaisi);
    return cents + Math.trunc((cents * bp) / 10_000);
  });

  onMount(async () => {
    locale = localeAffichee();
    connecte = await restaurerSession();
    if (connecte) await rafraichir();
    reprise = false;
  });

  /**
   * Recharge la liste.
   *
   * `effacerErreur` vaut faux quand le rechargement **suit** un refus : sinon
   * le message qui explique pourquoi l'intervention a échappé au prestataire
   * disparaît dans la même seconde, et il ne reste rien à l'écran. Trouvé en
   * filmant la course entre deux prestataires : le perdant n'avait aucune
   * explication.
   */
  async function rafraichir(effacerErreur = true) {
    if (effacerErreur) erreur = null;
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
      // recharge plutôt que de laisser une ligne qui ne mène nulle part. Le
      // message, lui, reste : c'est la seule chose qui explique ce qui vient de
      // se passer.
      await rafraichir(false);
    } finally {
      occupe = false;
    }
  }

  async function proposer(evenement: Event) {
    evenement.preventDefault();
    if (occupe || !mission) return;
    const cents = centimesDepuisEuros(montantSaisi);
    const minutes = Number(delaiSaisi.trim());
    // Les deux contrôles locaux sont là pour éviter d'envoyer `null` au serveur,
    // pas pour dupliquer ses règles : les bornes, les taux et les textes sont
    // vérifiés par le domaine, et ses refus s'affichent tels quels.
    if (cents === null) {
      erreur = messageErreur(locale, "AMOUNT_ZERO");
      return;
    }
    if (!Number.isFinite(minutes)) {
      erreur = messageErreur(locale, "DELAY_INVALID");
      return;
    }

    occupe = true;
    erreur = null;
    try {
      await envoyerDevis(mission.id, {
        montant_htva_cents: cents,
        taux_tva_bp: Number(tauxSaisi),
        delai_minutes: Math.trunc(minutes),
        note: noteSaisie.trim() || undefined,
        preuve_tva_reduite: preuveSaisie.trim() || undefined,
      });
      // Relu plutôt que déduit de la réponse : le serveur sait aussi combien
      // de devis restent, et c'est ce qui décide de la suite de l'écran.
      mission = await lireMission(mission.id);
      montantSaisi = "";
      delaiSaisi = "";
      noteSaisie = "";
      preuveSaisie = "";
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
      // Le plafond atteint annule la Mission côté serveur : la relire évite de
      // laisser à l'écran une intervention qui n'existe plus.
      try {
        mission = await lireMission(mission.id);
      } catch {
        // La Mission n'est plus lisible : le message d'erreur suffit.
      }
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

    {#if mission.devis}
      <section data-devis={mission.devis.statut} class="devis">
        <h4>Devis envoyé</h4>
        <p data-devis-total>
          {montantLisible(mission.devis.total_ttc_cents)} TTC
          <span class="klaar-tempere">
            ({montantLisible(mission.devis.montant_htva_cents)} HTVA + {montantLisible(
              mission.devis.tva_cents,
            )} de TVA à {mission.devis.taux_tva_bp / 100} %)
          </span>
        </p>
        <p class="klaar-tempere">
          Intervention sous {delaiLisible(mission.devis.delai_minutes)} ·
          <span data-devis-statut>{libelleStatutDevis(mission.devis)}</span>
        </p>
        {#if mission.devis.note}
          <p>{mission.devis.note}</p>
        {/if}
      </section>
    {/if}

    {#if mission.suites.length > 0 && !devisEnAttente}
      {#if mission.devis_restants === 0}
        <p role="status" data-devis="plafond">
          Trois devis ont déjà été envoyés pour cette intervention. Un de plus
          l'annulerait.
        </p>
      {:else}
        <form onsubmit={proposer} data-formulaire="devis">
          <h4>Envoyer un devis</h4>
          <p class="klaar-tempere">
            Vous fixez votre prix. Klaar n'en propose aucun, n'en suggère aucun
            et n'en corrige aucun. Il vous reste {mission.devis_restants} envoi{mission.devis_restants >
            1
              ? "s"
              : ""}.
          </p>

          <label>
            Montant hors TVA, en euros
            <input
              type="text"
              inputmode="decimal"
              bind:value={montantSaisi}
              name="montant"
              required
            />
          </label>

          <label>
            Taux de TVA
            <select bind:value={tauxSaisi} name="taux">
              <option value="2100">21 % — taux normal</option>
              <option value="600">6 % — logement de plus de 5 ans</option>
              <option value="1200">12 % — isolation thermique</option>
            </select>
          </label>

          {#if tauxSaisi !== "2100"}
            <label>
              Preuve du taux réduit
              <input type="text" bind:value={preuveSaisie} name="preuve" required />
            </label>
          {/if}

          <label>
            Délai d'intervention, en minutes
            <input
              type="text"
              inputmode="numeric"
              bind:value={delaiSaisi}
              name="delai"
              required
            />
          </label>

          <label>
            Note pour le demandeur
            <input type="text" bind:value={noteSaisie} name="note" />
          </label>

          {#if apercuTtc !== null}
            <p class="klaar-tempere" data-devis-apercu>
              Le demandeur verra {montantLisible(apercuTtc)} TTC.
            </p>
          {/if}

          <button type="submit" disabled={occupe} data-action="envoyer-devis">
            {occupe ? "Un instant…" : "Envoyer le devis"}
          </button>
        </form>
      {/if}
    {/if}

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
  h4 { margin: 0.4rem 0 0.3rem; }
  .devis {
    border: 1px solid var(--klaar-bord);
    border-radius: 8px;
    padding: 0.6rem 0.8rem;
    margin: 0.8rem 0;
  }
  form label {
    display: block;
    margin: 0.5rem 0;
  }
  form input,
  form select {
    display: block;
    font: inherit;
    width: 100%;
    max-width: 22rem;
    margin-top: 0.2rem;
    padding: 0.45rem 0.5rem;
    border: 1px solid var(--klaar-bord);
    border-radius: 6px;
  }
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
