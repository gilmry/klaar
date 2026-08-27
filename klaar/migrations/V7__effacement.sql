-- Story 1.9 — droit à l'effacement (FR-005, RGPD art. 17).

ALTER TABLE utilisateur
    DROP CONSTRAINT utilisateur_statut_connu,
    ADD CONSTRAINT utilisateur_statut_connu
        CHECK (statut IN ('PENDING_EMAIL_VERIFY', 'ACTIVE', 'ERASED_PENDING', 'ERASED')),
    -- Échéance du délai de grâce. Sa seule raison d'être est la réversibilité :
    -- un effacement immédiat n'aurait pas besoin de délai.
    ADD COLUMN efface_le TIMESTAMPTZ;

-- L'empreinte du mot de passe disparaît à l'effacement, comme l'exige FR-005.
-- Nullable plutôt qu'une valeur sentinelle : une chaîne impossible se
-- comparerait quand même à un mot de passe, et rien ne dirait au lecteur que
-- cette comparaison n'a pas de sens.
ALTER TABLE utilisateur ALTER COLUMN empreinte_mot_de_passe DROP NOT NULL;

-- Un compte effacé n'a plus d'empreinte, et un compte vivant en a
-- nécessairement une. La contrainte dit cette règle une fois, plutôt que de la
-- laisser au bon vouloir de chaque chemin d'écriture.
ALTER TABLE utilisateur
    ADD CONSTRAINT utilisateur_empreinte_coherente
    CHECK (
        (statut = 'ERASED' AND empreinte_mot_de_passe IS NULL)
        OR (statut <> 'ERASED' AND empreinte_mot_de_passe IS NOT NULL)
    );

-- Sert au relevé des effacements arrivés à échéance, que le binaire
-- `klaar-effacer` interroge. Partiel : seules les lignes en attente comptent.
CREATE INDEX utilisateur_effacement_du_idx
    ON utilisateur (efface_le)
    WHERE statut = 'ERASED_PENDING';
