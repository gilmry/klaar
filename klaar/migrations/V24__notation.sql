-- Story 7.1 — notation double sens après intervention (FR-033).

CREATE TABLE notation (
    id           UUID PRIMARY KEY,
    -- `CASCADE` : une note n'existe que pour une intervention. Si celle-ci
    -- disparaît avec la Demande — ce qui n'arrive qu'à l'effacement d'un
    -- compte — la note n'a plus d'objet.
    mission_id   UUID NOT NULL REFERENCES mission (id) ON DELETE CASCADE,
    -- Le compte qui note. `SET NULL` et non `CASCADE` : FR-033 `@security`
    -- demande que les commentaires **restent** après l'effacement d'un compte,
    -- sous « anonyme ». Les supprimer réécrirait la réputation d'un prestataire
    -- au gré des départs de ses clients.
    auteur_id    UUID REFERENCES utilisateur (id) ON DELETE SET NULL,
    -- Qui est noté. Déduit de l'auteur côté service, jamais reçu : sans cela,
    -- quelqu'un pourrait se noter lui-même.
    cible        TEXT NOT NULL
        CONSTRAINT notation_cible_connue CHECK (cible IN ('PROVIDER', 'USER')),
    note         SMALLINT NOT NULL
        CONSTRAINT notation_echelle CHECK (note BETWEEN 1 AND 5),
    commentaire  TEXT
        CONSTRAINT notation_commentaire_borne
            CHECK (commentaire IS NULL OR char_length(commentaire) <= 500),
    cree_le      TIMESTAMPTZ NOT NULL,

    -- **Une note par côté et par intervention** (FR-033 `@security` : « la
    -- contrainte unique est en base, la tentative de double est techniquement
    -- impossible »). La clé porte sur la cible et non sur l'auteur : c'est ce
    -- qui empêche aussi un second compte de noter à la place du premier.
    CONSTRAINT notation_une_par_cote UNIQUE (mission_id, cible)
);

-- La lecture de la réputation d'un prestataire agrège ses notes ; sans cet
-- index, elle parcourrait toute la table à chaque affichage de fiche.
CREATE INDEX notation_cible_idx ON notation (cible, mission_id);

-- Immuabilité : une note est un avis daté, pas un état qu'on ajuste. La
-- modifier après coup permettrait de la retourner sous pression.
--
-- **Sauf `auteur_id`**, que l'effacement d'un compte doit pouvoir mettre à
-- NULL (RGPD art. 17) : le commentaire reste, le nom s'efface.
CREATE OR REPLACE FUNCTION notation_figee() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.mission_id  IS DISTINCT FROM OLD.mission_id
    OR NEW.cible       IS DISTINCT FROM OLD.cible
    OR NEW.note        IS DISTINCT FROM OLD.note
    OR NEW.commentaire IS DISTINCT FROM OLD.commentaire
    OR NEW.cree_le     IS DISTINCT FROM OLD.cree_le
    THEN
        RAISE EXCEPTION
            'une note émise ne se modifie plus (FR-033) : seul l''auteur peut être anonymisé'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER notation_figee
    BEFORE UPDATE ON notation
    FOR EACH ROW EXECUTE FUNCTION notation_figee();

-- Réputation agrégée d'un prestataire.
--
-- **Une table d'agrégat**, comme les fourchettes de prix, et pour la même
-- raison : le matching lit la réputation à chaque tour, et recalculer Wilson
-- sur toutes les notes à chaque candidat mettrait un balayage complet dans le
-- chemin critique d'une recherche censée durer moins de cinq minutes.
CREATE TABLE reputation_provider (
    provider_id   UUID PRIMARY KEY REFERENCES provider (id) ON DELETE CASCADE,
    -- Somme des étoiles et nombre de notes : les deux entrées de Wilson.
    -- Conservées plutôt que la borne seule, pour que le calcul soit refaisable
    -- et vérifiable après coup.
    somme_notes   INT NOT NULL CONSTRAINT reputation_somme_positive CHECK (somme_notes >= 0),
    nombre_notes  INT NOT NULL CONSTRAINT reputation_nombre_positif CHECK (nombre_notes >= 0),
    CONSTRAINT reputation_somme_coherente
        CHECK (somme_notes >= nombre_notes AND somme_notes <= nombre_notes * 5),
    mise_a_jour_le TIMESTAMPTZ NOT NULL
);
