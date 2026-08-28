/**
 * Story 8.3 — console d'exploitation, côté navigateur (FR-040).
 *
 * **Ce qui se teste ici et nulle part ailleurs :** que le jeton d'exploitation
 * ne soit jamais écrit dans un stockage persistant, qu'il ne parte pas dans une
 * URL, et qu'un taux sans assiette ne se lise pas comme un échec.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  DECISIONS,
  connexionOps,
  deconnexionOps,
  jetonOps,
  montantOps,
  noteLisible,
  oublierJetonOps,
  pourcentage,
  resteAvantEcheance,
  sessionFinie,
  tableauDeBord,
  fileKyc,
  fileMediation,
  MOTIF_KYC_MIN,
  reviserKyc,
  libelleMotif,
  trancherLitige,
} from "../src/lib/ops";
import { ApiError } from "../src/lib/api";

function reponse(corps: unknown, statut = 200): Response {
  return new Response(JSON.stringify(corps), { status: statut });
}

const SESSION = {
  id: "11111111-1111-1111-1111-111111111111",
  role: "READER",
  code: "OPS_AUTHENTICATED",
  jeton: "un-jeton-de-session-opaque",
  expire_le: new Date(Date.now() + 30 * 60 * 1000).toISOString(),
};

const TABLEAU = {
  depuis: "2026-07-29T00:00:00Z",
  fenetre_jours: 30,
  comptes_actifs: 12,
  demandes: 40,
  demandes_attribuees: 30,
  taux_remplissage: 0.75,
  gmv_htva_cents: 1234500,
  commission_htva_cents: 222210,
  litiges_ouverts: 2,
  notes: 8,
  note_moyenne: 4.25,
  sorties_de_zone: 1,
  kyc_en_attente: 3,
};

describe("@happy", () => {
  it("ouvre une session et porte le jeton en en-tête", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(reponse(SESSION))
      .mockResolvedValueOnce(reponse(TABLEAU));
    vi.stubGlobal("fetch", fetchMock);

    await connexionOps("ops@klaar.test", "Ops@2026Securise", "123456");
    await tableauDeBord();

    const [, optionsConnexion] = fetchMock.mock.calls[0];
    // Les identifiants voyagent dans le corps, une seule fois.
    expect(optionsConnexion.method).toBe("POST");
    expect(JSON.parse(optionsConnexion.body).mot_de_passe).toBe("Ops@2026Securise");

    const [urlTableau, optionsTableau] = fetchMock.mock.calls[1];
    expect(optionsTableau.headers.Authorization).toBe(`Bearer ${SESSION.jeton}`);
    expect(String(urlTableau)).toContain("/ops/dashboard");
  });

  it("rend l'échéance pour que l'écran prévienne avant de couper", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(reponse(SESSION)));
    await connexionOps("ops@klaar.test", "mdp", "123456");
    const reste = resteAvantEcheance();
    expect(reste).not.toBeNull();
    expect(reste!).toBeGreaterThan(25 * 60 * 1000);
  });
});

describe("@security", () => {
  it("n'écrit jamais le jeton dans un stockage persistant", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(reponse(SESSION)));
    await connexionOps("ops@klaar.test", "mdp", "123456");

    // Un jeton d'exploitation en `localStorage` survivrait à la fermeture de
    // l'onglet et resterait lisible par n'importe quel script injecté.
    expect(jetonOps()).toBe(SESSION.jeton);
    expect(globalThis.localStorage).toBeUndefined();
    expect(globalThis.sessionStorage).toBeUndefined();
  });

  it("ne met jamais le jeton dans une URL", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(reponse(SESSION))
      .mockResolvedValueOnce(reponse(TABLEAU));
    vi.stubGlobal("fetch", fetchMock);
    await connexionOps("ops@klaar.test", "mdp", "123456");
    await tableauDeBord();

    for (const [url] of fetchMock.mock.calls) {
      expect(String(url)).not.toContain(SESSION.jeton);
      expect(String(url)).not.toContain("mot_de_passe");
    }
  });

  it("oublie le jeton même si la déconnexion échoue", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(reponse(SESSION)));
    await connexionOps("ops@klaar.test", "mdp", "123456");

    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    await deconnexionOps();

    // Un écran qui se croit connecté sans l'être ferait perdre ce qui y est
    // saisi au premier envoi.
    expect(jetonOps()).toBeNull();
    expect(resteAvantEcheance()).toBeNull();
  });

  it("reconnaît une session finie sans la confondre avec une panne", () => {
    expect(sessionFinie(new ApiError(401, ""))).toBe(true);
    expect(sessionFinie(new ApiError(503, ""))).toBe(false);
    expect(sessionFinie(new Error("réseau"))).toBe(false);
  });
});

describe("@edge", () => {
  it("n'affiche pas « 0 % » quand il n'y a rien à mesurer", () => {
    // Zéro pour cent se lit comme un échec de la plateforme. À J0, il n'y a
    // pas d'échec (FR-040 `@edge`).
    expect(pourcentage(null)).not.toContain("0 %");
    expect(pourcentage(null)).toBe("pas encore mesurable");
    expect(pourcentage(0.75)).toBe("75 %");
  });

  it("dit « aucune note » plutôt que zéro sur cinq", () => {
    expect(noteLisible(null, 0)).toBe("aucune note");
    expect(noteLisible(4.25, 8)).toContain("4.3 / 5");
    // L'assiette accompagne toujours la moyenne.
    expect(noteLisible(4.25, 8)).toContain("8 notes");
    expect(noteLisible(5, 1)).toContain("1 note");
  });

  it("rend les montants en euros depuis des centimes", () => {
    // Les centimes ne deviennent des euros qu'à l'affichage : les manipuler en
    // flottants plus tôt ferait apparaître des 12345.000000000002.
    expect(montantOps(1234500).replace(/ | /g, " ")).toBe("12 345,00 €");
    expect(montantOps(0)).toContain("0,00");
  });
});

describe("@negative", () => {
  it("remonte le refus d'identifiants sans ouvrir de session", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(reponse({ code: "OPS_CREDENTIALS_INVALID" }, 401)),
    );
    await expect(connexionOps("ops@klaar.test", "faux", "000000")).rejects.toThrow();
    expect(jetonOps()).toBeNull();
  });
});


describe("@happy médiation", () => {
  it("lit la file et tranche partiellement", async () => {
    const dossier = {
      id: "d-1",
      mission_id: "m-1",
      partie: "USER",
      motif: "NOT_DONE",
      description: "Rien n'a été fait.",
      ouvert_le: "2026-08-01T09:00:00Z",
      age_jours: 27,
      a_escalader: false,
      total_ttc_cents: 21780,
    };
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(reponse(SESSION))
      .mockResolvedValueOnce(reponse({ dossiers: [dossier] }))
      .mockResolvedValueOnce(
        reponse({
          statut: "RESOLVED_USER_FAVOR",
          remboursement_cents: 6534,
          reversement_cents: 15246,
          execute: false,
        }),
      );
    vi.stubGlobal("fetch", fetchMock);

    await connexionOps("ops@klaar.test", "mdp", "123456");
    const file = await fileMediation();
    expect(file).toHaveLength(1);

    const issue = await trancherLitige("d-1", "PARTIAL_REFUND", 3000);
    const [, options] = fetchMock.mock.calls[2];
    expect(JSON.parse(options.body)).toEqual({ decision: "PARTIAL_REFUND", part_bp: 3000 });
    expect(issue.remboursement_cents).toBe(6534);
    // Rien n'a bougé sur l'argent : l'écran doit pouvoir le dire.
    expect(issue.execute).toBe(false);
  });

  it("n'envoie pas de part sur une décision qui n'en prend pas", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(reponse(SESSION))
      .mockResolvedValueOnce(reponse({ statut: "CLOSED_NO_FAULT", remboursement_cents: 0, reversement_cents: 0, execute: false }));
    vi.stubGlobal("fetch", fetchMock);

    await connexionOps("ops@klaar.test", "mdp", "123456");
    // Une part passée par erreur ne doit pas partir : le service la refuserait,
    // et laisser le front l'envoyer transformerait une faute de frappe en 422.
    await trancherLitige("d-1", "NO_FAULT", 3000);
    expect(JSON.parse(fetchMock.mock.calls[1][1].body)).toEqual({ decision: "NO_FAULT" });
  });

  it("traduit les motifs sans en inventer", () => {
    expect(libelleMotif("NOT_DONE")).toBe("Travail non fait");
    expect(libelleMotif("QUALITY")).toBe("Travail mal fait");
    // Un motif inconnu est rendu tel quel plutôt que masqué : le masquer
    // laisserait un dossier sans grief lisible à l'écran.
    expect(libelleMotif("MOTIF_FUTUR")).toBe("MOTIF_FUTUR");
  });

  it("propose les quatre décisions, sans doublon", () => {
    expect(DECISIONS).toHaveLength(4);
    expect(new Set(DECISIONS.map((d) => d.code)).size).toBe(4);
  });
});

describe("@happy revue KYC", () => {
  const DOSSIER = {
    provider_id: "p-1",
    // Clé de contrôle **volontairement fausse** : un numéro BCE à clé valide
    // peut désigner une entreprise réelle, et la barrière de publication les
    // refuse. Ce champ n'est ici que du texte affiché.
    numero_bce: "0123456700",
    raison_sociale: "Candidate SPRL",
    secteurs: ["plomberie"],
    inscrit_le: "2026-08-18T09:00:00Z",
    attente_jours: 9,
    attente_longue: true,
    refus_en_attente: null,
  };

  it("valide sans motif et refuse avec", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(reponse(SESSION))
      .mockResolvedValueOnce(reponse({ dossiers: [DOSSIER] }))
      .mockResolvedValueOnce(
        reponse({ code: "REVIEW_RECORDED", statut: "ACTIVE", attend_confirmation: false, notifie: false }),
      )
      .mockResolvedValueOnce(
        reponse({
          code: "REVIEW_PENDING_CONFIRMATION",
          statut: null,
          attend_confirmation: true,
          notifie: false,
        }),
      );
    vi.stubGlobal("fetch", fetchMock);

    await connexionOps("ops@klaar.test", "mdp", "123456");
    const file = await fileKyc();
    expect(file[0].attente_longue).toBe(true);

    await reviserKyc("p-1", "APPROVE");
    // **Aucun motif sur une validation.** Le service le refuserait, et
    // l'envoyer transformerait une maladresse d'interface en 422.
    expect(JSON.parse(fetchMock.mock.calls[2][1].body)).toEqual({ decision: "APPROVE" });

    const issue = await reviserKyc("p-1", "REJECT", "Le numéro d'entreprise est inconnu à la BCE.");
    expect(JSON.parse(fetchMock.mock.calls[3][1].body).motif).toContain("inconnu");
    // Le refus n'a pas encore d'effet : l'écran doit pouvoir le dire.
    expect(issue.attend_confirmation).toBe(true);
    expect(issue.statut).toBeNull();
    expect(issue.notifie).toBe(false);
  });

  it("exige un motif d'au moins vingt caractères", () => {
    // Le seuil est celui du domaine : « non » n'est pas un motif.
    expect(MOTIF_KYC_MIN).toBe(20);
    expect("non".length >= MOTIF_KYC_MIN).toBe(false);
    expect("Le numéro d'entreprise est inconnu.".length >= MOTIF_KYC_MIN).toBe(true);
  });
});

beforeEach(() => {
  vi.unstubAllGlobals();
  oublierJetonOps();
});
