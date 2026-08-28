-- Story 4.7 — annulation d'une Mission en cours et ses conséquences (FR-022).

-- **Un statut, pas deux.** FR-022 nomme `CANCELLED_USER` et
-- `CANCELLED_PROVIDER` ; la Mission reste ici en `CANCELLED`, et l'auteur vit
-- sur la ligne d'annulation. Aucune transition ne dépend de qui a annulé :
-- dédoubler le statut aurait obligé à répondre deux fois dans chaque `match` de
-- la machine à états pour une distinction qui n'en change aucun.
CREATE TABLE annulation_mission (
    id                     UUID PRIMARY KEY,
    -- Une Mission s'annule au plus une fois. La contrainte le grave : deux
    -- annulations feraient deux remboursements.
    mission_id             UUID NOT NULL UNIQUE REFERENCES mission (id) ON DELETE CASCADE,
    auteur                 TEXT NOT NULL
        CONSTRAINT annulation_auteur_connu
            CHECK (auteur IN ('CANCELLED_USER', 'CANCELLED_PROVIDER')),
    -- Le statut d'où la Mission a été annulée : c'est lui qui justifie le
    -- forfait, et le déduire après coup demanderait de relire tout l'historique.
    depuis                 TEXT NOT NULL
        CONSTRAINT annulation_depuis_connu
            CHECK (depuis IN ('ACCEPTED', 'PROVIDER_EN_ROUTE', 'ON_SITE')),
    -- Vocabulaire fermé : FR-022 `@security` demande que le motif serve à des
    -- statistiques, et un texte libre ne se compterait pas.
    motif                  TEXT
        CONSTRAINT annulation_motif_connu
            CHECK (motif IS NULL OR motif IN
                   ('NO_LONGER_NEEDED', 'UNAVAILABLE', 'DISAGREEMENT', 'NO_ACCESS', 'OTHER')),

    -- Tout en centimes. Zéro quand aucun devis n'avait été accepté : l'annulation
    -- ne coûte alors rien à personne.
    forfait_deplacement_cents BIGINT NOT NULL
        CONSTRAINT annulation_forfait_positif CHECK (forfait_deplacement_cents >= 0),
    remboursement_cents       BIGINT NOT NULL
        CONSTRAINT annulation_remboursement_positif CHECK (remboursement_cents >= 0),
    -- Vrai quand le prestataire s'est désisté : c'est ce que compte la règle des
    -- trois désistements en trente jours.
    penalise_le_prestataire   BOOLEAN NOT NULL,

    decidee_le             TIMESTAMPTZ NOT NULL
);

-- Le compteur de désistements d'un prestataire (FR-022 `@edge`) et celui des
-- annulations d'un demandeur lisent cette table sur une fenêtre glissante. Sans
-- ces index, chaque annulation la parcourrait entière.
CREATE INDEX annulation_penalite_idx ON annulation_mission (decidee_le)
    WHERE penalise_le_prestataire;
CREATE INDEX annulation_date_idx ON annulation_mission (decidee_le);

-- Immuabilité, pour les mêmes raisons que le devis et la libération : une
-- annulation consignée est un fait daté, pas un état qu'on ajuste.
CREATE OR REPLACE FUNCTION annulation_figee() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'annulation_mission est append-only (FR-022) : % refusé', TG_OP
        USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

-- `UPDATE` seulement : la suppression reste permise, parce qu'une annulation
-- disparaît légitimement avec la Mission dont elle dépend quand un compte est
-- effacé (RGPD art. 17). L'interdire ferait échouer l'effacement.
CREATE TRIGGER annulation_append_only
    BEFORE UPDATE ON annulation_mission
    FOR EACH ROW EXECUTE FUNCTION annulation_figee();
