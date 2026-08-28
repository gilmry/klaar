/**
 * Le devis, des deux côtés.
 *
 * **Ce parcours existe pour montrer une chose que le code seul ne montre pas :
 * le prix vient du prestataire.** L'écran ne propose aucun montant, n'en
 * suggère aucun, n'en corrige aucun — c'est l'invariant §10.2 et la mitigation
 * de la loi belge du 26 avril 2024 sur le travail de plateforme. Un champ vide
 * filmé vaut mieux qu'un paragraphe qui l'affirme.
 *
 * Ce qu'il démontre au passage :
 *   - la TVA belge calculée et **conservée** : 180 € HTVA à 21 % font 217,80 € ;
 *   - le devis parvient au demandeur sans qu'il rafraîchisse ;
 *   - un second devis est refusé tant que le premier attend une réponse.
 */
import { test, expect } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, seConnecter, type Acteur } from "./scene";

/** À deux pas de l'électricien de Schaerbeek.
 *
 * Même raison qu'ailleurs : le classement retient les dix plus proches, et une
 * base de développement partagée avec la suite de tests contient des centaines
 * de prestataires posés au centre. Poser la Demande près de l'atelier le fait
 * arriver premier sans rien modifier aux données des autres.
 */
const PRES_DE_SCHAERBEEK = { latitude: 50.8676, longitude: 4.3737 };

test("Un devis envoyé, reçu, et le second qui ne passe pas", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const camilleActeur = await ouvrirActeur(
    browser,
    "Camille · demandeuse",
    "devis-demandeuse",
    PRES_DE_SCHAERBEEK,
  );
  const elecActeur = await ouvrirActeur(
    browser,
    "Élec Schaerbeek · prestataire",
    "devis-prestataire",
    PRES_DE_SCHAERBEEK,
  );
  acteurs.push(camilleActeur, elecActeur);
  const camille = camilleActeur.scene;
  const elec = elecActeur.scene;

  try {
    // --- Le prestataire se met en service ---------------------------------
    await elec.aller("/prestataire", "Un électricien ouvre son espace.");
    await seConnecter(elec, COMPTES.electricien);
    await elec.aller("/prestataire", "Il est en service.");

    // --- La demandeuse décrit son problème --------------------------------
    await camille.aller("/", "Camille n'a plus de courant dans la moitié de son appartement.");
    await seConnecter(camille, COMPTES.demandeur);
    await camille.aller("/demande", "Elle décrit ce qui se passe.");
    await camille.choisir('[data-champ="secteur"]', "electricite", "Elle choisit le secteur.");
    await camille.saisir(
      '[data-champ="description"]',
      "Plus de courant dans la cuisine et le salon depuis ce matin, le disjoncteur saute dès que je le remonte.",
      "Elle explique le symptôme, pas la panne : c'est au professionnel de la nommer.",
    );
    await camille.cliquer('[data-action="envoyer-demande"]', "Et elle envoie.");
    await camille.page.waitForSelector('[data-demande="creee"]', { timeout: 20000 });
    await camille.cliquer('[data-action="suivre"]', "Elle ouvre le suivi.");
    await camille.page.waitForSelector("[data-suivi-etat]", { timeout: 20000 });

    // --- Le prestataire prend l'intervention ------------------------------
    //
    // Peu de narration ici : la fenêtre de diffusion dure trente secondes.
    await elec.aller("/prestataire", "L'électricien voit arriver la Demande.");
    await elec.page.click('[data-action="rafraichir"]');
    await elec.page.waitForSelector('[data-demandes="liste"]', { timeout: 20000 });
    await elec.page.click('[data-action="accepter"]');
    await elec.page.waitForSelector("[data-mission-statut]", { timeout: 20000 });
    await elec.raconter("Il prend l'intervention, et obtient l'adresse.");

    // --- Il se déplace, puis chiffre --------------------------------------
    //
    // Le devis part **après** le diagnostic, et c'est le cas réel : un
    // électricien ne chiffre pas une panne qu'il n'a pas vue. Le service
    // l'autorise depuis tous les états non terminaux, exprès.
    await elec.cliquer('[data-vers="PROVIDER_EN_ROUTE"]', "Il part.");
    await elec.cliquer('[data-vers="ON_SITE"]', "Il arrive, ouvre le tableau, trouve la panne.");

    await elec.montrer(
      '[data-formulaire="devis"]',
      "Le formulaire de devis est vide. Klaar ne propose aucun montant.",
    );
    await elec.raconter(
      "Aucune valeur par défaut, aucun prix conseillé, aucun rappel de ce qu'il a facturé la dernière fois : c'est lui qui fixe son prix.",
    );

    await elec.saisir('input[name="montant"]', "180", "Il chiffre son intervention à 180 € hors TVA.");
    await elec.saisir('input[name="delai"]', "45", "Il annonce quarante-cinq minutes.");
    await elec.saisir(
      'input[name="note"]',
      "Remplacement du différentiel et remise en service",
      "Il dit ce qu'il va faire.",
    );
    await elec.montrer(
      "[data-devis-apercu]",
      "Le service lui montre ce que Camille verra : la TVA belge à 21 % s'ajoute à son prix, elle ne le remplace pas.",
    );
    await elec.cliquer('[data-action="envoyer-devis"]', "Il envoie.");
    await elec.page.waitForSelector("[data-devis-total]", { timeout: 20000 });
    await expect(elec.page.locator("[data-devis-total]")).toContainText("217,80");
    await elec.montrer("[data-devis-statut]", "Son devis attend une réponse pendant une heure.");

    // --- Camille le reçoit, sans rien rafraîchir --------------------------
    //
    // Depuis la Story 4.9, une socket relaie l'événement : l'écran de Camille
    // se met à jour en une seconde au lieu d'attendre le prochain sondage.
    await camille.raconter("Camille reçoit le devis sans rien faire.");
    await camille.page.waitForSelector("[data-devis-total]", { timeout: 30000 });
    await expect(camille.page.locator("[data-devis-total]")).toContainText("217,80", {
      timeout: 30000,
    });
    await camille.montrer(
      "[data-devis-total]",
      "217,80 € TTC : 180 € pour l'électricien, 37,80 € de TVA. Le détail est écrit, pas un total opaque.",
    );
    await camille.montrer("[data-devis-note]", "Et ce qu'il va faire, en une ligne.");

    // --- Un second devis ne passe pas -------------------------------------
    await elec.raconter("Il se dit qu'il ferait bien un second devis, plus cher.");
    await elec.montrer(
      '[data-devis="SENT"]',
      "Le formulaire a disparu : tant qu'un devis attend une réponse, il n'y en a pas d'autre.",
    );
    await elec.raconter(
      "Deux prix affichés en même temps laisseraient Camille sans savoir lequel l'engage.",
    );

    // --- L'intervention se termine ----------------------------------------
    await elec.cliquer('[data-vers="COMPLETED"]', "Le courant est revenu.");
    await elec.page.waitForSelector("[data-mission-close]", { timeout: 20000 });

    await camille.conclure(
      "Un prix annoncé avant les travaux, sa TVA détaillée, et personne d'autre que le professionnel pour le décider.",
    );
    await elec.conclure(
      "Trois devis au maximum par intervention : passé ce nombre, l'affaire se referme et le demandeur reprend la main.",
    );
  } finally {
    await ranger(acteurs);
  }
});
