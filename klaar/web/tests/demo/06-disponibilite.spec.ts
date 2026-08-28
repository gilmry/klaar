/**
 * Un prestataire règle son flux.
 *
 * Trois raisons distinctes de ne rien recevoir — statut, pause, intervention en
 * cours — et l'écran les distingue. Les confondre ferait d'une pause une
 * sanction, ou laisserait quelqu'un chercher un interrupteur qui n'y est pour
 * rien.
 */
import { test } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, seConnecter, type Acteur } from "./scene";

test("Se mettre en pause, régler jusqu'où l'on se déplace", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const acteur = await ouvrirActeur(browser, "Serrurerie Midi", "disponibilite");
  acteurs.push(acteur);
  const s = acteur.scene;

  try {
    await s.aller("/", "Un prestataire ouvre son espace.");
    await seConnecter(s, COMPTES.serrurier);
    await s.aller("/prestataire", "Il gère sa disponibilité.");
    await s.montrer(
      '[data-sollicitable="true"]',
      "En service : les Demandes de ses métiers lui parviennent.",
    );

    await s.cliquer('[data-action="basculer-disponibilite"]', "Il se met en pause.");
    await s.montrer(
      '[data-sollicitable="false"]',
      "Le service explique pourquoi il ne reçoit plus rien. Une pause n'est pas une radiation.",
    );

    await s.cliquer('[data-action="basculer-disponibilite"]', "Il reprend le service.");
    await s.montrer(
      '[data-champ="rayon"]',
      "Il règle aussi la distance au-delà de laquelle il ne se déplace pas.",
    );
    await s.page.fill('[data-champ="rayon"]', "5");
    await s.souffler();
    await s.cliquer('[data-action="enregistrer-rayon"]', "Cinq kilomètres, pas plus.");
    await s.souffler();
    await s.conclure(
      "Sa limite à lui, distincte de celle de la recherche : les deux s'appliquent.",
    );
  } finally {
    await ranger(acteurs);
  }
});
