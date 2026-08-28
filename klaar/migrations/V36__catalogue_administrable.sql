-- Story 2.4 — administration du catalogue par l'exploitation (FR-010).
--
-- **Un secteur naît en brouillon, et un autre compte le publie.** Publier un
-- secteur le rend proposable à toute la Région : les Demandes s'y rangeront,
-- les prestataires s'y déclareront compétents, et le retirer ensuite laissera
-- des Missions orphelines. Le geste est irréversible en pratique, d'où la
-- seconde paire d'yeux — la même règle que pour un refus de contrôle
-- d'entreprise (FR-038), et pour la même raison : ce qui ne se défait pas se
-- décide à deux.
ALTER TABLE secteur
    ADD COLUMN statut TEXT NOT NULL DEFAULT 'PUBLISHED'
        CONSTRAINT secteur_statut_connu
            CHECK (statut IN ('DRAFT', 'PUBLISHED', 'DISABLED')),
    -- `SET NULL` : le secteur survit au départ de qui l'a créé ou publié.
    ADD COLUMN cree_par     UUID REFERENCES compte_ops (id) ON DELETE SET NULL,
    ADD COLUMN cree_le      TIMESTAMPTZ,
    ADD COLUMN publie_par   UUID REFERENCES compte_ops (id) ON DELETE SET NULL,
    ADD COLUMN publie_le    TIMESTAMPTZ;

-- **Les cinq secteurs du MVP restent publiés.** Le `DEFAULT 'PUBLISHED'`
-- ci-dessus les couvre : les basculer en brouillon retirerait le catalogue
-- entier au premier déploiement de cette migration, c'est-à-dire casserait le
-- service pour ajouter une fonction d'administration.
--
-- Ils n'ont ni auteur ni approbateur, et c'est exact : ils viennent du
-- peuplement initial, pas d'une décision d'exploitation. Inventer un compte
-- pour remplir la colonne écrirait une décision qui n'a pas eu lieu.

-- **Publier exige les deux paires d'yeux, et la base le grave.** Un secteur
-- publié par son propre créateur ne serait pas une validation, ce serait un
-- second clic.
ALTER TABLE secteur ADD CONSTRAINT secteur_quatre_yeux CHECK (
    publie_par IS NULL OR cree_par IS NULL OR publie_par <> cree_par
);

-- La date de publication accompagne la sortie du brouillon — **mais seulement
-- pour les secteurs qui viennent de la console.**
--
-- La première écriture de cette contrainte exigeait `publie_le` de tout secteur
-- publié, et refusait donc les cinq du peuplement initial : ils sont publiés et
-- n'ont jamais été approuvés par personne. Le déploiement a échoué là-dessus,
-- ce qui était la bonne façon de l'apprendre. Leur inventer une date et un
-- approbateur aurait écrit une décision qui n'a pas eu lieu ; `cree_par IS
-- NULL` dit exactement ce qu'ils sont — un jeu de départ, pas un geste
-- d'exploitation.
ALTER TABLE secteur ADD CONSTRAINT secteur_publication_complete CHECK (
    cree_par IS NULL OR (statut = 'DRAFT') = (publie_le IS NULL)
);

-- La lecture publique ne montre que les secteurs publiés ; l'index le sert.
CREATE INDEX secteur_publies_idx ON secteur (ordre) WHERE statut = 'PUBLISHED';
