/**
 * Le demandeur retire sa Demande pendant qu'un prestataire la regarde.
 *
 * Les deux écritures portent sur la même ligne : l'une gagne, l'autre reçoit un
 * refus. Ce parcours montre le cas où c'est l'annulation qui gagne — le
 * prestataire apprend que la Demande a été retirée, et non qu'« un autre a été
 * plus rapide ». Dire l'un pour l'autre l'enverrait chercher un concurrent qui
 * n'existe pas.
 */
import { test, expect } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, seConnecter, type Acteur } from "./scene";

const AU_SUD = { latitude: 50.8022, longitude: 4.3402 };

test("Une Demande retirée pendant qu'un prestataire la regarde", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const demandeurActeur = await ouvrirActeur(browser, "Sacha · demandeur", "annulation-demandeur", AU_SUD);
  // Celui qui a perdu la course du parcours précédent : il est libre.
  // Reprendre le gagnant l'aurait trouvé en pleine intervention, et l'écran
  // aurait montré une Mission au lieu de la liste des Demandes.
  const prestaActeur = await ouvrirActeur(browser, "Dépannage Sud", "annulation-prestataire", AU_SUD);
  acteurs.push(demandeurActeur, prestaActeur);
  const sacha = demandeurActeur.scene;
  const presta = prestaActeur.scene;

  try {
    await presta.aller("/", "Un plombier voisin est en service.");
    await seConnecter(presta, COMPTES.plombierSudB);

    await sacha.aller("/", "Sacha a une fuite.");
    await seConnecter(sacha, COMPTES.secondDemandeur);
    await sacha.aller("/demande", "Il décrit son problème.");
    await sacha.choisir('[data-champ="secteur"]', "plomberie");
    await sacha.saisir(
      '[data-champ="description"]',
      "Robinet extérieur qui goutte, rien d'urgent.",
      "Rien d'urgent, précise-t-il.",
    );
    await sacha.cliquer('[data-action="envoyer-demande"]', "Il envoie.");
    await sacha.page.waitForSelector("[data-demande-diffusion]", { timeout: 20000 });
    await sacha.page.click('[data-action="suivre"]');
    await sacha.page.waitForSelector("[data-suivi-etat]", { timeout: 20000 });

    await presta.aller("/prestataire", "Le plombier voit la Demande.");
    await presta.page.click('[data-action="rafraichir"]');
    await presta.page.waitForSelector('[data-demandes="liste"]', { timeout: 20000 });
    await presta.montrer('[data-demandes="liste"] li', "Il la lit, il hésite.");

    await sacha.raconter("Pendant ce temps, Sacha a réglé le problème lui-même.");
    await sacha.choisir(
      '[data-champ="motif"]',
      "RESOLVED_ITSELF",
      "Il donne un motif, pris dans une liste fermée.",
    );
    await sacha.cliquer('[data-action="annuler"]', "Il retire sa demande.");
    await sacha.page.waitForSelector('[data-suivi="CANCELLED"]', { timeout: 20000 });

    await presta.raconter("Le plombier, lui, touche « je prends ».");
    await presta.page.click('[data-action="accepter"]');
    await presta.page.waitForSelector("[data-erreur-demandes]", { timeout: 20000 });
    const refus = await presta.page.locator("[data-erreur-demandes]").innerText();
    // Le bon message : « retirée », pas « un autre a été plus rapide ».
    expect(refus).toMatch(/retir/i);
    await presta.montrer(
      "[data-erreur-demandes]",
      "Le service dit ce qui s'est passé : la demande a été retirée, pas prise par un autre.",
    );
    await presta.conclure(
      "Distinguer les deux évite d'envoyer quelqu'un chercher un concurrent qui n'existe pas.",
    );
    await sacha.conclure("Rien n'a été engagé, rien n'est facturé.");
  } finally {
    await ranger(acteurs);
  }
});
