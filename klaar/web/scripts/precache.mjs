#!/usr/bin/env node
/**
 * Inscrit dans le service worker construit la liste de ce qu'il doit
 * pré-charger, et l'empreinte de la construction.
 *
 * **Le défaut que cela corrige.** Au tout premier passage, la page et ses
 * scripts sont demandés avant que le service worker ne contrôle l'onglet : ils
 * ne traversent pas son gestionnaire `fetch`, donc rien n'est mis en cache.
 * Couper le réseau juste après donnait une page servie depuis le cache dont
 * aucun îlot Svelte ne s'hydratait — l'indicateur de connexion restait sur
 * « Vérification… » au lieu d'annoncer « Hors ligne ». Le cas se voyait comme
 * un test intermittent ; c'en est un, mais il décrivait un vrai trou : une PWA
 * installée pour les coupures réseau ne peut pas exiger d'avoir rechargé une
 * fois avant la coupure.
 *
 * **Ce qui est pré-chargé, et ce qui ne l'est pas.** Les fichiers de
 * `_astro/` (JavaScript et CSS de la construction, dont le nom porte déjà une
 * empreinte) et les pages HTML générées. Pas les images ni les icônes autres
 * que celle du manifeste : elles ne bloquent pas l'affichage, et pré-charger un
 * dossier entier ferait grossir le cache sans qu'on s'en aperçoive.
 *
 * **Pourquoi une empreinte.** Un navigateur ne réinstalle un service worker que
 * si son fichier a changé, octet pour octet. Sans elle, une nouvelle
 * construction laisserait le cache de l'ancienne en place ; avec elle, le
 * fichier change dès qu'un fichier construit change, et `activate` supprime le
 * cache précédent au lieu de l'empiler.
 */
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const RACINE = fileURLToPath(new URL("..", import.meta.url));
const DIST = join(RACINE, "dist");
const SW = join(DIST, "service-worker.js");

/** Tous les fichiers sous `repertoire`, chemins relatifs à `dist`. */
function fichiers(repertoire) {
  const trouves = [];
  for (const entree of readdirSync(repertoire)) {
    const chemin = join(repertoire, entree);
    if (statSync(chemin).isDirectory()) trouves.push(...fichiers(chemin));
    else trouves.push(relative(DIST, chemin));
  }
  return trouves;
}

const tous = fichiers(DIST);

const ressources = tous
  .filter((f) => f.startsWith("_astro/") && (f.endsWith(".js") || f.endsWith(".css")))
  .map((f) => `/${f.split("\\").join("/")}`);

// `dist/catalogue/index.html` est servi à `/catalogue`. La racine garde sa
// barre : c'est l'URL que `APP_SHELL` pré-charge déjà, et la dédoublonner ici
// évite de la demander deux fois à l'installation.
const pages = tous
  .filter((f) => f.endsWith("index.html"))
  .map((f) => "/" + f.split("\\").join("/").replace(/index\.html$/, ""))
  .map((url) => (url === "/" ? url : url.replace(/\/$/, "")))
  .filter((url) => url !== "/");

const aPrecharger = [...ressources, ...pages].sort();

// L'empreinte porte sur le **contenu** et non sur les seuls noms : une page
// HTML modifiée garde son nom, et le cache doit quand même se renouveler.
const empreinte = createHash("sha256");
for (const url of aPrecharger) {
  const chemin = url.endsWith("/") || !url.includes(".") ? join(DIST, url, "index.html") : join(DIST, url);
  empreinte.update(url);
  empreinte.update(readFileSync(chemin));
}
const version = empreinte.digest("hex").slice(0, 12);

let source = readFileSync(SW, "utf8");
const avant = source;
source = source.replace(/^const VERSION = "[^"]*";$/m, `const VERSION = "${version}";`);
source = source.replace(
  /^const PRECACHE = \[\];$/m,
  `const PRECACHE = ${JSON.stringify(aPrecharger, null, 2)};`,
);

if (source === avant) {
  // Échouer plutôt que de livrer un service worker qui ne pré-charge rien : le
  // silence donnerait une PWA qui se croit hors-ligne-capable et ne l'est pas.
  console.error(
    "précache : les repères `const VERSION` / `const PRECACHE` sont introuvables dans " +
      "dist/service-worker.js. public/service-worker.js a-t-il changé de forme ?",
  );
  process.exit(1);
}

writeFileSync(SW, source);
console.log(
  `précache : ${aPrecharger.length} entrées (${ressources.length} ressources, ` +
    `${pages.length} pages), version ${version}`,
);
