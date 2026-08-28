-- Story 8.1 — revue KYC par l'exploitation (FR-038).

-- Deux statuts de plus pour un prestataire.
--
-- **`REJECTED` n'est pas `SUSPENDED`.** Un suspendu a été actif et pourra
-- l'être à nouveau ; un refusé n'est jamais entré. Les confondre ferait
-- apparaître dans les statistiques de sanction des entreprises qui n'ont jamais
-- travaillé.
--
-- **`WITHDRAWN` n'est pas `REJECTED`.** L'entreprise a retiré sa demande avant
-- décision : personne n'a rien jugé, et lui inscrire un refus au dossier
-- consignerait une décision qui n'a pas été prise.
ALTER TABLE provider DROP CONSTRAINT provider_statut_connu;
ALTER TABLE provider ADD CONSTRAINT provider_statut_connu
    CHECK (statut IN ('PENDING_KYC', 'ACTIVE', 'SUSPENDED', 'REJECTED', 'WITHDRAWN'));

-- Une troisième origine de contrôle : un humain a lu les pièces.
--
-- Ce n'est pas la BCE, et l'écrire ainsi permettra de distinguer les dossiers
-- validés à la main le jour où l'adaptateur BCE existera.
ALTER TABLE provider DROP CONSTRAINT provider_origine_connue;
ALTER TABLE provider ADD CONSTRAINT provider_origine_connue
    CHECK (origine_kyc IN ('BCE', 'OPS_REVIEW', 'DEMONSTRATION'));

-- La revue elle-même.
--
-- **Une ligne par décision, pas une colonne sur `provider`.** Un refus se
-- prépare puis se confirme (règle des quatre yeux) : deux gestes, deux
-- horodatages, deux auteurs. Une colonne ne saurait porter que le dernier état,
-- et la question « qui a confirmé ce refus » n'aurait plus de réponse.
CREATE TABLE revue_kyc (
    id            UUID PRIMARY KEY,
    provider_id   UUID NOT NULL REFERENCES provider (id) ON DELETE CASCADE,
    -- `APPROVE` ou `REJECT`.
    decision      TEXT NOT NULL
        CONSTRAINT revue_decision_connue CHECK (decision IN ('APPROVE', 'REJECT')),
    -- Exigé pour un refus (FR-038 `@negative`). Une entreprise refusée doit
    -- pouvoir savoir ce qu'on lui reproche, sans quoi elle ne peut ni corriger
    -- ni contester.
    motif         TEXT
        CONSTRAINT revue_motif_borne CHECK (motif IS NULL OR char_length(btrim(motif)) BETWEEN 20 AND 1000),
    CONSTRAINT revue_motif_si_refus CHECK ((decision = 'REJECT') = (motif IS NOT NULL)),

    -- `SET NULL` : la revue survit au départ de celui qui l'a faite. C'est
    -- même le cas où elle sert le plus.
    premier_ops   UUID REFERENCES compte_ops (id) ON DELETE SET NULL,
    propose_le    TIMESTAMPTZ NOT NULL,

    -- La règle des quatre yeux (FR-038 `@edge`) : un refus n'a d'effet que
    -- confirmé par un **autre** compte. Nuls tant que personne n'a confirmé.
    second_ops    UUID REFERENCES compte_ops (id) ON DELETE SET NULL,
    confirme_le   TIMESTAMPTZ,

    CONSTRAINT revue_confirmation_complete CHECK ((second_ops IS NULL) = (confirme_le IS NULL)),
    -- **Deux yeux différents.** La contrainte le grave plutôt que de s'en
    -- remettre au code : un refus confirmé par son propre auteur ne serait pas
    -- une seconde paire d'yeux, ce serait un second clic.
    CONSTRAINT revue_quatre_yeux CHECK (second_ops IS NULL OR second_ops <> premier_ops)
);

-- Une seule revue en cours par entreprise : proposer deux refus concurrents
-- ferait confirmer l'un et laisserait l'autre en suspens sans que personne ne
-- sache lequel fait foi.
CREATE UNIQUE INDEX revue_kyc_en_cours_idx ON revue_kyc (provider_id)
    WHERE confirme_le IS NULL;

CREATE INDEX revue_kyc_provider_idx ON revue_kyc (provider_id, propose_le DESC);

-- La console lit les entreprises en attente, de la plus ancienne à la plus
-- récente : c'est celle qui attend depuis le plus longtemps qui doit remonter.
CREATE INDEX provider_en_attente_idx ON provider (cree_le) WHERE statut = 'PENDING_KYC';
