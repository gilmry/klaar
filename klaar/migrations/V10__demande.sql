-- Story 3.1 — soumission d'une Demande (FR-011).

CREATE TABLE demande (
    id            UUID PRIMARY KEY,
    demandeur_id  UUID NOT NULL REFERENCES utilisateur (id) ON DELETE CASCADE,
    -- Référence au catalogue plutôt qu'un texte libre : un secteur renommé ou
    -- retiré doit se voir, pas se perdre dans des chaînes orphelines.
    -- `RESTRICT` : retirer un secteur qui porte des Demandes doit échouer
    -- bruyamment plutôt que les emporter.
    secteur_code  TEXT NOT NULL REFERENCES secteur (code) ON DELETE RESTRICT,
    description   TEXT NOT NULL
        CONSTRAINT demande_description_non_vide CHECK (btrim(description) <> '')
        CONSTRAINT demande_description_bornee CHECK (char_length(description) <= 2000),
    -- `geography` et non `geometry` : les calculs de distance se font en mètres
    -- sur l'ellipsoïde, sans que chaque requête ait à choisir une projection.
    -- C'est ce dont la Story 3.2 aura besoin pour son rayon de 5 km.
    position      geography(Point, 4326) NOT NULL,
    urgence       TEXT NOT NULL
        CONSTRAINT demande_urgence_connue CHECK (urgence IN ('LOW', 'NORMAL', 'HIGH')),
    statut        TEXT NOT NULL
        CONSTRAINT demande_statut_connu CHECK (statut IN ('BROADCASTING', 'NO_MATCH', 'CANCELLED')),
    cree_le       TIMESTAMPTZ NOT NULL
);

-- Sert la détection de doublon et le quota horaire, qui interrogent tous deux
-- « les Demandes de ce compte, récentes d'abord ».
CREATE INDEX demande_demandeur_recent_idx ON demande (demandeur_id, cree_le DESC);

-- Index spatial, pour la recherche par rayon de la Story 3.2. Posé maintenant :
-- l'ajouter sur une table déjà remplie prend un verrou, alors qu'il ne coûte
-- rien sur une table vide.
CREATE INDEX demande_position_idx ON demande USING GIST (position);

CREATE INDEX demande_statut_idx ON demande (statut) WHERE statut = 'BROADCASTING';

-- Méthodes de paiement (FR-006, Story 1.7).
--
-- La table existe, elle restera **vide** : l'enregistrement d'une carte passe
-- par Stripe Elements, et le compte Stripe est hors du périmètre vitrine. Elle
-- est créée ici parce que FR-011 en fait une précondition, et qu'un contrôle
-- qui interroge une table absente ne se teste pas.
--
-- Aucune donnée de carte n'y figure ni n'y figurera : seule la référence
-- opaque rendue par le prestataire de paiement, qui détient les données
-- réelles. C'est ce qui maintient le service hors du périmètre PCI-DSS.
CREATE TABLE methode_paiement (
    id             UUID PRIMARY KEY,
    utilisateur_id UUID NOT NULL REFERENCES utilisateur (id) ON DELETE CASCADE,
    -- Identifiant chez le prestataire de paiement. Jamais un numéro de carte.
    reference      TEXT NOT NULL,
    par_defaut     BOOLEAN NOT NULL DEFAULT FALSE,
    cree_le        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX methode_paiement_utilisateur_idx ON methode_paiement (utilisateur_id);
