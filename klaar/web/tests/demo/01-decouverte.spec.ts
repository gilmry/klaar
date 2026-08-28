/**
 * Ce qu'un visiteur voit avant d'avoir un compte.
 *
 * Le catalogue et ses fourchettes de prix sont publics, et c'est un choix :
 * demander de s'inscrire pour savoir combien coûte un dépannage reviendrait à
 * faire payer l'information en données personnelles.
 *
 * **Le parcours montre ce qui est là, pas ce qui devrait l'être.** Les
 * fourchettes se calculent sur un historique d'interventions réelles, et il
 * n'en existe pas encore : le catalogue affiche alors « prix sur devis ». Une
 * démonstration qui exigerait des prix pour se dérouler mentirait sur l'état du
 * service.
 */
import { test } from "@playwright/test";
import { ouvrirActeur, ranger, type Acteur } from "./scene";

test("Découvrir le service sans compte", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const visiteur = await ouvrirActeur(browser, "Un visiteur", "decouverte");
  acteurs.push(visiteur);
  const s = visiteur.scene;

  try {
    await s.aller("/", "Quelqu'un arrive sur Klaar, sans compte.");
    await s.montrer("h1", "La page dit ce que le service fait, et rien d'autre.");
    await s.montrer(
      "[data-etat]",
      "L'état de la connexion est affiché en permanence : Klaar sert un dépannage, et le réseau manque souvent quand on en a besoin.",
    );

    await s.cliquer('a[href="/catalogue"]', "Il veut savoir ce qu'on répare.");
    await s.page.waitForSelector("[data-secteur]", { timeout: 20000 });
    await s.montrer("[data-secteur]", "Les métiers couverts, avec pour chacun une indication de prix.");
    await s.raconter(
      "Ces informations sont publiques : demander un compte pour connaître un prix reviendrait à le faire payer en données personnelles.",
    );

    // Ce que le catalogue affiche dépend de ce qu'il sait : une fourchette
    // quand l'historique en permet une, « prix sur devis » sinon. Le parcours
    // commente ce qui est à l'écran plutôt que d'exiger l'un des deux.
    const surDevis = await s.page.locator('[data-prix="sur-devis"]').count();
    if (surDevis > 0) {
      await s.montrer(
        '[data-prix="sur-devis"]',
        "Ici, « prix sur devis » : les fourchettes se calculent sur des interventions réelles, et il n'y en a pas encore.",
      );
      await s.raconter(
        "Un blanc laisserait croire à un défaut d'affichage. L'absence de fourchette est une information, et elle est dite.",
      );
    } else {
      await s.montrer(
        "[data-mention-prix]",
        "La mention obligatoire accompagne chaque fourchette : c'est une indication, pas un devis.",
      );
    }

    await s.conclure("Tout cela se voit sans donner la moindre information sur soi.");
  } finally {
    await ranger(acteurs);
  }
});
