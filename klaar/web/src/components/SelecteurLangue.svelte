<script lang="ts">
  /**
   * Choix de la langue d'affichage (Story 9.1, FR-043).
   *
   * **Le choix est enregistré et rétabli au chargement.** Bruxelles est
   * bilingue : quelqu'un qui a demandé le néerlandais une fois ne doit pas
   * avoir à le redemander à chaque page, sans quoi le sélecteur est un gadget.
   *
   * **Il agit sur `<html lang>`.** C'est lui que lisent les messages d'erreur
   * de l'API (via `localeAffichee`), les lecteurs d'écran et la césure du
   * navigateur. Ne changer qu'un état interne laisserait une page annoncée en
   * français lue à voix haute comme du français alors qu'elle est en
   * néerlandais.
   *
   * **Limite écrite : la coquille Astro reste en français.** Les pages sont
   * générées statiquement à la construction ; les traduire demande soit trois
   * jeux de pages, soit un rendu au serveur. Ce que ce sélecteur traduit est le
   * contenu applicatif, c'est-à-dire ce que les gens lisent quand ils font
   * quelque chose.
   */
  import { onMount } from "svelte";
  import { choisirLangue, LANGUES, restaurerLangue, t } from "../lib/i18n";
  import type { LocaleKlaar } from "../lib/inscription";

  let locale = $state<LocaleKlaar>("fr");

  onMount(() => {
    locale = restaurerLangue();
  });

  function changer(evenement: Event) {
    const valeur = (evenement.currentTarget as HTMLSelectElement).value as LocaleKlaar;
    locale = valeur;
    choisirLangue(valeur);
    // **La page est rechargée.** Les composants montés lisent leur langue à
    // l'initialisation ; leur demander de réagir à un changement supposerait un
    // magasin partagé que chacun devrait penser à écouter, et le premier oubli
    // laisserait un écran à moitié traduit.
    location.reload();
  }
</script>

<label class="klaar-langue">
  <span class="klaar-tempere">{t(locale, "app.langue")}</span>
  <select value={locale} onchange={changer} data-champ="langue">
    {#each LANGUES as l}
      <option value={l.code}>{l.nom}</option>
    {/each}
  </select>
</label>
