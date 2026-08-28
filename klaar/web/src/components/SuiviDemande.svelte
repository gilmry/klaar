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
  import { restaurerLangue, t } from "../lib/i18n";
  import { restaurerSession } from "../lib/connexion";
  import { ouvrirFlux } from "../lib/tempsReel";
  import Conversation from "./Conversation.svelte";
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
    MOTIFS_LITIGE,
    MOTIFS_REFUS,
    peutAnnuler,
    peutAnnulerMission,
    peutElargir,
    peutNoter,
    peutValider,
    noterIntervention,
    ouvrirLitige,
    peutContester,
    RECIT_MIN_CARACTERES,
    refuserDevis,
    suivreDemande,
    suivreTrajet,
    libelleTrajet,
    validerMission,
    type TrajetSuivi,
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
  let motifLitige = $state("");
  let recitLitige = $state("");
  let litigeOuvert = $state(false);
  let locale = $state<LocaleKlaar>("fr");
  let trajet = $state<TrajetSuivi | null>(null);
  let minuterie: ReturnType<typeof setInterval> | null = null;

  /** Cadence du sondage seul, et cadence quand la socket vit (Story 4.9). */
  const SONDAGE_MS = 5000;
  const SONDAGE_AVEC_SOCKET_MS = 30000;

  let fermerFlux: (() => void) | null = null;
  let missionSuivie: string | null = null;
  let socketOuverte = $state(false);

  onMount(async () => {
    locale = restaurerLangue();
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

  const etat = $derived(suivi ? libelleStatutDemande(suivi, locale) : null);
  const intervention = $derived(suivi ? libelleMission(suivi.mission_statut, locale) : null);

  async function rafraichir() {
    try {
      suivi = await suivreDemande(id);
      suivreEnDirect(suivi.mission_id);
      await rafraichirTrajet();
      // Une Demande close ne bouge plus : arrêter le sondage évite de
      // continuer à interroger le serveur pour rien.
      if (suivi.statut === "MATCHED" && suivi.mission_statut === "COMPLETED") arreter();
      if (suivi.statut === "CANCELLED") arreter();
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
      arreter();
    }
  }

  /**
   * Lit le trajet, et seulement pendant le trajet (FR-019).
   *
   * **L'échec est silencieux.** Une position indisponible n'est pas une panne
   * de la page : afficher une erreur rouge parce qu'un GPS n'a rien renvoyé
   * ferait douter d'une intervention qui se passe bien. L'état `POSITION_LOST`
   * dit déjà ce qu'il faut.
   */
  async function rafraichirTrajet() {
    if (!suivi?.mission_id || suivi.mission_statut !== "PROVIDER_EN_ROUTE") {
      trajet = null;
      return;
    }
    try {
      trajet = await suivreTrajet(suivi.mission_id);
    } catch {
      trajet = null;
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

  /**
   * Ouvre un litige (FR-034).
   *
   * Le formulaire exige un récit : « pas content » ne permet à personne de
   * trancher, et le dire avant l'envoi vaut mieux qu'un refus après.
   */
  async function contester() {
    if (occupe || !suivi?.mission_id || motifLitige === "") return;
    occupe = true;
    erreur = null;
    try {
      await ouvrirLitige(suivi.mission_id, motifLitige, recitLitige);
      litigeOuvert = true;
      recitLitige = "";
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
  <p data-etat-suivi="reprise">{t(locale, "connexion.reprise")}</p>
{:else if !connecte}
  <p role="status" data-etat-suivi="anonyme">
    {t(locale, "commun.connexion_requise")}
    <a href="/connexion">{t(locale, "commun.me_connecter")}</a>
  </p>
{:else if suivi === null}
  <p role="alert" data-etat-suivi="introuvable">{erreur ?? t(locale, "suivi.introuvable")}</p>
{:else}
  <section data-suivi={suivi.statut} data-direct={socketOuverte ? "ouvert" : "ferme"}>
    <p role="status" data-suivi-etat>{etat}</p>
    {#if intervention}
      <p data-suivi-intervention>{intervention}</p>
    {/if}
    {#if trajet}
      <p data-suivi-trajet={trajet.etat} class="klaar-tempere">
        {libelleTrajet(trajet.etat, locale)}
        <!--
          La position est rendue en clair plutôt que sur une carte : la maille
          de cinquante mètres appliquée par le serveur rend un point sur un plan
          plus précis qu'il n'est, et une carte donnerait à croire à un pointé
          au mètre. Une carte viendra, avec un cercle et non un point.
        -->
        {#if trajet.position}
          <span data-suivi-position>({trajet.position.lat.toFixed(3)}, {trajet.position.lon.toFixed(3)}, {t(locale, "trajet.precision")})</span>
        {/if}
      </p>
    {/if}
    <p class="klaar-tempere">
      {t(locale, "suivi.km", {
        secteur: suivi.secteur,
        km: (suivi.rayon_metres / 1000).toFixed(0),
      })}{#if suivi.elargissements > 0}, {t(locale, "suivi.elargie", { n: suivi.elargissements })}{/if}
    </p>
    <p>{suivi.description}</p>

    {#if suivi.devis}
      <section data-devis={suivi.devis.statut} class="devis">
        <h3>{t(locale, "suivi.devis_recu")}</h3>
        <p data-devis-total>
          <strong>{montantLisible(suivi.devis.total_ttc_cents, locale)} {t(locale, "devis.ttc")}</strong>
          <span class="klaar-tempere">
            {t(locale, "devis.detail", {
              htva: montantLisible(suivi.devis.montant_htva_cents, locale),
              tva: montantLisible(suivi.devis.tva_cents, locale),
              taux: suivi.devis.taux_tva_bp / 100,
            })}
          </span>
        </p>
        <p class="klaar-tempere">
          {t(locale, "devis.delai", { delai: delaiLisible(suivi.devis.delai_minutes) })}
        </p>
        {#if suivi.devis.note}
          <p data-devis-note>{suivi.devis.note}</p>
        {/if}
        <p role="status" data-devis-etat>{libelleDevis(suivi.devis, locale)}</p>

        {#if attendUneReponse(suivi.devis)}
          <div data-bloc="reponse-devis">
            <button
              type="button"
              onclick={() => repondreAuDevis(true)}
              disabled={occupe}
              data-action="accepter-devis"
            >
              {occupe ? t(locale, "commun.attendez") : t(locale, "devis.accepter")}
            </button>

            <label for="motif-refus">{t(locale, "devis.motif_refus")}</label>
            <select id="motif-refus" bind:value={motifRefus} data-champ="motif-refus">
              <option value="">{t(locale, "commun.sans_motif")}</option>
              {#each MOTIFS_REFUS as m}
                <option value={m.code}>{t(locale, m.cle)}</option>
              {/each}
            </select>
            <button
              type="button"
              onclick={() => repondreAuDevis(false)}
              disabled={occupe}
              data-action="refuser-devis"
            >
              {occupe ? t(locale, "commun.attendez") : t(locale, "devis.refuser")}
            </button>
          </div>
        {/if}

        <p class="klaar-tempere">
          {t(locale, "devis.prix_libre")}
          <!-- Le paiement lui-même relève de l'Epic 5 (Stripe). L'accord est
               enregistré ; le règlement se fait pour l'instant entre les deux
               parties, et le dire vaut mieux que de laisser croire le contraire. -->
          {t(locale, "devis.reglement_direct")}
        </p>
      </section>
    {/if}

    {#if peutNoter(suivi) && !noteEnvoyee}
      <div data-bloc="notation">
        <p>{t(locale, "note.question")}</p>
        <label for="note-etoiles">{t(locale, "note.etoiles")}</label>
        <select id="note-etoiles" bind:value={noteChoisie} data-champ="note">
          <option value={0} disabled>{t(locale, "demande.choisissez")}</option>
          {#each [1, 2, 3, 4, 5] as etoiles}
            <option value={etoiles}>{etoiles} ★</option>
          {/each}
        </select>
        <label for="note-commentaire">{t(locale, "note.commentaire")}</label>
        <input id="note-commentaire" type="text" bind:value={commentaireNote} data-champ="commentaire-note" />
        <button
          type="button"
          onclick={noter}
          disabled={occupe || noteChoisie < 1}
          data-action="noter"
        >
          {occupe ? t(locale, "commun.attendez") : t(locale, "note.envoyer")}
        </button>
        <p class="klaar-tempere">
          {t(locale, "note.cachee")}
        </p>
      </div>
    {:else if peutNoter(suivi) && noteEnvoyee}
      <p role="status" data-notation="envoyee">
        {t(locale, "note.merci")}
      </p>
    {/if}

    {#if suivi.mission_id}
      <Conversation missionId={suivi.mission_id} />
    {/if}

    {#if peutContester(suivi) && !litigeOuvert}
      <details data-bloc="litige">
        <summary>{t(locale, "litige.ouvrir_details")}</summary>
        <p class="klaar-tempere">
          {t(locale, "litige.explication")}
        </p>
        <label for="motif-litige">{t(locale, "litige.motif")}</label>
        <select id="motif-litige" bind:value={motifLitige} data-champ="motif-litige">
          <option value="" disabled>{t(locale, "demande.choisissez")}</option>
          {#each MOTIFS_LITIGE as m}
            <option value={m.code}>{t(locale, m.cle)}</option>
          {/each}
        </select>
        <label for="recit-litige">{t(locale, "litige.recit")}</label>
        <textarea
          id="recit-litige"
          bind:value={recitLitige}
          rows="3"
          data-champ="recit-litige"
        ></textarea>
        <button
          type="button"
          onclick={contester}
          disabled={occupe || motifLitige === "" || recitLitige.trim().length < RECIT_MIN_CARACTERES}
          data-action="ouvrir-litige"
        >
          {occupe ? t(locale, "commun.attendez") : t(locale, "litige.ouvrir")}
        </button>
      </details>
    {:else if litigeOuvert}
      <p role="status" data-litige="ouvert">
        {t(locale, "litige.ouvert")}
      </p>
    {/if}

    {#if peutAnnulerMission(suivi)}
      <div data-bloc="arret-intervention">
        <label for="motif-arret">{t(locale, "arret.motif")}</label>
        <select id="motif-arret" bind:value={motifArret} data-champ="motif-arret">
          <option value="">{t(locale, "commun.sans_motif")}</option>
          {#each MOTIFS_ANNULATION_MISSION as m}
            <option value={m.code}>{t(locale, m.cle)}</option>
          {/each}
        </select>
        <button
          type="button"
          onclick={arreter_intervention}
          disabled={occupe}
          data-action="annuler-intervention"
        >
          {occupe ? t(locale, "commun.attendez") : t(locale, "arret.bouton")}
        </button>
        {#if suivi.mission_statut === "ON_SITE"}
          <p class="klaar-tempere">
            {t(locale, "arret.forfait")}
          </p>
        {/if}
      </div>
    {/if}

    {#if peutValider(suivi)}
      <div data-bloc="validation">
        <p>
          {t(locale, "validation.question")}
        </p>
        <button type="button" onclick={valider} disabled={occupe} data-action="valider">
          {occupe ? t(locale, "commun.attendez") : t(locale, "validation.bouton")}
        </button>
        <p class="klaar-tempere">
          {t(locale, "validation.automatique")}
        </p>
      </div>
    {/if}

    {#if peutElargir(suivi)}
      <button type="button" onclick={elargir} disabled={occupe} data-action="elargir">
        {occupe ? t(locale, "commun.attendez") : t(locale, "suivi.elargir")}
      </button>
    {/if}

    {#if peutAnnuler(suivi)}
      <div data-bloc="annulation">
        <label for="motif-annulation">{t(locale, "suivi.motif_retrait")}</label>
        <select id="motif-annulation" bind:value={motif} data-champ="motif">
          <option value="">{t(locale, "commun.sans_motif")}</option>
          {#each MOTIFS_ANNULATION as m}
            <option value={m.code}>{t(locale, m.cle)}</option>
          {/each}
        </select>
        <button type="button" onclick={annuler} disabled={occupe} data-action="annuler">
          {occupe ? t(locale, "commun.attendez") : t(locale, "suivi.retirer")}
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
  textarea {
    font: inherit;
    display: block;
    width: 100%;
    max-width: 30rem;
    padding: 0.45rem;
    border-radius: 8px;
    border: 1px solid var(--klaar-bord);
  }
  details { margin-top: 0.8rem; }
  summary { cursor: pointer; }
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
