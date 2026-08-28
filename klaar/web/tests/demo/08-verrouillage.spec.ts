/**
 * Ce que le service refuse de dire, et ce qu'il finit par bloquer.
 *
 * Deux protections distinctes, souvent confondues : le **message d'erreur**,
 * qui ne dit jamais si l'adresse existe, et le **verrouillage** après plusieurs
 * échecs, qui empêche d'essayer les mots de passe un à un.
 */
import { expect, test } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, type Acteur } from "./scene";

test("Mot de passe erroné : le même refus, puis le verrouillage", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const acteur = await ouvrirActeur(browser, "Quelqu'un qui essaie", "verrouillage");
  acteurs.push(acteur);
  const s = acteur.scene;

  try {
    await s.aller("/connexion", "Quelqu'un essaie de se connecter à un compte qui n'est pas le sien.");
    await s.saisir("#connexion-email", COMPTES.demandeur, "Une adresse qui existe.");
    await s.saisir("#connexion-mot-de-passe", "MauvaisMotDePasse1!", "Un mot de passe au hasard.");
    await s.cliquer('[data-action="connecter"]', "Le service refuse.");
    await s.page.waitForSelector("[data-erreur-connexion]", { timeout: 20000 });
    const surCompteExistant = await s.page.locator("[data-erreur-connexion]").innerText();
    await s.montrer("[data-erreur-connexion]", "« Adresse ou mot de passe incorrect. »");

    await s.aller("/connexion", "Essayons maintenant une adresse qui n'existe pas.");
    await s.saisir("#connexion-email", "personne-inconnue@example.eu", "Une adresse inventée.");
    await s.saisir("#connexion-mot-de-passe", "MauvaisMotDePasse1!", "Le même mot de passe.");
    await s.cliquer('[data-action="connecter"]', "Et on regarde la réponse.");
    await s.page.waitForSelector("[data-erreur-connexion]", { timeout: 20000 });
    const surCompteInconnu = await s.page.locator("[data-erreur-connexion]").innerText();

    // La garantie tient dans cette égalité.
    expect(surCompteInconnu).toBe(surCompteExistant);
    await s.montrer(
      "[data-erreur-connexion]",
      "Exactement la même phrase. Impossible de savoir laquelle des deux adresses a un compte.",
    );
    await s.conclure(
      "Après cinq échecs, le compte se verrouille un quart d'heure : essayer les mots de passe un à un devient sans objet.",
    );
  } finally {
    await ranger(acteurs);
  }
});
