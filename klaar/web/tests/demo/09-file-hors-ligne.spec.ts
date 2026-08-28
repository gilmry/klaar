/**
 * Décrire son problème sans réseau, et le voir partir au retour.
 *
 * **C'est le cas d'usage central d'un service de dépannage** : la cave, le
 * parking, l'ascenseur. Faire retaper le formulaire au retour du réseau
 * reviendrait à punir quelqu'un pour un problème qui ne le concerne pas.
 */
import { test } from "@playwright/test";
import { COMPTES, ouvrirActeur, ranger, seConnecter, type Acteur } from "./scene";

/**
 * Au sud, mais à cinq cents mètres du parcours de la course.
 *
 * **La distance est le point.** Une Demande du même compte, dans le même
 * secteur et à moins de cent mètres, est tenue pour un doublon pendant cinq
 * minutes (FR-011) : le service rend la première au lieu d'en créer une
 * seconde. Deux parcours au même endroit se marcheraient dessus, et le second
 * échouerait sans que la cause soit visible à l'écran.
 */
const AU_SUD_EST = { latitude: 50.806, longitude: 4.345 };

test("Une demande écrite sans réseau part au retour de la connexion", async ({ browser }) => {
  const acteurs: Acteur[] = [];
  const acteur = await ouvrirActeur(browser, "Camille · demandeuse", "file-hors-ligne", AU_SUD_EST);
  acteurs.push(acteur);
  const s = acteur.scene;

  try {
    await s.aller("/", "Camille ouvre l'application.");
    await seConnecter(s, COMPTES.demandeur);
    // Le premier chargement précède la prise de contrôle du service worker :
    // recharger une fois est ce qui met la page de côté.
    await s.page.waitForFunction(
      async () => {
        const r = await navigator.serviceWorker.getRegistration("/");
        return Boolean(r?.active && navigator.serviceWorker.controller);
      },
      undefined,
      { timeout: 30000 },
    );
    await s.aller("/demande", "Elle ouvre le formulaire de demande.");
    await s.page.reload();
    await s.souffler();

    await s.raconter("Elle descend à la cave. Plus de réseau.");
    await s.contexteHorsLigne(true);
    await s.souffler();

    await s.choisir('[data-champ="secteur"]', "plomberie", "Elle décrit quand même son problème.");
    await s.saisir(
      '[data-champ="description"]',
      "Fuite au compteur d'eau, dans la cave.",
      "Elle écrit ce qu'elle voit.",
    );
    await s.cliquer('[data-action="envoyer-demande"]', "Et elle envoie.");
    await s.page.waitForSelector('[data-demande="en-file"]', { timeout: 20000 });
    await s.montrer(
      '[data-demande="en-file"]',
      "Rien n'est perdu : la demande est conservée sur l'appareil.",
    );
    await s.raconter(
      "Le service dit aussi ce qui n'a pas eu lieu : aucun prestataire n'a été prévenu pour l'instant.",
    );

    await s.raconter("Elle remonte. Le réseau revient.");
    await s.contexteHorsLigne(false);
    await s.souffler(2);
    await s.aller("/", "La demande part d'elle-même.");
    await s.page.waitForSelector('[data-etat="en-ligne"]', { timeout: 40000 });
    await s.montrer('[data-etat="en-ligne"]', "En ligne, et la file est vidée.");
    await s.conclure(
      "Écrire dans une cave et voir sa demande partir en remontant : c'est exactement ce qu'on attend d'un service de dépannage.",
    );
  } finally {
    await ranger(acteurs);
  }
});
