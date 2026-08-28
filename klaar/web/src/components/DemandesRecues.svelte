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
   * **Le temps réel (Story 4.9).** Une socket suit l'intervention en cours et
   * relit dès qu'un événement arrive — utile surtout au prestataire dont le
   * devis vient d'expirer, ou dont la Mission a été annulée sous ses pieds.
   * Sans elle, cet écran ne bougeait qu'au clic.
   *
   * **Le prix est saisi, jamais suggéré (FR-016, invariant §10.2).** Le champ
   * est vide au départ et le reste : aucune valeur par défaut, aucun montant
   * « conseillé », aucun rappel de ce qu'on a facturé la dernière fois. Une
   * suggestion serait une fixation de prix douce, et c'est précisément ce que
   * la loi sur le travail de plateforme regarde.
   */
  import { onDestroy, onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerLangue, t } from "../lib/i18n";
  import { restaurerSession } from "../lib/connexion";
  import { ouvrirFlux } from "../lib/tempsReel";
  import Conversation from "./Conversation.svelte";
  import {
    accepter,
    annulerMission,
    consentirSuivi,
    envoyerPosition,
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
    MOTIFS_ANNULATION_MISSION,
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
  /** Partage de position pour l'intervention en cours (Story 4.4, FR-019). */
  let suiviConsenti = $state(false);
  let suiviErreur = $state<string | null>(null);
  let batteurPosition: ReturnType<typeof setInterval> | null = null;

  /**
   * Cadence d'envoi des positions.
   *
   * **Trente secondes, pas cinq.** Le serveur dégrade à cinquante mètres : en
   * ville, cinq secondes d'écart tombent le plus souvent dans la même maille,
   * et l'envoi n'apprend rien tout en vidant la batterie de quelqu'un qui
   * travaille. Trente secondes est aussi le délai au-delà duquel le serveur
   * déclare la position perdue : c'est le pas le plus lent qui reste utile.
   */
  const CADENCE_POSITION_MS = 30000;

  // Champs du devis. Vides à l'ouverture, et jamais préremplis : voir l'en-tête.
  let montantSaisi = $state("");
  let delaiSaisi = $state("");
  let noteSaisie = $state("");
  let tauxSaisi = $state("2100");
  let preuveSaisie = $state("");

  let motifAnnulation = $state("");
  let fermerFlux: (() => void) | null = null;
  let missionSuivie: string | null = null;

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
    locale = restaurerLangue();
    connecte = await restaurerSession();
    if (connecte) await rafraichir();
    reprise = false;
  });

  onDestroy(() => {
    fermerFlux?.();
    arreterPartage();
  });

  /**
   * Démarre ou arrête le partage de position (FR-019).
   *
   * **Le consentement est demandé au serveur d'abord, la géolocalisation
   * ensuite.** L'ordre compte : demander l'autorisation du navigateur avant que
   * le serveur ait accepté ferait surgir la fenêtre du système même quand le
   * partage sera refusé, et une permission arrachée pour rien ne se rend pas.
   */
  async function basculerSuivi(accepte: boolean) {
    if (!mission || occupe) return;
    occupe = true;
    suiviErreur = null;
    try {
      await consentirSuivi(mission.id, accepte);
      // Relire plutôt que croire la réponse : l'état affiché vient d'une seule
      // source, et c'est le serveur. Deux chemins vers la même case à cocher
      // finissent toujours par diverger.
      mission = await lireMission(mission.id);
    } catch (e) {
      suiviErreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
    }
  }

  /**
   * Aligne le battement d'envoi sur ce que dit le serveur.
   *
   * **Une seule source pour l'état du partage.** Le retenir dans l'écran le
   * ferait survivre à une révocation faite ailleurs, ou disparaître à un simple
   * rechargement ; le champ `suivi_consenti` de la Mission tranche.
   */
  $effect(() => {
    const partage = mission?.suivi_consenti === true && mission?.statut === "PROVIDER_EN_ROUTE";
    suiviConsenti = mission?.suivi_consenti === true;
    if (partage) {
      // Sans ce garde, chaque relecture de Mission relancerait un envoi
      // immédiat, et le rythme choisi ne voudrait plus rien dire.
      if (!batteurPosition) demarrerPartage();
    } else {
      arreterPartage();
    }
  });

  function demarrerPartage() {
    arreterPartage();
    void pousserPosition();
    batteurPosition = setInterval(() => void pousserPosition(), CADENCE_POSITION_MS);
  }

  function arreterPartage() {
    if (batteurPosition) clearInterval(batteurPosition);
    batteurPosition = null;
  }

  /**
   * Envoie une position, une seule fois.
   *
   * **Rien n'est mis en file hors-ligne.** Une position rejouée dix minutes
   * plus tard placerait le prestataire où il n'est plus, et le demandeur
   * descendrait attendre dans la rue. Un envoi manqué est perdu, et c'est le
   * bon comportement.
   */
  async function pousserPosition() {
    if (!mission || mission.statut !== "PROVIDER_EN_ROUTE") return;
    if (!navigator.geolocation) {
      suiviErreur = t(locale, "prestataire.geoloc_absente");
      arreterPartage();
      return;
    }
    const point = await new Promise<GeolocationPosition | null>((resoudre) => {
      navigator.geolocation.getCurrentPosition(
        (p) => resoudre(p),
        () => resoudre(null),
        { enableHighAccuracy: true, timeout: 10000, maximumAge: 15000 },
      );
    });
    if (!point) return;
    try {
      await envoyerPosition(mission.id, point.coords.latitude, point.coords.longitude);
      suiviErreur = null;
    } catch (e) {
      // Un refus du serveur arrête le battement : continuer à taper sur une
      // route qui répond 403 ne changerait rien et remplirait ses journaux.
      suiviErreur = messageErreur(locale, codeDepuisErreur(e));
      arreterPartage();
    }
  }

  /**
   * Ouvre la socket sur la Mission en cours, une seule fois par Mission.
   *
   * Rouvrir à chaque relecture dépenserait un billet par clic et finirait par
   * ressembler à un abus.
   */
  function suivreEnDirect(missionId: string | null) {
    if (missionId === missionSuivie) return;
    fermerFlux?.();
    fermerFlux = null;
    missionSuivie = missionId;
    if (!missionId) return;

    fermerFlux = ouvrirFlux(missionId, {
      surEvenement: async () => {
        // L'événement dit qu'il s'est passé quelque chose ; la relecture dit
        // quoi. Un échec ici est silencieux : l'écran garde son état plutôt que
        // d'afficher une erreur pour un rafraîchissement que personne n'a
        // demandé.
        try {
          if (missionSuivie) mission = await lireMission(missionSuivie);
        } catch {
          // La Mission n'est plus lisible : le prochain geste le dira.
        }
      },
    });
  }

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
      suivreEnDirect(mission.id);
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

  /**
   * Se désister d'une intervention (FR-022).
   *
   * Le libellé ne cache pas la conséquence : trois désistements en trente jours
   * suspendent le compte. La dire avant le clic vaut mieux que de la découvrir
   * après.
   */
  async function seDesister() {
    if (occupe || !mission) return;
    occupe = true;
    erreur = null;
    try {
      await annulerMission(mission.id, motifAnnulation || undefined);
      suivreEnDirect(null);
      mission = null;
      motifAnnulation = "";
      await rafraichir();
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
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
        // la liste des Demandes reprend son sens. La socket se ferme avec elle,
        // sinon le service tiendrait un abonné pour une Mission finie.
        suivreEnDirect(null);
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
  <p data-etat-demandes="reprise">{t(locale, "connexion.reprise")}</p>
{:else if !connecte}
  <p role="status" data-etat-demandes="anonyme">
    {t(locale, "commun.connexion_requise")}
    <a href="/connexion">{t(locale, "commun.me_connecter")}</a>
  </p>
{:else if mission}
  <section data-mission={mission.statut}>
    <h3>{t(locale, "pro.intervention_en_cours", { secteur: mission.secteur })}</h3>
    <p data-mission-statut>{libelleStatut(mission.statut, locale)}</p>
    <p>{mission.description}</p>
    <p class="klaar-tempere">
      {t(locale, "pro.urgence_adresse", { urgence: libelleUrgence(mission.urgence, locale) })}
      <span data-mission-position>{mission.latitude.toFixed(5)}, {mission.longitude.toFixed(5)}</span>
    </p>

    <Conversation missionId={mission.id} />

    {#if mission.devis}
      <section data-devis={mission.devis.statut} class="devis">
        <h4>{t(locale, "pro.devis_envoye")}</h4>
        <p data-devis-total>
          {montantLisible(mission.devis.total_ttc_cents, locale)} {t(locale, "devis.ttc")}
          <span class="klaar-tempere">
            {t(locale, "pro.devis_detail", {
              htva: montantLisible(mission.devis.montant_htva_cents, locale),
              tva: montantLisible(mission.devis.tva_cents, locale),
              taux: mission.devis.taux_tva_bp / 100,
            })}
          </span>
        </p>
        <p class="klaar-tempere">
          {t(locale, "pro.intervention_sous", { delai: delaiLisible(mission.devis.delai_minutes) })} ·
          <span data-devis-statut>{libelleStatutDevis(mission.devis, locale)}</span>
        </p>
        {#if mission.devis.note}
          <p>{mission.devis.note}</p>
        {/if}
      </section>
    {/if}

    {#if mission.suites.length > 0 && !devisEnAttente}
      {#if mission.devis_restants === 0}
        <p role="status" data-devis="plafond">
          {t(locale, "pro.plafond_devis")}
        </p>
      {:else}
        <form onsubmit={proposer} data-formulaire="devis">
          <h4>{t(locale, "pro.envoyer_devis_titre")}</h4>
          <p class="klaar-tempere">
            {t(locale, "pro.prix_libre", { n: mission.devis_restants })}
          </p>

          <label>
            {t(locale, "pro.montant_htva")}
            <input
              type="text"
              inputmode="decimal"
              bind:value={montantSaisi}
              name="montant"
              required
            />
          </label>

          <label>
            {t(locale, "pro.taux_tva")}
            <select bind:value={tauxSaisi} name="taux">
              <option value="2100">{t(locale, "pro.taux_normal")}</option>
              <option value="600">{t(locale, "pro.taux_logement")}</option>
              <option value="1200">{t(locale, "pro.taux_isolation")}</option>
            </select>
          </label>

          {#if tauxSaisi !== "2100"}
            <label>
              {t(locale, "pro.preuve_taux")}
              <input type="text" bind:value={preuveSaisie} name="preuve" required />
            </label>
          {/if}

          <label>
            {t(locale, "pro.delai_minutes")}
            <input
              type="text"
              inputmode="numeric"
              bind:value={delaiSaisi}
              name="delai"
              required
            />
          </label>

          <label>
            {t(locale, "pro.note_demandeur")}
            <input type="text" bind:value={noteSaisie} name="note" />
          </label>

          {#if apercuTtc !== null}
            <p class="klaar-tempere" data-devis-apercu>
              {t(locale, "pro.apercu", { ttc: montantLisible(apercuTtc, locale) })}
            </p>
          {/if}

          <button type="submit" disabled={occupe} data-action="envoyer-devis">
            {occupe ? t(locale, "commun.attendez") : t(locale, "pro.envoyer_devis")}
          </button>
        </form>
      {/if}
    {/if}

    {#if mission.suites.length === 0}
      <p role="status" data-mission-close>{t(locale, "prestataire.intervention_close")}</p>
      <button
        type="button"
        onclick={() => {
          suivreEnDirect(null);
          mission = null;
        }}
        data-action="revenir"
      >
        {t(locale, "prestataire.revenir_demandes")}
      </button>
    {:else}
      {#if mission.statut === "PROVIDER_EN_ROUTE"}
        <div data-bloc="suivi">
          <p class="klaar-tempere">
            {t(locale, "prestataire.partage_explication")}
          </p>
          <button
            type="button"
            onclick={() => void basculerSuivi(!suiviConsenti)}
            disabled={occupe}
            data-action="basculer-suivi"
            data-suivi={suiviConsenti ? "actif" : "inactif"}
          >
            {suiviConsenti
              ? t(locale, "prestataire.arreter_partage")
              : t(locale, "prestataire.partager_position")}
          </button>
          {#if suiviErreur}
            <p role="alert" data-suivi-erreur>{suiviErreur}</p>
          {/if}
        </div>
      {/if}

      <div data-bloc="desistement">
        <label for="motif-desistement">{t(locale, "pro.motif_desistement")}</label>
        <select id="motif-desistement" bind:value={motifAnnulation} data-champ="motif-desistement">
          <option value="">{t(locale, "commun.sans_motif")}</option>
          {#each MOTIFS_ANNULATION_MISSION as m}
            <option value={m.code}>{t(locale, m.cle)}</option>
          {/each}
        </select>
        <button type="button" onclick={seDesister} disabled={occupe} data-action="se-desister">
          {occupe ? t(locale, "commun.attendez") : t(locale, "pro.se_desister")}
        </button>
        <p class="klaar-tempere">
          {t(locale, "pro.suspension")}
        </p>
      </div>

      {#each mission.suites.filter((s) => s !== "CANCELLED") as suite}
        <button
          type="button"
          onclick={() => avancer(suite)}
          disabled={occupe}
          data-action="avancer"
          data-vers={suite}
        >
          {occupe ? t(locale, "commun.attendez") : libelleTransition(suite, locale)}
        </button>
      {/each}
    {/if}
  </section>
{:else}
  <button type="button" onclick={() => void rafraichir()} disabled={occupe} data-action="rafraichir">
    {t(locale, "commun.rafraichir")}
  </button>

  {#if demandes.length === 0}
    <p role="status" data-demandes="aucune">
      {t(locale, "pro.aucune_demande")}
    </p>
  {:else}
    <ul data-demandes="liste">
      {#each demandes as d (d.id)}
        <li data-demande-id={d.id}>
          <h3>{d.secteur} · {distanceLisible(d.distance_metres)}</h3>
          <p>{d.description}</p>
          <p class="klaar-tempere">
            {t(locale, "pro.urgence_restant", {
              urgence: libelleUrgence(d.urgence, locale),
              s: d.secondes_restantes,
            })}
          </p>
          <p class="klaar-tempere">
            {t(locale, "pro.adresse_apres")}
          </p>
          <button
            type="button"
            onclick={() => prendre(d.id)}
            disabled={occupe}
            data-action="accepter"
          >
            {occupe ? t(locale, "commun.attendez") : t(locale, "pro.je_prends")}
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
