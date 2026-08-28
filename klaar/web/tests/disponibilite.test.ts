/**
 * Story 3.7 — disponibilité du prestataire.
 *
 * L'essentiel ici est `raisonDeSilence` : c'est la fonction qui répond à la
 * seule question que le prestataire se pose quand rien n'arrive.
 */
import { describe, expect, it } from "vitest";
import { ApiError, OfflineError } from "../src/lib/api";
import type { LocaleKlaar } from "../src/lib/inscription";
import {
  codeDepuisErreur,
  messageErreur,
  raisonDeSilence,
  RAYON_MAX_METRES,
  RAYON_MIN_METRES,
  type Disponibilite,
} from "../src/lib/disponibilite";

const LOCALES: LocaleKlaar[] = ["fr", "nl", "en"];

function etat(surcharge: Partial<Disponibilite> = {}): Disponibilite {
  return {
    provider_id: "11111111-1111-4111-8111-111111111111",
    statut: "ACTIVE",
    disponible: true,
    rayon_intervention_metres: RAYON_MAX_METRES,
    occupe: false,
    sollicitable: true,
    ...surcharge,
  };
}

describe("@happy", () => {
  it("ne dit rien quand le prestataire reçoit bien des Demandes", () => {
    expect(raisonDeSilence(etat())).toBeNull();
  });

  it("traduit chaque refus dans les trois langues", () => {
    for (const locale of LOCALES) {
      for (const code of ["NOT_A_PROVIDER", "SERVICE_RADIUS_OUT_OF_RANGE"]) {
        expect(messageErreur(locale, code).length).toBeGreaterThan(0);
      }
    }
  });

  it("nomme les bornes du rayon dans le message de refus", () => {
    // Un « rayon invalide » sans borne oblige à deviner : le message dit les
    // deux chiffres.
    const texte = messageErreur("fr", "SERVICE_RADIUS_OUT_OF_RANGE");
    expect(texte).toContain(String(RAYON_MIN_METRES / 1000));
    expect(texte).toContain(String(RAYON_MAX_METRES / 1000));
  });
});

describe("@negative", () => {
  it("explique la pause plutôt que de laisser un silence", () => {
    const raison = raisonDeSilence(etat({ disponible: false, sollicitable: false }));
    expect(raison).toMatch(/pause/i);
  });

  it("explique une intervention en cours sans parler de pause", () => {
    // Confondre les deux ferait chercher un interrupteur qui n'y est pour rien.
    const raison = raisonDeSilence(etat({ occupe: true, sollicitable: false }));
    expect(raison).toMatch(/intervention/i);
    expect(raison).not.toMatch(/pause/i);
  });

  it("explique l'attente de contrôle", () => {
    const raison = raisonDeSilence(
      etat({ statut: "PENDING_KYC", disponible: false, sollicitable: false }),
    );
    expect(raison).toMatch(/contrôle/i);
  });

  it("dit qu'une reprise de service ne lève pas une suspension", () => {
    // Sinon le prestataire suspendu bascule l'interrupteur en boucle sans
    // comprendre pourquoi rien ne change.
    const raison = raisonDeSilence(etat({ statut: "SUSPENDED", sollicitable: false }));
    expect(raison).toMatch(/suspendu/i);
    expect(raison).toMatch(/ne le réactive pas/i);
  });
});

describe("@edge", () => {
  it("donne la priorité au statut sur la pause", () => {
    // Un prestataire suspendu **et** en pause a d'abord un problème de statut :
    // lui parler de sa pause l'enverrait appuyer sur un bouton sans effet.
    const raison = raisonDeSilence(
      etat({ statut: "SUSPENDED", disponible: false, sollicitable: false }),
    );
    expect(raison).toMatch(/suspendu/i);
  });

  it("retombe sur un message générique pour un code inconnu", () => {
    expect(messageErreur("fr", "CODE_QUI_N_EXISTE_PAS").length).toBeGreaterThan(0);
  });

  it("distingue une coupure réseau d'un refus du serveur", () => {
    expect(codeDepuisErreur(new OfflineError())).toBe("HORS_LIGNE");
    expect(codeDepuisErreur(new ApiError(503, "{}"))).toBe("SERVICE_UNAVAILABLE");
    expect(codeDepuisErreur(new ApiError(403, '{"code":"NOT_A_PROVIDER"}'))).toBe(
      "NOT_A_PROVIDER",
    );
  });
});

describe("@security", () => {
  it("n'expose jamais le corps brut d'une réponse d'erreur", () => {
    // Une passerelle peut renvoyer une page HTML entière ; l'afficher telle
    // quelle mettrait des détails d'infrastructure sous les yeux de
    // l'utilisateur.
    const brut = "<html><body>upstream 10.0.0.4 timed out</body></html>";
    const message = messageErreur("fr", codeDepuisErreur(new ApiError(502, brut)));
    expect(message).not.toContain("10.0.0.4");
    expect(message).not.toContain("upstream");
  });

  it("ne prend pour code que ce que l'API a réellement écrit", () => {
    // Un corps sans champ `code` ne doit pas se transformer en code inventé.
    expect(codeDepuisErreur(new ApiError(400, '{"message":"nope"}'))).toBe("INCONNU");
    expect(codeDepuisErreur(new ApiError(400, "pas du json"))).toBe("INCONNU");
  });
});
