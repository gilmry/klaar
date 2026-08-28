-- Story 7.2 — ouverture de litige (FR-034) et seuils de sanction (FR-035).

CREATE TABLE litige (
    id          UUID PRIMARY KEY,
    -- Une intervention donne au plus un litige. FR-034 `@edge` demande 409 sur
    -- une seconde tentative, et la contrainte le grave : rouvrir sur la même
    -- affaire multiplierait les examens sur un seul fait.
    mission_id  UUID NOT NULL UNIQUE REFERENCES mission (id) ON DELETE CASCADE,
    -- `SET NULL` : un litige survit à l'effacement du compte qui l'a ouvert.
    -- Il porte une décision qui engage l'autre partie, et le faire disparaître
    -- réécrirait son historique de sanctions.
    auteur_id   UUID REFERENCES utilisateur (id) ON DELETE SET NULL,
    partie      TEXT NOT NULL
        CONSTRAINT litige_partie_connue CHECK (partie IN ('USER', 'PROVIDER')),
    -- Vocabulaire fermé et asymétrique : les griefs des deux parties ne sont
    -- pas les mêmes. La cohérence entre partie et motif est tenue par le
    -- domaine, qui sait laquelle peut invoquer quoi.
    motif       TEXT NOT NULL
        CONSTRAINT litige_motif_connu CHECK (motif IN
            ('QUALITY', 'NOT_DONE', 'AMOUNT_DISPUTED', 'USER_NO_SHOW',
             'IMPOSSIBLE_CONDITIONS', 'OTHER')),
    description TEXT NOT NULL
        CONSTRAINT litige_description_suffisante
            CHECK (char_length(btrim(description)) BETWEEN 20 AND 2000),
    statut      TEXT NOT NULL
        CONSTRAINT litige_statut_connu CHECK (statut IN
            ('OPENED', 'RESOLVED_USER_FAVOR', 'RESOLVED_PROVIDER_FAVOR', 'CLOSED_NO_FAULT')),
    ouvert_le   TIMESTAMPTZ NOT NULL,
    -- Renseigné quand l'exploitation tranche (FR-036, pas encore livré).
    tranche_le  TIMESTAMPTZ,

    -- Un litige tranché porte une date de décision, un litige ouvert n'en a
    -- pas. Sans cette contrainte, un comptage « litiges perdus ce mois-ci »
    -- ne saurait pas sur quelle date se fonder.
    CONSTRAINT litige_date_si_tranche
        CHECK ((statut = 'OPENED') = (tranche_le IS NULL))
);

-- Le comptage des litiges perdus par un prestataire (FR-035) passe par la
-- Mission ; celui des litiges ouverts par un demandeur (FR-034 `@edge`) par
-- l'auteur. Deux index, deux questions.
CREATE INDEX litige_tranche_idx ON litige (statut, tranche_le);
CREATE INDEX litige_auteur_idx ON litige (auteur_id, ouvert_le);

-- Immuabilité du récit et du grief.
--
-- **Seul le statut et sa date bougent**, et c'est l'exploitation qui les
-- change. Laisser réécrire la description après coup permettrait d'adapter son
-- histoire à la décision qui se dessine, ce qui viderait l'examen de son objet.
CREATE OR REPLACE FUNCTION litige_recit_fige() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.mission_id  IS DISTINCT FROM OLD.mission_id
    OR NEW.partie      IS DISTINCT FROM OLD.partie
    OR NEW.motif       IS DISTINCT FROM OLD.motif
    OR NEW.description IS DISTINCT FROM OLD.description
    OR NEW.ouvert_le   IS DISTINCT FROM OLD.ouvert_le
    THEN
        RAISE EXCEPTION
            'le récit d''un litige ne se réécrit pas (FR-034) : seule la décision est modifiable'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER litige_recit_fige
    BEFORE UPDATE ON litige
    FOR EACH ROW EXECUTE FUNCTION litige_recit_fige();
