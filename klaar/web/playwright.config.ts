import { defineConfig, devices } from "@playwright/test";

// Le build statique est servi tel quel : c'est exactement ce qui sera déployé,
// et un service worker ne s'enregistre que sur une origine sécurisée, ce que
// localhost est par convention.
export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: false,
  workers: 1,
  reporter: process.env.CI ? [["html", { open: "never" }], ["list"]] : "list",
  use: {
    baseURL: "http://127.0.0.1:4321",
    trace: "retain-on-failure",
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
