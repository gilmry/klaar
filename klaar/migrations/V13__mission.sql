-- Story 3.4 — acceptation par le premier répondant (FR-013).

-- Le statut `MATCHED` rejoint les statuts connus d'une Demande. La contrainte
-- est remplacée et non complétée : PostgreSQL ne sait pas étendre un CHECK.
ALTER TABLE demande DROP CONSTRAINT demande_statut_connu;
ALTER TABLE demande ADD CONSTRAINT demande_statut_connu
    CHECK (statut IN ('BROADCASTING', 'MATCHED', 'NO_MATCH', 'CANCELLED'));

-- Mission (FR-013, FR-018 à FR-023).
--
-- **Ce que cette table sait, et ce qu'elle ne sait pas encore.** Une Mission
-- naît de l'acceptation d'une Demande. Sa machine à états — en route, sur
-- place, terminée, validée, annulée — appartient à FR-018 et suivants ; un seul
-- statut est donc admis ici, et la contrainte le dit plutôt que d'accepter par
-- avance des valeurs qu'aucun code ne produit.
CREATE TABLE mission (
    id          UUID PRIMARY KEY,
    -- Une Demande donne au plus une Mission. C'est déjà ce que garantit
    -- l'attribution atomique ; la contrainte le grave, pour qu'une insertion
    -- directe ou un futur chemin d'écriture ne puisse pas en créer deux.
    demande_id  UUID NOT NULL UNIQUE REFERENCES demande (id) ON DELETE CASCADE,
    -- `RESTRICT` : supprimer un prestataire qui porte une Mission doit échouer
    -- bruyamment. L'effacement d'un compte (FR-010) passe par une
    -- anonymisation, pas par un DELETE qui emporterait l'intervention d'un
    -- tiers avec lui.
    provider_id UUID NOT NULL REFERENCES provider (id) ON DELETE RESTRICT,
    statut      TEXT NOT NULL
        CONSTRAINT mission_statut_connu CHECK (statut IN ('ASSIGNED')),
    cree_le     TIMESTAMPTZ NOT NULL
);

-- Une Mission à la fois par prestataire (FR-013 `@edge`, politique MVP).
--
-- **Un index, et non un contrôle applicatif.** Vérifier puis insérer laisserait
-- passer deux acceptations simultanées : c'est exactement la course que cette
-- story doit fermer, et un contrôle en amont ne la ferme pas. L'index la ferme,
-- au prix d'une erreur à traduire, ce que l'adaptateur fait.
--
-- **À étendre avec FR-018.** Quand des statuts terminaux existeront, ils
-- devront sortir de ce filtre, faute de quoi un prestataire ayant terminé une
-- intervention resterait bloqué à vie.
CREATE UNIQUE INDEX mission_provider_en_cours_idx ON mission (provider_id)
    WHERE statut IN ('ASSIGNED');

CREATE INDEX mission_provider_idx ON mission (provider_id, cree_le DESC);
