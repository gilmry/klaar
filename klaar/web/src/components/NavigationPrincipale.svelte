<script lang="ts">
  /**
   * Navigation persistante de la coquille.
   *
   * **Pourquoi elle existe.** Jusqu'ici, les pages n'étaient reliées que par
   * quelques liens posés dans l'accueil : depuis `/catalogue`, il n'y avait
   * aucun chemin vers `/demande` sans repasser par le bouton « précédent » du
   * navigateur, et `/ops` n'était atteignable par aucun lien du site. Une page
   * qu'aucun lien n'atteint n'existe pas pour qui ne connaît pas son URL.
   *
   * **Elle ne rend pas de texte en dur.** Les libellés passent par `t()` : la
   * coquille Astro reste générée en français, mais ce qui est monté côté client
   * suit la langue choisie, comme le reste de l'application.
   *
   * **Ce qu'elle affiche avant de savoir.** Les liens publics, tout de suite, et
   * le bloc « compte » seulement une fois la reprise de session tranchée. Un
   * menu qui afficherait « Créer un compte » puis basculerait sur « Mon
   * compte » un instant plus tard ferait cliquer à côté ; l'attente est courte
   * et silencieuse.
   */
  import { onMount } from "svelte";
  import { restaurerLangue, t } from "../lib/i18n";
  import type { LocaleKlaar } from "../lib/inscription";
  import { observerSession, restaurerSession } from "../lib/connexion";
  import { liensPrincipaux, type LienNavigation } from "../lib/navigation";

  interface Props {
    /** Chemin de la page courante, fourni par la coquille Astro. */
    chemin?: string;
  }
  const { chemin = "/" }: Props = $props();

  let locale = $state<LocaleKlaar>("fr");
  let connecte = $state(false);
  /** Vrai tant que la reprise de session n'a pas tranché. */
  let reprise = $state(true);

  const liens = $derived<LienNavigation[]>(liensPrincipaux(connecte));

  onMount(() => {
    locale = restaurerLangue();

    // Le désabonnement est posé avant l'appel réseau : si le composant est
    // démonté pendant la reprise, l'observateur ne survit pas à la page.
    const desabonner = observerSession((etat) => {
      connecte = etat;
    });

    void restaurerSession()
      .then((ouverte) => {
        connecte = ouverte;
      })
      .finally(() => {
        reprise = false;
      });

    return desabonner;
  });

  /**
   * Marque la page courante.
   *
   * `aria-current="page"` est ce qu'un lecteur d'écran annonce ; la classe qui
   * l'accompagne n'est là que pour l'œil. Comparer les chemins en retirant la
   * barre finale évite que `/demande/` et `/demande` se comportent
   * différemment selon la façon dont le serveur statique a servi la page.
   */
  function courant(href: string): boolean {
    const normaliser = (c: string) => (c.length > 1 ? c.replace(/\/$/, "") : c);
    return normaliser(chemin) === normaliser(href);
  }
</script>

<nav
  class="klaar-nav"
  aria-label={t(locale, "nav.menu")}
  data-navigation
  data-etat-navigation={reprise ? "reprise" : connecte ? "connecte" : "visiteur"}
>
  <ul>
    {#each liens as lien (lien.href + lien.cle)}
      <li>
        <a
          href={lien.href}
          data-lien={lien.href}
          aria-current={courant(lien.href) ? "page" : undefined}
          class:actuel={courant(lien.href)}
        >
          {t(locale, lien.cle)}
        </a>
      </li>
    {/each}
  </ul>
</nav>

<style>
  /* Une liste qui passe à la ligne plutôt qu'un menu déroulant : sur un
     téléphone, un menu replié demande un geste de plus pour atteindre ce que
     les gens viennent faire, et il ne fonctionne pas sans JavaScript. */
  .klaar-nav ul {
    display: flex;
    flex-wrap: wrap;
    gap: 0.25rem 1rem;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .klaar-nav a {
    display: inline-block;
    padding: 0.35rem 0;
    /* Cible tactile : 44 px est le minimum recommandé par les WCAG 2.2
       (2.5.8 Target Size), et une barre de navigation est précisément ce qu'on
       touche en marchant. */
    min-height: 44px;
    line-height: 1.9;
  }
  .klaar-nav a.actuel {
    font-weight: 600;
    text-decoration: none;
    /* Le soulignement inférieur marque la page courante sans reposer sur la
       seule graisse, qui ne se voit pas à côté d'un lien long. */
    box-shadow: inset 0 -2px 0 currentColor;
  }
</style>
