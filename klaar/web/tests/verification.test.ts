/**
 * Story 1.2 — lecture du jeton et traduction des codes de vérification.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, OfflineError } from "../src/lib/api";
import type { LocaleKlaar } from "../src/lib/inscription";
import {
  codeDepuisErreur,
  jetonDepuisUrl,
  messageErreur,
  messageSucces,
  verifier,
  type CodeErreurVerification,
} from "../src/lib/verification";

const LOCALES: LocaleKlaar[] = ["fr", "nl", "en"];
const CODES: CodeErreurVerification[] = [
  "TOKEN_MISSING",
  "TOKEN_INVALID",
  "TOKEN_EXPIRED",
  "SERVICE_UNAVAILABLE",
  "INCONNU",
  "HORS_LIGNE",
];

describe("@happy", () => {
  it("présente le jeton par POST sur /auth/verify-email", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify({ code: "EMAIL_VERIFIED" }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const reponse = await verifier("jeton-abc");

    expect(reponse.code).toBe("EMAIL_VERIFIED");
    const [url, options] = fetchMock.mock.calls[0];
    expect(String(url)).toContain("/auth/verify-email");
    // POST et non GET : un GET serait consommé par les analyseurs de liens des
    // messageries d'entreprise avant que le destinataire ne clique.
    expect(options.method).toBe("POST");
    expect(JSON.parse(options.body)).toEqual({ jeton: "jeton-abc" });
    expect(String(url)).not.toContain("jeton-abc");
  });

  it("lit le jeton dans la chaîne de requête", () => {
    expect(jetonDepuisUrl("https://klaar.be/verifier-email?jeton=abc123")).toBe("abc123");
  });

  it("traduit chaque code dans chaque locale, sans texte manquant", () => {
    for (const locale of LOCALES) {
      for (const code of CODES) {
        expect(messageErreur(locale, code).length, `${locale}/${code}`).toBeGreaterThan(0);
      }
      for (const code of ["EMAIL_VERIFIED", "EMAIL_ALREADY_VERIFIED"] as const) {
        expect(messageSucces(locale, code).length, `${locale}/${code}`).toBeGreaterThan(0);
      }
    }
  });
});

describe("@negative", () => {
  it("distingue un jeton expiré d'un jeton invalide", () => {
    expect(codeDepuisErreur(new ApiError(410, '{"code":"TOKEN_EXPIRED"}'))).toBe("TOKEN_EXPIRED");
    expect(codeDepuisErreur(new ApiError(404, '{"code":"TOKEN_INVALID"}'))).toBe("TOKEN_INVALID");
    // Le message d'expiration doit dire quoi faire, pas seulement constater.
    expect(messageErreur("fr", "TOKEN_EXPIRED")).toMatch(/inscription/i);
  });

  it("rend une chaîne vide quand l'URL ne porte pas de jeton", () => {
    expect(jetonDepuisUrl("https://klaar.be/verifier-email")).toBe("");
    expect(jetonDepuisUrl("https://klaar.be/verifier-email?jeton=")).toBe("");
    expect(jetonDepuisUrl("pas-une-url")).toBe("");
  });

  it("distingue une coupure réseau d'un refus", () => {
    expect(codeDepuisErreur(new OfflineError())).toBe("HORS_LIGNE");
  });
});

describe("@edge", () => {
  it("un second clic est présenté comme un succès, pas comme une erreur", () => {
    // Le backend répond 200 EMAIL_ALREADY_VERIFIED. L'interface doit dire la
    // même chose que la première fois : le compte est actif.
    for (const locale of LOCALES) {
      expect(messageSucces(locale, "EMAIL_ALREADY_VERIFIED")).toMatch(
        /actif|actief|active/i,
      );
    }
  });

  it("ignore les espaces autour du jeton collé à la main", () => {
    expect(jetonDepuisUrl("https://klaar.be/verifier-email?jeton=%20abc%20")).toBe("abc");
  });

  it("retombe sur un message générique quand le corps n'est pas du JSON", () => {
    expect(codeDepuisErreur(new ApiError(502, "<html>Bad Gateway</html>"))).toBe(
      "SERVICE_UNAVAILABLE",
    );
  });

  it("retombe sur un message générique pour un code jamais vu", () => {
    expect(messageErreur("fr", "CODE_DU_FUTUR")).toBe(messageErreur("fr", "INCONNU"));
  });
});

describe("@security", () => {
  it("aucun message ne réaffiche le jeton", () => {
    for (const locale of LOCALES) {
      for (const code of CODES) {
        expect(messageErreur(locale, code)).not.toMatch(/jeton=|token=/);
      }
    }
  });

  it("le jeton ne part jamais dans l'URL", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify({ code: "EMAIL_VERIFIED" }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    await verifier("SECRET-TRES-RECONNAISSABLE");
    expect(String(fetchMock.mock.calls[0][0])).not.toContain("SECRET-TRES-RECONNAISSABLE");
  });
});

beforeEach(() => {
  vi.unstubAllGlobals();
});
