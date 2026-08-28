/**
 * Assemble le site publié sur GitHub Pages.
 *
 * Trois choses, dans un seul dossier :
 *   - une page d'accueil qui présente chaque parcours et **incruste sa vidéo**,
 *     les parcours à deux acteurs côte à côte ;
 *   - le rapport Playwright des parcours filmés ;
 *   - le rapport Playwright de la suite de vérification.
 *
 * **Pourquoi une page à la main plutôt que le seul rapport Playwright.** Ce
 * dernier est fait pour diagnostiquer un échec : il liste des étapes, pas des
 * intentions. Une documentation vivante doit dire ce que chaque parcours
 * démontre, et montrer les deux côtés d'un échange en même temps — ce qu'aucun
 * rapport de test ne fait.
 */
import { mkdir, cp, writeFile, readFile, readdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join } from "node:path";

const RACINE = new URL("..", import.meta.url).pathname;
const SORTIE = join(RACINE, "vitrine");

/**
 * Les parcours, dans l'ordre où ils racontent quelque chose.
 *
 * Écrit à la main et non déduit des fichiers : l'ordre d'un récit n'est pas
 * l'ordre alphabétique, et le propos de chaque parcours ne se devine pas depuis
 * son nom de fichier.
 */
const PARCOURS = [
  {
    titre: "Découvrir le service sans compte",
    propos:
      "Le catalogue et les fourchettes de prix sont publics. Demander de s'inscrire pour " +
      "connaître un prix reviendrait à le faire payer en données personnelles.",
    videos: [{ fichier: "decouverte", acteur: "Un visiteur" }],
  },
  {
    titre: "S'inscrire, et l'adresse qu'on ne confirme jamais",
    propos:
      "Réinscrire une adresse déjà prise donne mot pour mot la même réponse que la première " +
      "fois. C'est ce qui empêche de dresser la liste des personnes inscrites en essayant des " +
      "adresses.",
    videos: [{ fichier: "inscription", acteur: "Un nouveau venu" }],
  },
  {
    titre: "Un dépannage de bout en bout",
    propos:
      "Le parcours qui porte la valeur du service, filmé des deux côtés à la fois. Le " +
      "prestataire ne connaît pas l'adresse avant d'avoir pris la Demande, et la connaît " +
      "après. Le demandeur apprend qui vient, puis suit chaque étape sans rien rafraîchir.",
    videos: [
      { fichier: "depannage-demandeuse", acteur: "Camille · demandeuse" },
      { fichier: "depannage-prestataire", acteur: "Atelier · prestataire" },
    ],
  },
  {
    titre: "Personne ne répond : élargir, puis retirer sa demande",
    propos:
      "La pire issue n'est pas « personne n'est venu », c'est « on ne vous a rien dit ». " +
      "Trente secondes, une réponse, et deux choix : élargir la zone ou retirer sa demande — " +
      "avec un motif pris dans une liste fermée.",
    videos: [{ fichier: "sans-reponse", acteur: "Sacha · demandeur" }],
  },
  {
    titre: "Un prestataire règle son flux",
    propos:
      "Trois raisons distinctes de ne rien recevoir — statut, pause, intervention en cours — " +
      "et l'écran les distingue. Une pause n'est pas une radiation.",
    videos: [{ fichier: "disponibilite", acteur: "Serrurerie Midi" }],
  },
  {
    titre: "L'application sans réseau",
    propos:
      "Klaar sert un dépannage : la connexion est mauvaise précisément quand on en a besoin. " +
      "Les pages déjà visitées restent lisibles, et l'état de la connexion est signalé.",
    videos: [{ fichier: "hors-ligne", acteur: "Un visiteur hors réseau" }],
  },
];

/** Retrouve une vidéo dans l'arborescence de résultats. */
async function trouverVideo(nom) {
  const base = join(RACINE, "demo-resultats");
  if (!existsSync(base)) return null;
  const pile = [base];
  while (pile.length) {
    const dossier = pile.pop();
    for (const entree of await readdir(dossier, { withFileTypes: true })) {
      const chemin = join(dossier, entree.name);
      if (entree.isDirectory()) pile.push(chemin);
      else if (entree.name === `${nom}.webm`) return chemin;
    }
  }
  return null;
}

