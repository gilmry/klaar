<script lang="ts">
  /**
   * Soumission d'une Demande (Story 3.1, FR-011).
   *
   * La position est demandée **au moment de l'envoi** et non au chargement de
   * la page : une invite de géolocalisation à l'arrivée, avant que le visiteur
   * n'ait rien demandé, est refusée par réflexe — et un refus de
   * géolocalisation est définitif dans plusieurs navigateurs.
   */
  import { onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerSession } from "../lib/connexion";
  import { chargerCatalogue, type SecteurCatalogue } from "../lib/catalogue";
  import {
    codeDepuisErreur,
    DESCRIPTION_MAX,
    messageErreur,
    positionActuelle,
    soumettreDemande,
    type UrgenceKlaar,
  } from "../lib/demande";

  let connecte = $state(false);
  let reprise = $state(true);
  let secteurs = $state<SecteurCatalogue[]>([]);
  let secteur = $state("");
  let description = $state("");
  let urgence = $state<UrgenceKlaar>("NORMAL");
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let creee = $state<{ id: string; doublon: boolean } | null>(null);
  let locale = $state<LocaleKlaar>("fr");

  onMount(async () => {
    locale = localeAffichee();
    connecte = await restaurerSession();
    try {
      secteurs = (await chargerCatalogue(locale)).secteurs;
    } catch {
      // Le catalogue absent n'est pas une erreur à afficher ici : le champ
      // reste vide et le serveur refusera un secteur inconnu de toute façon.
    }
    reprise = false;
  });

  const restant = $derived(DESCRIPTION_MAX - [...description].length);
  const complet = $derived(secteur !== "" && description.trim() !== "");

  async function envoyer(evenement: SubmitEvent) {
    evenement.preventDefault();
    if (occupe || !complet) return;
    occupe = true;
    erreur = null;
    try {
      const position = await positionActuelle();
      const reponse = await soumettreDemande({
        secteur,
        description,
        latitude: position.coords.latitude,
        longitude: position.coords.longitude,
        urgence,
      });
      creee = { id: reponse.id, doublon: reponse.code === "REQUEST_DUPLICATE" };
    } catch (e) {
      erreur =
        e instanceof Error && e.message === "POSITION_REFUSEE"
          ? messageErreur(locale, "POSITION_REFUSEE")
          : messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
    }
  }
</script>

{#if reprise}
  <p data-etat-demande="reprise">Un instant…</p>
{:else if !connecte}
  <p role="status" data-etat-demande="anonyme">
    Faire une demande suppose un compte. <a href="/connexion">Me connecter</a>
  </p>
{:else if creee}
  <p role="status" data-demande="creee">
    {#if creee.doublon}
      Vous aviez déjà une demande en cours pour ce secteur, ici même. C'est
      elle qui est en train d'être diffusée.
    {:else}
      Votre demande est diffusée aux prestataires disponibles.
    {/if}
  </p>
  <p class="klaar-tempere">
    La mise en relation n'est pas encore livrée : aucune notification ne partira
    pour l'instant.
  </p>
{:else}
  <form onsubmit={envoyer} data-formulaire="demande" novalidate>
    <label for="demande-secteur">Secteur</label>
    <select id="demande-secteur" bind:value={secteur} data-champ="secteur" required>
      <option value="" disabled>Choisissez…</option>
      {#each secteurs as s (s.code)}
        <option value={s.code}>{s.libelle}</option>
      {/each}
    </select>

    <label for="demande-description">Que se passe-t-il ?</label>
    <textarea
      id="demande-description"
      bind:value={description}
      rows="4"
      maxlength={DESCRIPTION_MAX}
      data-champ="description"
      required
    ></textarea>
    <p class="klaar-tempere" data-restant>{restant} caractères restants</p>

    <fieldset>
      <legend>Urgence</legend>
      {#each [["LOW", "Peut attendre"], ["NORMAL", "Dans la journée"], ["HIGH", "Tout de suite"]] as [valeur, libelle] (valeur)}
        <label>
          <input type="radio" bind:group={urgence} value={valeur} name="urgence" />
          {libelle}
        </label>
      {/each}
    </fieldset>

    <button type="submit" disabled={occupe || !complet} data-action="envoyer-demande">
      {occupe ? "Un instant…" : "Envoyer ma demande"}
    </button>
    <p class="klaar-tempere">
      Votre position sera demandée à l'envoi : sans elle, aucun prestataire ne
      peut être averti.
    </p>
  </form>
{/if}

{#if erreur}
  <p role="alert" data-erreur-demande>{erreur}</p>
{/if}

<style>
  form { display: grid; gap: 0.35rem; max-width: 32rem; }
  label { font-weight: 600; }
  select, textarea {
    font: inherit;
    padding: 0.5rem;
    border: 1px solid var(--klaar-bord);
    border-radius: 8px;
  }
  fieldset { border: 1px solid var(--klaar-bord); border-radius: 8px; margin-top: 0.6rem; }
  fieldset label { font-weight: 400; display: inline-block; margin-right: 1rem; }
  button {
    font: inherit;
    margin-top: 0.6rem;
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
