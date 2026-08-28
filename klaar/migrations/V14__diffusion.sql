-- Story 3.6 — fin de tour et élargissement du rayon (FR-015).

-- Rayon du tour en cours. Cinq kilomètres pour l'existant : c'est le rayon avec
-- lequel ces Demandes ont réellement été diffusées, et leur en prêter un autre
-- après coup rendrait leur trace de matching incohérente.
ALTER TABLE demande ADD COLUMN rayon_metres DOUBLE PRECISION NOT NULL DEFAULT 5000
    CONSTRAINT demande_rayon_positif CHECK (rayon_metres > 0);

-- Élargissements consommés. Le compteur ne se remet jamais à zéro : c'est ce
-- qui rend la limite de FR-015 effective.
ALTER TABLE demande ADD COLUMN elargissements SMALLINT NOT NULL DEFAULT 0
    CONSTRAINT demande_elargissements_bornes CHECK (elargissements BETWEEN 0 AND 3);

-- Début du tour de diffusion en cours, distinct de la date de création : un
-- élargissement rouvre une fenêtre entière, et la faire courir depuis la
-- création la rendrait déjà écoulée au moment où on l'offre.
--
-- `DEFAULT now()` le temps de la migration, puis repris sur `cree_le` : les
-- Demandes existantes n'ont jamais été élargies, leur tour a donc commencé à
-- leur création. Les dater de maintenant les ressusciterait toutes.
ALTER TABLE demande ADD COLUMN diffuse_depuis TIMESTAMPTZ NOT NULL DEFAULT now();
UPDATE demande SET diffuse_depuis = cree_le;
ALTER TABLE demande ALTER COLUMN diffuse_depuis DROP DEFAULT;

-- Sert le balayage de fin de tour : « les Demandes diffusées dont le tour est
-- écoulé ». Partiel, parce que c'est la seule population qui l'intéresse et
-- qu'elle reste petite même quand la table grossit.
CREATE INDEX demande_tour_echu_idx ON demande (diffuse_depuis)
    WHERE statut = 'BROADCASTING';
