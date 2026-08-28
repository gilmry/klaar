-- Story 4.8 — reprogrammation d'une intervention annulée (FR-023).

-- **L'invariant « une Demande donne au plus une Mission » devient « au plus une
-- Mission vivante ».**
--
-- C'était une contrainte d'unicité simple depuis V13, et elle disait quelque
-- chose de juste : deux Missions simultanées sur une Demande feraient partir
-- deux camionnettes. Ce qui change n'est pas cela — une Mission annulée n'envoie
-- personne. Reprogrammer, c'est précisément reprendre là où l'annulation s'est
-- arrêtée, avec le même devis et le même prestataire ; interdire une seconde
-- ligne obligerait à recréer une Demande, donc à rediffuser et à renégocier un
-- prix déjà convenu.
--
-- L'index partiel garde la garantie utile et lève celle qui gênait.
ALTER TABLE mission DROP CONSTRAINT mission_demande_id_key;
CREATE UNIQUE INDEX mission_demande_vivante_idx ON mission (demande_id)
    WHERE statut <> 'CANCELLED';

CREATE TABLE reprogrammation (
    id          UUID PRIMARY KEY,
    -- L'intervention annulée qu'on veut reprendre. Une seule proposition par
    -- intervention : en rouvrir plusieurs laisserait le prestataire devant
    -- deux demandes identiques.
    mission_id  UUID NOT NULL UNIQUE REFERENCES mission (id) ON DELETE CASCADE,
    -- Le devis dont le prix est repris. `RESTRICT` : le supprimer ferait perdre
    -- ce sur quoi les deux parties s'étaient mises d'accord.
    devis_id    UUID NOT NULL REFERENCES devis (id) ON DELETE RESTRICT,
    statut      TEXT NOT NULL
        CONSTRAINT reprogrammation_statut_connu
            CHECK (statut IN ('PROPOSED', 'ACCEPTED', 'DECLINED')),
    proposee_le TIMESTAMPTZ NOT NULL,
    -- La Mission née de l'acceptation, quand il y en a une.
    nouvelle_mission_id UUID REFERENCES mission (id) ON DELETE SET NULL,

    -- Une proposition acceptée a produit une Mission ; les autres non. Sans
    -- cette contrainte, un « accepté » sans Mission laisserait le demandeur
    -- devant une promesse que rien ne porte.
    CONSTRAINT reprogrammation_mission_si_acceptee
        CHECK ((statut = 'ACCEPTED') = (nouvelle_mission_id IS NOT NULL))
);

CREATE INDEX reprogrammation_attente_idx ON reprogrammation (proposee_le)
    WHERE statut = 'PROPOSED';
