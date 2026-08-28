-- Story 4.1 — le devis, une fois émis, ne change plus de prix (FR-016 `@security`).
--
-- **Pourquoi un déclencheur et pas une convention.** FR-016 `@security` demande
-- que « l'absence d'algorithme de fixation de prix soit auditable ». Un audit ne
-- vaut que si ce qu'il lit est ce qui a été présenté : une table où le montant
-- peut être réécrit après coup ne prouve rien, et le code applicatif qui ne le
-- fait pas aujourd'hui n'engage pas celui de demain.
--
-- **Ce qui reste modifiable, et seulement cela : le statut.** Un devis passe de
-- `SENT` à accepté, refusé ou expiré ; c'est sa vie normale. Le montant, le
-- taux, la TVA, le total, le délai, l'émetteur, la Mission et la date
-- d'émission sont figés. Le tout dans un seul déclencheur, parce que la liste
-- des colonnes gelées se lit alors d'un coup d'œil au lieu de se déduire de ce
-- qui manque.
--
-- **`DELETE` reste permis**, contrairement à `trace_matching`. Un devis n'est
-- pas une trace réglementaire : il porte le prix libre d'un prestataire, et il
-- disparaît légitimement avec la Mission dont il dépend quand un compte est
-- effacé (RGPD art. 17). Interdire la suppression ferait échouer l'effacement
-- d'un compte, ce qui est le contraire du but.
--
-- **Ce que ce déclencheur ne garantit pas** : quelqu'un qui a les droits de
-- superutilisateur peut le supprimer. C'est la même limite que partout ailleurs,
-- et elle est écrite dans `COMPLIANCE.md`.
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
            'un devis émis ne change plus de contenu (FR-016) : seul le statut est modifiable'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER devis_contenu_fige
    BEFORE UPDATE ON devis
    FOR EACH ROW EXECUTE FUNCTION devis_montant_fige();
