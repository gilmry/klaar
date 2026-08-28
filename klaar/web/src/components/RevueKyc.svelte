<script lang="ts">
  /**
   * Revue du contrôle d'entreprise (Story 8.1, FR-038).
   *
   * **Valider et refuser ne se présentent pas de la même façon.** Valider est un
   * bouton ; refuser demande un motif écrit, puis la confirmation d'un autre
   * compte. L'asymétrie est celle du FR, et elle a une raison : une validation
   * trop généreuse se corrige par une suspension, un refus injuste ne se corrige
   * pas — l'entreprise est déjà partie.
   *
   * **Le second examinateur voit le motif du premier.** Sans cela, il en
   * rédigerait un qui ne serait jamais consigné.
   */
  import { onMount } from "svelte";
  import {
    fileKyc,
    MOTIF_KYC_MIN,
    reviserKyc,
    sessionFinie,
    type DossierKyc,
    type IssueRevue,
  } from "../lib/ops";

  let dossiers = $state<DossierKyc[]>([]);
  let choisi = $state<string | null>(null);
  let motif = $state("");
  let issue = $state<IssueRevue | null>(null);
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let charge = $state(false);

  onMount(() => void rafraichir());

  async function rafraichir() {
    try {
      dossiers = await fileKyc();
      erreur = null;
    } catch (e) {
      erreur = sessionFinie(e)
        ? "Votre session a expiré."
        : "La file de contrôle est indisponible.";
    } finally {
      charge = true;
    }
  }

  const dossier = $derived(dossiers.find((d) => d.provider_id === choisi) ?? null);

  /**
   * Un refus déjà proposé par quelqu'un d'autre : ce geste est une
   * confirmation, pas une nouvelle proposition. Le sien propre ne compte pas.
   */
  const aConfirmer = $derived(
    dossier?.refus_en_attente !== null &&
      dossier?.refus_en_attente !== undefined &&
      !dossier.refus_en_attente.propose_par_moi,
  );
  const motifSuffisant = $derived(motif.trim().length >= MOTIF_KYC_MIN);

  async function decider(decision: "APPROVE" | "REJECT") {
    if (!dossier || occupe) return;
    occupe = true;
    erreur = null;
    try {
      issue = await reviserKyc(
        dossier.provider_id,
        decision,
        // Sur une confirmation, le motif est déjà écrit : le renvoyer ferait
        // porter au dossier une raison que le premier examinateur n'a pas
        // formulée.
        decision === "REJECT" && !aConfirmer ? motif.trim() : undefined,
      );
      choisi = null;
      motif = "";
      await rafraichir();
    } catch (e) {
      erreur = messageRevue(e);
      await rafraichir();
    } finally {
      occupe = false;
    }
  }

  function messageRevue(e: unknown): string {
    if (sessionFinie(e)) return "Votre session a expiré ; la décision n'a pas été enregistrée.";
    const corps = e instanceof Error && "body" in e ? String(e.body) : "";
    if (corps.includes("PROVIDER_CANCELLED")) {
      return "Cette entreprise a retiré sa demande. Il n'y a plus rien à décider.";
    }
    if (corps.includes("REVIEW_ALREADY_DONE")) {
      return "Cette entreprise a déjà été traitée.";
    }
    if (corps.includes("FOUR_EYES_REQUIRED")) {
      return "Un refus se confirme par un autre compte que le vôtre.";
    }
    if (corps.includes("MOTIVE_REQUIRED")) {
      return `Dites ce qui est reproché, en ${MOTIF_KYC_MIN} caractères au moins.`;
    }
    return "La décision n'a pas pu être enregistrée.";
  }
</script>

{#if erreur}
  <p role="alert" data-erreur="kyc">{erreur}</p>
{/if}

{#if issue}
  <p role="status" data-issue-kyc={issue.code}>
    {#if issue.attend_confirmation}
      Refus enregistré. Il ne prendra effet qu'après confirmation par un autre
      compte : l'entreprise reste en attente d'ici là.
    {:else}
      Décision enregistrée : l'entreprise est <strong>{issue.statut}</strong>.
    {/if}
    {#if !issue.notifie}
      <!--
        FR-038 demande un courriel à l'entreprise. Il ne part pas : le dire
        évite que quelqu'un compte sur un avis qui n'a pas été envoyé.
      -->
      Aucun courriel ne lui a été envoyé.
    {/if}
  </p>
{/if}

{#if !charge}
  <p role="status">Lecture des dossiers…</p>
{:else if dossiers.length === 0}
  <p role="status" data-kyc-vide>Aucune entreprise en attente de contrôle.</p>
{:else}
  <ul data-kyc-file>
    {#each dossiers as d (d.provider_id)}
      <li data-kyc={d.provider_id} data-attente={d.attente_longue ? "longue" : "normale"}>
        <p>
          <strong>{d.raison_sociale}</strong> · BCE {d.numero_bce} ·
          {d.secteurs.join(", ")}
        </p>
        <p class="klaar-tempere">
          En attente depuis {d.attente_jours} jour{d.attente_jours > 1 ? "s" : ""}
          {#if d.attente_longue}
            · <span data-alerte="attente">cette entreprise attend son autorisation d'exercer</span>
          {/if}
        </p>
        {#if d.refus_en_attente}
          <p data-refus-en-attente={d.refus_en_attente.propose_par_moi ? "moi" : "autre"}>
            Refus proposé{d.refus_en_attente.propose_par_moi ? " par vous" : ""} :
            « {d.refus_en_attente.motif} »
            {#if d.refus_en_attente.propose_par_moi}
              — il attend la confirmation d'un autre compte que le vôtre.
            {/if}
          </p>
        {/if}
        <button
          type="button"
          onclick={() => {
            choisi = d.provider_id;
            motif = "";
            issue = null;
          }}
          data-action="ouvrir-kyc"
        >
          Examiner ce dossier
        </button>
      </li>
    {/each}
  </ul>
{/if}

{#if dossier}
  <section data-revue-pour={dossier.provider_id}>
    <h3>{dossier.raison_sociale}</h3>

    {#if aConfirmer}
      <p role="status" data-mode="confirmation">
        Un autre compte a proposé de refuser cette entreprise pour le motif
        ci-dessus. Confirmer rend le refus définitif.
      </p>
      <button
        type="button"
        onclick={() => void decider("REJECT")}
        disabled={occupe}
        data-action="confirmer-refus"
      >
        {occupe ? "Un instant…" : "Confirmer le refus"}
      </button>
    {:else}
      <button
        type="button"
        onclick={() => void decider("APPROVE")}
        disabled={occupe || dossier.refus_en_attente !== null}
        data-action="valider-kyc"
      >
        {occupe ? "Un instant…" : "Valider cette entreprise"}
      </button>

      <label for="motif-kyc">
        Motif du refus, {MOTIF_KYC_MIN} caractères au moins
      </label>
      <textarea id="motif-kyc" bind:value={motif} rows="3" data-champ="motif-kyc"></textarea>
      <p class="klaar-tempere">
        Ce motif est ce que l'entreprise pourra lire : sans lui, elle ne peut ni
        corriger ni contester.
      </p>
      <button
        type="button"
        onclick={() => void decider("REJECT")}
        disabled={occupe || !motifSuffisant}
        data-action="refuser-kyc"
      >
        {occupe ? "Un instant…" : "Proposer le refus"}
      </button>
    {/if}

    <button type="button" onclick={() => (choisi = null)} data-action="renoncer-kyc">
      Revenir sans décider
    </button>
  </section>
{/if}
