-- Story 8.4 — comptes d'exploitation, rôles et seconde authentification (FR-041).

-- **Une table à part, et non un rôle sur `utilisateur`.** Un compte
-- d'exploitation n'a ni Demande, ni Mission, ni notation ; il regarde celles
-- des autres. Les mêler aurait donné à chaque requête de matching une colonne
-- « rôle » à ignorer, et à chaque revue de sécurité une question de plus.
CREATE TABLE compte_ops (
    id                    UUID PRIMARY KEY,
    -- Adresse professionnelle, identifiant de connexion. Unique, comme celle
    -- d'un utilisateur, et dans un espace de noms séparé : rien n'interdit à
    -- quelqu'un d'avoir les deux.
    email                 TEXT NOT NULL UNIQUE,
    empreinte_mot_de_passe TEXT NOT NULL,
    role                  TEXT NOT NULL
        CONSTRAINT compte_ops_role_connu
            CHECK (role IN ('SUPER_ADMIN', 'KYC_REVIEWER', 'MEDIATOR', 'READER')),
    -- Secret TOTP, `NULL` tant que la seconde authentification n'est pas
    -- configurée. FR-041 `@security` exige que le premier accès serve à cela et
    -- à rien d'autre.
    secret_totp           BYTEA
        CONSTRAINT compte_ops_secret_longueur
            CHECK (secret_totp IS NULL OR octet_length(secret_totp) >= 20),
    -- Dernier pas de temps TOTP accepté. **C'est ce qui ferme le rejeu** : sans
    -- lui, un code lu par-dessus une épaule reste utilisable une minute et
    -- demie, puisque la fenêtre de tolérance couvre trois pas de trente
    -- secondes.
    dernier_pas_totp      BIGINT,
    actif                 BOOLEAN NOT NULL DEFAULT TRUE,
    -- Sert à la révocation par inactivité (FR-041 `@edge`).
    derniere_activite     TIMESTAMPTZ NOT NULL,
    cree_le               TIMESTAMPTZ NOT NULL
);

-- Le balayage des comptes inactifs lit cette colonne sur les seuls comptes
-- encore actifs : un index partiel suffit et reste petit.
CREATE INDEX compte_ops_inactivite_idx ON compte_ops (derniere_activite) WHERE actif;

-- Journal des gestes d'exploitation (FR-042).
--
-- **Séparé du journal d'audit des utilisateurs**, et pour une raison de fond :
-- celui-ci enregistre ce que fait celui qui surveille. Les mélanger
-- permettrait à quelqu'un d'effacer sa propre trace en même temps qu'il purge
-- celles des autres, et rendrait illisible la question « qui a regardé quoi ».
CREATE TABLE journal_ops (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- `SET NULL` : la trace survit à la suppression du compte qui l'a produite.
    -- C'est même le cas où elle sert le plus.
    ops_id      UUID REFERENCES compte_ops (id) ON DELETE SET NULL,
    -- Ce qui a été fait, dans le vocabulaire des permissions.
    geste       TEXT NOT NULL,
    -- Sur quoi. Volontairement libre : un identifiant de Mission, de compte, ou
    -- une période exportée. Ce qui compte est de pouvoir revenir dessus.
    cible       TEXT,
    fait_le     TIMESTAMPTZ NOT NULL
);

CREATE INDEX journal_ops_date_idx ON journal_ops (fait_le DESC);
CREATE INDEX journal_ops_acteur_idx ON journal_ops (ops_id, fait_le DESC);

-- Strictement insert-only (FR-042 `@security` : « même un super-admin ne peut
-- modifier »).
CREATE OR REPLACE FUNCTION journal_ops_immuable() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'journal_ops est insert-only (FR-042) : % refusé', TG_OP
        USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER journal_ops_insert_only
    BEFORE UPDATE OR DELETE ON journal_ops
    FOR EACH ROW EXECUTE FUNCTION journal_ops_immuable();
