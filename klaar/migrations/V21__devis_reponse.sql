-- Story 4.2 (partie hors séquestre) — réponse du demandeur à un devis (FR-017).

-- Motif du refus, vocabulaire fermé.
--
-- **Fermé, et non un champ libre.** Un texte libre serait une invitation à
-- écrire ce qu'on pense du prestataire, dans une donnée qu'il pourrait lire un
-- jour ; et il ne se compterait pas. Ces codes se comptent, ce qui permettra de
-- savoir si les refus viennent du prix ou du délai.
ALTER TABLE devis ADD COLUMN motif_refus TEXT
    CONSTRAINT devis_motif_connu
        CHECK (motif_refus IS NULL
               OR motif_refus IN ('TOO_EXPENSIVE', 'DELAY_TOO_LONG', 'NO_LONGER_NEEDED', 'OTHER'));

-- Un motif n'a de sens que sur un refus.
--
-- Sans cette contrainte, un devis accepté pourrait porter « trop cher », et
-- toute statistique construite dessus serait fausse sans que rien ne le dise.
ALTER TABLE devis ADD CONSTRAINT devis_motif_si_refuse
    CHECK (motif_refus IS NULL OR statut = 'REFUSED');

-- Le déclencheur de V20 gèle le contenu d'un devis émis ; le motif de refus
-- doit pouvoir être écrit **avec** le passage en `REFUSED`. Il n'entre donc pas
-- dans la liste des colonnes figées, et la fonction est remplacée telle quelle
-- pour que cette exception soit visible ici plutôt que devinée.
--
-- Le motif reste écrit une fois pour toutes, en pratique : il n'est posé que
-- par la transition `SENT` → `REFUSED`, et V20 interdit de revenir sur un
-- statut terminal côté application. Ce n'est pas la base qui le tient, et c'est
-- écrit plutôt que sous-entendu.
CREATE OR REPLACE FUNCTION devis_montant_fige() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.mission_id         IS DISTINCT FROM OLD.mission_id
    OR NEW.provider_id        IS DISTINCT FROM OLD.provider_id
    OR NEW.montant_htva_cents IS DISTINCT FROM OLD.montant_htva_cents
    OR NEW.taux_tva_bp        IS DISTINCT FROM OLD.taux_tva_bp
    OR NEW.tva_cents          IS DISTINCT FROM OLD.tva_cents
    OR NEW.total_ttc_cents    IS DISTINCT FROM OLD.total_ttc_cents
    OR NEW.delai_minutes      IS DISTINCT FROM OLD.delai_minutes
    OR NEW.cree_le            IS DISTINCT FROM OLD.cree_le
    THEN
        RAISE EXCEPTION
            'un devis émis ne change plus de contenu (FR-016) : seuls le statut et le motif de refus sont modifiables'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
