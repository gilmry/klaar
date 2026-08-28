/**
 * Quand personne ne répond, et ce qu'on propose alors.
 *
 * L'issue la plus mauvaise n'est pas « personne n'est venu » : c'est « on ne
 * vous a rien dit ». Ce parcours montre la réponse, l'élargissement de la zone,
 * et le retrait de la demande avec un motif pris dans une liste fermée.
 */
import { test } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, seConnecter, type Acteur } from "./scene";

/** Un coin de la Région où aucun prestataire de démonstration n'intervient. */
const LOIN_DE_TOUT = { latitude: 50.795, longitude: 4.47 };

test("Personne ne répond : élargir, puis retirer sa demande", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const acteur = await ouvrirActeur(browser, "Sacha · demandeur", "sans-reponse", LOIN_DE_TOUT);
  acteurs.push(acteur);
  const s = acteur.scene;

  try {
    await s.aller("/", "Sacha habite à l'écart, là où peu de prestataires interviennent.");
    await seConnecter(s, COMPTES.secondDemandeur);
    await s.aller("/demande", "Il décrit un problème de serrurerie.");
    await s.choisir('[data-champ="secteur"]', "serrurerie", "Il choisit le métier.");
    await s.saisir(
      '[data-champ="description"]',
      "Porte claquée, les clés sont restées à l'intérieur.",
      "Il explique la situation.",
    );
    await s.cliquer('[data-action="envoyer-demande"]', "Il envoie.");
    await s.page.waitForSelector("[data-demande-diffusion]", { timeout: 20000 });
    await s.montrer(
      "[data-demande-diffusion]",
      "Le service dit tout de suite combien de prestataires ont été retenus.",
    );
    await s.cliquer('[data-action="suivre"]', "Il ouvre le suivi.");
    await s.page.waitForSelector("[data-suivi-etat]", { timeout: 20000 });

    await s.raconter("Le tour de diffusion dure trente secondes. On attend avec lui.");
    await s.page.waitForSelector('[data-action="elargir"]', { timeout: 90000 });
    await s.montrer(
      "[data-suivi-etat]",
      "Personne n'a répondu, et le service le dit plutôt que de laisser attendre.",
    );

    await s.cliquer('[data-action="elargir"]', "Il élargit la zone de recherche.");
    await s.souffler(2);
    await s.montrer(
      "[data-suivi] p.klaar-tempere",
      "La zone passe à dix kilomètres. Trois élargissements au maximum, puis la demande est annulée.",
    );

    await s.raconter("Il préfère finalement retirer sa demande.");
    await s.page.waitForSelector('[data-champ="motif"]', { timeout: 90000 });
    await s.choisir(
      '[data-champ="motif"]',
      "FOUND_ELSEWHERE",
      "Le motif est facultatif, et pris dans une liste fermée.",
    );
    await s.raconter(
      "Un champ libre inviterait à écrire une donnée personnelle dans un champ dont la finalité est statistique.",
    );
    await s.cliquer('[data-action="annuler"]', "Il retire sa demande.");
    await s.page.waitForSelector('[data-suivi="CANCELLED"]', { timeout: 20000 });
    await s.conclure("Annulée avant toute intervention : rien n'est facturé, rien n'est engagé.");
  } finally {
    await ranger(acteurs);
  }
});
