/**
 * Story 6.1 — refus de coordonnées côté navigateur (FR-032).
 *
 * Ce qui est testé ici est la lecture du refus et le ton du message : le
 * compteur du serveur doit être compris, et la personne qui vient de donner son
 * numéro doit lire une explication, pas une accusation.
 */
import { describe, expect, it } from "vitest";
import { ApiError } from "../src/lib/api";
import { messageRefus, refusCoordonnees } from "../src/lib/conversation";

function refus(corps: unknown): ApiError {
  return new ApiError(403, JSON.stringify(corps));
}

describe("@happy", () => {
  it("lit le compteur d'un refus pour coordonnées", () => {
    const lu = refusCoordonnees(
      refus({ code: "CONTACT_INFO_FORBIDDEN", tentatives: 2, signale: false }),
    );
    expect(lu).toEqual({ code: "CONTACT_INFO_FORBIDDEN", tentatives: 2, signale: false });
  });

  it("explique sans accuser à la première tentative", () => {
    const texte = messageRefus({ code: "CONTACT_INFO_FORBIDDEN", tentatives: 1, signale: false });
    expect(texte).toContain("ne s'échangent pas");
    // Le mot compte : la personne n'a pas forcément voulu contourner.
    expect(texte.toLowerCase()).not.toContain("tentative");
  });
});

describe("@negative", () => {
  it("rend null sur une autre erreur", () => {
    expect(refusCoordonnees(refus({ code: "MESSAGE_TOO_LONG" }))).toBeNull();
    expect(refusCoordonnees(new Error("réseau"))).toBeNull();
    expect(refusCoordonnees(null)).toBeNull();
  });

  it("rend null sur un corps illisible", () => {
    expect(refusCoordonnees(new ApiError(403, "pas du json"))).toBeNull();
  });
});

describe("@edge", () => {
  it("supporte un corps incomplet sans planter", () => {
    // Une passerelle peut réécrire la réponse : le client ne doit pas casser
    // pour autant.
    const lu = refusCoordonnees(refus({ code: "CONTACT_INFO_FORBIDDEN" }));
    expect(lu).toEqual({ code: "CONTACT_INFO_FORBIDDEN", tentatives: 0, signale: false });
  });
});

describe("@security", () => {
  it("dit ce qui est en jeu quand le compte est signalé", () => {
    // Découvrir la sanction sans jamais avoir été prévenu serait déloyal.
    const texte = messageRefus({ code: "CONTACT_INFO_FORBIDDEN", tentatives: 3, signale: true });
    expect(texte).toContain("Plusieurs tentatives");
    expect(texte).toContain("litige");
  });

  it("ne fait jamais confiance au champ signale d'un corps douteux", () => {
    // `signale` décide d'un message plus sévère : une valeur non booléenne ne
    // doit pas le déclencher.
    const lu = refusCoordonnees(
      refus({ code: "CONTACT_INFO_FORBIDDEN", tentatives: 1, signale: "oui" }),
    );
    expect(lu?.signale).toBe(false);
  });
});
