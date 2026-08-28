-- Story 4.6 — validation de fin de Mission et libération (FR-021).

-- `VALIDATED` rejoint les statuts connus. La contrainte est remplacée et non
-- complétée : PostgreSQL ne sait pas étendre un CHECK.
--
-- **Les listes d'états « en cours » ne changent pas**, et c'est voulu : une
-- Mission validée n'occupe pas plus son prestataire qu'une Mission terminée.
-- Attendre la validation du demandeur pour libérer quelqu'un l'empêcherait de
-- travailler pendant trois jours.
ALTER TABLE mission DROP CONSTRAINT mission_statut_connu;
ALTER TABLE mission ADD CONSTRAINT mission_statut_connu CHECK (
    statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE', 'COMPLETED', 'VALIDATED', 'CANCELLED')
);

ALTER TABLE mission_transition DROP CONSTRAINT mission_transition_statut_connu;
ALTER TABLE mission_transition ADD CONSTRAINT mission_transition_statut_connu CHECK (
    statut IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE', 'COMPLETED', 'VALIDATED', 'CANCELLED')
);

-- Libération de l'argent d'une Mission (FR-021, FR-025).
--
-- **Une décision, pas un virement.** Le compte Stripe n'est pas ouvert : cette
-- table enregistre la répartition et le fait qu'elle soit autorisée ou en
-- attente d'un second regard. Le versement rejoindra l'Epic 5, et il lira ces
-- lignes plutôt que de recalculer — recalculer après un changement de taux de
-- commission réécrirait ce qui a été décidé.
CREATE TABLE liberation (
    id                   UUID PRIMARY KEY,
    -- Une Mission donne au plus une libération. La contrainte le grave plutôt
    -- que de s'en remettre au code : deux libérations pour une intervention
    -- feraient payer deux fois, et c'est le genre d'erreur qui se découvre au
    -- relevé bancaire.
    mission_id           UUID NOT NULL UNIQUE REFERENCES mission (id) ON DELETE CASCADE,
    -- Le devis qui fixe le montant. Conservé : la répartition ne se relit pas
    -- sans savoir sur quel accord elle portait.
    devis_id             UUID NOT NULL REFERENCES devis (id) ON DELETE RESTRICT,
    -- `RESTRICT` : supprimer un prestataire qui attend un versement doit
    -- échouer bruyamment.
    provider_id          UUID NOT NULL REFERENCES provider (id) ON DELETE RESTRICT,

    -- Tout en centimes, jamais en flottant (Architecture §1.1).
    total_ttc_cents      BIGINT NOT NULL CONSTRAINT liberation_total_positif CHECK (total_ttc_cents > 0),
    commission_htva_cents BIGINT NOT NULL CONSTRAINT liberation_commission_positive CHECK (commission_htva_cents >= 0),
    tva_commission_cents BIGINT NOT NULL CONSTRAINT liberation_tva_positive CHECK (tva_commission_cents >= 0),
    commission_ttc_cents BIGINT NOT NULL,
    reversement_cents    BIGINT NOT NULL CONSTRAINT liberation_reversement_positif CHECK (reversement_cents >= 0),

    -- L'invariant comptable, gravé : rien ne se crée, rien ne disparaît. Une
    -- erreur d'arrondi introduite un jour dans le calcul échouera ici plutôt
    -- que de se retrouver dans une comptabilité.
    CONSTRAINT liberation_somme_coherente
        CHECK (commission_ttc_cents + reversement_cents = total_ttc_cents),
    CONSTRAINT liberation_commission_coherente
        CHECK (commission_htva_cents + tva_commission_cents = commission_ttc_cents),

    origine              TEXT NOT NULL
        CONSTRAINT liberation_origine_connue CHECK (origine IN ('USER_VALIDATION', 'AUTO_RELEASE_72H')),
    statut               TEXT NOT NULL
        CONSTRAINT liberation_statut_connu CHECK (statut IN ('AUTHORISED', 'PENDING_OPS')),
    decidee_le           TIMESTAMPTZ NOT NULL
);

-- Le balayage des validations automatiques (FR-021 `@edge`) cherche les
-- Missions terminées depuis plus de soixante-douze heures. Il lit la date de la
-- transition vers `COMPLETED`, seule source qui dise **quand** l'intervention
-- s'est terminée.
CREATE INDEX mission_transition_terminee_idx
    ON mission_transition (horodate_le)
    WHERE statut = 'COMPLETED';

-- Immuabilité de la décision, pour les mêmes raisons que le devis.
--
-- Seul le statut bouge, et dans un seul sens : `PENDING_OPS` devient
-- `AUTHORISED` quand un second regard l'a validée. Les montants, eux, sont ce
-- qui a été décidé ce jour-là.
CREATE OR REPLACE FUNCTION liberation_montants_figes() RETURNS TRIGGER AS $$
BEGIN
    IF NEW.mission_id            IS DISTINCT FROM OLD.mission_id
    OR NEW.devis_id              IS DISTINCT FROM OLD.devis_id
    OR NEW.provider_id           IS DISTINCT FROM OLD.provider_id
    OR NEW.total_ttc_cents       IS DISTINCT FROM OLD.total_ttc_cents
    OR NEW.commission_htva_cents IS DISTINCT FROM OLD.commission_htva_cents
    OR NEW.tva_commission_cents  IS DISTINCT FROM OLD.tva_commission_cents
    OR NEW.commission_ttc_cents  IS DISTINCT FROM OLD.commission_ttc_cents
    OR NEW.reversement_cents     IS DISTINCT FROM OLD.reversement_cents
    OR NEW.origine               IS DISTINCT FROM OLD.origine
    OR NEW.decidee_le            IS DISTINCT FROM OLD.decidee_le
    THEN
        RAISE EXCEPTION
            'une libération prononcée ne change plus de montant (FR-021) : seul le statut est modifiable'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER liberation_montants_figes
    BEFORE UPDATE ON liberation
    FOR EACH ROW EXECUTE FUNCTION liberation_montants_figes();
