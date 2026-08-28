/**
 * Story 4.4 — suivi géolocalisé du trajet (FR-019), côté navigateur.
 *
 * **Ce qui se teste ici et nulle part ailleurs :** que le front n'arrondisse
 * rien lui-même, qu'il n'invente pas de mise en file hors-ligne pour une
 * position, et que le vocabulaire de l'écran ne transforme pas un droit exercé
 * en panne.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { OfflineError } from "../src/lib/api";
import { libelleTrajet, suivreTrajet, type TrajetSuivi } from "../src/lib/demande";
import { consentirSuivi, envoyerPosition } from "../src/lib/prestataire";

function reponse(corps: unknown, statut = 200): Response {
  return new Response(JSON.stringify(corps), { status: statut });
}

const TRAJET: TrajetSuivi = {
  etat: "EN_ROUTE",
  position: { lat: 50.8465, lon: 4.3521 },
  relevee_le: "2026-08-27T10:00:00Z",
  perte_apres_secondes: 30,
};

describe("@happy", () => {
  it("lit le trajet sur la route de la Mission", async () => {
    const fetchMock = vi.fn().mockResolvedValue(reponse(TRAJET));
    vi.stubGlobal("fetch", fetchMock);

    const lu = await suivreTrajet("m-1");

    expect(lu.etat).toBe("EN_ROUTE");
    expect(String(fetchMock.mock.calls[0][0])).toContain("/missions/m-1/tracking");
  });

  it("consent au partage par intervention", async () => {
    const fetchMock = vi.fn().mockResolvedValue(reponse({ code: "TRACKING_CONSENTED", consenti: true }));
    vi.stubGlobal("fetch", fetchMock);

    const etat = await consentirSuivi("m-1", true);

    expect(etat.consenti).toBe(true);
    const [url, options] = fetchMock.mock.calls[0];
    // **Le consentement porte l'identifiant de la Mission.** Une route sans lui
    // serait un réglage de compte, c'est-à-dire un consentement global, ce que
    // le RGPD ne reconnaît pas comme éclairé.
    expect(String(url)).toContain("/missions/m-1/tracking/consent");
    expect(JSON.parse(options.body)).toEqual({ accepte: true });
  });

  it("rend la position dégradée par le serveur, pas celle envoyée", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      reponse({
        code: "TRACKING_RECORDED",
        lat: 50.8465,
        lon: 4.3521,
        hors_zone: false,
        relevee_le: "2026-08-27T10:00:00Z",
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const envoye = { lat: 50.8467123, lon: 4.3525897 };
    const rendu = await envoyerPosition("m-1", envoye.lat, envoye.lon);

    // Le front envoie ce que le GPS lui donne et affiche ce que le serveur lui
    // rend : arrondir ici en plus laisserait croire que la maille est une
    // affaire d'affichage, alors qu'elle porte sur ce qui est conservé.
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual(envoye);
    expect(rendu.lat).toBe(50.8465);
    expect(rendu.lat).not.toBe(envoye.lat);
  });
});

describe("@security", () => {
  it("n'envoie jamais une position par la file hors-ligne", async () => {
    // Une position rejouée dix minutes plus tard place le prestataire où il
    // n'est plus, et le demandeur descend attendre dans la rue.
    const fetchMock = vi.fn().mockRejectedValue(new TypeError("Failed to fetch"));
    vi.stubGlobal("fetch", fetchMock);

    // La coupure remonte telle quelle à l'appelant, qui l'oublie. Une réussite
    // silencieuse signalerait au contraire une mise en file, et l'écran
    // afficherait « position envoyée » pour un point qui partira plus tard.
    await expect(envoyerPosition("m-1", 50.8, 4.3)).rejects.toBeInstanceOf(OfflineError);
  });

  it("porte le jeton en en-tête, jamais dans l'URL", async () => {
    const fetchMock = vi.fn().mockResolvedValue(reponse(TRAJET));
    vi.stubGlobal("fetch", fetchMock);
    await suivreTrajet("m-1");
    expect(String(fetchMock.mock.calls[0][0])).not.toContain("Bearer");
  });
});

describe("@negative", () => {
  it("ne présente pas l'absence de partage comme une panne", () => {
    const texte = libelleTrajet("POSITION_LOST");
    // Le prestataire peut n'avoir pas consenti : le mot « erreur » ferait
    // douter d'une intervention qui se déroule normalement.
    expect(texte.toLowerCase()).not.toContain("erreur");
    expect(texte.toLowerCase()).not.toContain("panne");
    expect(texte).toContain("non partagée");
  });

  it("remonte l'échec du consentement plutôt que de le supposer acquis", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(reponse({ code: "MISSION_NOT_FOUND" }, 404)));
    await expect(consentirSuivi("inconnue", true)).rejects.toThrow();
  });
});

describe("@edge", () => {
  it("a un libellé pour chacun des quatre états", () => {
    const etats = ["EN_ROUTE", "POSITION_LOST", "OUT_OF_ZONE", "STOPPED"] as const;
    const libelles = etats.map((e) => libelleTrajet(e));
    expect(libelles.every((l) => l.length > 0)).toBe(true);
    // Quatre états distincts doivent se lire différemment, sinon l'écran ne
    // dit rien de plus que « quelque chose se passe ».
    expect(new Set(libelles).size).toBe(4);
  });
});

beforeEach(() => {
  vi.unstubAllGlobals();
});
