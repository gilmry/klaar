/**
 * Story 4.1 — conversions et libellés d'un devis (FR-016).
 *
 * Ce qui est testé ici est ce qui touche à l'argent avant qu'il ne parte au
 * serveur : la saisie en euros vers des centimes entiers, et l'affichage en
 * sens inverse. Une erreur d'un facteur cent y est invisible à la relecture et
 * évidente à l'exécution.
 */
import { describe, expect, it } from "vitest";
import {
  centimesDepuisEuros,
  delaiLisible,
  libelleStatutDevis,
  montantLisible,
  type Devis,
} from "../src/lib/prestataire";
import {
  attendUneReponse,
  libelleDevis,
  MOTIFS_ANNULATION_MISSION,
  MOTIFS_LITIGE,
  MOTIFS_REFUS,
  peutAnnulerMission,
  peutContester,
  peutValider,
  type DevisRecu,
  type SuiviDemande,
} from "../src/lib/demande";

function devis(champs: Partial<Devis> = {}): Devis {
  return {
    id: "d-1",
    montant_htva_cents: 18_000,
    taux_tva_bp: 2100,
    tva_cents: 3_780,
    total_ttc_cents: 21_780,
    delai_minutes: 45,
    note: null,
    statut: "SENT",
    secondes_restantes: 3_600,
    echu: false,
    ...champs,
  };
}

describe("@happy", () => {
  it("convertit une saisie en euros en centimes entiers", () => {
    expect(centimesDepuisEuros("180")).toBe(18_000);
    expect(centimesDepuisEuros("180.50")).toBe(18_050);
    expect(centimesDepuisEuros(" 12 ")).toBe(1_200);
  });

  it("accepte la virgule décimale, qui est ce qu'on tape en Belgique", () => {
    expect(centimesDepuisEuros("180,50")).toBe(18_050);
  });

  it("affiche un montant en euros avec deux décimales", () => {
    expect(montantLisible(21_780)).toContain("217,80");
    expect(montantLisible(0)).toContain("0,00");
  });

  it("rend un délai en heures au-delà de soixante minutes", () => {
    expect(delaiLisible(45)).toBe("45 min");
    expect(delaiLisible(60)).toBe("1 h");
    expect(delaiLisible(135)).toBe("2 h 15");
  });
});

describe("@negative", () => {
  it("rend null sur une saisie qui n'est pas un montant", () => {
    // Sans cela, `NaN` partirait au serveur en `null` et produirait un 400 que
    // personne ne comprendrait.
    for (const saisie of ["", "   ", "abc", "12€", "--3", ".", "-", "1.2.3"]) {
      expect(centimesDepuisEuros(saisie), saisie).toBeNull();
    }
  });

  it("transmet un montant négatif plutôt que de le refuser en silence", () => {
    // Le refuser ici priverait l'utilisateur de l'explication : c'est le
    // serveur qui répond AMOUNT_NEGATIVE, et ce message s'affiche.
    expect(centimesDepuisEuros("-10")).toBe(-1_000);
  });
});

describe("@edge", () => {
  it("n'introduit pas de dérive flottante sur les centimes", () => {
    // `parseInt("18.7" * 100)` vaudrait 18 ; `18.7 * 100` vaut
    // 1869.9999999999998 en binaire. Les deux pièges d'un coup.
    expect(centimesDepuisEuros("18.70")).toBe(1_870);
    expect(centimesDepuisEuros("0.29")).toBe(29);
    expect(centimesDepuisEuros("1.005")).toBe(101);
    // Sans séparateur d'un côté ou de l'autre.
    expect(centimesDepuisEuros(".5")).toBe(50);
    expect(centimesDepuisEuros("7.")).toBe(700);
  });

  it("dit « expiré » sur un devis échu que le balayage n'a pas encore vu", () => {
    // Le statut stocké dit encore « envoyé ». Le croire ferait attendre une
    // réponse qui ne peut plus venir, des deux côtés.
    expect(libelleStatutDevis(devis({ statut: "SENT", echu: true }))).toBe(
      "Expiré sans réponse",
    );
    expect(libelleDevis({ ...devis({ statut: "SENT", echu: true }) } as DevisRecu)).toBe(
      "Ce devis a expiré sans réponse.",
    );
  });
});

describe("@security", () => {
  it("ne fabrique aucun montant quand la saisie est vide", () => {
    // L'invariant §10.2 côté écran : pas de valeur par défaut, pas de montant
    // « conseillé ». Une saisie vide ne doit produire aucun prix, jamais zéro.
    expect(centimesDepuisEuros("")).toBeNull();
  });

  it("rend le montant exact sur toute l'échelle admissible", () => {
    // Ce qui est saisi est ce qui part. Le jour où une grille tarifaire
    // s'invite dans la conversion, ce test tombe.
    const cas: Array<[string, number]> = [
      ["0.01", 1],
      ["49.99", 4_999],
      ["180.00", 18_000],
      ["9999.99", 999_999],
      ["10000.00", 1_000_000],
    ];
    for (const [saisie, cents] of cas) {
      expect(centimesDepuisEuros(saisie), saisie).toBe(cents);
    }
  });
});

describe("@happy réponse au devis", () => {
  function recu(champs: Partial<DevisRecu> = {}): DevisRecu {
    return { ...(devis() as unknown as DevisRecu), ...champs };
  }

  it("propose de répondre à un devis en attente", () => {
    expect(attendUneReponse(recu())).toBe(true);
  });
});

