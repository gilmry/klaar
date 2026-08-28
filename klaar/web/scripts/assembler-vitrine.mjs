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
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Découvrir le service sans compte",
    propos:
      "Le catalogue et les fourchettes de prix sont publics. Demander de s'inscrire pour " +
      "connaître un prix reviendrait à le faire payer en données personnelles.",
    videos: [{ fichier: "decouverte", acteur: "Un visiteur" }],
  },
  {
    titre: "S'inscrire, et l'adresse qu'on ne confirme jamais",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "S'inscrire, et l'adresse qu'on ne confirme jamais",
    propos:
      "Réinscrire une adresse déjà prise donne mot pour mot la même réponse que la première " +
      "fois. C'est ce qui empêche de dresser la liste des personnes inscrites en essayant des " +
      "adresses.",
    videos: [{ fichier: "inscription", acteur: "Un nouveau venu" }],
  },
  {
    titre: "Un dépannage de bout en bout",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Un dépannage de bout en bout, vu par le demandeur et par le prestataire",
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
    titre: "Un devis envoyé, reçu, et le second qui ne passe pas",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Un devis envoyé, reçu, et le second qui ne passe pas",
    propos:
      "Le prix vient du prestataire. L'écran ne propose aucun montant, n'en suggère aucun et " +
      "n'en corrige aucun : c'est ce que la loi belge sur le travail de plateforme regarde, et " +
      "un champ vide filmé le montre mieux qu'un paragraphe. La TVA belge s'ajoute au prix et " +
      "reste détaillée ; un second devis est refusé tant que le premier attend une réponse.",
    videos: [
      { fichier: "devis-demandeuse", acteur: "Camille · demandeuse" },
      { fichier: "devis-prestataire", acteur: "Élec Schaerbeek · prestataire" },
    ],
  },
  {
    titre: "Deux prestataires, une Demande : le premier arrivé gagne",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Deux prestataires, une Demande : le premier arrivé gagne",
    propos:
      "Cinq prestataires reçoivent la même notification et peuvent toucher « je prends » dans " +
      "la même seconde. Un seul obtient l'intervention ; l'autre reçoit une phrase qui dit ce " +
      "qui s'est passé, pas un code d'erreur. Côté service, une écriture que PostgreSQL " +
      "sérialise et que rien ne peut doubler.",
    videos: [
      { fichier: "course-demandeuse", acteur: "Camille · demandeuse" },
      { fichier: "course-gagnant", acteur: "Plomberie Sud · gagne" },
      { fichier: "course-perdant", acteur: "Dépannage Sud · arrive après" },
    ],
  },
  {
    titre: "Une Demande retirée pendant qu'un prestataire la regarde",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Une Demande retirée pendant qu'un prestataire la regarde",
    propos:
      "L'annulation et l'acceptation portent sur la même ligne : l'une gagne. Quand c'est " +
      "l'annulation, le prestataire apprend que la Demande a été retirée — et non qu'un autre " +
      "a été plus rapide. Dire l'un pour l'autre l'enverrait chercher un concurrent qui " +
      "n'existe pas.",
    videos: [
      { fichier: "annulation-demandeur", acteur: "Sacha · demandeur" },
      { fichier: "annulation-prestataire", acteur: "Dépannage Sud · prestataire" },
    ],
  },
  {
    titre: "Personne ne répond : élargir, puis retirer sa demande",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Personne ne répond : élargir, puis retirer sa demande",
    propos:
      "La pire issue n'est pas « personne n'est venu », c'est « on ne vous a rien dit ». " +
      "Trente secondes, une réponse, et deux choix : élargir la zone ou retirer sa demande — " +
      "avec un motif pris dans une liste fermée.",
    videos: [{ fichier: "sans-reponse", acteur: "Sacha · demandeur" }],
  },
  {
    titre: "Un prestataire règle son flux",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Se mettre en pause, régler jusqu'où l'on se déplace",
    propos:
      "Trois raisons distinctes de ne rien recevoir — statut, pause, intervention en cours — " +
      "et l'écran les distingue. Une pause n'est pas une radiation.",
    videos: [{ fichier: "disponibilite", acteur: "Serrurerie Midi" }],
  },
  {
    titre: "Une demande écrite sans réseau part au retour de la connexion",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Une demande écrite sans réseau part au retour de la connexion",
    propos:
      "Le cas d'usage central d'un service de dépannage : la cave, le parking, l'ascenseur. " +
      "La demande est conservée sur l'appareil et part d'elle-même au retour du réseau. Le " +
      "service dit aussi ce qui n'a pas eu lieu : aucun prestataire n'a encore été prévenu.",
    videos: [{ fichier: "file-hors-ligne", acteur: "Camille · demandeuse" }],
  },
  {
    titre: "Demander l'effacement de son compte, puis changer d'avis",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Demander l'effacement de son compte, puis changer d'avis",
    propos:
      "L'article 17 du RGPD donne ce droit, et l'exercer ne doit pas être un parcours du " +
      "combattant : deux protections contre le clic malheureux, et aucune de plus. Le délai " +
      "de trente jours est annulable, sans quoi ce seraient trente jours d'attente pour rien.",
    videos: [{ fichier: "effacement", acteur: "Camille · demandeuse" }],
  },
  {
    titre: "Mot de passe erroné : le même refus, quel que soit le compte",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "Mot de passe erroné : le même refus, puis le verrouillage",
    propos:
      "Deux protections distinctes, souvent confondues : le message d'erreur, qui ne dit " +
      "jamais si l'adresse existe, et le verrouillage après cinq échecs, qui empêche " +
      "d'essayer les mots de passe un à un.",
    videos: [{ fichier: "verrouillage", acteur: "Quelqu'un qui essaie" }],
  },
  {
    titre: "L'application sans réseau",
    // Titre du test qui produit ces vidéos. Explicite parce que le titre
    // affiché est une accroche, pas un nom de cas — et parce qu'apparier à
    // l'aveugle laisserait publier un parcours en échec.
    test: "L'application sans réseau",
    propos:
      "Klaar sert un dépannage : la connexion est mauvaise précisément quand on en a besoin. " +
      "Les pages déjà visitées restent lisibles, et l'état de la connexion est signalé.",
    videos: [{ fichier: "hors-ligne", acteur: "Un visiteur hors réseau" }],
  },
];

