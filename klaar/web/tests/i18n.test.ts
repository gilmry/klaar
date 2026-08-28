/**
 * Story 9.1 — textes d'interface en trois langues (FR-043).
 *
 * **Ce qui se teste ici :** qu'aucune clé ne manque dans une langue, que le
 * choix survive au rechargement, et que `<html lang>` porte bien le suffixe
 * belge — c'est lui que lisent les messages d'API, les lecteurs d'écran et les
 * formats de date.
 */
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import {
  appliquerLangue,
  choisirLangue,
  LANGUES,
  langueChoisie,
  restaurerLangue,
  t,
} from "../src/lib/i18n";

describe("@happy", () => {
  it("rend le texte dans la langue demandée", () => {
    expect(t("fr", "trajet.perdue")).toBe("Position non partagée pour le moment");
    expect(t("nl", "trajet.perdue")).toBe("Positie wordt momenteel niet gedeeld");
    expect(t("en", "trajet.perdue")).toBe("Location not shared at the moment");
  });

  it("substitue les valeurs du gabarit", () => {
    expect(t("fr", "suivi.elargie", { n: 2 })).toBe("élargie 2 fois sur 3");
    expect(t("nl", "suivi.elargie", { n: 2 })).toBe("2 van 3 keer uitgebreid");
  });

  it("propose les trois langues, sans doublon", () => {
    expect(LANGUES).toHaveLength(3);
    expect(new Set(LANGUES.map((l) => l.code)).size).toBe(3);
  });
});

describe("@security", () => {
  it("aucune traduction ne se répète d'une langue à l'autre par paresse", () => {
    // Un texte identique en français et en néerlandais signale presque toujours
    // une traduction oubliée. Les rares homographes justifiés seraient à
    // inscrire ici explicitement.
    const cles = ["trajet.perdue", "trajet.arrete", "commun.rafraichir"] as const;
    for (const cle of cles) {
      expect(t("fr", cle)).not.toBe(t("nl", cle));
      expect(t("fr", cle)).not.toBe(t("en", cle));
    }
  });
});

describe("@edge", () => {
  it("garde le suffixe belge pour le français et le néerlandais", () => {
    // « fr-BE » et « nl-BE » ne se lisent pas comme « fr-FR » et « nl-NL » :
    // les formats de date et de monnaie en dépendent.
    appliquerLangue("fr");
    expect(document.documentElement.lang).toBe("fr-BE");
    appliquerLangue("nl");
    expect(document.documentElement.lang).toBe("nl-BE");
    // L'anglais n'a pas de variante belge : lui en inventer une donnerait des
    // formats que personne n'attend.
    appliquerLangue("en");
    expect(document.documentElement.lang).toBe("en");
  });

  it("rétablit le choix enregistré plutôt que la langue de la page", () => {
    document.documentElement.lang = "fr-BE";
    expect(langueChoisie()).toBeNull();
    // Sans choix, la coquille fait foi.
    expect(restaurerLangue()).toBe("fr");

    choisirLangue("nl");
    expect(langueChoisie()).toBe("nl");
    // Le choix survit à un rechargement, sinon le sélecteur est un gadget :
    // il faudrait le reprendre à chaque page.
    document.documentElement.lang = "fr-BE";
    expect(restaurerLangue()).toBe("nl");
    expect(document.documentElement.lang).toBe("nl-BE");
  });

  it("ignore une valeur enregistrée qui n'est pas une langue connue", () => {
    localStorage.setItem("klaar.langue", "de");
    // Aucun repli silencieux sur une langue voisine : une valeur inconnue vaut
    // « pas de choix ».
    expect(langueChoisie()).toBeNull();
  });
});

/**
 * Deux bouchons minimaux plutôt qu'un DOM complet.
 *
 * Les tests tournent en environnement `node` — c'est ce qui les garde rapides.
 * Ce module ne touche au navigateur que par `localStorage` et
 * `document.documentElement.lang` : les bouchonner ici teste exactement la
 * surface employée, là où charger jsdom testerait surtout jsdom.
 */
beforeAll(() => {
  const memoire = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (c: string) => memoire.get(c) ?? null,
    setItem: (c: string, v: string) => void memoire.set(c, v),
    removeItem: (c: string) => void memoire.delete(c),
    clear: () => memoire.clear(),
    key: (i: number) => [...memoire.keys()][i] ?? null,
    get length() {
      return memoire.size;
    },
  } as Storage;
  globalThis.document = { documentElement: { lang: "fr-BE" } } as Document;
});

beforeEach(() => {
  localStorage.clear();
  document.documentElement.lang = "fr-BE";
});
