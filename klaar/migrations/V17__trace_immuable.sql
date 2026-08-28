-- Story 3.8 — trace immuable et signée (FR-012 `@security`, AI Act art. 12).

-- Signature HMAC-SHA256 de la ligne, chaînée sur la précédente.
--
-- **Nullable, et c'est une dette assumée.** Les lignes écrites avant cette
-- migration n'ont pas de signature, et il n'y a aucune façon honnête de leur en
-- fabriquer une : les signer après coup dirait qu'elles ont été scellées à
-- l'écriture, ce qui serait faux. Le rapport d'audit les compte à part et dit
-- pourquoi.
ALTER TABLE trace_matching ADD COLUMN signature BYTEA
    CONSTRAINT trace_signature_longueur CHECK (signature IS NULL OR octet_length(signature) = 32);

-- Maillon précédent, pour rejouer la chaîne sans dépendre de l'ordre des `id`.
-- `NULL` pour le premier maillon signé.
ALTER TABLE trace_matching ADD COLUMN signature_precedente BYTEA
    CONSTRAINT trace_signature_precedente_longueur CHECK (
        signature_precedente IS NULL OR octet_length(signature_precedente) = 32
    );

-- Tête de chaîne, une seule ligne.
--
-- La contrainte sur `unique_ligne` est le procédé habituel pour une table à
-- ligne unique : la colonne ne peut valoir que `TRUE`, et elle est clé
-- primaire. Une seconde tête rendrait la chaîne ambiguë.
CREATE TABLE trace_chaine (
    unique_ligne       BOOLEAN PRIMARY KEY DEFAULT TRUE
        CONSTRAINT trace_chaine_ligne_unique CHECK (unique_ligne),
    derniere_signature BYTEA
        CONSTRAINT trace_chaine_signature_longueur CHECK (
            derniere_signature IS NULL OR octet_length(derniere_signature) = 32
        ),
    mis_a_jour_le      TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO trace_chaine (unique_ligne, derniere_signature) VALUES (TRUE, NULL);

-- Immuabilité (AI Act art. 12).
--
-- **Un déclencheur, pas une convention.** « On ne modifie pas cette table » est
-- une phrase ; ceci est un refus. Il porte sur `UPDATE` et sur `DELETE`, y
-- compris ceux qui viendraient d'un `ON DELETE CASCADE` : supprimer une Demande
-- échouera donc bruyamment plutôt que d'emporter sa trace en silence.
--
-- **Tension réelle avec le droit à l'effacement (RGPD art. 17).** L'effacement
-- d'un compte est ici une anonymisation et non un `DELETE`, donc aucune cascade
-- ne se déclenche aujourd'hui. Le jour où quelqu'un voudra supprimer une ligne
-- de `demande`, ce déclencheur l'en empêchera, et ce sera la bonne réponse :
-- l'art. 17 §3 b) réserve le cas des traitements imposés par une obligation
-- légale, ce qu'est cette trace. La trace ne porte du reste ni nom, ni adresse,
-- ni description : deux identifiants, un score et une distance.
--
-- **Ce que ce déclencheur ne garantit pas** : quelqu'un qui a les droits de
-- superutilisateur sur la base peut le supprimer. C'est la signature chaînée
-- qui couvre ce cas, et elle-même ne couvre pas la compromission complète du
-- serveur, où la clé est lisible. Voir `COMPLIANCE.md`.
CREATE OR REPLACE FUNCTION trace_matching_immuable() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'trace_matching est append-only (AI Act art. 12) : % refusé', TG_OP
        USING ERRCODE = 'restrict_violation';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trace_matching_append_only
    BEFORE UPDATE OR DELETE ON trace_matching
    FOR EACH ROW EXECUTE FUNCTION trace_matching_immuable();

-- Sert la vérification de la chaîne et les agrégats du rapport d'audit, qui
-- parcourent la trace dans l'ordre d'écriture sur une période.
CREATE INDEX trace_matching_ordre_idx ON trace_matching (id);
