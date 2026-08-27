<script lang="ts">
  /**
   * Catalogue des secteurs (Story 2.2, FR-008).
   *
   * Chargé dans la langue de la page, la même que celle des messages et des
   * courriels. Un secteur sans Skill s'affiche quand même : c'est un secteur
   * ouvert dont les compétences ne sont pas encore décrites, pas une anomalie.
   */
  import { onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import {
    chargerCatalogue,
    codeDepuisErreur,
    messageErreur,
    type SecteurCatalogue,
  } from "../lib/catalogue";

  let secteurs = $state<SecteurCatalogue[]>([]);
  let chargement = $state(true);
  let erreur = $state<string | null>(null);
  let locale = $state<LocaleKlaar>("fr");

  onMount(async () => {
    locale = localeAffichee();
    try {
      const catalogue = await chargerCatalogue(locale);
      secteurs = catalogue.secteurs;
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      chargement = false;
    }
  });
</script>

{#if chargement}
  <p data-etat-catalogue="chargement">Chargement du catalogue…</p>
{:else if erreur}
  <p role="alert" data-erreur-catalogue>{erreur}</p>
{:else if secteurs.length === 0}
  <p data-etat-catalogue="vide">
    Le catalogue est vide pour le moment.
  </p>
{:else}
  <ul data-liste="secteurs">
    {#each secteurs as secteur (secteur.code)}
      <li data-secteur={secteur.code}>
        <strong>{secteur.libelle}</strong>
        {#if secteur.skills.length > 0}
          <ul>
            {#each secteur.skills as skill (skill.code)}
              <li data-skill={skill.code}>{skill.libelle}</li>
            {/each}
          </ul>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  ul { list-style: none; padding-left: 0; }
  ul ul { padding-left: 1.2rem; }
  li[data-secteur] { margin-bottom: 0.8rem; }
  li[data-skill] { color: #3d5a68; }
  p[role="alert"] { color: #c2543a; }
</style>
