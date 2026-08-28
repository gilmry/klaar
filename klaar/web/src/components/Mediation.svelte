<script lang="ts">
  /**
   * File de médiation et décision (Story 7.4, FR-036).
   *
   * **Une décision est définitive, et l'écran le dit avant de la prendre.**
   * Rouvrir un litige tranché permettrait de revenir sur un remboursement déjà
   * annoncé ; le recours après décision est judiciaire. Un bouton qui engage
   * cela sans le dire serait un piège.
   *
   * **Rien n'est exécuté sur l'argent.** Le séquestre est chez Stripe, qui
   * n'est pas provisionné : l'écran écrit « décision enregistrée, montant à
   * verser » et non « remboursé ». Annoncer un virement qui n'aura pas lieu
   * transforme un litige tranché en second litige.
   */
  import { onMount } from "svelte";
  import {
    DECISIONS,
    fileMediation,
    libelleMotif,
    montantOps,
    sessionFinie,
    trancherLitige,
    type DecisionOps,
    type DossierLitige,
    type IssueLitige,
  } from "../lib/ops";

  let dossiers = $state<DossierLitige[]>([]);
  let choisi = $state<string | null>(null);
  let decision = $state<DecisionOps>("USER_FAVOR");
  let partPourcent = $state(30);
  let issue = $state<IssueLitige | null>(null);
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let charge = $state(false);

  onMount(() => void rafraichir());

  async function rafraichir() {
    try {
      dossiers = await fileMediation();
      erreur = null;
    } catch (e) {
      erreur = sessionFinie(e)
        ? "Votre session a expiré."
        : "La file de médiation est indisponible.";
    } finally {
      charge = true;
    }
  }

  const dossier = $derived(dossiers.find((d) => d.id === choisi) ?? null);

  /**
   * Le montant que la décision rendrait, calculé pour l'aperçu.
   *
   * **C'est un aperçu, pas la vérité.** Le montant qui fera foi est celui que
   * le service renvoie : refaire le calcul ici pour l'afficher est utile, s'en
   * servir pour décider ferait diverger deux arithmétiques.
   */
  const apercu = $derived.by(() => {
    if (!dossier) return null;
    const total = dossier.total_ttc_cents;
    switch (decision) {
      case "USER_FAVOR":
        return total;
      case "PARTIAL_REFUND":
        return Math.ceil((total * partPourcent) / 100);
      default:
        return 0;
    }
  });

  async function trancher() {
    if (!dossier || occupe) return;
    occupe = true;
    erreur = null;
    try {
      issue = await trancherLitige(
        dossier.id,
        decision,
        decision === "PARTIAL_REFUND" ? partPourcent * 100 : undefined,
      );
      choisi = null;
      await rafraichir();
    } catch (e) {
      erreur = messageDecision(e);
      await rafraichir();
    } finally {
      occupe = false;
    }
  }

  function messageDecision(e: unknown): string {
    if (sessionFinie(e)) return "Votre session a expiré ; la décision n'a pas été enregistrée.";
    const corps = e instanceof Error && "body" in e ? String(e.body) : "";
    if (corps.includes("DISPUTE_ALREADY_RESOLVED")) {
      // Le cas de deux médiateurs sur le même dossier. Le dire clairement évite
      // de croire à un bogue.
      return "Ce litige vient d'être tranché par quelqu'un d'autre. La première décision fait foi.";
    }
    if (corps.includes("REFUND_SHARE_OUT_OF_RANGE")) {
      return "Une part se situe entre 1 et 99 pour cent : 0 % est une décision pour le prestataire, 100 % une décision pour le demandeur.";
    }
    return "La décision n'a pas pu être enregistrée.";
  }
</script>

{#if erreur}
  <p role="alert" data-erreur="mediation">{erreur}</p>
{/if}

{#if issue}
  <p role="status" data-issue={issue.statut}>
    Décision enregistrée : {montantOps(issue.remboursement_cents)} au demandeur,
    {montantOps(issue.reversement_cents)} au prestataire.
    {#if !issue.execute}
      <!--
        Le mot compte : « à verser » et non « versé ». Le séquestre n'est pas
        provisionné, et quelqu'un attendrait un virement qui ne vient pas.
      -->
      <strong>Montants à verser</strong> : aucun mouvement n'a encore eu lieu.
    {/if}
  </p>
{/if}

{#if !charge}
  <p role="status">Lecture des dossiers…</p>
{:else if dossiers.length === 0}
  <p role="status" data-mediation-vide>Aucun litige en attente.</p>
{:else}
  <ul data-mediation-file>
    {#each dossiers as d (d.id)}
      <li data-dossier={d.id} data-escalade={d.a_escalader ? "oui" : "non"}>
        <p>
          <strong>{libelleMotif(d.motif)}</strong> ·
          ouvert par {d.partie === "USER" ? "le demandeur" : "le prestataire"} ·
          il y a {d.age_jours} jour{d.age_jours > 1 ? "s" : ""}
          {#if d.a_escalader}
            · <span data-alerte="escalade">à escalader</span>
          {/if}
        </p>
        <p>{d.description}</p>
        <p class="klaar-tempere">
          Montant en jeu : {montantOps(d.total_ttc_cents)}
          {#if d.total_ttc_cents === 0}
            (aucun devis accepté : il n'y a rien à répartir, mais le dossier doit
            être clos)
          {/if}
        </p>
        <button
          type="button"
          onclick={() => {
            choisi = d.id;
            issue = null;
          }}
          data-action="ouvrir-dossier"
        >
          Trancher ce dossier
        </button>
      </li>
    {/each}
  </ul>
{/if}

{#if dossier}
  <section data-decision-pour={dossier.id}>
    <h3>Décision</h3>
    <p role="status" data-avertissement="definitif">
      Une décision est définitive : elle ne se reprend pas, et le recours au-delà
      est judiciaire.
    </p>

    {#each DECISIONS as d}
      <label>
        <input type="radio" bind:group={decision} value={d.code} />
        {d.libelle}
      </label>
    {/each}

    {#if decision === "PARTIAL_REFUND"}
      <label for="part">Part remboursée, en pour cent</label>
      <input
        id="part"
        type="number"
        min="1"
        max="99"
        bind:value={partPourcent}
        data-champ="part"
      />
    {/if}

    {#if apercu !== null}
      <p class="klaar-tempere" data-apercu>
        Cette décision rendrait {montantOps(apercu)} au demandeur et
        {montantOps(dossier.total_ttc_cents - apercu)} au prestataire.
      </p>
    {/if}

    <button type="button" onclick={() => void trancher()} disabled={occupe} data-action="trancher">
      {occupe ? "Un instant…" : "Enregistrer cette décision"}
    </button>
    <button type="button" onclick={() => (choisi = null)} data-action="renoncer">
      Revenir sans trancher
    </button>
  </section>
{/if}
