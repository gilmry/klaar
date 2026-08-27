-- Story 2.3 — fourchettes de prix indicatives par Secteur (FR-009).
--
-- Table **d'agrégat**, alimentée par un calcul périodique, et non lue à la
-- volée depuis l'historique des Missions. Deux raisons : le calcul balaie tout
-- l'historique d'un secteur et n'a pas sa place dans une requête servie au
-- visiteur ; et un agrégat figé se relit à l'identique tant qu'il n'a pas été
-- recalculé, ce qui rend l'`ETag` du catalogue stable.
--
-- Elle restera **vide** tant que les Missions n'existent pas (Epic 3). C'est
-- l'état attendu : FR-009 `@negative` prévoit qu'au lancement, aucune
-- fourchette ne s'affiche et que la mention « prix sur devis » la remplace.

CREATE TABLE fourchette_prix (
    secteur_code  TEXT PRIMARY KEY REFERENCES secteur (code) ON DELETE CASCADE,
    -- En centimes, comme tout montant : jamais de flottant (Architecture §1.1).
    min_cents     BIGINT      NOT NULL,
    max_cents     BIGINT      NOT NULL,
    -- Nombre de Missions retenues. Conservé pour que le seuil d'anonymat de
    -- FR-009 `@security` soit vérifiable après coup, et non seulement au
    -- moment du calcul.
    nb_missions   INT         NOT NULL
        CONSTRAINT fourchette_seuil_anonymat CHECK (nb_missions >= 5),
    calculee_le   TIMESTAMPTZ NOT NULL,

    CONSTRAINT fourchette_bornes_ordonnees CHECK (min_cents <= max_cents),
    -- Un prix négatif n'existe pas ; l'écrire signalerait un calcul faux plutôt
    -- qu'une donnée inhabituelle.
    CONSTRAINT fourchette_bornes_positives CHECK (min_cents >= 0)
);
