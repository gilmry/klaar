<script lang="ts">
  /**
   * Disponibilité du prestataire (Story 3.7).
   *
   * Un interrupteur et un curseur, et rien de plus. Le reste de l'écran sert à
   * répondre à la seule question qu'un prestataire se pose : « est-ce que je
   * reçois des Demandes en ce moment, et sinon pourquoi ». Trois causes
   * possibles, qui n'ont rien à voir entre elles — sa pause, une intervention
   * en cours, son statut — et les taire ferait conclure que le service est
   * cassé.
   *
   * Le rayon se règle en kilomètres parce que personne ne pense en mètres pour
   * un déplacement, et se transmet en mètres parce que c'est l'unité de l'API.
   */
  import { onMount } from "svelte";
  import { localeAffichee, type LocaleKlaar } from "../lib/inscription";
  import { restaurerLangue, t } from "../lib/i18n";
  import { restaurerSession } from "../lib/connexion";
  import {
    codeDepuisErreur,
    lireDisponibilite,
    messageErreur,
    raisonDeSilence,
    reglerDisponibilite,
    RAYON_MAX_METRES,
    RAYON_MIN_METRES,
    type Disponibilite,
  } from "../lib/disponibilite";

  let connecte = $state(false);
  let reprise = $state(true);
  let etat = $state<Disponibilite | null>(null);
  let rayonKm = $state(RAYON_MAX_METRES / 1000);
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let locale = $state<LocaleKlaar>("fr");

  onMount(async () => {
    locale = restaurerLangue();
    connecte = await restaurerSession();
    if (connecte) await charger();
    reprise = false;
  });

  const silence = $derived(etat ? raisonDeSilence(etat, locale) : null);

  async function charger() {
    try {
      etat = await lireDisponibilite();
      rayonKm = etat.rayon_intervention_metres / 1000;
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
    }
  }

  async function appliquer(reglage: {
    disponible?: boolean;
    rayon_intervention_metres?: number;
  }) {
    if (occupe) return;
    occupe = true;
    erreur = null;
    try {
      etat = await reglerDisponibilite(reglage);
      rayonKm = etat.rayon_intervention_metres / 1000;
    } catch (e) {
      erreur = messageErreur(locale, codeDepuisErreur(e));
      // Le curseur est remis sur la valeur réellement enregistrée : le laisser
      // sur un réglage refusé ferait croire qu'il a pris.
      if (etat) rayonKm = etat.rayon_intervention_metres / 1000;
    } finally {
      occupe = false;
    }
  }

  const basculer = () => appliquer({ disponible: !etat?.disponible });
  const enregistrerRayon = () =>
    appliquer({ rayon_intervention_metres: Math.round(rayonKm * 1000) });
</script>

{#if reprise}
  <p data-etat-dispo="reprise">{t(locale, "connexion.reprise")}</p>
{:else if !connecte}
  <p role="status" data-etat-dispo="anonyme">
    {t(locale, "commun.connexion_requise")}
    <a href="/connexion">{t(locale, "commun.me_connecter")}</a>
  </p>
{:else if etat === null}
  <p role="alert" data-etat-dispo="indisponible">
    {erreur ?? t(locale, "dispo.illisible")}
  </p>
{:else}
  <p role="status" data-sollicitable={etat.sollicitable}>
    {#if etat.sollicitable}
      {t(locale, "dispo.sollicitable")}
    {:else}
      {silence}
    {/if}
  </p>

  <button
    type="button"
    onclick={basculer}
    disabled={occupe}
    data-action="basculer-disponibilite"
  >
    {#if occupe}
      {t(locale, "commun.attendez")}
    {:else if etat.disponible}
      {t(locale, "dispo.pause")}
    {:else}
      {t(locale, "dispo.reprendre")}
    {/if}
  </button>

  <h2>{t(locale, "dispo.titre_rayon")}</h2>
  <p class="klaar-tempere">
    {t(locale, "dispo.explication_rayon")}
  </p>

  <label for="rayon">{t(locale, "dispo.rayon", { km: rayonKm })}</label>
  <input
    id="rayon"
    type="range"
    min={RAYON_MIN_METRES / 1000}
    max={RAYON_MAX_METRES / 1000}
    step="1"
    bind:value={rayonKm}
    disabled={occupe}
    data-champ="rayon"
  />
  <button
    type="button"
    onclick={enregistrerRayon}
    disabled={occupe || rayonKm * 1000 === etat.rayon_intervention_metres}
    data-action="enregistrer-rayon"
  >
    {t(locale, "dispo.enregistrer")}
  </button>
{/if}

{#if erreur && etat !== null}
  <p role="alert" data-erreur-dispo>{erreur}</p>
{/if}

<style>
  label { display: block; font-weight: 600; margin-top: 0.6rem; }
  input[type="range"] { width: min(100%, 24rem); display: block; }
  button {
    font: inherit;
    margin: 0.6rem 0.4rem 0 0;
    padding: 0.55rem 0.9rem;
    border-radius: 8px;
    border: 1px solid var(--klaar-bord);
    background: var(--klaar-accent);
    color: #1b3a4b;
    cursor: pointer;
  }
  button:disabled { opacity: 0.6; cursor: not-allowed; }
  p[role="alert"] { color: #c2543a; }
  p[data-sollicitable="false"] { color: #8a5a20; }
</style>
