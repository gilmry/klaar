-- Story 1.4 — contexte de session (FR-004 `@security`, binding).

-- Empreinte SHA-256 de l'agent utilisateur au moment de l'authentification.
--
-- RGPD : l'agent utilisateur est une donnée personnelle, qui contribue à
-- l'empreinte du navigateur. Il n'est donc pas conservé — seule son empreinte
-- l'est, et à une seule fin : repérer qu'un refresh est présenté depuis un
-- contexte différent de celui qui l'a obtenu. C'est une mesure de sécurité au
-- sens de l'art. 32 RGPD, pas une mesure d'analyse d'audience.
--
-- Nullable : les sessions ouvertes avant cette migration n'en ont pas, et une
-- session sans contexte connu ne doit pas être traitée comme une anomalie.
ALTER TABLE session_refresh ADD COLUMN empreinte_contexte CHAR(64);

-- Remplace l'index de famille par un index composite : la rotation cherche
-- toujours « les sessions vivantes de cette famille », jamais la famille seule.
DROP INDEX IF EXISTS session_refresh_famille_idx;
CREATE INDEX session_refresh_famille_vivante_idx
    ON session_refresh (famille_id, revoque_le);
