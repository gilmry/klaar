-- Story 4.3 — machine à états de la Mission (FR-018).

-- `ASSIGNED` devient `ACCEPTED`.
--
-- La Story 3.4 avait nommé cet état `ASSIGNED` faute que FR-013 le nomme ;
-- FR-018 l'appelle `ACCEPTED` dans toutes ses transitions. Aligner sur le PRD
-- coûte cette migration et évite d'entretenir un synonyme privé que personne ne
-- retrouverait en lisant les deux documents.
--
-- L'ordre compte, et le premier jet s'est trompé : PostgreSQL valide une
-- contrainte `CHECK` contre les lignes existantes au moment où on la pose. La
-- poser avant la mise à jour échoue donc sur les lignes encore `ASSIGNED`.
-- Retirer, mettre à jour, puis poser.
ALTER TABLE mission DROP CONSTRAINT mission_statut_connu;
UPDATE mission SET statut = 'ACCEPTED' WHERE statut = 'ASSIGNED';
ALTER TABLE mission ADD CONSTRAINT mission_statut_connu CHECK (
    statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE', 'COMPLETED', 'CANCELLED')
);

-- « Une Mission à la fois » porte désormais sur les états **occupants**.
--
-- La Story 3.4 avait laissé la note : « à étendre avec FR-018, faute de quoi un
-- prestataire ayant terminé une intervention resterait bloqué à vie ». C'est
-- fait ici. `COMPLETED` et `CANCELLED` le libèrent ; les trois autres non.
DROP INDEX mission_provider_en_cours_idx;
CREATE UNIQUE INDEX mission_provider_en_cours_idx ON mission (provider_id)
    WHERE statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE');

-- Historique des transitions (FR-018 `@security`).
--
-- **Append-only, comme la trace de matching et pour la même raison** : c'est la
-- preuve de ce qui s'est passé, et une preuve qu'on peut réécrire n'en est pas
-- une. Le déclencheur refuse `UPDATE` et `DELETE`.
--
-- La position est **facultative**. L'exiger rendrait l'autorisation de
-- géolocalisation de fait obligatoire, alors que quelqu'un sans GPS ou qui la
-- refuse doit pouvoir déclarer qu'il est arrivé. Son absence est une donnée en
-- soi, et `hors_zone` ne vaut jamais vrai sans position — ne pas savoir où
-- quelqu'un est n'est pas la même chose que le savoir ailleurs.
CREATE TABLE mission_transition (
    id            BIGSERIAL PRIMARY KEY,
    mission_id    UUID NOT NULL REFERENCES mission (id) ON DELETE RESTRICT,
    -- Recopié depuis la Mission plutôt que joint : FR-018 `@security` demande
    -- que l'entrée porte le `provider_id`, et une jointure dirait qui la porte
    -- *aujourd'hui*, pas qui l'a déclarée.
    provider_id   UUID NOT NULL REFERENCES provider (id) ON DELETE RESTRICT,
    statut        TEXT NOT NULL
        CONSTRAINT mission_transition_statut_connu CHECK (
            statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE', 'COMPLETED', 'CANCELLED')
        ),
    -- Quand le prestataire dit que c'est arrivé. Peut précéder l'enregistrement
    -- de quelques minutes : une transition faite hors connexion se synchronise
    -- plus tard, et écraser sa date réécrirait l'histoire.
    horodate_le   TIMESTAMPTZ NOT NULL,
    -- Quand le serveur l'a reçue.
    enregistre_le TIMESTAMPTZ NOT NULL,
    position      geography(Point, 4326),
    hors_zone     BOOLEAN NOT NULL DEFAULT FALSE,

    CONSTRAINT mission_transition_hors_zone_coherent CHECK (
        NOT hors_zone OR position IS NOT NULL
    )
);

CREATE INDEX mission_transition_mission_idx ON mission_transition (mission_id, id);
-- Sert la remontée d'alerte d'exploitation sur les sorties de zone.
CREATE INDEX mission_transition_hors_zone_idx ON mission_transition (enregistre_le DESC)
    WHERE hors_zone;

CREATE OR REPLACE FUNCTION mission_transition_immuable() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'mission_transition est append-only (FR-018) : % refusé', TG_OP
        USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER mission_transition_append_only
    BEFORE UPDATE OR DELETE ON mission_transition
    FOR EACH ROW EXECUTE FUNCTION mission_transition_immuable();
