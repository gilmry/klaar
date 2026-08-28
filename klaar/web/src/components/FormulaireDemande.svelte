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
  import { restaurerLangue, t, type CleTexte } from "../lib/i18n";

  /**
   * Les trois urgences, dans l'ordre croissant.
   *
   * Hors du gabarit, pour que le libellé soit une clé de traduction et non un
   * texte français figé dans la boucle.
   */
  const URGENCES: [UrgenceKlaar, CleTexte][] = [
    ["LOW", "urgence.basse"],
    ["NORMAL", "urgence.normale"],
    ["HIGH", "urgence.haute"],
  ];
  import { restaurerSession } from "../lib/connexion";
  import { chargerCatalogue, type SecteurCatalogue } from "../lib/catalogue";
  import { OfflineError } from "../lib/api";
  import { enqueue } from "../lib/offlineQueue";
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
  /**
   * Vrai quand la demande attend le retour du réseau.
   *
   * Distinct de `creee` : rien n'a encore été créé côté service, et le dire
   * autrement ferait croire que des prestataires ont été prévenus.
   */
  let enFile = $state(false);

  let creee = $state<{
    id: string;
    doublon: boolean;
    candidats: number;
    notifies: number;
  } | null>(null);
  let locale = $state<LocaleKlaar>("fr");

  onMount(async () => {
    locale = restaurerLangue();
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
      creee = {
        id: reponse.id,
        doublon: reponse.code === "REQUEST_DUPLICATE",
        candidats: reponse.candidats ?? 0,
        notifies: reponse.notifies ?? 0,
      };
    } catch (e) {
      // Hors connexion, la demande est **mise en file** plutôt que perdue.
      //
      // C'est le cas d'usage central d'un service de dépannage : la cave, le
      // parking, l'ascenseur. Faire retaper le formulaire au retour du réseau
      // reviendrait à punir quelqu'un pour un problème qui ne le concerne pas.
      //
      // Le rejeu porte une clé d'idempotence. Le service ne la lit pas encore ;
      // ce qui protège d'une double soumission est la fenêtre de doublon de
      // cinq minutes (FR-011), qui rend la Demande existante au lieu d'en créer
      // une seconde. C'est une garantie plus faible, et elle est écrite ici.
      if (e instanceof OfflineError) {
        const position = await positionActuelle().catch(() => null);
        if (position) {
          await enqueue("POST", "/requests", {
            secteur,
            description,
            latitude: position.coords.latitude,
            longitude: position.coords.longitude,
            urgence,
          });
          enFile = true;
          occupe = false;
          return;
        }
      }
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
  <p data-etat-demande="reprise">{t(locale, "commun.attendez")}</p>
{:else if !connecte}
  <p role="status" data-etat-demande="anonyme">
    {t(locale, "demande.compte_requis")}
    <a href="/connexion">{t(locale, "commun.me_connecter")}</a>
  </p>
{:else if enFile}
  <p role="status" data-demande="en-file">
    {t(locale, "demande.en_file")}
  </p>
  <p class="klaar-tempere">
    {t(locale, "demande.rien_envoye")}
  </p>
{:else if creee}
  <p role="status" data-demande="creee">
    {#if creee.doublon}
      {t(locale, "demande.doublon")}
    {:else}
      {t(locale, "demande.diffusee")}
    {/if}
  </p>
  <!--
    Deux nombres et non un : un prestataire retenu sans abonnement aux
    notifications verra la Demande en ouvrant l'application. Les confondre
    ferait croire que dix personnes ont été réveillées alors que personne n'a
    rien reçu.
  -->
  <p class="klaar-tempere" data-demande-diffusion>
    {#if creee.candidats === 0}
      {t(locale, "demande.aucun_candidat")}
    {:else}
      {t(locale, "demande.candidats", {
        c: creee.candidats ?? 0,
        n: creee.notifies ?? 0,
      })}
    {/if}
  </p>
  <p>
    <a href={`/demande?id=${creee.id}`} data-action="suivre">{t(locale, "demande.suivre")}</a>
  </p>
{:else}
  <form onsubmit={envoyer} data-formulaire="demande" novalidate>
    <label for="demande-secteur">{t(locale, "demande.secteur")}</label>
    <select id="demande-secteur" bind:value={secteur} data-champ="secteur" required>
      <option value="" disabled>{t(locale, "demande.choisissez")}</option>
      {#each secteurs as s (s.code)}
        <option value={s.code}>{s.libelle}</option>
      {/each}
    </select>

    <label for="demande-description">{t(locale, "demande.que_se_passe_t_il")}</label>
    <textarea
      id="demande-description"
      bind:value={description}
      rows="4"
      maxlength={DESCRIPTION_MAX}
      data-champ="description"
      required
    ></textarea>
    <p class="klaar-tempere" data-restant>{t(locale, "demande.restants", { n: restant })}</p>

    <fieldset>
      <legend>{t(locale, "demande.urgence")}</legend>
      {#each URGENCES as [valeur, cle] (valeur)}
        <label>
          <input type="radio" bind:group={urgence} value={valeur} name="urgence" />
          {t(locale, cle)}
        </label>
      {/each}
    </fieldset>

    <button type="submit" disabled={occupe || !complet} data-action="envoyer-demande">
      {occupe ? t(locale, "commun.attendez") : t(locale, "demande.envoyer")}
    </button>
    <p class="klaar-tempere">
      {t(locale, "demande.position_requise")}
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
