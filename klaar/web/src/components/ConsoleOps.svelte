<script lang="ts">
  /**
   * Console d'exploitation : connexion et tableau de bord (Story 8.3, FR-040).
   *
   * **Le jeton vit en mémoire de page, et nulle part ailleurs.** Fermer
   * l'onglet ferme la session. C'est volontairement moins confortable qu'un
   * `localStorage` : cette console ouvre les Demandes, les litiges et les
   * montants de tout le monde.
   *
   * **Rien n'est nominatif ici.** Le service ne rend que des agrégats, et
   * l'écran n'a donc aucun dossier à afficher. Un tableau de bord qui listerait
   * des noms deviendrait un moyen commode de consulter des dossiers sans passer
   * par les routes qui journalisent la cible.
   */
  import { onDestroy } from "svelte";
  import Mediation from "./Mediation.svelte";
  import RevueKyc from "./RevueKyc.svelte";
  import CatalogueOps from "./CatalogueOps.svelte";
  import {
    connexionOps,
    deconnexionOps,
    montantOps,
    noteLisible,
    oublierJetonOps,
    pourcentage,
    resteAvantEcheance,
    sessionFinie,
    tableauDeBord,
    type RoleOps,
    type TableauBord,
  } from "../lib/ops";
  import { ApiError, OfflineError } from "../lib/api";

  let email = $state("");
  let motDePasse = $state("");
  let codeTotp = $state("");
  let role = $state<RoleOps | null>(null);
  let tableau = $state<TableauBord | null>(null);
  /**
   * Dernière valeur connue, gardée quand le service ne répond plus.
   *
   * FR-040 `@negative` : vider l'écran sur une panne effacerait la seule
   * information dont dispose l'exploitation au moment précis où elle en a le
   * plus besoin. La bannière dit que le chiffre a vieilli.
   */
  let vuLe = $state<Date | null>(null);
  let panne = $state(false);
  let occupe = $state(false);
  let erreur = $state<string | null>(null);
  let reste = $state<number | null>(null);

  /** Trente secondes, comme le demande FR-040. */
  const RAFRAICHISSEMENT_MS = 30000;
  /** Sous cinq minutes, l'écran prévient plutôt que de couper sans dire. */
  const AVERTIR_SOUS_MS = 5 * 60 * 1000;

  let minuterie: ReturnType<typeof setInterval> | null = null;

  onDestroy(() => {
    arreter();
    // La session est fermée côté serveur : la laisser vivre trente minutes
    // après la fermeture de l'écran serait un jeton en circulation sans
    // porteur.
    void deconnexionOps();
  });

  function arreter() {
    if (minuterie) clearInterval(minuterie);
    minuterie = null;
  }

  async function entrer(evenement: Event) {
    evenement.preventDefault();
    if (occupe) return;
    occupe = true;
    erreur = null;
    try {
      const session = await connexionOps(email, motDePasse, codeTotp);
      role = session.role;
      // Le mot de passe et le code ne servent plus à rien : les garder dans
      // l'état du composant les exposerait à toute inspection de la page.
      motDePasse = "";
      codeTotp = "";
      await rafraichir();
      minuterie = setInterval(() => void rafraichir(), RAFRAICHISSEMENT_MS);
    } catch (e) {
      erreur = messageOps(e);
    } finally {
      occupe = false;
    }
  }

  async function rafraichir() {
    try {
      tableau = await tableauDeBord();
      vuLe = new Date();
      panne = false;
      erreur = null;
    } catch (e) {
      if (sessionFinie(e)) {
        // Une session finie n'est pas une panne : le dire autrement ferait
        // chercher un incident là où il n'y a qu'une échéance.
        sortir();
        erreur = "Votre session a expiré. Reconnectez-vous.";
        return;
      }
      // **La dernière valeur connue reste à l'écran.** La vider effacerait la
      // seule information disponible au moment où l'exploitation en a le plus
      // besoin (FR-040 `@negative`).
      panne = true;
    }
    reste = resteAvantEcheance();
  }

  function sortir() {
    arreter();
    oublierJetonOps();
    role = null;
    tableau = null;
    vuLe = null;
    reste = null;
  }

  async function quitter() {
    await deconnexionOps();
    sortir();
  }

  function messageOps(e: unknown): string {
    if (e instanceof OfflineError) return "Service injoignable. Vérifiez le réseau.";
    if (e instanceof ApiError && e.status === 401) {
      // Un seul message pour les trois causes : dire laquelle a échoué
      // apprendrait à qui essaie s'il a trouvé la bonne adresse.
      return "Adresse, mot de passe ou code refusé.";
    }
    if (e instanceof ApiError && e.status === 403) {
      return "Ce compte est désactivé, ou sa seconde authentification n'est pas configurée.";
    }
    return "Connexion impossible pour le moment.";
  }

  const bientotFini = $derived(reste !== null && reste < AVERTIR_SOUS_MS);
  const minutesRestantes = $derived(reste === null ? 0 : Math.ceil(reste / 60000));
</script>

