/**
 * Story 1.1 — traduction des codes d'inscription et appel HTTP.
 *
 * Ce qui est testé ici n'est pas cosmétique : un code non traduit produit une
 * page qui refuse sans dire pourquoi, et un message de succès trop bavard
 * défait côté interface l'anti-énumération que le backend construit.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, OfflineError } from "../src/lib/api";
import {
  codeDepuisErreur,
  inscrire,
  localeAffichee,
  LONGUEUR_MIN_MOT_DE_PASSE,
  messageErreur,
  messageSucces,
  type CodeErreurInscription,
  type LocaleKlaar,
} from "../src/lib/inscription";

const LOCALES: LocaleKlaar[] = ["fr", "nl", "en"];

const CODES: CodeErreurInscription[] = [
  "EMAIL_EMPTY",
  "EMAIL_MALFORMED",
  "PASSWORD_EMPTY",
  "PASSWORD_TOO_SHORT",
  "PASSWORD_TOO_LONG",
  "RATE_LIMIT_EXCEEDED",
  "SERVICE_UNAVAILABLE",
  "INCONNU",
  "HORS_LIGNE",
];

describe("@happy", () => {
  it("envoie la demande sur /auth/signup avec la locale", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ code: "SIGNUP_ACCEPTED" }), {
        status: 202,
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const reponse = await inscrire({
      email: "marie@example.eu",
      mot_de_passe: "Marie@2026Secure",
      locale: "nl",
    });

    expect(reponse.code).toBe("SIGNUP_ACCEPTED");
    const [url, options] = fetchMock.mock.calls[0];
    expect(String(url)).toContain("/auth/signup");
    expect(options.method).toBe("POST");
    expect(JSON.parse(options.body)).toEqual({
      email: "marie@example.eu",
      mot_de_passe: "Marie@2026Secure",
      locale: "nl",
    });
  });

  it("traduit chaque code dans chaque locale, sans texte manquant", () => {
    for (const locale of LOCALES) {
      for (const code of CODES) {
        const message = messageErreur(locale, code);
        expect(message.length, `${locale}/${code}`).toBeGreaterThan(0);
        expect(message, `${locale}/${code}`).not.toContain("undefined");
      }
    }
  });
});

describe("@negative", () => {
  it("extrait le code d'une réponse 400 de l'API", () => {
    const erreur = new ApiError(400, JSON.stringify({ code: "PASSWORD_TOO_SHORT" }));
    expect(codeDepuisErreur(erreur)).toBe("PASSWORD_TOO_SHORT");
  });

  it("reconnaît la limitation de débit", () => {
    const erreur = new ApiError(429, JSON.stringify({ code: "RATE_LIMIT_EXCEEDED" }));
    expect(messageErreur("fr", codeDepuisErreur(erreur))).toMatch(/heure/);
  });

  it("distingue une coupure réseau d'un refus du serveur", () => {
    expect(codeDepuisErreur(new OfflineError())).toBe("HORS_LIGNE");
    expect(messageErreur("fr", "HORS_LIGNE")).toMatch(/connexion/i);
  });
});

describe("@edge", () => {
  it("retombe sur un message générique quand le corps n'est pas du JSON", () => {
    // Une passerelle qui répond à la place de l'API renvoie du HTML. Afficher
    // ce HTML brut serait pire que de ne rien dire.
    const erreur = new ApiError(502, "<html><body>Bad Gateway</body></html>");
    expect(codeDepuisErreur(erreur)).toBe("SERVICE_UNAVAILABLE");
    expect(messageErreur("fr", codeDepuisErreur(erreur))).not.toContain("<html>");
  });

  it("retombe sur un message générique pour un code jamais vu", () => {
    const erreur = new ApiError(400, JSON.stringify({ code: "CODE_DU_FUTUR" }));
    expect(messageErreur("fr", codeDepuisErreur(erreur))).toBe(
      messageErreur("fr", "INCONNU"),
    );
  });

  it("suit la langue déclarée par la page, pas celle du navigateur", () => {
    // La régression que ce test empêche : un refus affiché en anglais au
    // milieu d'une page en français, parce que le Chromium de test annonce
    // `en-US`.
    stubDocument("fr-BE");
    stubNavigateur("en-US");
    expect(localeAffichee()).toBe("fr");

    stubDocument("nl-BE");
    expect(localeAffichee()).toBe("nl");
  });

  it("replie sur le navigateur quand la page ne déclare rien, puis sur le français", () => {
    stubDocument("");
    stubNavigateur("nl-BE");
    expect(localeAffichee()).toBe("nl");

    stubNavigateur("de-DE");
    expect(localeAffichee()).toBe("fr");
  });

  it("annonce la même longueur minimale que le domaine", () => {
    // NIST SP 800-63B, repris par FR-001. Si le backend durcit sans que cette
    // constante suive, l'interface promet un mot de passe que le serveur
    // refusera.
    expect(LONGUEUR_MIN_MOT_DE_PASSE).toBe(12);
    expect(messageErreur("fr", "PASSWORD_TOO_SHORT")).toContain("12");
  });
});

describe("@security", () => {
  it("le message de succès ne dit pas si un compte a été créé", () => {
    for (const locale of LOCALES) {
      const message = messageSucces(locale).toLowerCase();
      // Aucune formulation affirmative : « compte créé » ou « account created »
      // révélerait qu'aucun compte n'existait sur cette adresse.
      expect(message).not.toMatch(/compte créé|account created|account aangemaakt/);
      expect(message).toMatch(/si |als |if /);
    }
  });

  it("aucun message ne renvoie la saisie de l'utilisateur", () => {
    // Réafficher l'adresse ou le mot de passe soumis les ferait entrer dans le
    // DOM, donc dans les captures d'écran et les rapports d'erreur.
    for (const locale of LOCALES) {
      for (const code of CODES) {
        expect(messageErreur(locale, code)).not.toMatch(/@|mot de passe saisi/);
      }
    }
  });

  it("n'envoie jamais le mot de passe dans l'URL", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify({ code: "SIGNUP_ACCEPTED" }), { status: 202 }));
    vi.stubGlobal("fetch", fetchMock);
    await inscrire({ email: "marie@example.eu", mot_de_passe: "Marie@2026Secure" });
    expect(String(fetchMock.mock.calls[0][0])).not.toContain("Marie@2026Secure");
  });
});

/** Remplace `navigator.language` le temps d'une assertion. */
function stubNavigateur(langue: string) {
  vi.stubGlobal("navigator", { language: langue });
}

/** Remplace `<html lang>` le temps d'une assertion. */
function stubDocument(langue: string) {
  vi.stubGlobal("document", { documentElement: { lang: langue } });
}

beforeEach(() => {
  vi.unstubAllGlobals();
});
