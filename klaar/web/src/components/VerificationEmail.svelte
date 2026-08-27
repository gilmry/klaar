<script lang="ts">
  /**
   * Confirmation d'adresse (Story 1.2).
   *
   * La vérification part au montage, sans clic : l'utilisateur a déjà cliqué,
   * dans son courriel, et lui demander de confirmer sa confirmation n'ajoute
   * rien. Ce n'est pas un `GET` à effet de bord pour autant — le `POST` est
   * émis par du JavaScript, qu'une passerelle de messagerie n'exécute pas.
   *
   * Le jeton est retiré de la barre d'adresse dès qu'il est lu : il resterait
   * sinon dans l'historique du navigateur, dans les captures d'écran et dans
   * le `Referer` de tout lien suivi depuis cette page.
   */
  import { onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { codeDepuisErreur, jetonDepuisUrl, messageErreur, messageSucces, verifier } from "../lib/verification";

  type Etat = "en-cours" | "confirme" | "echec";

  let etat = $state<Etat>("en-cours");
  let message = $state("");
  let locale = $state<LocaleKlaar>("fr");

  onMount(async () => {
    locale = localeAffichee();
    const jeton = jetonDepuisUrl(window.location.href);

    if (jeton) {
      // Retiré avant même l'appel : si la requête traîne et que la personne
      // partage l'URL entre-temps, elle ne partage pas son jeton.
      window.history.replaceState({}, "", window.location.pathname);
    }

    try {
      const reponse = await verifier(jeton);
      message = messageSucces(locale, reponse.code);
      etat = "confirme";
    } catch (e) {
      message = messageErreur(locale, codeDepuisErreur(e));
      etat = "echec";
    }
  });
</script>

{#if etat === "en-cours"}
  <p data-etat-verification="en-cours">Confirmation en cours…</p>
{:else if etat === "confirme"}
  <p role="status" data-etat-verification="confirme">{message}</p>
  <p><a href="/">Retour à l'accueil</a></p>
{:else}
  <p role="alert" data-etat-verification="echec">{message}</p>
  <p><a href="/inscription">Recommencer l'inscription</a></p>
{/if}

<style>
  p[role="alert"] {
    color: #c2543a;
  }
  p[role="status"] {
    color: #1b3a4b;
  }
</style>
