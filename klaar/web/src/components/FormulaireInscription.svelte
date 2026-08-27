<script lang="ts">
  /**
   * Formulaire d'inscription (Story 1.1, FR-001).
   *
   * Deux partis pris d'interface découlent de l'anti-énumération côté serveur :
   *
   * 1. le message de succès ne dit pas si un compte a été créé, sinon
   *    l'interface révélerait ce que l'API refuse de dire ;
   * 2. aucune vérification « cette adresse est déjà prise » pendant la saisie,
   *    qui serait exactement l'oracle qu'on évite.
   *
   * La validation locale ne sert qu'à éviter un aller-retour inutile. Elle ne
   * remplace jamais celle du serveur, qui reste seule faisant foi.
   */
  import {
    codeDepuisErreur,
    inscrire,
    localeAffichee,
    LONGUEUR_MIN_MOT_DE_PASSE,
    messageErreur,
    messageSucces,
    type LocaleKlaar,
  } from "../lib/inscription";

  let email = $state("");
  let motDePasse = $state("");
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let succes = $state<string | null>(null);
  let locale = $state<LocaleKlaar>("fr");

  $effect(() => {
    locale = localeAffichee();
  });

  const motDePasseTropCourt = $derived(
    motDePasse.length > 0 && [...motDePasse].length < LONGUEUR_MIN_MOT_DE_PASSE,
  );

  async function soumettre(evenement: SubmitEvent) {
    evenement.preventDefault();
    if (occupe) return;
    occupe = true;
    erreur = null;
    succes = null;
    try {
      await inscrire({ email, mot_de_passe: motDePasse, locale });
      succes = messageSucces(locale);
      // Le mot de passe ne reste pas dans le champ après l'envoi : la page
      // peut rester ouverte longtemps sur un appareil partagé.
      motDePasse = "";
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    } finally {
      occupe = false;
    }
  }
</script>

<form onsubmit={soumettre} data-formulaire="inscription" novalidate>
  <label for="inscription-email">Adresse email</label>
  <input
    id="inscription-email"
    name="email"
    type="email"
    autocomplete="email"
    inputmode="email"
    bind:value={email}
    required
  />

  <label for="inscription-mot-de-passe">Mot de passe</label>
  <input
    id="inscription-mot-de-passe"
    name="mot_de_passe"
    type="password"
    autocomplete="new-password"
    bind:value={motDePasse}
    aria-describedby="inscription-aide-mot-de-passe"
    required
  />
  <p id="inscription-aide-mot-de-passe" class="klaar-tempere">
    Au moins {LONGUEUR_MIN_MOT_DE_PASSE} caractères. Aucune règle de composition :
    une phrase que vous retenez vaut mieux qu'un sigle que vous oublierez.
  </p>

  {#if motDePasseTropCourt}
    <p class="klaar-tempere" data-avertissement="mot-de-passe-court">
      Encore {LONGUEUR_MIN_MOT_DE_PASSE - [...motDePasse].length} caractère(s).
    </p>
  {/if}

  <button type="submit" disabled={occupe} data-action="inscrire">
    {occupe ? "Un instant…" : "Créer mon compte"}
  </button>
</form>

{#if erreur}
  <p role="alert" data-erreur-inscription>{erreur}</p>
{/if}

{#if succes}
  <p role="status" data-succes-inscription>{succes}</p>
{/if}

<style>
  form {
    display: grid;
    gap: 0.35rem;
    max-width: 28rem;
  }
  label {
    font-weight: 600;
  }
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
  button:disabled {
    opacity: 0.6;
    cursor: progress;
  }
  p[role="alert"] {
    color: #c2543a;
  }
  p[role="status"] {
    color: #1b3a4b;
  }
</style>
