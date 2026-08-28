-- Story 7.4 — médiation d'un litige par l'exploitation (FR-036).

-- Ce que la décision ajoute au litige.
--
-- **Qui a tranché, et combien.** Sans le premier, une décision n'engage
-- personne et le journal d'exploitation reste la seule trace, à côté du dossier
-- plutôt que dedans. Sans le second, « remboursement partiel » ne veut rien
-- dire : le montant rendu dépendrait de qui relit.
ALTER TABLE litige
    ADD COLUMN tranche_par          UUID REFERENCES compte_ops (id) ON DELETE SET NULL,
    ADD COLUMN remboursement_cents  BIGINT
        CONSTRAINT litige_remboursement_positif CHECK (remboursement_cents >= 0);

-- Les trois vont ensemble ou aucune : un litige tranché sans montant, ou un
-- montant sans date, seraient un dossier à moitié écrit qu'il faudrait
-- interpréter au cas par cas.
ALTER TABLE litige
    ADD CONSTRAINT litige_decision_complete CHECK (
        (statut = 'OPENED') = (remboursement_cents IS NULL)
    );

-- `tranche_par` reste séparé de la contrainte ci-dessus : `ON DELETE SET NULL`
-- le remet à NULL quand le compte d'exploitation disparaît, et une décision ne
-- doit pas devenir invalide parce que celui qui l'a prise a quitté la société.

-- La console de médiation lit les litiges ouverts, du plus ancien au plus
-- récent : c'est celui qui approche des trente jours qui doit sauter aux yeux.
CREATE INDEX litige_ouverts_idx ON litige (ouvert_le) WHERE statut = 'OPENED';

-- Le récit reste figé ; la décision, elle, s'écrit une fois.
--
-- **Une décision ne se réécrit pas non plus.** La version précédente laissait
-- `statut` et `tranche_le` libres, ce qui permettait de rouvrir un litige
-- tranché — donc de revenir sur un remboursement déjà versé, et de vider la
-- première décision de sa valeur pour celui qu'elle a débouté. Le recours après
-- décision n'est pas dans le produit : il est chez le juge.
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

    IF OLD.statut <> 'OPENED' AND (
        NEW.statut              IS DISTINCT FROM OLD.statut
     OR NEW.tranche_le          IS DISTINCT FROM OLD.tranche_le
     OR NEW.remboursement_cents IS DISTINCT FROM OLD.remboursement_cents
    ) THEN
        RAISE EXCEPTION
            'un litige tranché ne se retranche pas (FR-036) : le recours est judiciaire'
            USING ERRCODE = 'restrict_violation';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
