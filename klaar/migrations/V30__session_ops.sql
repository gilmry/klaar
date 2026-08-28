-- Story 8.3 — session d'exploitation (FR-040, FR-041).
--
-- **Pourquoi elle manquait, et pourquoi elle est nécessaire.** Les routes
-- d'exploitation se ré-authentifiaient à chaque requête par paramètres d'URL :
-- adresse, mot de passe et code TOTP. C'est tenable pour un appel en ligne de
-- commande, et intenable pour une console de navigateur — le mot de passe
-- passerait par la barre d'adresse, l'historique, l'en-tête `Referer` et les
-- journaux d'accès du serveur. Et comme le code TOTP tourne toutes les trente
-- secondes, il faudrait soit le redemander sans cesse, soit garder le mot de
-- passe en mémoire de page.
--
-- La session résout les deux : les identifiants ne circulent qu'une fois, au
-- `POST /ops/login`, dans un corps de requête.
CREATE TABLE session_ops (
    -- Empreinte SHA-256 du jeton, comme pour `session_refresh`. Le jeton
    -- lui-même n'est jamais conservé : une lecture de cette table ne permet pas
    -- d'usurper une session d'exploitation, et c'est précisément la table qu'un
    -- attaquant irait lire.
    empreinte  CHAR(64) PRIMARY KEY,
    ops_id     UUID NOT NULL REFERENCES compte_ops (id) ON DELETE CASCADE,
    cree_le    TIMESTAMPTZ NOT NULL,
    -- **Trente minutes, sans prolongation.** Une session d'exploitation donne
    -- accès à des dossiers nominatifs et à des décisions sur l'argent d'autrui ;
    -- une session qui se renouvelle à chaque clic finit ouverte toute la
    -- journée sur un poste partagé. Repasser par le code TOTP toutes les
    -- demi-heures est le prix de ces droits-là.
    expire_le  TIMESTAMPTZ NOT NULL
        CONSTRAINT session_ops_expire_apres CHECK (expire_le > cree_le),
    -- Non nul après déconnexion explicite.
    revoque_le TIMESTAMPTZ
);

-- Sert la déconnexion de toutes les sessions d'un compte, et le balayage.
CREATE INDEX session_ops_compte_idx ON session_ops (ops_id);
CREATE INDEX session_ops_expiration_idx ON session_ops (expire_le);
