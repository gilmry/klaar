-- Story 6.1 — conversation entre le demandeur et le prestataire (FR-030).

-- **Pas de table « conversation ».** Une Mission en tient lieu : elle désigne
-- exactement deux personnes, elle a une naissance et une fin, et c'est d'elle
-- que dépendent l'ouverture et la fermeture des échanges. Une table qui
-- n'aurait qu'un identifiant de Mission et rien d'autre aurait été un détour.
CREATE TABLE message (
    id          UUID PRIMARY KEY,
    mission_id  UUID NOT NULL REFERENCES mission (id) ON DELETE CASCADE,
    -- `SET NULL` : l'effacement d'un compte (RGPD art. 17) ne doit pas trouer
    -- la conversation de l'autre partie, qui a le droit de relire ce qui a été
    -- convenu. Le nom s'efface, le fil reste.
    auteur_id   UUID REFERENCES utilisateur (id) ON DELETE SET NULL,
    corps       TEXT NOT NULL
        CONSTRAINT message_non_vide CHECK (char_length(btrim(corps)) > 0)
        CONSTRAINT message_borne CHECK (char_length(corps) <= 4000),
    envoye_le   TIMESTAMPTZ NOT NULL
);

-- La lecture d'un fil et le comptage des cent messages passent par là.
CREATE INDEX message_mission_idx ON message (mission_id, envoye_le);

-- Immuabilité : un message envoyé ne se réécrit pas.
--
-- **Ce n'est pas une commodité, c'est ce qui donne sa valeur au fil.** Une
-- conversation sert de trace de ce qui a été convenu — un horaire, un accès,
-- une précision sur le travail — et pouvoir la réécrire après coup la viderait
-- de tout intérêt en cas de désaccord.
--
-- Sauf `auteur_id`, que l'effacement d'un compte doit pouvoir anonymiser.
CREATE OR REPLACE FUNCTION message_fige() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.mission_id IS DISTINCT FROM OLD.mission_id
    OR NEW.corps      IS DISTINCT FROM OLD.corps
    OR NEW.envoye_le  IS DISTINCT FROM OLD.envoye_le
    THEN
        RAISE EXCEPTION
            'un message envoyé ne se réécrit pas (FR-030) : seul l''auteur peut être anonymisé'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER message_fige
    BEFORE UPDATE ON message
    FOR EACH ROW EXECUTE FUNCTION message_fige();

-- Tentatives d'échange de coordonnées (FR-032 `@security`).
--
-- **Le message refusé n'est pas conservé, seulement la tentative.** Garder le
-- texte reviendrait à constituer un fichier de ce que les gens ont essayé de
-- s'écrire, pour une finalité — compter les récidives — qui n'en a pas besoin.
-- Ce qui est consigné : qui, quand, sur quelle Mission, et quel genre de
-- coordonnée.
CREATE TABLE tentative_contournement (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    mission_id  UUID NOT NULL REFERENCES mission (id) ON DELETE CASCADE,
    auteur_id   UUID REFERENCES utilisateur (id) ON DELETE SET NULL,
    genre       TEXT NOT NULL
        CONSTRAINT tentative_genre_connu CHECK (genre IN ('PHONE', 'EMAIL')),
    tentee_le   TIMESTAMPTZ NOT NULL
);

CREATE INDEX tentative_auteur_idx ON tentative_contournement (auteur_id, tentee_le);
