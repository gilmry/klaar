/**
 * Navigation persistante : ce que le menu propose, et à qui.
 *
 * **Deux des cas ici ne portent pas sur du comportement mais sur de la
 * cohérence de dépôt** : qu'aucun lien ne pointe vers une page absente, et
 * qu'aucune page ne reste sans lien entrant. Ce sont précisément les deux
 * défauts qui ont motivé ce travail — `/ops` n'était atteignable par aucun lien
 * du site — et une relecture ne les rattrape qu'une fois. Un test les rattrape
 * à chaque page ajoutée.
 */
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { CLES } from "../src/lib/i18n";
import { LIEN_OPS, PAGES_SANS_LIEN_ENTRANT, liensPrincipaux } from "../src/lib/navigation";

const RACINE = fileURLToPath(new URL("..", import.meta.url));

/** Les pages réellement présentes, par leur chemin servi. */
function pagesDuDepot(): string[] {
  return readdirSync(`${RACINE}src/pages`)
    .filter((f) => f.endsWith(".astro"))
    .map((f) => f.replace(/\.astro$/, ""))
    .map((nom) => (nom === "index" ? "/" : `/${nom}`));
}

const COQUILLE = readFileSync(`${RACINE}src/layouts/AppLayout.astro`, "utf8");

describe("@happy", () => {
  it("propose l'accueil, la demande et le catalogue à tout le monde", () => {
    for (const connecte of [false, true]) {
      const href = liensPrincipaux(connecte).map((l) => l.href);
      expect(href).toContain("/");
      expect(href).toContain("/demande");
      expect(href).toContain("/catalogue");
    }
  });

  it("place « demander un dépannage » avant le compte", () => {
    // L'ordre n'est pas cosmétique : c'est la raison d'être du service, et
    // c'est ce qu'on cherche en premier sur un téléphone.
    const href = liensPrincipaux(false).map((l) => l.href);
    expect(href.indexOf("/demande")).toBeLessThan(href.indexOf("/inscription"));
  });

  it("n'utilise que des clés i18n existantes", () => {
    // Une clé absente rendrait `undefined` à l'écran plutôt qu'un libellé.
    const cles = [...liensPrincipaux(false), ...liensPrincipaux(true), LIEN_OPS].map((l) => l.cle);
    for (const cle of cles) expect(CLES).toContain(cle);
  });
});

describe("@negative", () => {
  it("ne propose ni « mon compte » ni l'espace prestataire connecté à un visiteur", () => {
    const visiteur = liensPrincipaux(false);
    expect(visiteur.map((l) => l.href)).not.toContain("/mon-compte");
    // La page prestataire reste atteignable — elle demandera la connexion —
    // mais sous un libellé qui dit qu'on vient s'informer, pas travailler.
    const prestataire = visiteur.find((l) => l.href === "/prestataire");
    expect(prestataire?.cle).toBe("nav.prestataire_visiteur");
  });

  it("ne propose plus de créer un compte à quelqu'un de connecté", () => {
    const href = liensPrincipaux(true).map((l) => l.href);
    expect(href).not.toContain("/inscription");
    expect(href).not.toContain("/connexion");
    expect(href).toContain("/mon-compte");
  });
});

describe("@edge", () => {
  it("ne pointe vers aucune page absente du dépôt", () => {
    const pages = pagesDuDepot();
    const liens = [...liensPrincipaux(false), ...liensPrincipaux(true), LIEN_OPS];
    for (const lien of liens) expect(pages).toContain(lien.href);
  });

  it("laisse zéro page sans lien entrant, hors pages de destination", () => {
    // Une page qu'aucun lien n'atteint n'existe pas pour qui ne connaît pas son
    // URL. Les deux exceptions sont documentées dans `navigation.ts` : la page
    // hors-ligne est servie par le service worker, la vérification d'adresse
    // est ouverte depuis un courriel.
    const atteignables = new Set<string>([
      ...liensPrincipaux(false).map((l) => l.href),
      ...liensPrincipaux(true).map((l) => l.href),
      LIEN_OPS.href,
      // Le pied de page, lu dans la coquille plutôt que recopié ici : si le
      // lien des mentions légales disparaissait, ce test le dirait.
      ...[...COQUILLE.matchAll(/href="(\/[a-z0-9-]*)"/g)].map((m) => m[1]),
    ]);

    const orphelines = pagesDuDepot().filter(
      (page) =>
        !atteignables.has(page) &&
        !PAGES_SANS_LIEN_ENTRANT.some((nom) => page === `/${nom}`),
    );
    expect(orphelines).toEqual([]);
  });

  it("garde la console d'exploitation hors de la navigation principale", () => {
    // Elle est atteignable par le pied de page. La mettre dans le menu en
    // ferait une rubrique du site pour un visiteur, ce qu'elle n'est pas.
    for (const connecte of [false, true]) {
      expect(liensPrincipaux(connecte).map((l) => l.href)).not.toContain("/ops");
    }
    expect(COQUILLE).toContain('href="/ops"');
  });
});

describe("@security", () => {
  it("ne laisse pas la console d'exploitation être indexée depuis le pied de page", () => {
    // `nofollow` n'est pas une protection — la protection est côté serveur, où
    // chaque route `/api/v1/ops/*` revérifie le jeton. C'est une hygiène : la
    // console n'a rien à faire dans un index public.
    expect(COQUILLE).toMatch(/href="\/ops"[^>]*rel="nofollow"|rel="nofollow"[^>]*href="\/ops"/);
  });
});
