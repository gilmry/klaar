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
   *
   * **Le temps réel accélère, il ne remplace pas (Story 4.9).** Dès qu'une
   * Mission existe, une socket s'ouvre et chaque événement déclenche une
   * relecture. Le sondage reste, ralenti de cinq à trente secondes : une socket
   * coupée par un proxy ne se signale pas, et un écran qui cesse de bouger sans
   * le dire est pire qu'un écran lent.
   */
  import { onMount, onDestroy } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerSession } from "../lib/connexion";
  import { ouvrirFlux } from "../lib/tempsReel";
  import {
    accepterDevis,
    annulerDemande,
    annulerMissionEnCours,
    attendUneReponse,
    codeDepuisErreur,
    delaiLisible,
    elargirZone,
    libelleDevis,
    libelleMission,
    libelleStatutDemande,
    messageErreur,
    montantLisible,
    MOTIFS_ANNULATION,
    MOTIFS_ANNULATION_MISSION,
    MOTIFS_REFUS,
    peutAnnuler,
    peutAnnulerMission,
    peutElargir,
    peutNoter,
    peutValider,
    noterIntervention,
    refuserDevis,
    suivreDemande,
    validerMission,
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
  let motifRefus = $state("");
  let motifArret = $state("");
  let noteChoisie = $state(0);
  let commentaireNote = $state("");
  let noteEnvoyee = $state(false);
  let locale = $state<LocaleKlaar>("fr");
  let minuterie: ReturnType<typeof setInterval> | null = null;

  /** Cadence du sondage seul, et cadence quand la socket vit (Story 4.9). */
  const SONDAGE_MS = 5000;
  const SONDAGE_AVEC_SOCKET_MS = 30000;

  let fermerFlux: (() => void) | null = null;
  let missionSuivie: string | null = null;
  let socketOuverte = $state(false);

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
      minuterie = setInterval(rafraichir, SONDAGE_MS);
    }
    reprise = false;
  });

  onDestroy(() => {
    if (minuterie) clearInterval(minuterie);
    fermerFlux?.();
  });

  /** Reprogramme le sondage à la cadence qui correspond à l'état de la socket. */
  function cadencer(periode: number) {
    if (minuterie === null) return;
    clearInterval(minuterie);
    minuterie = setInterval(rafraichir, periode);
  }

  /**
   * Ouvre la socket dès qu'une Mission existe, une seule fois par Mission.
   *
   * Rouvrir à chaque sondage dépenserait un billet toutes les cinq secondes et
   * finirait par ressembler à un abus.
   */
  function suivreEnDirect(missionId: string | null) {
    if (missionId === missionSuivie) return;
    fermerFlux?.();
    fermerFlux = null;
    missionSuivie = missionId;
    socketOuverte = false;
    if (!missionId) return;

    fermerFlux = ouvrirFlux(missionId, {
      // L'événement dit qu'il s'est passé quelque chose ; c'est la relecture
      // qui dit quoi, avec les droits que le serveur vérifie déjà.
      surEvenement: () => void rafraichir(),
      surEtat: (ouverte) => {
        socketOuverte = ouverte;
        cadencer(ouverte ? SONDAGE_AVEC_SOCKET_MS : SONDAGE_MS);
      },
    });
  }

  const etat = $derived(suivi ? libelleStatutDemande(suivi) : null);
  const intervention = $derived(suivi ? libelleMission(suivi.mission_statut) : null);

  async function rafraichir() {
    try {
      suivi = await suivreDemande(id);
      suivreEnDirect(suivi.mission_id);
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
    // La socket se ferme avec le sondage : garder ouverte une socket sur une
    // Mission close laisserait le service tenir un abonné pour rien.
    suivreEnDirect(null);
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

  /**
   * Accepte ou refuse le devis en attente (FR-017).
   *
   * La relecture suit toujours, y compris après un refus : le prestataire peut
   * en renvoyer un, et l'écran doit montrer l'état réel plutôt que celui qu'on
   * déduirait de la réponse.
   */
  async function repondreAuDevis(accepte: boolean) {
    if (occupe || !suivi?.mission_id) return;
    occupe = true;
    erreur = null;
    try {
      if (accepte) {
        await accepterDevis(suivi.mission_id);
      } else {
        await refuserDevis(suivi.mission_id, motifRefus || undefined);
      }
      motifRefus = "";
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
      // Après coup dans les deux cas : un refus du serveur signifie souvent que
      // le devis a changé sous nos yeux, et c'est cela qu'il faut montrer.
      await rafraichir();
    }
  }

  /** Valide la fin de l'intervention (FR-021). */
  async function valider() {
    if (occupe || !suivi?.mission_id) return;
    occupe = true;
    erreur = null;
    try {
      await validerMission(suivi.mission_id);
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
      await rafraichir();
    }
  }

  /**
   * Annule l'intervention en cours (FR-022).
   *
   * Le texte dit ce que cela coûte **avant** le clic : trente euros restent au
   * prestataire s'il est déjà sur place. L'apprendre après serait déloyal.
   */
  async function arreter_intervention() {
    if (occupe || !suivi?.mission_id) return;
    occupe = true;
    erreur = null;
    try {
      await annulerMissionEnCours(suivi.mission_id, motifArret || undefined);
      motifArret = "";
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
      await rafraichir();
    }
  }

  /**
   * Note le prestataire (FR-033).
   *
   * Le texte dit que la note reste cachée tant que l'autre n'a pas noté : sans
   * cela, l'absence d'affichage passerait pour une panne.
   */
  async function noter() {
    if (occupe || !suivi?.mission_id || noteChoisie < 1) return;
    occupe = true;
    erreur = null;
    try {
      await noterIntervention(suivi.mission_id, noteChoisie, commentaireNote.trim() || undefined);
      noteEnvoyee = true;
      commentaireNote = "";
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
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
  <section data-suivi={suivi.statut} data-direct={socketOuverte ? "ouvert" : "ferme"}>
    <p role="status" data-suivi-etat>{etat}</p>
    {#if intervention}
      <p data-suivi-intervention>{intervention}</p>
    {/if}
    <p class="klaar-tempere">
      {suivi.secteur} · zone de {(suivi.rayon_metres / 1000).toFixed(0)} km{#if suivi.elargissements > 0}, élargie {suivi.elargissements} fois sur 3{/if}
    </p>
    <p>{suivi.description}</p>

    {#if suivi.devis}
      <section data-devis={suivi.devis.statut} class="devis">
        <h3>Devis reçu</h3>
        <p data-devis-total>
          <strong>{montantLisible(suivi.devis.total_ttc_cents)} TTC</strong>
          <span class="klaar-tempere">
            ({montantLisible(suivi.devis.montant_htva_cents)} hors TVA + {montantLisible(
              suivi.devis.tva_cents,
            )} de TVA à {suivi.devis.taux_tva_bp / 100} %)
          </span>
        </p>
        <p class="klaar-tempere">
          Intervention annoncée sous {delaiLisible(suivi.devis.delai_minutes)}.
        </p>
        {#if suivi.devis.note}
          <p data-devis-note>{suivi.devis.note}</p>
        {/if}
        <p role="status" data-devis-etat>{libelleDevis(suivi.devis)}</p>

        {#if attendUneReponse(suivi.devis)}
          <div data-bloc="reponse-devis">
            <button
              type="button"
              onclick={() => repondreAuDevis(true)}
              disabled={occupe}
              data-action="accepter-devis"
            >
              {occupe ? "Un instant…" : "J'accepte ce devis"}
            </button>

            <label for="motif-refus">Si vous refusez, pourquoi (facultatif)</label>
            <select id="motif-refus" bind:value={motifRefus} data-champ="motif-refus">
              <option value="">Sans motif</option>
              {#each MOTIFS_REFUS as m}
                <option value={m.code}>{m.libelle}</option>
              {/each}
            </select>
            <button
              type="button"
              onclick={() => repondreAuDevis(false)}
              disabled={occupe}
              data-action="refuser-devis"
            >
              {occupe ? "Un instant…" : "Je refuse"}
            </button>
          </div>
        {/if}

        <p class="klaar-tempere">
          C'est le prestataire qui fixe son prix. Klaar ne le lui suggère pas et
          ne le corrige pas.
          <!-- Le paiement lui-même relève de l'Epic 5 (Stripe). L'accord est
               enregistré ; le règlement se fait pour l'instant entre les deux
               parties, et le dire vaut mieux que de laisser croire le contraire. -->
          L'accord est enregistré ici ; le règlement se fait pour l'instant
          directement avec le prestataire.
        </p>
      </section>
    {/if}

    {#if peutNoter(suivi) && !noteEnvoyee}
      <div data-bloc="notation">
        <p>Comment s'est passée l'intervention ?</p>
        <label for="note-etoiles">De 1 à 5 étoiles</label>
        <select id="note-etoiles" bind:value={noteChoisie} data-champ="note">
          <option value={0} disabled>Choisissez…</option>
          {#each [1, 2, 3, 4, 5] as etoiles}
            <option value={etoiles}>{etoiles} ★</option>
          {/each}
        </select>
        <label for="note-commentaire">Un mot (facultatif)</label>
        <input id="note-commentaire" type="text" bind:value={commentaireNote} data-champ="commentaire-note" />
        <button
          type="button"
          onclick={noter}
          disabled={occupe || noteChoisie < 1}
          data-action="noter"
        >
          {occupe ? "Un instant…" : "Envoyer ma note"}
        </button>
        <p class="klaar-tempere">
          Votre note reste cachée tant que le prestataire n'a pas donné la
          sienne : les deux s'affichent ensemble, pour que personne n'ajuste la
          sienne en fonction de l'autre.
        </p>
      </div>
    {:else if peutNoter(suivi) && noteEnvoyee}
      <p role="status" data-notation="envoyee">
        Merci. Votre note s'affichera quand le prestataire aura donné la sienne.
      </p>
    {/if}

    {#if peutAnnulerMission(suivi)}
      <div data-bloc="arret-intervention">
        <label for="motif-arret">Annuler l'intervention (facultatif : pourquoi)</label>
        <select id="motif-arret" bind:value={motifArret} data-champ="motif-arret">
          <option value="">Sans motif</option>
          {#each MOTIFS_ANNULATION_MISSION as m}
            <option value={m.code}>{m.libelle}</option>
          {/each}
        </select>
        <button
          type="button"
          onclick={arreter_intervention}
          disabled={occupe}
          data-action="annuler-intervention"
        >
          {occupe ? "Un instant…" : "Annuler l'intervention"}
        </button>
        {#if suivi.mission_statut === "ON_SITE"}
          <p class="klaar-tempere">
            Le prestataire est déjà sur place : 30 € lui resteront pour son
            déplacement, le reste vous est rendu.
          </p>
        {/if}
      </div>
    {/if}

    {#if peutValider(suivi)}
      <div data-bloc="validation">
        <p>
          Le prestataire a déclaré avoir terminé. Si c'est bien le cas,
          confirmez-le : c'est ce qui déclenche son paiement.
        </p>
        <button type="button" onclick={valider} disabled={occupe} data-action="valider">
          {occupe ? "Un instant…" : "L'intervention est bien terminée"}
        </button>
        <p class="klaar-tempere">
          Sans réponse de votre part, elle sera validée automatiquement dans les
          72 heures.
        </p>
      </div>
    {/if}

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
  .devis {
    border: 1px solid var(--klaar-bord);
    border-radius: 8px;
    padding: 0.6rem 0.8rem;
    margin: 0.8rem 0;
  }
  .devis h3 { margin: 0 0 0.4rem; font-size: 1.05rem; }
  p[data-devis-total] { font-size: 1.15rem; margin: 0.2rem 0; }
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
