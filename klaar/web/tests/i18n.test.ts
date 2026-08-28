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
  CLES,
  etiquetteBcp47,
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
  /**
   * Homographes légitimes : un mot qui s'écrit pareil dans deux langues.
   *
   * Inscrits ici un par un plutôt que tolérés par une règle générale, pour
   * qu'ajouter une clé non traduite oblige à s'expliquer dans ce fichier.
   */
  const HOMOGRAPHES: Record<string, ("fr-nl" | "fr-en" | "nl-en")[]> = {
    "app.ville": ["fr-en"],
    "conversation.titre": ["fr-en", "nl-en"],
    "connexion.en_ligne": ["fr-nl", "fr-en", "nl-en"],
    "connexion.me_connecter": [],
    "demande.secteur": ["fr-en", "nl-en"],
    "demande.urgence": [],
    "pro.devis_accepte": [],
    "motif.autre": [],
    "dispo.enregistrer": [],
  };

  it("chaque clé a trois traductions réellement distinctes", () => {
    // Un texte identique d'une langue à l'autre signale presque toujours une
    // traduction oubliée. Le vérifier sur la table **entière** plutôt que sur
    // quelques clés choisies : c'est dans celles qu'on n'a pas regardées qu'un
    // oubli se cache.
    const suspects: string[] = [];
    for (const cle of CLES) {
      const [fr, nl, en] = [t("fr", cle), t("nl", cle), t("en", cle)];
      const permis = HOMOGRAPHES[cle] ?? [];
      if (fr === nl && !permis.includes("fr-nl")) suspects.push(`${cle} (fr = nl)`);
      if (fr === en && !permis.includes("fr-en")) suspects.push(`${cle} (fr = en)`);
      if (nl === en && !permis.includes("nl-en")) suspects.push(`${cle} (nl = en)`);
    }
    expect(suspects, `traductions probablement oubliées :\n${suspects.join("\n")}`).toEqual(
      [],
    );
  });

  it("aucune traduction n'est vide", () => {
    // Une chaîne vide compile mais laisse un bouton sans texte.
    for (const cle of CLES) {
      for (const l of ["fr", "nl", "en"] as const) {
        expect(t(l, cle).trim().length, `${cle} en ${l}`).toBeGreaterThan(0);
      }
    }
  });

  it("les gabarits portent les mêmes variables dans les trois langues", () => {
    // Une variable oubliée en néerlandais laisse un « {n} » à l'écran, ou pire,
    // fait disparaître le nombre sans que rien ne le signale.
    const variables = (texte: string) =>
      [...texte.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const cle of CLES) {
      const fr = variables(t("fr", cle));
      expect(variables(t("nl", cle)), `${cle} en nl`).toEqual(fr);
      expect(variables(t("en", cle)), `${cle} en en`).toEqual(fr);
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

  it("rend l'étiquette BCP 47 attendue par les formats", () => {
    expect(etiquetteBcp47("fr")).toBe("fr-BE");
    expect(etiquetteBcp47("nl")).toBe("nl-BE");
    expect(etiquetteBcp47("en")).toBe("en");
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
