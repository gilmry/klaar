import { defineConfig, devices } from "@playwright/test";

// Le build statique est servi tel quel : c'est exactement ce qui sera déployé,
// et un service worker ne s'enregistre que sur une origine sécurisée, ce que
// localhost est par convention.
export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: false,
  workers: 1,
  // Le rapport HTML embarque vidéos, captures et traces : c'est lui qu'on
  // ouvre après un échec, et c'est lui qu'on archive en CI. En local il n'est
  // produit que si on demande les vidéos, sinon la liste suffit et ne laisse
  // rien derrière elle.
  reporter:
    process.env.CI || process.env.KLAAR_VIDEO === "1"
      ? [["html", { open: "never" }], ["list"]]
      : "list",
  use: {
    baseURL: "http://127.0.0.1:4321",
    trace: "retain-on-failure",
    // Vidéo et capture d'écran : conservées sur échec seulement.
    //
    // Un enregistrement pèse quelques mégaoctets par cas ; en garder quatre-
    // vingts à chaque exécution verte remplirait le disque pour des fichiers
    // que personne n'ouvrira. Sur échec, en revanche, la vidéo dit en dix
    // secondes ce qu'une trace demande de reconstituer — surtout pour un test
    // de service worker, où la moitié de ce qui se passe est invisible dans
    // les assertions.
    //
    // KLAAR_VIDEO=1 enregistre **tout**, y compris les cas verts : c'est ce
    // qu'il faut pour produire une démonstration du parcours, pas pour le
    // travail courant.
    video: process.env.KLAAR_VIDEO === "1" ? "on" : "retain-on-failure",
    screenshot: "only-on-failure",
  },
  // Par défaut, le Chromium fourni par Playwright — c'est ce que la CI
  // installe. KLAAR_PLAYWRIGHT_CHANNEL permet de pointer un Chrome du système
  // là où le CDN de Playwright est inaccessible (il répond 403 depuis
  // certaines régions) ; le navigateur diffère alors, et c'est à savoir en
  // lisant un résultat local.
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        channel: process.env.KLAAR_PLAYWRIGHT_CHANNEL || undefined,
      },
    },
  ],
  webServer: {
    command: "npx astro preview --port 4321 --host 127.0.0.1",
    url: "http://127.0.0.1:4321/",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
