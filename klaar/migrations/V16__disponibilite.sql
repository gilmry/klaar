-- Story 3.7 — disponibilité et rayon d'intervention du prestataire.

-- Distance au-delà de laquelle le prestataire ne se déplace pas.
--
-- **Le défaut est le maximum, et non une valeur médiane.** Les prestataires
-- déjà en base n'ont jamais exprimé de limite : leur en prêter une les
-- retirerait du service sans qu'ils aient rien demandé. Vingt kilomètres depuis
-- n'importe quel point de la Région la couvrent entièrement, donc ce défaut ne
-- change rien au comportement observé jusqu'ici.
--
-- Le plancher à mille mètres n'est pas cosmétique : en dessous, un prestataire
-- ne serait trouvé par presque personne et conclurait que le service ne marche
-- pas.
ALTER TABLE provider ADD COLUMN rayon_intervention_metres DOUBLE PRECISION NOT NULL DEFAULT 20000
    CONSTRAINT provider_rayon_borne CHECK (rayon_intervention_metres BETWEEN 1000 AND 20000);

-- Sert le filtre « prestataires sollicitables » de la Story 3.2, désormais
-- élargi au rayon propre et à l'occupation. L'index partiel existant porte déjà
-- sur (statut, disponible) ; celui-ci évite un balayage quand la recherche
-- redescend sur le rayon individuel.
CREATE INDEX provider_rayon_idx ON provider (rayon_intervention_metres)
    WHERE statut = 'ACTIVE' AND disponible;
