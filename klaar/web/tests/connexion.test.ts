/**
 * Story 1.3 — appel de connexion, garde du jeton, traduction des codes.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, OfflineError } from "../src/lib/api";
import type { LocaleKlaar } from "../src/lib/inscription";
import {
  codeDepuisErreur,
  jetonAcces,
  messageErreur,
  oublierJeton,
  rafraichir,
  restaurerSession,
  seConnecter,
  seDeconnecter,
  type CodeErreurConnexion,
} from "../src/lib/connexion";

const LOCALES: LocaleKlaar[] = ["fr", "nl", "en"];
const CODES: CodeErreurConnexion[] = [
  "EMAIL_EMPTY",
  "EMAIL_MALFORMED",
  "PASSWORD_EMPTY",
  "PASSWORD_TOO_SHORT",
  "PASSWORD_TOO_LONG",
  "INVALID_CREDENTIALS",
  "ACCOUNT_NOT_VERIFIED",
  "ACCOUNT_LOCKED",
  "RATE_LIMIT_EXCEEDED",
  "REFRESH_MISSING",
  "REFRESH_INVALID",
  "REFRESH_EXPIRED",
  "REFRESH_REVOKED",
  "REFRESH_REUSED",
  "SERVICE_UNAVAILABLE",
  "INCONNU",
  "HORS_LIGNE",
];

function reponseOk(jeton = "jwt.de.test") {
  return vi.fn().mockResolvedValue(
    new Response(JSON.stringify({ jeton_acces: jeton, expire_dans: 3600 }), { status: 200 }),
  );
}

describe("@happy", () => {
  it("appelle /auth/login en POST et garde le jeton", async () => {
    const fetchMock = reponseOk();
    vi.stubGlobal("fetch", fetchMock);

    const session = await seConnecter({
      email: "marie@example.eu",
      mot_de_passe: "Marie@2026Secure",
    });

    expect(session.expire_dans).toBe(3600);
    expect(jetonAcces()).toBe("jwt.de.test");
    const [url, options] = fetchMock.mock.calls[0];
    expect(String(url)).toContain("/auth/login");
    expect(options.method).toBe("POST");
    // `credentials: include` : sans lui, le navigateur ignore le `Set-Cookie`
    // du refresh et la session ne survivrait à rien.
    expect(options.credentials).toBe("include");
  });

  it("traduit chaque code dans chaque locale, sans texte manquant", () => {
    for (const locale of LOCALES) {
      for (const code of CODES) {
        expect(messageErreur(locale, code).length, `${locale}/${code}`).toBeGreaterThan(0);
      }
    }
  });
});

describe("@negative", () => {
  it("ne garde aucun jeton quand la connexion échoue", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ code: "INVALID_CREDENTIALS" }), { status: 401 }),
      ),
    );
    await expect(
      seConnecter({ email: "marie@example.eu", mot_de_passe: "Marie@2026Secure" }),
    ).rejects.toBeInstanceOf(ApiError);
    expect(jetonAcces()).toBeNull();
  });

  it("distingue le compte non vérifié des identifiants faux", () => {
    expect(codeDepuisErreur(new ApiError(403, '{"code":"ACCOUNT_NOT_VERIFIED"}'))).toBe(
      "ACCOUNT_NOT_VERIFIED",
    );
    expect(messageErreur("fr", "ACCOUNT_NOT_VERIFIED")).toMatch(/courriel/i);
  });

  it("distingue le verrouillage de compte de la limitation par adresse", () => {
    // Deux protections différentes, deux messages différents : l'une se lève
    // en quinze minutes, l'autre en une heure, et l'utilisateur a besoin de
    // savoir laquelle le concerne.
    expect(codeDepuisErreur(new ApiError(423, '{"code":"ACCOUNT_LOCKED"}'))).toBe(
      "ACCOUNT_LOCKED",
    );
    expect(messageErreur("fr", "ACCOUNT_LOCKED")).not.toBe(
      messageErreur("fr", "RATE_LIMIT_EXCEEDED"),
    );
    expect(messageErreur("fr", "ACCOUNT_LOCKED")).toMatch(/quart d'heure/i);
  });

  it("reconnaît la limitation de débit et la coupure réseau", () => {
    expect(codeDepuisErreur(new ApiError(429, '{"code":"RATE_LIMIT_EXCEEDED"}'))).toBe(
      "RATE_LIMIT_EXCEEDED",
    );
    expect(codeDepuisErreur(new OfflineError())).toBe("HORS_LIGNE");
  });
});

describe("@edge", () => {
  it("retombe sur un message générique quand le corps n'est pas du JSON", () => {
    expect(codeDepuisErreur(new ApiError(502, "<html>Bad Gateway</html>"))).toBe(
      "SERVICE_UNAVAILABLE",
    );
  });

  it("retombe sur un message générique pour un code jamais vu", () => {
    expect(messageErreur("fr", "CODE_DU_FUTUR")).toBe(messageErreur("fr", "INCONNU"));
  });

  it("oublier le jeton le retire vraiment", async () => {
    vi.stubGlobal("fetch", reponseOk());
    await seConnecter({ email: "marie@example.eu", mot_de_passe: "Marie@2026Secure" });
    expect(jetonAcces()).not.toBeNull();
    oublierJeton();
    expect(jetonAcces()).toBeNull();
  });
});

describe("@security", () => {
  it("adresse inconnue et mot de passe faux donnent le même message", () => {
    // C'est ici que se joue l'anti-énumération côté interface : le backend
    // renvoie déjà le même code, l'interface ne doit pas le raffiner.
    for (const locale of LOCALES) {
      expect(messageErreur(locale, "INVALID_CREDENTIALS")).toBe(
        messageErreur(locale, "PASSWORD_TOO_SHORT"),
      );
    }
  });

  it("aucun message ne dit si l'adresse existe", () => {
    for (const locale of LOCALES) {
      const message = messageErreur(locale, "INVALID_CREDENTIALS").toLowerCase();
      expect(message).not.toMatch(/inconnu|unknown|onbekend|n'existe|does not exist/);
    }
  });

  it("le jeton ne part jamais dans l'URL", async () => {
    const fetchMock = reponseOk("JETON-RECONNAISSABLE");
    vi.stubGlobal("fetch", fetchMock);
    await seConnecter({ email: "marie@example.eu", mot_de_passe: "MotDePasseTresParticulier" });
    const [url, options] = fetchMock.mock.calls[0];
    expect(String(url)).not.toContain("MotDePasseTresParticulier");
    expect(String(url)).not.toContain("marie@example.eu");
    expect(options.method).toBe("POST");
  });
});

describe("@happy rotation", () => {
  it("échange le refresh sans envoyer de corps", async () => {
    // Le refresh voyage dans son cookie HttpOnly, que ce code ne peut pas lire.
    const fetchMock = reponseOk("jwt.rafraichi");
    vi.stubGlobal("fetch", fetchMock);

    await rafraichir();

    const [url, options] = fetchMock.mock.calls[0];
    expect(String(url)).toContain("/auth/refresh");
    expect(options.method).toBe("POST");
    expect(options.body).toBeUndefined();
    expect(options.credentials).toBe("include");
    expect(jetonAcces()).toBe("jwt.rafraichi");
  });

  it("reprend la session au chargement", async () => {
    vi.stubGlobal("fetch", reponseOk("jwt.repris"));
    await expect(restaurerSession()).resolves.toBe(true);
    expect(jetonAcces()).toBe("jwt.repris");
  });
});

describe("@negative rotation", () => {
  it("une reprise impossible ne lève pas d'erreur et ne garde rien", async () => {
    // Arriver sur une page sans être connecté est l'état normal d'un visiteur,
    // pas une anomalie à afficher.
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ code: "REFRESH_INVALID" }), { status: 401 }),
      ),
    );
    await expect(restaurerSession()).resolves.toBe(false);
    expect(jetonAcces()).toBeNull();
  });

  it("un rejeu détecté remonte comme tel, et son message explique pourquoi", () => {
    // Une déconnexion sans explication laisse croire à une panne. Ici, elle
    // signale un vol probable, ce que la personne a besoin de savoir.
    expect(codeDepuisErreur(new ApiError(401, '{"code":"REFRESH_REUSED"}'))).toBe(
      "REFRESH_REUSED",
    );
    expect(messageErreur("fr", "REFRESH_REUSED")).toMatch(/sécurité/i);
    expect(messageErreur("fr", "REFRESH_REUSED")).toMatch(/mot de passe/i);
  });
});

describe("@security déconnexion", () => {
  it("oublie le jeton même si l'appel au serveur échoue", async () => {
    // Laisser un jeton en mémoire après un clic sur « me déconnecter » serait
    // le pire des deux mondes.
    vi.stubGlobal("fetch", reponseOk("jwt.de.test"));
    await seConnecter({ email: "marie@example.eu", mot_de_passe: "Marie@2026Secure" });
    expect(jetonAcces()).not.toBeNull();

    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("réseau coupé")));
    await expect(seDeconnecter()).rejects.toBeInstanceOf(Error);
    expect(jetonAcces()).toBeNull();
  });

  it("appelle bien /auth/logout et oublie le jeton", async () => {
    vi.stubGlobal("fetch", reponseOk());
    await seConnecter({ email: "marie@example.eu", mot_de_passe: "Marie@2026Secure" });

    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    await seDeconnecter();

    expect(String(fetchMock.mock.calls[0][0])).toContain("/auth/logout");
    expect(jetonAcces()).toBeNull();
  });
});

beforeEach(() => {
  vi.unstubAllGlobals();
  oublierJeton();
});