describe("@negative réponse au devis", () => {
  function recu(champs: Partial<DevisRecu> = {}): DevisRecu {
    return { ...(devis() as unknown as DevisRecu), ...champs };
  }

  it("ne propose rien sur un devis déjà répondu", () => {
    for (const statut of ["ACCEPTED", "REFUSED", "EXPIRED"] as const) {
      expect(attendUneReponse(recu({ statut })), statut).toBe(false);
    }
  });
});

describe("@edge réponse au devis", () => {
  function recu(champs: Partial<DevisRecu> = {}): DevisRecu {
    return { ...(devis() as unknown as DevisRecu), ...champs };
  }

  it("ne propose rien sur un devis échu que le balayage n'a pas vu", () => {
    // Le statut stocké dit encore « envoyé ». Offrir un bouton « j'accepte »
    // sur un devis mort ferait cliquer pour recevoir un 410.
    expect(attendUneReponse(recu({ echu: true }))).toBe(false);
  });
});

describe("@security réponse au devis", () => {
  it("garde un vocabulaire fermé pour le motif de refus", () => {
    // Un champ libre serait une invitation à écrire ce qu'on pense du
    // prestataire, dans une donnée qu'il pourrait lire un jour.
    const codes = MOTIFS_REFUS.map((m) => m.code);
    expect(codes).toEqual(["TOO_EXPENSIVE", "DELAY_TOO_LONG", "NO_LONGER_NEEDED", "OTHER"]);
  });
});

describe("@happy cycle de fin d'intervention", () => {
  function suivi(mission_statut: string | null): SuiviDemande {
    return { mission_statut } as SuiviDemande;
  }

  it("propose de valider une intervention déclarée terminée", () => {
    expect(peutValider(suivi("COMPLETED"))).toBe(true);
  });

  it("propose d'annuler tant que l'intervention est en cours", () => {
    for (const statut of ["ACCEPTED", "PROVIDER_EN_ROUTE", "ON_SITE"]) {
      expect(peutAnnulerMission(suivi(statut)), statut).toBe(true);
    }
  });
});

describe("@negative cycle de fin d'intervention", () => {
  function suivi(mission_statut: string | null): SuiviDemande {
    return { mission_statut } as SuiviDemande;
  }

  it("ne propose pas de valider ce qui n'est pas terminé", () => {
    for (const statut of ["ACCEPTED", "PROVIDER_EN_ROUTE", "ON_SITE", null]) {
      expect(peutValider(suivi(statut)), String(statut)).toBe(false);
    }
  });

  it("ne propose pas d'annuler une intervention faite", () => {
    // Elle se conteste, elle ne s'annule pas : offrir le bouton ferait cliquer
    // pour recevoir un refus.
    for (const statut of ["COMPLETED", "VALIDATED", "CANCELLED", null]) {
      expect(peutAnnulerMission(suivi(statut)), String(statut)).toBe(false);
    }
  });
});

describe("@security cycle de fin d'intervention", () => {
  it("ne propose pas de valider une intervention déjà validée", () => {
    // Deux validations feraient deux versements ; le service refuse, et l'écran
    // n'a pas à mener jusque-là.
    expect(peutValider({ mission_statut: "VALIDATED" } as SuiviDemande)).toBe(false);
  });

  it("garde un vocabulaire fermé pour le motif d'annulation", () => {
    const codes = MOTIFS_ANNULATION_MISSION.map((m) => m.code);
    expect(codes).toEqual(["NO_LONGER_NEEDED", "NO_ACCESS", "DISAGREEMENT", "OTHER"]);
  });
});

describe("@happy recours", () => {
  function suivi(mission_statut: string | null): SuiviDemande {
    return { mission_statut } as SuiviDemande;
  }

  it("propose de contester une intervention terminée ou validée", () => {
    // Une intervention faite ne s'annule pas, elle se conteste : le recours
    // doit rester ouvert après la validation, pas seulement avant.
    for (const statut of ["COMPLETED", "VALIDATED"]) {
      expect(peutContester(suivi(statut)), statut).toBe(true);
    }
  });
});

describe("@negative recours", () => {
  function suivi(mission_statut: string | null): SuiviDemande {
    return { mission_statut } as SuiviDemande;
  }

  it("ne propose pas de contester ce qui n'a pas eu lieu", () => {
    for (const statut of ["ACCEPTED", "PROVIDER_EN_ROUTE", "ON_SITE", "CANCELLED", null]) {
      expect(peutContester(suivi(statut)), String(statut)).toBe(false);
    }
  });
});

describe("@security recours", () => {
  it("n'offre au demandeur que les motifs qui le concernent", () => {
    // « Personne n'a ouvert » est le grief du prestataire : le proposer au
    // demandeur ferait cliquer pour recevoir un refus, et rendrait tout
    // comptage par motif ininterprétable.
    const codes = MOTIFS_LITIGE.map((m) => m.code);
    expect(codes).toEqual(["QUALITY", "NOT_DONE", "AMOUNT_DISPUTED", "OTHER"]);
    expect(codes).not.toContain("USER_NO_SHOW");
    expect(codes).not.toContain("IMPOSSIBLE_CONDITIONS");
  });
});
