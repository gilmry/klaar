<script lang="ts">
  /**
   * Connexion et état de session (Stories 1.3 et 1.4, FR-004).
   *
   * Au montage, la session est reprise depuis le cookie de rafraîchissement :
   * recharger la page ne déconnecte donc plus. L'échec de cette reprise n'est
   * pas affiché — arriver sur la page sans être connecté est l'état normal
   * d'un visiteur, pas une erreur.
   *
   * Un seul message pour « adresse inconnue » et « mot de passe faux », comme
   * le backend : distinguer les deux ferait de cet écran un moyen de tester la
   * présence de n'importe quelle adresse.
   */
  import { onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerLangue, t } from "../lib/i18n";
  import {
    codeDepuisErreur,
    messageErreur,
    restaurerSession,
    seConnecter,
    seDeconnecter,
  } from "../lib/connexion";

  let email = $state("");
  let motDePasse = $state("");
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let connecte = $state(false);
  let reprise = $state(true);
  let locale = $state<LocaleKlaar>("fr");

  onMount(async () => {
    locale = restaurerLangue();
    connecte = await restaurerSession();
    reprise = false;
  });

  async function soumettre(evenement: SubmitEvent) {
    evenement.preventDefault();
    if (occupe) return;
    occupe = true;
    erreur = null;
    try {
      await seConnecter({ email, mot_de_passe: motDePasse });
      connecte = true;
      // Le secret ne reste pas dans le champ : la page peut rester ouverte
      // longtemps sur un appareil partagé.
      motDePasse = "";
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
    }
  }

  async function deconnecter() {
    occupe = true;
    try {
      await seDeconnecter();
    } finally {
      // Même si l'appel a échoué : le jeton local est oublié dans tous les cas,
      // laisser une session vivante après un clic sur « me déconnecter » serait
      // le pire des deux mondes.
      connecte = false;
      occupe = false;
    }
  }
</script>

{#if reprise}
  <p data-etat-session="reprise">{t(locale, "connexion.reprise")}</p>
{:else if connecte}
  <p role="status" data-succes-connexion>
    {t(locale, "connexion.connecte")}
  </p>
  <button type="button" onclick={deconnecter} disabled={occupe} data-action="deconnecter">
    {occupe ? t(locale, "commun.attendez") : t(locale, "connexion.deconnecter")}
  </button>
{:else}
  <form onsubmit={soumettre} data-formulaire="connexion" novalidate>
    <label for="connexion-email">{t(locale, "champ.email")}</label>
    <input
      id="connexion-email"
      name="email"
      type="email"
      autocomplete="email"
      inputmode="email"
      bind:value={email}
      required
    />

    <label for="connexion-mot-de-passe">{t(locale, "champ.mot_de_passe")}</label>
    <input
      id="connexion-mot-de-passe"
      name="mot_de_passe"
      type="password"
      autocomplete="current-password"
      bind:value={motDePasse}
      required
    />

    <button type="submit" disabled={occupe} data-action="connecter">
      {occupe ? t(locale, "commun.attendez") : t(locale, "commun.me_connecter")}
    </button>
  </form>
{/if}

{#if erreur}
  <p role="alert" data-erreur-connexion>{erreur}</p>
{/if}

<style>
  form {
    display: grid;
    gap: 0.35rem;
    max-width: 28rem;
  }
  label { font-weight: 600; }
  input {
    font: inherit;
    padding: 0.5rem;
    border: 1px solid var(--klaar-bord);
    border-radius: 8px;
  }
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
  button:disabled { opacity: 0.6; cursor: progress; }
  p[role="alert"] { color: #c2543a; }
  p[role="status"] { color: #1b3a4b; }
</style>
