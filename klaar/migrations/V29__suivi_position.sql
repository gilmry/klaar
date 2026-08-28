-- Story 4.4 — suivi de position pendant le trajet (FR-019).

-- Consentement au partage, **par intervention** (FR-019 `@security`).
--
-- **Une table et non une colonne sur `provider`.** Un consentement global
-- vaudrait pour toutes les interventions passées et futures, ce qui n'est pas
-- un consentement au sens du RGPD : il doit être spécifique et éclairé. Une
-- ligne par Mission le rend révocable et daté, et son absence bloque le suivi.
CREATE TABLE consentement_suivi (
    mission_id  UUID PRIMARY KEY REFERENCES mission (id) ON DELETE CASCADE,
    consenti_le TIMESTAMPTZ NOT NULL,
    -- Retiré à tout moment : le consentement se révoque (RGPD art. 7 §3), et
    -- une révocation qui demanderait de supprimer la ligne effacerait la preuve
    -- qu'il avait été donné.
    retire_le   TIMESTAMPTZ,
    CONSTRAINT consentement_retire_apres CHECK (retire_le IS NULL OR retire_le >= consenti_le)
);

-- Positions relevées pendant le trajet.
--
-- **Déjà dégradées à cinquante mètres à l'écriture.** Dégrader à l'affichage
-- laisserait la donnée fine ici, c'est-à-dire là où une fuite la prendrait et
-- là où une réquisition la trouverait. La minimisation (RGPD art. 5.1.c) porte
-- sur ce qui est conservé, pas sur ce qui est montré.
CREATE TABLE position_suivi (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    mission_id UUID NOT NULL REFERENCES mission (id) ON DELETE CASCADE,
    position   geography(Point, 4326) NOT NULL,
    hors_zone  BOOLEAN NOT NULL,
    relevee_le TIMESTAMPTZ NOT NULL
);

-- La lecture du suivi ne veut que la dernière position ; la purge veut les plus
-- anciennes. Le même index sert les deux.
CREATE INDEX position_suivi_mission_idx ON position_suivi (mission_id, relevee_le DESC);

-- Trajet agrégé, conservé après la purge (FR-019 `@security`).
--
-- **Ce qui reste quand les positions disparaissent** : une distance et une
-- durée, qui ne disent pas où quelqu'un est passé. C'est ce qui permet de
-- mesurer un temps d'intervention sans garder la trace des déplacements.
CREATE TABLE trajet_agrege (
    mission_id      UUID PRIMARY KEY REFERENCES mission (id) ON DELETE CASCADE,
    distance_metres DOUBLE PRECISION NOT NULL
        CONSTRAINT trajet_distance_positive CHECK (distance_metres >= 0),
    duree_secondes  BIGINT NOT NULL
        CONSTRAINT trajet_duree_positive CHECK (duree_secondes >= 0),
    releves         INT NOT NULL CONSTRAINT trajet_releves_positifs CHECK (releves >= 0),
    calcule_le      TIMESTAMPTZ NOT NULL
);
