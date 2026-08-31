/**
 * Socle des parcours filmés (documentation vivante).
 *
 * **Pourquoi une suite à part.** Les tests de `tests/e2e` vérifient : ils vont
 * vite, simulent l'API et n'ont pas à être regardés. Ceux-ci **montrent** : ils
 * tournent contre le vrai service, à vitesse humaine, et leur produit est une
 * vidéo. Ralentir la suite de vérification pour la filmer aurait donné une
 * barrière lente et des vidéos illisibles ; les séparer laisse chacune faire
 * son métier.
 *
 * **Une seconde entre chaque geste, au minimum.** En dessous, l'œil ne suit
 * pas : un formulaire se remplit et se soumet dans le même quart de seconde, et
 * la vidéo ne prouve rien à qui la regarde. La constante est un plancher, pas
 * une cible — les étapes qui demandent de lire tiennent plus longtemps.
 *
 * **La narration est incrustée dans la page.** Sans elle, la vidéo montre des
 * clics sans dire ce qu'ils démontrent. Le bandeau porte aussi le nom de
 * l'acteur, ce qui est indispensable aux parcours à deux navigateurs : deux
 * enregistrements côte à côte sans étiquette sont indéchiffrables.
 */
import { test, type Browser, type BrowserContext, type Locator, type Page } from "@playwright/test";
import { rename } from "node:fs/promises";
import { join, dirname } from "node:path";

/** Plancher entre deux gestes, en millisecondes. */
export const RYTHME_MS = 1000;

/** Temps de lecture d'une narration, proportionnel à sa longueur. */
function tempsDeLecture(texte: string): number {
  // Environ quinze caractères par seconde, borné entre une et cinq secondes.
  // Une phrase de dix mots ne se lit pas en une seconde, et aucune ne mérite
  // d'immobiliser la vidéo dix secondes.
  return Math.min(5000, Math.max(RYTHME_MS, texte.length * 66));
}

const STYLE = `
  #klaar-narration {
    position: fixed; inset: auto 0 0 0; z-index: 2147483647;
    background: #12303f; color: #fff;
    font: 500 17px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif;
    padding: 14px 18px; box-shadow: 0 -6px 24px rgba(0,0,0,.25);
    display: flex; gap: 14px; align-items: baseline;
  }
  #klaar-narration b {
    background: #ffd166; color: #12303f; border-radius: 999px;
    padding: 3px 12px; font-weight: 700; white-space: nowrap;
  }
  .klaar-projecteur {
    outline: 3px solid #ffd166 !important;
    outline-offset: 3px !important;
    border-radius: 6px;
  }
`;

/**
 * Un acteur devant son navigateur.
 *
 * Toutes les méthodes narrent puis agissent, et laissent le temps de voir. Les
 * appels directs à `page` restent possibles pour ce qui ne se filme pas — une
 * assertion, une lecture.
 */
export class Scene {
  constructor(
    readonly page: Page,
    private readonly acteur: string,
  ) {}

  /** Affiche une phrase et laisse le temps de la lire. */
  async raconter(texte: string): Promise<void> {
    await this.page.evaluate(
      ({ texte, acteur, style }) => {
        let bandeau = document.getElementById("klaar-narration");
        if (!bandeau) {
          const feuille = document.createElement("style");
          feuille.textContent = style;
          document.head.append(feuille);
          bandeau = document.createElement("div");
          bandeau.id = "klaar-narration";
          document.body.append(bandeau);
        }
        bandeau.innerHTML = "";
        const etiquette = document.createElement("b");
        etiquette.textContent = acteur;
        const phrase = document.createElement("span");
        phrase.textContent = texte;
        bandeau.append(etiquette, phrase);
      },
      { texte, acteur: this.acteur, style: STYLE },
    );
    await this.page.waitForTimeout(tempsDeLecture(texte));
  }

  /** Ouvre une adresse, puis réaffiche la narration que la navigation a effacée. */
  async aller(chemin: string, texte?: string): Promise<void> {
    if (texte) await this.raconter(texte);
    await this.page.goto(chemin);
    // Le bandeau vit dans le document : une navigation l'emporte. Le remettre
    // évite une vidéo qui perd sa narration à chaque page.
    if (texte) await this.reafficher(texte);
    await this.souffler();
  }

  private async reafficher(texte: string): Promise<void> {
    await this.raconter(texte);
  }

  /** Encadre un élément pour que l'œil sache où regarder. */
  async montrer(selecteur: string, texte?: string): Promise<Locator> {
    if (texte) await this.raconter(texte);
    const cible = this.page.locator(selecteur).first();
    await cible.scrollIntoViewIfNeeded();
    await cible.evaluate((n) => n.classList.add("klaar-projecteur"));
    await this.souffler();
    return cible;
  }

  async cliquer(selecteur: string, texte?: string): Promise<void> {
    const cible = await this.montrer(selecteur, texte);
    await cible.click();
    await this.souffler();
  }

  /** Saisit du texte caractère par caractère, comme quelqu'un qui tape. */
  async saisir(selecteur: string, valeur: string, texte?: string): Promise<void> {
    const cible = await this.montrer(selecteur, texte);
    await cible.fill("");
    // `pressSequentially` plutôt que `fill` : un champ qui se remplit d'un coup
    // ne se voit pas, et masquerait une validation qui réagit à la frappe.
    await cible.pressSequentially(valeur, { delay: 35 });
    await this.souffler();
  }

  async choisir(selecteur: string, valeur: string, texte?: string): Promise<void> {
    const cible = await this.montrer(selecteur, texte);
    await cible.selectOption(valeur);
    await this.souffler();
  }

