-- Story 1.1 — comptes utilisateur, vérification d'adresse, journal d'audit
-- (FR-001).

CREATE TABLE utilisateur (
    id                     UUID PRIMARY KEY,
    -- Normalisée par le domaine (minuscules puis NFC) avant d'arriver ici.
    -- L'unicité est portée par la base et non par l'application : deux
    -- inscriptions concurrentes passent toutes deux un contrôle applicatif
    -- préalable, aucune ne passe cette contrainte (FR-001 `@edge`).
    email                  TEXT        NOT NULL UNIQUE,
    -- Chaîne PHC argon2id complète, paramètres compris.
    empreinte_mot_de_passe TEXT        NOT NULL,
    -- 'PENDING_EMAIL_VERIFY' ou 'ACTIVE'. Contrainte de valeurs plutôt
    -- qu'ENUM : ajouter un statut à un type ENUM PostgreSQL demande un ALTER
    -- TYPE non transactionnel, là où un CHECK se remplace dans la migration.
    statut                 TEXT        NOT NULL
        CONSTRAINT utilisateur_statut_connu
        CHECK (statut IN ('PENDING_EMAIL_VERIFY', 'ACTIVE')),
    locale                 TEXT        NOT NULL
        CONSTRAINT utilisateur_locale_supportee
        CHECK (locale IN ('fr', 'nl', 'en')),
    cree_le                TIMESTAMPTZ NOT NULL
);

-- Jetons de vérification d'adresse.
--
-- Le jeton n'est jamais conservé en clair : la colonne porte son empreinte
-- SHA-256. Une lecture de cette table ne permet donc pas d'activer les comptes
-- en attente.
CREATE TABLE jeton_verification_email (
    empreinte      CHAR(64) PRIMARY KEY,
    utilisateur_id UUID        NOT NULL REFERENCES utilisateur (id) ON DELETE CASCADE,
    expire_le      TIMESTAMPTZ NOT NULL,
    -- Non nul dès la première utilisation : c'est ce qui rend le jeton non
    -- rejouable, ce qu'un JWT autoporteur n'aurait pas permis.
    consomme_le    TIMESTAMPTZ
);

CREATE INDEX jeton_verification_email_utilisateur_idx
    ON jeton_verification_email (utilisateur_id);

-- Journal d'audit (FR-001, traçabilité NIS2).
--
-- Sans adresse IP ni agent utilisateur, comme les journaux applicatifs : ces
-- données sont personnelles et aucune finalité ni durée de conservation n'est
-- encore établie pour elles ici. Limite assumée, écrite dans COMPLIANCE.md.
CREATE TABLE journal_audit (
    id         BIGSERIAL PRIMARY KEY,
    code       TEXT        NOT NULL,
    -- Nul quand l'événement ne doit pas être relié à un compte : consigner le
    -- titulaire sur une tentative d'inscription en doublon ferait de ce
    -- journal l'oracle d'énumération que le reste du code évite.
    sujet_id   UUID,
    horodatage TIMESTAMPTZ NOT NULL
);

CREATE INDEX journal_audit_code_horodatage_idx ON journal_audit (code, horodatage);
CREATE INDEX journal_audit_sujet_idx ON journal_audit (sujet_id);

-- Dette annoncée par V2 : la colonne existait sans contrainte parce que la
-- table `utilisateur` n'existait pas encore. ON DELETE CASCADE plutôt que SET
-- NULL — un abonnement orphelin continuerait à recevoir les notifications d'un
-- compte supprimé, ce que l'effacement RGPD interdit.
ALTER TABLE push_subscription
    ADD CONSTRAINT push_subscription_sujet_fk
    FOREIGN KEY (sujet_id) REFERENCES utilisateur (id) ON DELETE CASCADE;
