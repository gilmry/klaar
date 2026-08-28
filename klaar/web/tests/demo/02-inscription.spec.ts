/**
 * S'inscrire, et ce que le service refuse de dire.
 *
 * Le passage qui compte est le dernier : réinscrire une adresse déjà prise
 * donne **exactement la même réponse** que la première fois. C'est ce qui
 * empêche de découvrir qui a un compte en essayant des adresses, et ça ne se
 * voit qu'en le montrant deux fois de suite.
 */
import { expect, test } from "@playwright/test";
import { ouvrirActeur, ranger, type Acteur } from "./scene";

test("S'inscrire, et l'adresse qu'on ne confirme jamais", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const visiteur = await ouvrirActeur(browser, "Un nouveau venu", "inscription");
  acteurs.push(visiteur);
  const s = visiteur.scene;
  // Une adresse neuve à chaque enregistrement : rejouer le parcours ne doit pas
  // buter sur l'inscription de la veille.
  const adresse = `visiteur-${Date.now()}@example.eu`;

  try {
    await s.aller("/inscription", "Il crée un compte.");
    await s.saisir("#inscription-email", adresse, "Son adresse.");
    await s.saisir("#inscription-mot-de-passe", "court", "Un mot de passe trop court.");
    await s.cliquer('[data-action="inscrire"]', "Le service refuse, et dit pourquoi.");
    await s.montrer("[data-erreur-inscription]", "La règle est annoncée, pas devinée.");

    await s.saisir(
      "#inscription-mot-de-passe",
      "Marie@2026Secure",
      "Il choisit un mot de passe solide.",
    );
    await s.cliquer('[data-action="inscrire"]', "Et il s'inscrit.");
    await s.page.waitForSelector("[data-succes-inscription]", { timeout: 20000 });
    const premiere = await s.page.locator("[data-succes-inscription]").innerText();
    await s.montrer(
      "[data-succes-inscription]",
      "Le service dit qu'un courriel part, sans confirmer que l'adresse était libre.",
    );

    await s.aller("/inscription", "Essayons la même adresse une seconde fois.");
    await s.saisir("#inscription-email", adresse, "La même adresse, déjà prise.");
    await s.saisir("#inscription-mot-de-passe", "Marie@2026Secure", "Un mot de passe quelconque.");
    await s.cliquer('[data-action="inscrire"]', "Et on regarde ce que le service répond.");
    await s.page.waitForSelector("[data-succes-inscription]", { timeout: 20000 });
    const seconde = await s.page.locator("[data-succes-inscription]").innerText();

    // La garantie tient dans cette égalité, et elle mérite d'être vérifiée.
    expect(seconde).toBe(premiere);
    await s.montrer(
      "[data-succes-inscription]",
      "Mot pour mot la même réponse. Impossible de savoir si l'adresse avait déjà un compte.",
    );
    await s.conclure(
      "C'est ce qui empêche de dresser la liste des personnes inscrites en essayant des adresses.",
    );
  } finally {
    await ranger(acteurs);
  }
});
