-- Story 5.5 — journal des webhooks Stripe (FR-028).
--
-- **La table d'idempotence est la garantie, pas le code qui la lit.** Stripe
-- garantit l'« au moins une fois » : le même événement arrive deux ou trois
-- fois, et une capture rejouée prélèverait deux fois. Un `INSERT` sur une clé
-- primaire tranche la course entre deux réceptions simultanées, là où « lire
-- puis décider » laisserait les deux passer.
CREATE TABLE evenement_stripe (
    -- L'identifiant Stripe (`evt_…`) **est** la clé. Pas un UUID à nous : un
    -- second identifiant permettrait deux lignes pour un événement.
    id             TEXT PRIMARY KEY
        CONSTRAINT evenement_stripe_id_forme CHECK (id ~ '^evt_[A-Za-z0-9_]{1,250}$'),
    type_          TEXT NOT NULL,
    -- L'objet concerné : `pi_…`, `acct_…`, `tr_…`. C'est sur lui que se calcule
    -- l'ordre, puisque deux objets distincts n'ont pas d'ordre entre eux.
    objet_id       TEXT NOT NULL,
    -- **L'horodatage de Stripe**, et non celui de la réception. C'est lui qui
    -- donne l'ordre réel : deux webhooks arrivés à l'envers ont des dates de
    -- création qui, elles, ne mentent pas.
    cree_le        TIMESTAMPTZ NOT NULL,
    recu_le        TIMESTAMPTZ NOT NULL,
    -- Faux quand l'événement est arrivé après un plus récent déjà appliqué :
    -- il est consigné pour la trace sans que son effet soit rejoué.
    applique       BOOLEAN NOT NULL,
    -- Motif de non-application, en clair, pour que le journal se relise.
    suite          TEXT NOT NULL
        CONSTRAINT evenement_stripe_suite_connue
            CHECK (suite IN ('APPLIED', 'SUPERSEDED', 'IGNORED')),

    CONSTRAINT evenement_stripe_applique_coherent CHECK ((suite = 'APPLIED') = applique)
);

-- L'ordre se calcule par objet : « le dernier événement appliqué à ce
-- paiement ». Un index global sur la date ne répondrait pas à cette question.
CREATE INDEX evenement_stripe_objet_idx ON evenement_stripe (objet_id, cree_le DESC)
    WHERE applique;

-- Le journal se relit du plus récent au plus ancien lors d'un incident.
CREATE INDEX evenement_stripe_reception_idx ON evenement_stripe (recu_le DESC);

-- Insert-only, comme les autres journaux.
--
-- **Un événement traité ne se réécrit pas.** Pouvoir remettre `applique` à faux
-- permettrait de rejouer une capture en effaçant sa trace, c'est-à-dire de
-- contourner exactement ce que cette table protège.
CREATE OR REPLACE FUNCTION evenement_stripe_fige() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION
        'le journal des webhooks Stripe est en insertion seule (FR-028)'
        USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER evenement_stripe_fige
    BEFORE UPDATE OR DELETE ON evenement_stripe
    FOR EACH ROW EXECUTE FUNCTION evenement_stripe_fige();
