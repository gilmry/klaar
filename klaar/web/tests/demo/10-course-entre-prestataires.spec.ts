/**
 * Deux prestataires, une Demande : le premier arrivé gagne.
 *
 * **La garantie que ce parcours montre ne se voit pas dans une capture
 * d'écran.** Cinq prestataires reçoivent la même notification et peuvent
 * toucher « je prends » dans la même seconde. Un seul obtient l'intervention,
 * et l'autre reçoit un refus qui dit pourquoi — pas une erreur technique.
 *
 * Côté service, c'est un `UPDATE … WHERE statut = 'BROADCASTING' RETURNING id`
 * que PostgreSQL sérialise. Ici, on le regarde arriver.
 */
import { test, expect } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, seConnecter, type Acteur } from "./scene";

/** Au sud, là où les deux plombiers de démonstration sont à égale distance. */
const AU_SUD = { latitude: 50.8022, longitude: 4.3402 };

test("Deux prestataires, une Demande : le premier arrivé gagne", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const demandeurActeur = await ouvrirActeur(browser, "Camille · demandeuse", "course-demandeuse", AU_SUD);
  const aActeur = await ouvrirActeur(browser, "Plomberie Sud", "course-gagnant", AU_SUD);
  const bActeur = await ouvrirActeur(browser, "Dépannage Sud", "course-perdant", AU_SUD);
  acteurs.push(demandeurActeur, aActeur, bActeur);
  const camille = demandeurActeur.scene;
  const a = aActeur.scene;
  const b = bActeur.scene;

  try {
    await a.aller("/", "Deux plombiers voisins sont en service.");
    await seConnecter(a, COMPTES.plombierSudA);
    await b.aller("/", "Le second aussi, à quelques mètres de là.");
    await seConnecter(b, COMPTES.plombierSudB);

    await camille.aller("/", "Camille a un problème de plomberie.");
    await seConnecter(camille, COMPTES.demandeur);
    await camille.aller("/demande", "Elle décrit son problème.");
    await camille.choisir('[data-champ="secteur"]', "plomberie");
    await camille.saisir(
      '[data-champ="description"]',
      "Chasse d'eau qui fuit en continu depuis hier.",
      "Elle explique la situation.",
    );
    await camille.cliquer('[data-action="envoyer-demande"]', "Elle envoie.");
    await camille.page.waitForSelector("[data-demande-diffusion]", { timeout: 20000 });
    await camille.montrer(
      "[data-demande-diffusion]",
      "Les deux plombiers sont retenus. Le premier qui accepte l'obtiendra.",
    );

    // Peu de narration ici : la fenêtre de diffusion dure trente secondes.
    await a.aller("/prestataire", "Les deux voient la Demande arriver.");
    await b.aller("/prestataire");
    await a.page.click('[data-action="rafraichir"]');
    await b.page.click('[data-action="rafraichir"]');
    await a.page.waitForSelector('[data-demandes="liste"]', { timeout: 20000 });
    await b.page.waitForSelector('[data-demandes="liste"]', { timeout: 20000 });
    await a.souffler();

    await a.raconter("Le premier touche « je prends ».");
    await a.page.click('[data-action="accepter"]');
    await a.page.waitForSelector("[data-mission-statut]", { timeout: 20000 });

    await b.raconter("Le second touche le même bouton, une seconde trop tard.");
    await b.page.click('[data-action="accepter"]');
    await b.page.waitForSelector("[data-erreur-demandes]", { timeout: 20000 });

    await a.montrer("[data-mission-statut]", "L'un a l'intervention, et son adresse.");
    const refus = await b.page.locator("[data-erreur-demandes]").innerText();
    // Un refus qui dit ce qui s'est passé, pas un code technique.
    expect(refus).toMatch(/plus rapide/i);
    await b.montrer(
      "[data-erreur-demandes]",
      "L'autre reçoit une phrase qui dit ce qui s'est passé, pas un code d'erreur.",
    );

    await b.conclure(
      "Un seul part. C'est PostgreSQL qui tranche, en une écriture que rien ne peut doubler.",
    );
    await a.conclure("Deux camionnettes pour une seule fuite : c'est exactement ce qu'on évite.");
  } finally {
    await ranger(acteurs);
  }
});
