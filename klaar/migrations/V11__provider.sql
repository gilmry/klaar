-- Story 1.6 (partielle) — prestataires (FR-003).
--
-- **Le contrôle KYC n'est pas fait.** FR-003 exige la validation du numéro à la
-- Banque-Carrefour des Entreprises, le contrôle de l'état de faillite et la
-- collecte d'une attestation d'assurance : l'API de la BCE, le stockage objet
-- chiffré et l'antivirus sont hors du périmètre vitrine.
--
-- Ce que la base impose à la place : un prestataire actif porte forcément
-- l'origine de son activation, et cette origine peut valoir 'DEMONSTRATION'.
-- Un prestataire actif sans contrôle réel se retrouve donc par une requête,
-- longtemps après que la commande qui l'a créé a été oubliée.

CREATE TABLE provider (
    id             UUID PRIMARY KEY,
    -- Le prestataire se connecte comme tout le monde : son compte est un
    -- compte utilisateur, et l'effacer emporte sa fiche prestataire.
    utilisateur_id UUID NOT NULL UNIQUE REFERENCES utilisateur (id) ON DELETE CASCADE,
    -- Dix chiffres, sans séparateur. La clé de contrôle est vérifiée par le
    -- domaine ; la base impose seulement la forme, qu'une insertion directe ne
    -- doit pas pouvoir contourner.
    numero_bce     CHAR(10) NOT NULL UNIQUE
        CONSTRAINT provider_bce_dix_chiffres CHECK (numero_bce ~ '^[01][0-9]{9}$'),
    raison_sociale TEXT NOT NULL
        CONSTRAINT provider_raison_non_vide CHECK (btrim(raison_sociale) <> '')
        CONSTRAINT provider_raison_bornee CHECK (char_length(raison_sociale) <= 200),
    -- Point de départ des interventions, d'où se calcule la distance.
    base           geography(Point, 4326) NOT NULL,
    statut         TEXT NOT NULL
        CONSTRAINT provider_statut_connu CHECK (statut IN ('PENDING_KYC', 'ACTIVE', 'SUSPENDED')),
    origine_kyc    TEXT
        CONSTRAINT provider_origine_connue CHECK (origine_kyc IN ('BCE', 'DEMONSTRATION')),
    -- Interrupteur simple, en attendant les plages horaires de la Story 3.7.
    -- Un prestataire actif mais indisponible ne reçoit rien : c'est deux
    -- notions distinctes, et les confondre ferait de « je suis en congé » une
    -- radiation.
    disponible     BOOLEAN NOT NULL DEFAULT FALSE,
    cree_le        TIMESTAMPTZ NOT NULL,

    -- Un prestataire actif porte toujours l'origine de son activation, et un
    -- prestataire en attente n'en porte aucune. C'est ce qui rend le contrôle
    -- vérifiable après coup plutôt que sur parole.
    CONSTRAINT provider_origine_coherente CHECK (
        (statut = 'PENDING_KYC' AND origine_kyc IS NULL)
        OR (statut <> 'PENDING_KYC' AND origine_kyc IS NOT NULL)
    )
);

CREATE INDEX provider_base_idx ON provider USING GIST (base);
-- Sert la recherche de la Story 3.2 : « les prestataires actifs et disponibles ».
CREATE INDEX provider_sollicitable_idx ON provider (statut, disponible)
    WHERE statut = 'ACTIVE' AND disponible;

CREATE TABLE provider_competence (
    provider_id  UUID NOT NULL REFERENCES provider (id) ON DELETE CASCADE,
    -- `RESTRICT` : retirer un secteur que des prestataires couvrent doit
    -- échouer bruyamment plutôt que les désaffilier en silence.
    secteur_code TEXT NOT NULL REFERENCES secteur (code) ON DELETE RESTRICT,
    PRIMARY KEY (provider_id, secteur_code)
);

CREATE INDEX provider_competence_secteur_idx ON provider_competence (secteur_code);
