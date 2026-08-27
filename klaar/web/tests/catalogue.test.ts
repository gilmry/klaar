/**
 * Story 2.3 — mise en forme des fourchettes de prix (FR-009).
 */
import { describe, expect, it } from "vitest";
import type { LocaleKlaar } from "../src/lib/inscription";
import { formaterFourchette, mentionPrix } from "../src/lib/catalogue";

const LOCALES: LocaleKlaar[] = ["fr", "nl", "en"];

describe("@happy", () => {
  it("met en forme une fourchette en euros", () => {
    const texte = formaterFourchette("fr", { min_cents: 8000, max_cents: 20000 });
    expect(texte).toMatch(/80/);
    expect(texte).toMatch(/200/);
  });

  it("annonce la mention obligatoire dans chaque langue", () => {
    // FR-009 `@happy` : sans elle, une fourchette indicative se lit comme un
    // devis, et l'écart avec le prix facturé devient un litige.
    for (const locale of LOCALES) {
      expect(mentionPrix(locale).length).toBeGreaterThan(0);
    }
    expect(mentionPrix("fr")).toMatch(/prestataire/i);
  });
});

describe("@negative", () => {
  it("dit « prix sur devis » plutôt que de laisser un blanc", () => {
    // Un blanc laisse croire à un défaut d'affichage, alors que l'absence de
    // fourchette est une information (FR-009 `@negative`).
    expect(formaterFourchette("fr", undefined)).toBe("Prix sur devis");
    expect(formaterFourchette("nl", undefined)).toBe("Prijs op aanvraag");
    expect(formaterFourchette("en", undefined)).toBe("Price on request");
  });
});

describe("@edge", () => {
  it("n'affiche pas de centimes trompeurs", () => {
    // « 80,00 € – 200,00 € » suggère une précision que la fourchette n'a pas :
    // c'est un agrégat sur un historique, pas un tarif.
    const texte = formaterFourchette("fr", { min_cents: 8000, max_cents: 20000 });
    expect(texte).not.toMatch(/,00/);
  });

  it("gère une fourchette plate", () => {
    const texte = formaterFourchette("fr", { min_cents: 10000, max_cents: 10000 });
    expect(texte.match(/100/g)?.length).toBe(2);
  });

  it("gère le zéro sans produire de texte vide", () => {
    expect(formaterFourchette("fr", { min_cents: 0, max_cents: 0 }).length).toBeGreaterThan(0);
  });
});

describe("@security", () => {
  it("aucune langue ne laisse la fourchette absente sans explication", () => {
    for (const locale of LOCALES) {
      const texte = formaterFourchette(locale, undefined);
      expect(texte.trim().length).toBeGreaterThan(0);
      expect(texte).not.toMatch(/undefined|NaN|null/);
    }
  });

  it("la mise en forme ne révèle jamais un montant qu'on ne lui a pas donné", () => {
    const texte = formaterFourchette("fr", { min_cents: 8000, max_cents: 20000 });
    expect(texte).not.toMatch(/\d{5,}/);
  });
});
