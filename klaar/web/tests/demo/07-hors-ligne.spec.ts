/**
 * Sans réseau.
 *
 * Klaar sert un dépannage : la connexion est mauvaise précisément quand on en a
 * besoin. Ce parcours coupe le réseau et montre que l'application reste
 * lisible plutôt que d'afficher la page d'erreur du navigateur.
 */
import { test } from "@playwright/test";
import { ouvrirActeur, ranger, type Acteur } from "./scene";

test("L'application sans réseau", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const acteur = await ouvrirActeur(browser, "Un visiteur hors réseau", "hors-ligne");
  acteurs.push(acteur);
  const s = acteur.scene;

  try {
    await s.aller("/", "L'application s'installe et met ses pages de côté.");
    await s.page.waitForFunction(
      async () => {
        const inscription = await navigator.serviceWorker.getRegistration("/");
        return Boolean(inscription?.active && navigator.serviceWorker.controller);
      },
      undefined,
      { timeout: 30000 },
    );
    await s.raconter("Le service worker s'installe. Il met de côté ce qui passe par lui.");
    // Le tout premier chargement d'une page se fait **avant** que le service
    // worker ne prenne la main : ses fichiers ne sont donc pas encore de côté.
    // Recharger une fois est ce que fait un visiteur qui revient, et c'est ce
    // qui rend la page disponible hors ligne.
    await s.page.reload();
    await s.souffler();

    await s.aller("/catalogue", "Il consulte le catalogue une première fois.");
    await s.page.waitForSelector("[data-secteur]", { timeout: 20000 });
    await s.page.reload();
    await s.souffler();

    await s.raconter("On coupe le réseau, comme dans une cave ou un parking.");
    await s.contexteHorsLigne(true);
    await s.souffler();

    await s.aller("/catalogue", "Il rouvre la page déjà visitée.");
    await s.page.waitForSelector("[data-secteur]", { timeout: 20000 });
    await s.montrer("[data-secteur]", "Elle s'affiche : elle avait été mise de côté.");

    // La pastille d'état vit sur l'accueil, seule page à la porter aujourd'hui.
    await s.aller("/", "L'accueil, lui, signale l'état de la connexion.");
    await s.montrer(
      '[data-etat="hors-ligne"]',
      "Hors ligne, et c'est dit — plutôt que laissé à deviner.",
    );

    await s.aller("/hors-ligne", "Et une adresse jamais visitée mène à une page qui l'explique.");
    await s.souffler();
    await s.conclure("Le service dit ce qu'il peut et ce qu'il ne peut pas. Il ne fait pas semblant.");
  } finally {
    await ranger(acteurs);
  }
});
