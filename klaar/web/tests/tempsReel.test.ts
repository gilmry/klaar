/**
 * Story 4.9 — reprise et adressage du flux temps réel.
 *
 * Ce qui est testé ici est ce qui ne se voit pas à l'œil : la progression des
 * attentes de reconnexion, et l'URL dérivée. Une socket en clair sous une page
 * en TLS est refusée par le navigateur, au bon moment pour nous et au mauvais
 * moment pour l'utilisateur.
 */
import { describe, expect, it } from "vitest";
import { attenteReprise, REPRISES_MS, urlSocket } from "../src/lib/tempsReel";

/** Un `Location` réduit à ce que `urlSocket` en lit. */
function origine(href: string): Location {
  return { origin: new URL(href).origin } as Location;
}

describe("@happy", () => {
  it("dérive une URL de socket depuis une page en clair", () => {
    const url = urlSocket("M-1", "billet-abc", origine("http://localhost:4321/demande"));
    expect(url).toBe("ws://localhost:4321/api/v1/missions/M-1/events?billet=billet-abc");
  });

  it("passe en wss sous une page en TLS", () => {
    // Une socket en clair sous une page chiffrée est refusée par le navigateur.
    const url = urlSocket("M-1", "b", origine("https://klaar.be/demande"));
    expect(url.startsWith("wss://klaar.be/")).toBe(true);
  });

  it("attend de plus en plus longtemps entre deux tentatives", () => {
    const attentes = [0, 1, 2, 3, 4].map(attenteReprise);
    expect(attentes).toEqual(REPRISES_MS);
    for (let i = 1; i < attentes.length; i += 1) {
      expect(attentes[i]).toBeGreaterThan(attentes[i - 1]);
    }
  });
});

describe("@negative", () => {
  it("ne renonce jamais : le dernier palier est gardé", () => {
    // Quelqu'un qui laisse son suivi ouvert pendant une panne doit le voir
    // repartir tout seul. Abandonner laisserait un écran mort sans le dire.
    const palier = REPRISES_MS[REPRISES_MS.length - 1];
    expect(attenteReprise(50)).toBe(palier);
    expect(attenteReprise(5000)).toBe(palier);
  });

  it("traite une tentative négative comme la première", () => {
    expect(attenteReprise(-3)).toBe(REPRISES_MS[0]);
  });
});

describe("@edge", () => {
  it("ne double pas la barre oblique du chemin", () => {
    const url = urlSocket("M-1", "b", origine("http://localhost:4321/"));
    expect(url).not.toContain("//missions");
  });

  it("plafonne l'attente pour ne pas marteler un service qui redémarre", () => {
    // Chaque onglet ouvert reconnecte : sans plafond, un redémarrage
    // produirait une tempête de connexions au moment le plus fragile.
    expect(Math.max(...REPRISES_MS)).toBeLessThanOrEqual(60_000);
    expect(REPRISES_MS[0]).toBeGreaterThanOrEqual(1_000);
  });
});

describe("@security", () => {
  it("encode le billet plutôt que de le coller tel quel", () => {
    // Un billet est du base64url, donc sans caractère à échapper — mais s'en
    // remettre au format d'aujourd'hui pour construire une URL est le genre de
    // pari qu'on perd le jour où le format change.
    const url = urlSocket("M-1", "a b&c=d", origine("http://localhost:4321/"));
    expect(url).toContain("billet=a%20b%26c%3Dd");
    expect(url).not.toContain("&c=d");
  });

  it("ne fait pas voyager le jeton d'accès dans l'URL", () => {
    // Toute la raison d'être du billet : une URL de socket finit dans les
    // journaux du serveur, du proxy et l'historique du navigateur.
    const url = urlSocket("M-1", "billet-abc", origine("http://localhost:4321/"));
    expect(url).not.toContain("Bearer");
    expect(url).not.toContain("jeton");
    expect(url.match(/eyJ/)).toBeNull();
  });
});