{#if role === null}
  <form onsubmit={entrer} data-formulaire="ops-connexion">
    <p class="klaar-tempere">
      La session dure trente minutes et ne se prolonge pas. Fermer cet onglet la
      ferme aussi.
    </p>

    <label for="ops-email">Adresse professionnelle</label>
    <input id="ops-email" type="email" bind:value={email} required autocomplete="username" />

    <label for="ops-mdp">Mot de passe</label>
    <input
      id="ops-mdp"
      type="password"
      bind:value={motDePasse}
      required
      autocomplete="current-password"
    />

    <label for="ops-code">Code à six chiffres</label>
    <input
      id="ops-code"
      type="text"
      inputmode="numeric"
      bind:value={codeTotp}
      required
      autocomplete="one-time-code"
    />

    <button type="submit" disabled={occupe} data-action="ops-entrer">
      {occupe ? "Un instant…" : "Entrer"}
    </button>

    {#if erreur}
      <p role="alert" data-erreur="ops">{erreur}</p>
    {/if}
  </form>
{:else}
  <section data-console={role}>
    <p data-ops-role>Connecté comme <strong>{role}</strong>.</p>

    {#if bientotFini}
      <p role="status" data-ops-echeance>
        Votre session se termine dans {minutesRestantes} minute{minutesRestantes > 1 ? "s" : ""}.
        Terminez ce que vous écrivez avant de vous reconnecter.
      </p>
    {/if}

    {#if panne}
      <p role="alert" data-ops-panne>
        Le service ne répond plus. Les chiffres ci-dessous sont ceux de
        {vuLe ? vuLe.toLocaleTimeString("fr-BE") : "la dernière lecture"}, ils
        n'ont pas été rafraîchis.
      </p>
    {/if}

    {#if tableau === null}
      <p role="status">Lecture des indicateurs…</p>
    {:else}
      <p class="klaar-tempere">
        Sur {tableau.fenetre_jours} jours, depuis le
        {new Date(tableau.depuis).toLocaleDateString("fr-BE")}.
      </p>

      <dl data-ops-indicateurs>
        <dt>Comptes actifs</dt>
        <dd data-kpi="comptes-actifs">{tableau.comptes_actifs}</dd>

        <dt>Demandes</dt>
        <dd data-kpi="demandes">{tableau.demandes}</dd>

        <dt>Demandes attribuées</dt>
        <!--
          Le taux vient avec son assiette : « 60 % » sur trois Demandes se lit
          autrement que sur trois mille.
        -->
        <dd data-kpi="remplissage">
          {tableau.demandes_attribuees} sur {tableau.demandes} ·
          {pourcentage(tableau.taux_remplissage)}
        </dd>

        <dt>Volume d'affaires (HTVA)</dt>
        <dd data-kpi="gmv">{montantOps(tableau.gmv_htva_cents)}</dd>

        <dt>Commission (HTVA)</dt>
        <dd data-kpi="commission">{montantOps(tableau.commission_htva_cents)}</dd>

        <dt>Litiges ouverts</dt>
        <dd data-kpi="litiges">{tableau.litiges_ouverts}</dd>

        <dt>Satisfaction</dt>
        <!--
          Nommée « satisfaction » et non « NPS » : le produit ne pose jamais la
          question « recommanderiez-vous ». Appeler NPS une moyenne de notes sur
          cinq serait donner à une mesure le nom d'une autre.
        -->
        <dd data-kpi="satisfaction">{noteLisible(tableau.note_moyenne, tableau.notes)}</dd>

        <dt>Sorties de zone</dt>
        <dd data-kpi="sorties-de-zone">{tableau.sorties_de_zone}</dd>

        <dt>Contrôles d'entreprise en attente</dt>
        <dd data-kpi="kyc">{tableau.kyc_en_attente}</dd>
      </dl>

      {#if tableau.demandes === 0}
        <p role="status" data-ops-vide>
          Aucune Demande sur la période. Ce n'est pas un taux de remplissage nul,
          c'est une absence de mesure : les indicateurs prendront du sens dès les
          premières Demandes.
        </p>
      {/if}
    {/if}

    <!--
      La médiation n'est offerte qu'aux rôles qui peuvent trancher. Afficher
      l'onglet à un lecteur le ferait cliquer pour recevoir un 403, et proposer
      un geste qu'on refusera est déjà une erreur de conception.
    -->
    {#if role === "MEDIATOR" || role === "SUPER_ADMIN"}
      <h2>Litiges à trancher</h2>
      <Mediation />
    {/if}

    {#if role === "KYC_REVIEWER" || role === "SUPER_ADMIN"}
      <h2>Entreprises à contrôler</h2>
      <RevueKyc />
    {/if}

    <!--
      Le catalogue est réservé au super-administrateur : le montrer à un autre
      rôle le ferait cliquer pour recevoir un 403.
    -->
    {#if role === "SUPER_ADMIN"}
      <h2>Catalogue</h2>
      <CatalogueOps />
    {/if}

    <button type="button" onclick={() => void quitter()} data-action="ops-sortir">
      Fermer la session
    </button>
  </section>
{/if}
