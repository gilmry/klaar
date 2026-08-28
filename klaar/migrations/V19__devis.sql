-- Story 4.1 — envoi d'un Devis par le prestataire attribué (FR-016).

CREATE TABLE devis (
    id                 UUID PRIMARY KEY,
    -- `CASCADE` : un devis n'existe que pour une Mission. Si la Mission
    -- disparaît avec la Demande dont elle est née — ce qui n'arrive
    -- aujourd'hui qu'à l'effacement d'un compte (FR-010) — le devis n'a plus
    -- ni objet ni destinataire.
    mission_id         UUID NOT NULL REFERENCES mission (id) ON DELETE CASCADE,
    -- Recopié plutôt que déduit par jointure. FR-016 `@security` demande un
    -- journal « timestamp + Provider ID + montant » : les trois doivent se
    -- lire sur la ligne, y compris si la Mission change de main un jour.
    provider_id        UUID NOT NULL REFERENCES provider (id) ON DELETE RESTRICT,

    -- Tous les montants en centimes, jamais en flottant (Architecture §1.1).
    montant_htva_cents BIGINT NOT NULL
        CONSTRAINT devis_montant_positif CHECK (montant_htva_cents > 0)
        -- 10 000 € HTVA : Klaar est un service de dépannage, pas de chantier.
        -- La borne protège aussi d'une faute de frappe à deux zéros près, qui
        -- serait autrement pré-autorisée sur la carte du demandeur.
        CONSTRAINT devis_montant_borne CHECK (montant_htva_cents <= 1000000),
    -- Taux en points de base : 2100, 1200 ou 600 (Architecture §6.5). Les
    -- trois taux belges applicables à une intervention à domicile, et pas un de
    -- plus : accepter 300 laisserait émettre une facture fiscalement fausse.
    taux_tva_bp        SMALLINT NOT NULL
        CONSTRAINT devis_taux_belge CHECK (taux_tva_bp IN (2100, 1200, 600)),
    -- TVA et total **conservés**, pas recalculés à la lecture. Le taux peut
    -- changer, et un devis relu dans deux ans doit montrer ce qui a été
    -- présenté ce jour-là. Recalculer réécrirait un document contractuel.
    tva_cents          BIGINT NOT NULL
        CONSTRAINT devis_tva_positive CHECK (tva_cents >= 0),
    total_ttc_cents    BIGINT NOT NULL
        CONSTRAINT devis_total_coherent CHECK (total_ttc_cents = montant_htva_cents + tva_cents),

    delai_minutes      INT NOT NULL
        CONSTRAINT devis_delai_borne CHECK (delai_minutes BETWEEN 1 AND 1440),
    note               TEXT,
    -- Référence de la preuve justifiant un taux réduit (FR-016 `@edge`).
    -- Obligatoire dès que le taux n'est pas le taux normal : sans elle, tout
    -- devis passerait à 6 %, et c'est nous qui aurions documenté la fraude.
    preuve_tva_reduite TEXT
        CONSTRAINT devis_preuve_si_taux_reduit
            CHECK (taux_tva_bp = 2100 OR preuve_tva_reduite IS NOT NULL),

    statut             TEXT NOT NULL
        CONSTRAINT devis_statut_connu
            CHECK (statut IN ('SENT', 'ACCEPTED', 'REFUSED', 'EXPIRED')),
    cree_le            TIMESTAMPTZ NOT NULL,
    expire_le          TIMESTAMPTZ NOT NULL
        CONSTRAINT devis_expire_apres_creation CHECK (expire_le > cree_le)
);

-- Un seul devis en attente de réponse par Mission.
--
-- **Un index partiel, et non un contrôle applicatif.** Lire puis insérer
-- laisserait deux envois simultanés poser deux devis, et le demandeur verrait
-- deux prix pour la même intervention sans savoir lequel l'engage. L'index le
-- ferme au niveau où les écritures sont sérialisées, au prix d'une erreur à
-- traduire, ce que l'adaptateur fait.
CREATE UNIQUE INDEX devis_un_seul_en_cours_idx ON devis (mission_id)
    WHERE statut = 'SENT';

-- Les devis d'une Mission, du plus récent au plus ancien : c'est la lecture du
-- suivi comme celle du compteur des trois envois (FR-016 `@edge`).
CREATE INDEX devis_mission_idx ON devis (mission_id, cree_le DESC);

-- Le balayage des expirations (FR-016 `@edge`) ne lit que les devis en attente
-- dont l'heure est passée. Sans cet index, il parcourrait toute la table à
-- chaque passage.
CREATE INDEX devis_echu_idx ON devis (expire_le) WHERE statut = 'SENT';
