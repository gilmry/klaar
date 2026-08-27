-- Story 0.12 — abonnements Web Push (ADR-010).
--
-- RGPD : `endpoint` identifie un appareil et constitue à ce titre une donnée à
-- caractère personnel. La table est donc minimale (aucun agent utilisateur,
-- aucune adresse IP, aucun horodatage de connexion) et purgée dès qu'un
-- service de push déclare l'abonnement disparu.
CREATE TABLE push_subscription (
    id           UUID PRIMARY KEY,
    -- URL du service de push du navigateur. Unique : un même appareil ne doit
    -- pas recevoir deux fois la même notification.
    endpoint     TEXT        NOT NULL UNIQUE,
    -- Clé publique P-256 du navigateur, base64url, forme non compressée.
    p256dh       TEXT        NOT NULL,
    -- Secret d'authentification de 16 octets, base64url.
    auth         TEXT        NOT NULL,
    -- Rattachement au compte. Nullable et sans clé étrangère à ce stade : la
    -- table `utilisateur` n'existe pas encore (Epic 1). La contrainte sera
    -- ajoutée par la migration qui la crée, plutôt que de laisser croire à un
    -- lien qui n'est pas tenu.
    sujet_id     UUID,
    cree_le      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Dernier envoi réussi, pour repérer les abonnements dormants.
    dernier_envoi_le TIMESTAMPTZ
);

CREATE INDEX push_subscription_sujet_idx ON push_subscription (sujet_id);