/**
 * Parcours qui ont échoué, d'après le rapport JSON de Playwright.
 *
 * **Une vidéo existe même quand le parcours échoue** : Playwright filme jusqu'à
 * l'interruption. Publier ces enregistrements sans le dire montrerait un
 * parcours qui s'arrête au milieu comme s'il démontrait quelque chose. Ils sont
 * donc écartés, et comptés dans l'avertissement.
 *
 * Le rapport absent est traité comme « rien n'a échoué » : c'est le cas d'un
 * assemblage lancé à la main sur des résultats déjà là, et refuser de publier
 * pour cette raison serait disproportionné.
 */
async function parcoursEnEchec() {
  const chemin = join(RACINE, "demo-resultats.json");
  if (!existsSync(chemin)) return { rates: new Set(), connus: new Set() };
  try {
    const rapport = JSON.parse(await readFile(chemin, "utf-8"));
    const rates = new Set();
    const connus = new Set();
    const parcourir = (suites) => {
      for (const suite of suites ?? []) {
        for (const cas of suite.specs ?? []) {
          connus.add(cas.title);
          if (!cas.ok) rates.add(cas.title);
        }
        parcourir(suite.suites);
      }
    };
    parcourir(rapport.suites);
    return { rates, connus };
  } catch {
    return { rates: new Set(), connus: new Set() };
  }
}

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

  const { rates: enEchec, connus } = await parcoursEnEchec();

  // Un lien cassé entre la vitrine et les tests fait échouer l'assemblage.
  // Sans ce contrôle, renommer un test rendrait son échec invisible : la
  // vidéo serait publiée comme si le parcours avait abouti.
  if (connus.size > 0) {
    const orphelins = PARCOURS.filter((p) => !connus.has(p.test)).map((p) => p.test);
    if (orphelins.length > 0) {
      console.error(`parcours introuvables dans le rapport : ${orphelins.join(" · ")}`);
      process.exit(1);
    }
  }
  const sections = [];
  const manquants = [];
  for (const parcours of PARCOURS) {
    // Un parcours en échec n'est pas publié : sa vidéo existe, mais elle
    // s'arrête au milieu et ne démontre rien.
    if (enEchec.has(parcours.test)) {
      manquants.push(`${parcours.titre} (parcours en échec)`);
      continue;
    }
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
    Klaar est un logiciel libre sous licence MIT (ADR-009).
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