  /** Coupe ou rétablit le réseau du contexte. */
  async contexteHorsLigne(coupe: boolean): Promise<void> {
    await this.page.context().setOffline(coupe);
  }

  /** Le plancher de rythme. */
  async souffler(facteur = 1): Promise<void> {
    await this.page.waitForTimeout(RYTHME_MS * facteur);
  }

  /** Marque la fin d'un parcours, pour que la vidéo ne coupe pas net. */
  async conclure(texte: string): Promise<void> {
    await this.raconter(texte);
    await this.souffler(2);
  }
}

/**
 * Position au centre de la Région, injectée dans le navigateur.
 *
 * Les parcours ne peuvent pas cliquer « autoriser » dans une boîte de dialogue
 * système : la permission se donne au contexte, et la position se fixe. C'est
 * une différence avec un usage réel, et elle est écrite ici plutôt que
 * découverte en regardant la vidéo.
 */
export const GRAND_PLACE = { latitude: 50.8467, longitude: 4.3525 };

/** Comptes du jeu de démonstration, créés par `klaar-prestataires-demo`. */
export const COMPTES = {
  demandeur: "camille@demo.klaar.invalid",
  secondDemandeur: "sacha@demo.klaar.invalid",
  motDePasse: "demonstration-klaar-2026",
  plombier: "plomberie-centre@demo.klaar.invalid",
  serrurier: "serrurerie-midi@demo.klaar.invalid",
  electricien: "elec-schaerbeek@demo.klaar.invalid",
  multiservices: "multi-anderlecht@demo.klaar.invalid",
  plombierSudA: "plomberie-sud@demo.klaar.invalid",
  plombierSudB: "depannage-sud@demo.klaar.invalid",
} as const;

/** Ouvre une session par le formulaire, comme un vrai visiteur. */
/**
 * Un acteur : un contexte de navigateur, sa vidéo, sa narration.
 *
 * Chaque acteur a **son** enregistrement. Playwright filme par contexte, et un
 * parcours à deux personnes ne peut pas tenir dans une seule image : ce qui
 * compte est ce que chacun voit pendant que l'autre agit.
 */
export interface Acteur {
  scene: Scene;
  contexte: BrowserContext;
  /** Nom du fichier vidéo, une fois le contexte fermé. */
  fichier: string;
}

/** Ouvre un navigateur pour un acteur, prêt à être filmé. */
export async function ouvrirActeur(
  browser: Browser,
  nom: string,
  fichier: string,
  position: { latitude: number; longitude: number } = GRAND_PLACE,
): Promise<Acteur> {
  const contexte = await browser.newContext({
    recordVideo: { dir: "demo-resultats", size: { width: 1280, height: 720 } },
    // **`notifications` en plus de `geolocation`.** Sans elle, Chrome piloté
    // refuse d'office et l'accueil filmé annonçait « Les notifications sont
    // bloquées pour ce site » — un état qu'un visiteur qui arrive pour la
    // première fois n'a pas. Accordée, l'écran montre l'invitation à les
    // activer, c'est-à-dire ce que ce visiteur voit vraiment. Rien n'est
    // souscrit pour autant : l'abonnement demande un clic, et aucun parcours
    // ne le fait.
    permissions: ["geolocation", "notifications"],
    geolocation: position,
    locale: "fr-BE",
    timezoneId: "Europe/Brussels",
    viewport: { width: 1280, height: 720 },
  });
  return { scene: new Scene(await contexte.newPage(), nom), contexte, fichier };
}

/**
 * Ferme les navigateurs et range les vidéos sous un nom lisible.
 *
 * Playwright nomme ses enregistrements d'après un condensé : deux fichiers
 * indiscernables pour un parcours à deux acteurs. Les renommer est ce qui
 * permet ensuite de les publier côte à côte en disant qui est qui. Ils sont
 * aussi attachés au rapport, où ils deviennent lisibles sans quitter la page.
 */
export async function ranger(acteurs: Acteur[]): Promise<void> {
  for (const acteur of acteurs) {
    const video = acteur.scene.page.video();
    // Le fichier n'est écrit qu'à la fermeture du contexte.
    await acteur.contexte.close();
    if (!video) continue;
    const origine = await video.path();
    const cible = join(dirname(origine), `${acteur.fichier}.webm`);
    await rename(origine, cible);

    // Attachée au rapport **seulement en cas d'échec**.
    //
    // Un parcours vert voit sa vidéo publiée sur la page d'accueil de la
    // vitrine ; l'attacher en plus au rapport la stockait deux fois et faisait
    // passer le site de cinquante mégaoctets à trois cents. Sur un échec, en
    // revanche, c'est dans le rapport qu'on regarde, à côté de la trace.
    if (test.info().errors.length > 0) {
      await test.info().attach(acteur.fichier, { path: cible, contentType: "video/webm" });
    }
  }
}

/** Ouvre une session par le formulaire, comme le ferait un visiteur. */
export async function seConnecter(scene: Scene, email: string): Promise<void> {
  await scene.aller("/connexion", "Il ouvre la page de connexion.");
  await scene.saisir("#connexion-email", email, "Il saisit son adresse.");
  await scene.saisir("#connexion-mot-de-passe", COMPTES.motDePasse, "Puis son mot de passe.");
  await scene.cliquer('[data-action="connecter"]', "Et il se connecte.");
  await scene.page.waitForSelector("[data-succes-connexion]", { timeout: 15000 });
  await scene.souffler();
}
