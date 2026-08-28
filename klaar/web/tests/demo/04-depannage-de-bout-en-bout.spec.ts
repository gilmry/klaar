/**
 * Le dépannage, des deux côtés à la fois.
 *
 * **C'est le parcours qui prouve la valeur du service**, et il ne se montre pas
 * dans un seul navigateur : ce qui compte est ce que chacun voit *pendant* que
 * l'autre agit. Deux contextes, deux enregistrements, une narration qui les
 * relie — le bandeau porte le nom de l'acteur, sans quoi deux vidéos côte à
 * côte sont indéchiffrables.
 *
 * Ce que ce parcours démontre au passage, sans le dire deux fois :
 *   - le prestataire ne connaît **pas l'adresse** avant d'avoir pris la
 *     Demande, et la connaît après ;
 *   - le demandeur apprend **qui vient** dès l'attribution ;
 *   - chaque étape de l'intervention lui parvient sans qu'il rafraîchisse.
 */
import { test, expect } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, seConnecter, type Acteur } from "./scene";

/** À deux pas de l'atelier d'Anderlecht.
 *
 * **Pourquoi pas la Grand-Place.** Le classement retient les dix plus proches,
 * et une base de développement partagée avec la suite de tests contient des
 * centaines de prestataires posés au centre : ils évinceraient celui de la
 * démonstration, et le parcours s'arrêterait sur une liste vide sans que la
 * cause soit visible à l'écran. Poser la Demande près de l'atelier le fait
 * arriver premier, sans rien modifier aux données des autres. En intégration
 * continue, la base est neuve et la question ne se pose pas.
 */
const PRES_DE_L_ATELIER = { latitude: 50.837, longitude: 4.31 };

test("Un dépannage de bout en bout, vu par le demandeur et par le prestataire", async ({
  browser,
}) => {
  const acteurs: Acteur[] = [];
  const camilleActeur = await ouvrirActeur(
    browser,
    "Camille · demandeuse",
    "depannage-demandeuse",
    PRES_DE_L_ATELIER,
  );
  const atelierActeur = await ouvrirActeur(
    browser,
    "Atelier · prestataire",
    "depannage-prestataire",
    PRES_DE_L_ATELIER,
  );
  acteurs.push(camilleActeur, atelierActeur);
  const camille = camilleActeur.scene;
  const atelier = atelierActeur.scene;

  try {
    // --- Le prestataire se met en service ---------------------------------
    await atelier.aller("/prestataire", "Un prestataire ouvre son espace.");
    await seConnecter(atelier, COMPTES.multiservices);
    await atelier.aller(
      "/prestataire",
      "Il vérifie qu'il est bien en service : sans cela, aucune Demande ne lui parviendra.",
    );
    await atelier.montrer(
      '[data-sollicitable]',
      "Le service lui dit s'il reçoit des Demandes, et sinon pourquoi.",
    );

    // --- La demandeuse décrit son problème --------------------------------
    await camille.aller("/", "Pendant ce temps, Camille a une fuite sous son évier.");
    await seConnecter(camille, COMPTES.demandeur);
    await camille.aller("/demande", "Elle décrit son problème.");
    await camille.choisir('[data-champ="secteur"]', "plomberie", "Elle choisit le secteur.");
    await camille.saisir(
      '[data-champ="description"]',
      "Fuite sous l'évier de la cuisine, l'eau coule en continu depuis ce matin.",
      "Elle explique ce qui se passe, en quelques mots.",
    );
    await camille.cliquer('[data-action="envoyer-demande"]', "Et elle envoie.");
    await camille.page.waitForSelector('[data-demande="creee"]', { timeout: 20000 });
    await camille.montrer(
      "[data-demande-diffusion]",
      "Sa Demande part vers les prestataires disponibles autour d'elle.",
    );
    await camille.cliquer('[data-action="suivre"]', "Elle ouvre le suivi de sa demande.");
    await camille.page.waitForSelector("[data-suivi-etat]", { timeout: 20000 });

    // --- Le prestataire la voit, sans l'adresse ---------------------------
    //
    // Peu de narration ici, et c'est délibéré : la fenêtre de diffusion dure
    // trente secondes. Commenter longuement avant d'accepter ferait expirer la
    // Demande sous nos yeux — ce serait honnête, mais ce n'est pas ce que ce
    // parcours démontre. Les explications viennent juste après.
    await atelier.aller("/prestataire", "Le prestataire voit arriver la Demande.");
    await atelier.page.click('[data-action="rafraichir"]');
    await atelier.page.waitForSelector('[data-demandes="liste"]', { timeout: 20000 });
    const listeAvant = await atelier.page.locator('[data-demandes="liste"]').innerText();
    await atelier.montrer('[data-demandes="liste"] li', "Métier, problème, urgence, distance.");
    await atelier.page.click('[data-action="accepter"]');
    await atelier.page.waitForSelector("[data-mission-statut]", { timeout: 20000 });
    await atelier.raconter("Il prend l'intervention.");

    // --- Ce que la liste ne contenait pas ---------------------------------
    //
    // L'assertion porte sur le texte capturé avant l'acceptation : c'est là
    // que la garantie compte.
    expect(listeAvant).not.toMatch(/\d{2}\.\d{4}/);
    await atelier.raconter(
      "Avant d'accepter, il n'avait pas l'adresse : dix entreprises n'ont pas à connaître l'adresse d'un foyer.",
    );
    await atelier.montrer(
      "[data-mission-position]",
      "Maintenant qu'elle est à lui, il l'obtient. C'est le moment, et pas avant.",
    );

    // --- Camille apprend qui vient ----------------------------------------
    await camille.raconter("Camille, elle, apprend qui vient.");
    await camille.page.waitForSelector('[data-suivi="MATCHED"]', { timeout: 30000 });
    await camille.montrer(
      "[data-suivi-etat]",
      "Le nom de l'entreprise s'affiche. Elle sait qui sonnera à sa porte.",
    );

    // --- L'intervention avance -------------------------------------------
    await atelier.cliquer('[data-vers="PROVIDER_EN_ROUTE"]', "Le prestataire part.");
    await camille.raconter("Camille le voit partir, sans rien rafraîchir.");
    await camille.page.waitForSelector("[data-suivi-intervention]", { timeout: 30000 });
    await expect(camille.page.locator("[data-suivi-intervention]")).toContainText("en route", {
      timeout: 30000,
    });
    await camille.montrer("[data-suivi-intervention]", "« Le prestataire est en route. »");

    await atelier.cliquer('[data-vers="ON_SITE"]', "Il arrive sur place.");
    await expect(camille.page.locator("[data-suivi-intervention]")).toContainText("arrivé", {
      timeout: 30000,
    });
    await camille.montrer("[data-suivi-intervention]", "« Le prestataire est arrivé. »");

    await atelier.cliquer('[data-vers="COMPLETED"]', "La fuite est réparée.");
    await atelier.page.waitForSelector("[data-mission-close]", { timeout: 20000 });
    await expect(camille.page.locator("[data-suivi-intervention]")).toContainText("terminée", {
      timeout: 30000,
    });

    await camille.conclure(
      "Une fuite décrite, un prestataire prévenu, une intervention suivie de bout en bout.",
    );
    await atelier.conclure(
      "Il redevient disponible pour la Demande suivante : une intervention à la fois.",
    );
  } finally {
    // Les vidéos ne sont écrites qu'à la fermeture du contexte, et rangées sous
    // un nom lisible : un condensé ne dirait pas qui est qui.
    await ranger(acteurs);
  }
});
