import { defineConfig } from "astro/config";
import svelte from "@astrojs/svelte";

// Build statique : la PWA est pré-rendue et servie en fichiers plats. Aucun
// rendu serveur, donc aucune donnée personnelle ne transite par le processus
// de build (RGPD, cf. klaar/COMPLIANCE.md).
export default defineConfig({
  integrations: [svelte()],
  output: "static",
});
