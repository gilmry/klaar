<script lang="ts">
  /**
   * Administration du catalogue (Story 2.4, FR-010).
   *
   * **Publier est un geste qui ne se défait pas, et l'écran le dit avant le
   * clic.** Un secteur publié devient proposable à toute la Région ; le retirer
   * ensuite laisse des Missions orphelines. Un bouton qui engagerait cela sans
   * le dire serait un piège.
   *
   * **Le bouton de publication est absent, pas grisé, sur son propre
   * brouillon.** Un bouton grisé invite à chercher pourquoi ; une phrase qui
   * explique qu'un autre compte doit valider dit ce qu'il faut faire.
   */
  import { onMount } from "svelte";
  import {
    catalogueAdmin,
    creerSecteur,
    desactiverSecteur,
    libelleStatutSecteur,
    publierSecteur,
    sessionFinie,
    type SecteurAdmin,
  } from "../lib/ops";
  import { ApiError } from "../lib/api";

  let secteurs = $state<SecteurAdmin[]>([]);
  let charge = $state(false);
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let message = $state<string | null>(null);

  let deplie = $state(false);
  let code = $state("");
  let libelleFr = $state("");
  let libelleNl = $state("");
  let libelleEn = $state("");
  let ordre = $state(10);

  onMount(() => void rafraichir());

  async function rafraichir() {
    try {
      secteurs = await catalogueAdmin();
      erreur = null;
    } catch (e) {
      erreur = sessionFinie(e) ? "Votre session a expiré." : "Le catalogue est indisponible.";
    } finally {
      charge = true;
    }
  }

  /**
   * Les trois libellés sont exigés dès la création.
   *
   * Le service les exige aussi ; le vérifier ici évite un aller-retour et dit
   * pourquoi — un secteur publié sans néerlandais s'afficherait en français à
   * un néerlandophone.
   */
  const complet = $derived(
    code.trim() !== "" &&
      libelleFr.trim() !== "" &&
      libelleNl.trim() !== "" &&
      libelleEn.trim() !== "",
  );

  async function creer(evenement: Event) {
    evenement.preventDefault();
    if (occupe || !complet) return;
    occupe = true;
    erreur = null;
    message = null;
    try {
      await creerSecteur({
        code: code.trim(),
        libelle_fr: libelleFr.trim(),
        libelle_nl: libelleNl.trim(),
        libelle_en: libelleEn.trim(),
        ordre,
      });
      message = "Secteur créé en brouillon. Un autre compte doit le publier.";
      code = "";
      libelleFr = "";
      libelleNl = "";
      libelleEn = "";
      deplie = false;
      await rafraichir();
    } catch (e) {
      erreur = messageCatalogue(e);
    } finally {
      occupe = false;
    }
  }

  async function publier(secteur: SecteurAdmin) {
    if (occupe) return;
    occupe = true;
    erreur = null;
    message = null;
    try {
      await publierSecteur(secteur.code);
      message = `« ${secteur.libelle_fr} » est publié.`;
      await rafraichir();
    } catch (e) {
      erreur = messageCatalogue(e);
      await rafraichir();
    } finally {
      occupe = false;
    }
  }

  async function retirer(secteur: SecteurAdmin) {
    if (occupe) return;
    occupe = true;
    erreur = null;
    message = null;
    try {
      await desactiverSecteur(secteur.code);
      message = `« ${secteur.libelle_fr} » est retiré du public.`;
      await rafraichir();
    } catch (e) {
      erreur = messageCatalogue(e);
      await rafraichir();
    } finally {
      occupe = false;
    }
  }

  function messageCatalogue(e: unknown): string {
    if (sessionFinie(e)) return "Votre session a expiré ; rien n'a été enregistré.";
    const corps = e instanceof ApiError ? e.body : "";
    if (corps.includes("SECTOR_CODE_TAKEN")) return "Un secteur porte déjà ce code.";
    if (corps.includes("FOUR_EYES_REQUIRED")) {
      return "Un secteur se publie par un autre compte que celui qui l'a créé.";
    }
    if (corps.includes("SECTOR_HAS_ACTIVE_MISSIONS")) {
      // Le nombre vient du service ; le message dit quoi faire, c'est-à-dire
      // attendre plutôt que réessayer.
      return "Des interventions sont en cours dans ce secteur. Il se retirera quand elles seront terminées.";
    }
    if (corps.includes("LABEL_REQUIRED")) return "Les trois libellés sont exigés.";
    if (corps.includes("SECTOR_CODE_INVALID")) {
      return "Le code s'écrit en minuscules, chiffres et tirets : « chauffage-gaz ».";
    }
    if (corps.includes("SECTOR_TRANSITION_INVALID")) {
      return "L'état du secteur a changé entre-temps. La liste vient d'être relue.";
    }
    return "Le geste n'a pas pu être enregistré.";
  }
