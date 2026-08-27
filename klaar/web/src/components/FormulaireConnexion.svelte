<script lang="ts">
  /**
   * Formulaire de connexion (Story 1.3, FR-004).
   *
   * Un seul message pour « adresse inconnue » et « mot de passe faux », comme
   * le backend : distinguer les deux ferait de cet écran un moyen de tester la
   * présence de n'importe quelle adresse.
   *
   * « Compte non vérifié » est distingué, lui, parce que l'atteindre suppose
   * déjà de connaître le bon mot de passe, et que la personne a besoin de
   * savoir qu'il lui reste un courriel à ouvrir.
   */
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { codeDepuisErreur, messageErreur, seConnecter } from "../lib/connexion";

  let email = $state("");
  let motDePasse = $state("");
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let connecte = $state(false);
  let locale = $state<LocaleKlaar>("fr");

  $effect(() => {
    locale = localeAffichee();
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
</script>

{#if connecte}
  <p role="status" data-succes-connexion>
    Vous êtes connecté. La session dure une heure et se renouvellera d'elle-même
    dès que le rafraîchissement sera livré.
  </p>
{:else}
  <form onsubmit={soumettre} data-formulaire="connexion" novalidate>
    <label for="connexion-email">Adresse email</label>
    <input
      id="connexion-email"
      name="email"
      type="email"
      autocomplete="email"
      inputmode="email"
      bind:value={email}
      required
    />

    <label for="connexion-mot-de-passe">Mot de passe</label>
    <input
      id="connexion-mot-de-passe"
      name="mot_de_passe"
      type="password"
      autocomplete="current-password"
      bind:value={motDePasse}
      required
    />

    <button type="submit" disabled={occupe} data-action="connecter">
      {occupe ? "Un instant…" : "Me connecter"}
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