function echapper(texte) {
  return texte.replace(/[&<>"]/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c],
  );
}

async function main() {
  await mkdir(join(SORTIE, "videos"), { recursive: true });

  const sections = [];
  const manquants = [];
  for (const parcours of PARCOURS) {
    const lecteurs = [];
    for (const video of parcours.videos) {
      const source = await trouverVideo(video.fichier);
      if (!source) {
        manquants.push(video.fichier);
        continue;
      }
      await cp(source, join(SORTIE, "videos", `${video.fichier}.webm`));
      lecteurs.push(
        `<figure><figcaption>${echapper(video.acteur)}</figcaption>` +
          `<video controls preload="metadata" playsinline src="videos/${video.fichier}.webm"></video>` +
          `</figure>`,
      );
    }
    if (lecteurs.length === 0) continue;
    sections.push(
      `<section>
  <h2>${echapper(parcours.titre)}</h2>
  <p>${echapper(parcours.propos)}</p>
  <div class="lecteurs${lecteurs.length > 1 ? " double" : ""}">${lecteurs.join("")}</div>
</section>`,
    );
  }

  // Un parcours absent est **annoncé** et non tu : une page qui montre cinq
  // vidéos sur six sans le dire laisse croire qu'il n'y en a jamais eu que
  // cinq.
  const avertissement = manquants.length
    ? `<p class="manquant">Enregistrements absents de cette publication : ${echapper(
        manquants.join(", "),
      )}. Le parcours correspondant a échoué ou n'a pas été joué.</p>`
    : "";

  const rapports = [];
  if (existsSync(join(RACINE, "playwright-report-demo"))) {
    await cp(join(RACINE, "playwright-report-demo"), join(SORTIE, "rapport-parcours"), {
      recursive: true,
    });
    rapports.push('<li><a href="rapport-parcours/">Rapport détaillé des parcours filmés</a></li>');
  }
  if (existsSync(join(RACINE, "playwright-report"))) {
    await cp(join(RACINE, "playwright-report"), join(SORTIE, "rapport-verification"), {
      recursive: true,
    });
    rapports.push(
      '<li><a href="rapport-verification/">Rapport de la suite de vérification</a></li>',
    );
  }

  const page = `<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Klaar — parcours filmés</title>
<meta name="description" content="Documentation vivante de Klaar : chaque parcours du service, filmé de bout en bout contre le service réel." />
<style>
  :root { --bord: #d8dee2; --accent: #ffd166; --fond: #fbfaf8; --encre: #17313f; }
  * { box-sizing: border-box; }
  body {
    margin: 0 auto; padding: 2rem 1.2rem 5rem; max-width: 68rem;
    font: 17px/1.6 system-ui, -apple-system, "Segoe UI", sans-serif;
    color: var(--encre); background: var(--fond);
  }
  h1 { font-size: 2rem; margin-bottom: .3rem; }
  .chapeau { font-size: 1.05rem; max-width: 46rem; }
  section { border-top: 1px solid var(--bord); padding-top: 1.6rem; margin-top: 2.4rem; }
  section h2 { margin-bottom: .4rem; }
  section p { max-width: 46rem; }
  .lecteurs { display: grid; gap: 1rem; margin-top: 1rem; }
  .lecteurs.double { grid-template-columns: repeat(auto-fit, minmax(22rem, 1fr)); }
  figure { margin: 0; }
  figcaption {
    display: inline-block; background: var(--accent); color: var(--encre);
    border-radius: 999px; padding: .2rem .8rem; font-weight: 700; font-size: .9rem;
    margin-bottom: .4rem;
  }
  video { width: 100%; border-radius: 10px; border: 1px solid var(--bord); background: #000; }
  .manquant { color: #a8481f; font-weight: 600; }
  footer { border-top: 1px solid var(--bord); margin-top: 3rem; padding-top: 1.2rem; font-size: .95rem; }
  a { color: #0b6b8a; }
</style>
</head>
<body>
<h1>Klaar — parcours filmés</h1>
<p class="chapeau">
  Chaque vidéo ci-dessous est l'enregistrement d'un test qui tourne contre le
  service réel : PostgreSQL, l'API, le navigateur. Rien n'est simulé, rien n'est
  rejoué au montage. Une seconde au moins sépare chaque geste, pour que ce soit
  regardable ; le bandeau du bas dit ce qui se passe et qui agit.
</p>
<p class="chapeau">
  Les données sont fictives : les comptes sont sur <code>demo.klaar.invalid</code>,
  domaine réservé par la RFC 2606 où rien ne peut être livré, et les numéros
  d'entreprise sont construits, jamais copiés d'une société réelle.
</p>
${avertissement}
${sections.join("\n")}
<footer>
  <ul>${rapports.join("")}</ul>
  <p>
    Klaar est un logiciel libre sous AGPL-3.0.
    <a href="https://github.com/gilmry/klaar">Code source</a>.
  </p>
</footer>
</body>
</html>
`;
  await writeFile(join(SORTIE, "index.html"), page, "utf-8");

  // GitHub Pages passe le site par Jekyll par défaut, qui ignore les dossiers
  // commençant par un tiret bas — ceux du rapport Playwright en font partie.
  await writeFile(join(SORTIE, ".nojekyll"), "", "utf-8");

  console.log(
    `vitrine assemblée : ${sections.length} parcours, ${manquants.length} enregistrement(s) manquant(s)`,
  );
  // Sortie non nulle si tout manque : mieux vaut un échec de publication qu'une
  // page vide en ligne.
  if (sections.length === 0) process.exit(1);
}

await main();
