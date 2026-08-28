/**
 * Le droit à l'effacement, exercé sans justification à fournir.
 *
 * L'article 17 du RGPD donne ce droit ; l'exercer ne doit pas être un parcours
 * du combattant. Deux protections contre le clic malheureux, et aucune de plus :
 * l'action est repliée, et il faut recopier un mot. Le délai de trente jours est
 * annulable, sans quoi ce seraient trente jours d'attente pour rien.
 */
import { test } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, seConnecter, type Acteur } from "./scene";

test("Demander l'effacement de son compte, puis changer d'avis", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const acteur = await ouvrirActeur(browser, "Camille · demandeuse", "effacement");
  acteurs.push(acteur);
  const s = acteur.scene;

  try {
    await s.aller("/", "Camille veut faire effacer son compte.");
    await seConnecter(s, COMPTES.demandeur);
    await s.aller("/mon-compte", "Elle ouvre son compte.");
    await s.montrer(
      ".klaar-tempere",
      "Le droit est rappelé, avec son fondement : article 17 du RGPD, sans justification à fournir.",
    );

    await s.cliquer('[data-action="ouvrir-effacement"]', "Elle ouvre la demande d'effacement.");
    await s.montrer(
      '[data-champ="confirmation"]',
      "Un mot à recopier : un effacement n'a pas à être facile, il n'a pas non plus à être un parcours du combattant.",
    );
    await s.saisir('[data-champ="confirmation"]', "DELETE", "Elle recopie le mot.");
    await s.cliquer('[data-action="confirmer-effacement"]', "Elle confirme.");
    await s.page.waitForSelector('[data-effacement="programme"]', { timeout: 20000 });
    await s.montrer(
      '[data-effacement="programme"]',
      "L'effacement est programmé à trente jours. Le délai existe pour laisser le temps de changer d'avis.",
    );

    await s.cliquer('[data-action="annuler-effacement"]', "Et justement, elle change d'avis.");
    await s.page.waitForSelector('[data-action="ouvrir-effacement"]', { timeout: 20000 });
    await s.conclure(
      "Le compte est rétabli. Le journal d'audit garde la trace que ce droit a été exercé, sans porter aucune donnée personnelle.",
    );
  } finally {
    await ranger(acteurs);
  }
});