</script>

{#if erreur}
  <p role="alert" data-erreur="catalogue">{erreur}</p>
{/if}
{#if message}
  <p role="status" data-message="catalogue">{message}</p>
{/if}

{#if !charge}
  <p role="status">Lecture du catalogue…</p>
{:else}
  <ul data-catalogue-ops>
    {#each secteurs as s (s.code)}
      <li data-secteur-ops={s.code} data-statut={s.statut}>
        <p>
          <strong>{s.libelle_fr}</strong> · <code>{s.code}</code> ·
          <span data-statut-libelle>{libelleStatutSecteur(s.statut)}</span>
        </p>
        <p class="klaar-tempere">
          {s.libelle_nl} · {s.libelle_en}
          {#if s.missions_en_cours > 0}
            · {s.missions_en_cours} intervention{s.missions_en_cours > 1 ? "s" : ""} en cours
          {/if}
        </p>

        {#if s.statut === "DRAFT"}
          {#if s.cree_par_moi}
            <!--
              Une phrase plutôt qu'un bouton grisé : le bouton grisé invite à
              chercher pourquoi, la phrase dit ce qu'il faut faire.
            -->
            <p class="klaar-tempere" data-attente="autre-compte">
              Vous avez créé ce brouillon : un autre compte doit le publier.
            </p>
          {:else}
            <p class="klaar-tempere" data-avertissement="definitif">
              Publier le rend proposable à toute la Région. Le retirer ensuite
              laisserait des interventions en cours sans secteur.
            </p>
            <button
              type="button"
              onclick={() => void publier(s)}
              disabled={occupe}
              data-action="publier-secteur"
            >
              {occupe ? "Un instant…" : "Publier ce secteur"}
            </button>
          {/if}
        {:else if s.statut === "PUBLISHED"}
          <button
            type="button"
            onclick={() => void retirer(s)}
            disabled={occupe || s.missions_en_cours > 0}
            data-action="retirer-secteur"
          >
            {occupe ? "Un instant…" : "Retirer du public"}
          </button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

{#if !deplie}
  <button type="button" onclick={() => (deplie = true)} data-action="ouvrir-creation">
    Ajouter un secteur
  </button>
{:else}
  <form onsubmit={creer} data-formulaire="secteur">
    <label for="secteur-code">Code</label>
    <input id="secteur-code" type="text" bind:value={code} data-champ="code" required />
    <p class="klaar-tempere">
      Minuscules, chiffres et tirets. Il voyage dans les URL et les exports : il
      ne se renomme pas.
    </p>

    <label for="secteur-fr">Libellé français</label>
    <input id="secteur-fr" type="text" bind:value={libelleFr} data-champ="fr" required />

    <label for="secteur-nl">Libellé néerlandais</label>
    <input id="secteur-nl" type="text" bind:value={libelleNl} data-champ="nl" required />

    <label for="secteur-en">Libellé anglais</label>
    <input id="secteur-en" type="text" bind:value={libelleEn} data-champ="en" required />
    <p class="klaar-tempere">
      Les trois sont exigés : un secteur publié sans néerlandais s'afficherait en
      français à un néerlandophone, et le corriger après coup ne rattraperait pas
      ceux qui l'ont lu.
    </p>

    <label for="secteur-ordre">Ordre d'affichage</label>
    <input id="secteur-ordre" type="number" bind:value={ordre} data-champ="ordre" />

    <button type="submit" disabled={occupe || !complet} data-action="creer-secteur">
      {occupe ? "Un instant…" : "Créer en brouillon"}
    </button>
    <button type="button" onclick={() => (deplie = false)} data-action="renoncer-secteur">
      Renoncer
    </button>
  </form>
{/if}
