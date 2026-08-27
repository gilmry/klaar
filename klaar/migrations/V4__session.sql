-- Story 1.3 — sessions de rafraîchissement (FR-004).

CREATE TABLE session_refresh (
    -- Empreinte SHA-256 du refresh. Le jeton lui-même n'est jamais conservé :
    -- une lecture de cette table ne permet pas d'usurper une session.
    empreinte      CHAR(64) PRIMARY KEY,
    utilisateur_id UUID        NOT NULL REFERENCES utilisateur (id) ON DELETE CASCADE,
    -- Relie tous les refresh issus d'une même authentification. La rotation
    -- (Story 1.4) en crée un nouveau à chaque usage ; c'est la famille qui
    -- permet, à la détection d'un rejeu, de couper la chaîne entière plutôt
    -- que le seul jeton rejoué — sans quoi le voleur garde le sien.
    famille_id     UUID        NOT NULL,
    cree_le        TIMESTAMPTZ NOT NULL DEFAULT now(),
    expire_le      TIMESTAMPTZ NOT NULL,
    -- Non nul dès la rotation : c'est ce qui rend le rejeu détectable.
    consomme_le    TIMESTAMPTZ,
    -- Non nul après déconnexion ou coupure de famille.
    revoque_le     TIMESTAMPTZ
);

CREATE INDEX session_refresh_utilisateur_idx ON session_refresh (utilisateur_id);
CREATE INDEX session_refresh_famille_idx ON session_refresh (famille_id);
-- Sert la purge des sessions périmées, qui sans index parcourt toute la table.
CREATE INDEX session_refresh_expire_idx ON session_refresh (expire_le);
