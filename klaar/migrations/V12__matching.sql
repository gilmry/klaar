-- Story 3.2 — matching géolocalisé et sa trace (FR-012).

-- Date du contrôle d'entreprise. FR-012 en fait un critère du score : un
-- contrôle vieux d'un an ne dit plus grand-chose de l'état de l'entreprise.
ALTER TABLE provider ADD COLUMN kyc_verifie_le TIMESTAMPTZ;

-- Reprise de l'existant **avant** de poser la contrainte. Les prestataires déjà
-- en base portent une origine de contrôle sans date : ajouter la contrainte
-- sans les traiter d'abord la fait échouer sur eux, ce qui est exactement ce
-- qui s'est produit au premier jet de cette migration.
--
-- La date de création tient lieu de date de contrôle pour ces lignes. C'est une
-- approximation, et la seule disponible : elle ne peut être que postérieure ou
-- égale au contrôle réel, donc elle sous-estime la fraîcheur plutôt que de la
-- surestimer — le sens qui ne favorise personne à tort.
UPDATE provider SET kyc_verifie_le = cree_le
    WHERE origine_kyc IS NOT NULL AND kyc_verifie_le IS NULL;

-- Cohérence avec l'origine : un prestataire dont le contrôle a une origine a
-- forcément une date, et réciproquement. Sans quoi le score calculerait une
-- ancienneté sur une date absente, et traiterait tout le monde comme périmé.
ALTER TABLE provider
    ADD CONSTRAINT provider_kyc_date_coherente CHECK (
        (origine_kyc IS NULL AND kyc_verifie_le IS NULL)
        OR (origine_kyc IS NOT NULL AND kyc_verifie_le IS NOT NULL)
    );

-- Trace du matching (FR-012 `@happy`, AI Act art. 12).
--
-- **Une décision automatisée doit pouvoir s'expliquer.** Cette table conserve,
-- pour chaque Demande, qui a été retenu, avec quel score, et de quels critères
-- ce score était fait. Elle répond à la question « pourquoi n'ai-je pas été
-- notifié ? », qu'un prestataire est en droit de poser.
--
-- Les candidats **écartés** y figurent aussi : ne garder que les retenus
-- rendrait la trace inutile pour celui qui demande des comptes, c'est-à-dire
-- pour la seule personne à qui elle est destinée.
CREATE TABLE trace_matching (
    id            BIGSERIAL PRIMARY KEY,
    demande_id    UUID NOT NULL REFERENCES demande (id) ON DELETE CASCADE,
    provider_id   UUID NOT NULL REFERENCES provider (id) ON DELETE CASCADE,
    -- Score final, entre 0 et 1.
    score         DOUBLE PRECISION NOT NULL
        CONSTRAINT trace_score_borne CHECK (score >= 0 AND score <= 1),
    distance_metres DOUBLE PRECISION NOT NULL
        CONSTRAINT trace_distance_positive CHECK (distance_metres >= 0),
    -- Ventilation du score, telle que le domaine l'a produite. Conservée en
    -- JSON et non éclatée en colonnes : les critères changeront, et une
    -- migration par changement de pondération rendrait l'historique illisible.
    ventilation   JSONB NOT NULL,
    -- Vrai si le prestataire a été retenu pour notification.
    retenu        BOOLEAN NOT NULL,
    -- Pourquoi il ne l'a pas été, quand il ne l'a pas été.
    motif_ecart   TEXT
        CONSTRAINT trace_motif_connu CHECK (
            motif_ecart IS NULL OR motif_ecart IN ('HORS_TOP', 'HORS_RAYON')
        ),
    tracee_le     TIMESTAMPTZ NOT NULL,

    -- Un retenu n'a pas de motif d'écart, un écarté en a forcément un.
    CONSTRAINT trace_motif_coherent CHECK (
        (retenu AND motif_ecart IS NULL) OR (NOT retenu AND motif_ecart IS NOT NULL)
    ),
    -- Un prestataire n'apparaît qu'une fois par Demande : deux lignes
    -- contradictoires rendraient la trace inexploitable.
    CONSTRAINT trace_unique_par_demande UNIQUE (demande_id, provider_id)
);

CREATE INDEX trace_matching_demande_idx ON trace_matching (demande_id);
CREATE INDEX trace_matching_provider_idx ON trace_matching (provider_id, tracee_le DESC);
