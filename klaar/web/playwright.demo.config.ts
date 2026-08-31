import { defineConfig, devices } from "@playwright/test";

/**
 * Parcours filmés — documentation vivante.
 *
 * Configuration séparée de celle de la vérification, et pas un simple
 * interrupteur : ces parcours tournent contre le **vrai service** (API et
 * PostgreSQL), à vitesse humaine, et leur produit est une vidéo. Les mélanger
 * ferait d'une barrière de qualité une séance de cinéma de dix minutes.
 *
 * `KLAAR_API_URL` doit pointer une instance réelle, peuplée par
 * `klaar-prestataires-demo`. Sans elle, rien à démontrer.
 */
export default defineConfig({
  testDir: "./tests/demo",
  // Un seul à la fois : deux parcours simultanés se disputeraient les comptes
  // de démonstration, et un prestataire pris par l'un manquerait à l'autre.
  fullyParallel: false,
  workers: 1,
  // Aucun réessai : une vidéo de deuxième tentative montrerait un parcours qui
  // a échoué la première fois sans le dire.
  retries: 0,
  // Les parcours sont lents par construction : une seconde au moins entre
  // chaque geste, et le temps de lire les narrations. Le délai par défaut de
  // Playwright les couperait au milieu.
  timeout: 600_000,
  reporter: [
    ["html", { open: "never", outputFolder: "playwright-report-demo" }],
    ["json", { outputFile: "demo-resultats.json" }],
    ["list"],
  ],
  outputDir: "demo-resultats",
  // Sans borne, une action sur un sélecteur absent attend jusqu'au délai du
  // parcours entier : dix minutes pour apprendre qu'un attribut a changé de
  // nom. Quinze secondes suffisent, et l'échec dit alors *où*.
  use: {
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
    baseURL: process.env.KLAAR_DEMO_BASE_URL ?? "http://127.0.0.1:4321",
    // Tout est filmé : c'est l'objet même de cette suite.
    video: { mode: "on", size: { width: 1280, height: 720 } },
    // La trace, elle, seulement sur échec. Elle pèse plus lourd que la vidéo
    // pour un parcours vert que personne n'ouvrira, et cette publication est
    // faite pour être regardée, pas pour être diagnostiquée.
    trace: "retain-on-failure",
    // Une fenêtre nette, aux proportions d'une vidéo : le rendu par défaut de
    // Playwright est plus étroit et coupe le bandeau de narration.
    viewport: { width: 1280, height: 720 },
    // La géolocalisation ne se clique pas dans une boîte système : elle
    // s'accorde au contexte. C'est un écart avec un usage réel, écrit ici.
    permissions: ["geolocation"],
    geolocation: { latitude: 50.8467, longitude: 4.3525 },
    locale: "fr-BE",
    timezoneId: "Europe/Brussels",
  },
  projects: [
    {
      name: "parcours",
      use: {
        ...devices["Desktop Chrome"],
        channel: process.env.KLAAR_PLAYWRIGHT_CHANNEL || undefined,
        // **Avec affichage quand il y en a un.** Chromium sans affichage n'a
        // pas de service de notification : l'accueil filmé annonçait « Les
        // notifications sont bloquées pour ce site », phrase fausse publiée
        // sur la vitrine. `scripts/parcours-filmes.sh` fournit un serveur X
        // virtuel quand la machine n'en a pas ; sans lui, on retombe sur le
        // mode sans affichage plutôt que de ne rien enregistrer du tout.
        headless: !process.env.DISPLAY,
      },
    },
  ],
  // Site et API sur la **même origine**, comme derrière le proxy inverse d'un
  // déploiement réel. Pointer le front sur un autre port aurait demandé du CORS
  // sur l'API — relâcher une garantie de production pour une démonstration —
  // et intercepter les appels dans le navigateur aurait montré un chemin réseau
  // qui n'existe pas.
  //
  // **Sauf si `KLAAR_DEMO_BASE_URL` désigne un déploiement externe.** Le
  // conteneur `klaar-site` sert le site et relaie l'API sur la même origine :
  // il fait exactement ce que ce petit serveur fait, et le lancer en plus
  // relaierait vers un port que le déploiement conteneurisé ne publie pas.
  //
  // Le défaut a coûté une heure : les parcours pointés sur le conteneur
  // tournaient en réalité contre ce serveur-ci, dont le relais visait un
  // service natif éteint. Les vidéos continuaient de sortir, et c'est le
  // journal du service — muet depuis le début — qui l'a montré.
  webServer: process.env.KLAAR_DEMO_BASE_URL
    ? undefined
    : {
        command: "node scripts/serveur-demo.mjs",
        url: "http://127.0.0.1:4321/",
        reuseExistingServer: false,
        timeout: 60_000,
      },
});
