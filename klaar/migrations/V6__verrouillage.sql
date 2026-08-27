-- Story 1.8 — verrouillage anti-brute-force (FR-007, CyFun Basic).

ALTER TABLE utilisateur
    -- Échecs consécutifs dans la fenêtre glissante de dix minutes. Remis à zéro
    -- par une authentification réussie.
    ADD COLUMN echecs_consecutifs INT NOT NULL DEFAULT 0
        CONSTRAINT utilisateur_echecs_positifs CHECK (echecs_consecutifs >= 0),
    -- Sert à savoir si le dernier échec est encore dans la fenêtre. Sans lui,
    -- le compteur ne redescendrait jamais et verrouillerait un utilisateur
    -- simplement distrait sur plusieurs semaines.
    ADD COLUMN dernier_echec_le TIMESTAMPTZ,
    -- Non nul tant que le verrou tient. Aucune tâche de fond ne le remet à
    -- zéro : l'expiration se lit à la comparaison, ce qui évite un travail
    -- périodique pour un état qui se périme tout seul.
    ADD COLUMN verrouille_jusqu_a TIMESTAMPTZ;

-- Sert au relevé des comptes actuellement verrouillés (supervision, support).
-- Partiel : la très grande majorité des lignes a `verrouille_jusqu_a` nul et
-- n'a rien à faire dans l'index.
CREATE INDEX utilisateur_verrouille_idx
    ON utilisateur (verrouille_jusqu_a)
    WHERE verrouille_jusqu_a IS NOT NULL;
