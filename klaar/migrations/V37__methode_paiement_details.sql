-- Story 1.7 — ce qu'on garde d'une carte, et ce qu'on ne garde pas (FR-006).
--
-- **Aucun numéro de carte, jamais.** Le périmètre PCI SAQ-A tient à cela : le
-- numéro est capté par l'iframe du prestataire de paiement et ne touche aucun
-- de nos serveurs. Ce qui est ajouté ici est ce que la norme autorise
-- explicitement à conserver, et rien de plus.
ALTER TABLE methode_paiement
    -- Les quatre derniers chiffres. Autorisés par PCI, et nécessaires : sans
    -- eux, quelqu'un qui a deux cartes ne peut pas distinguer celle qu'il
    -- supprime de celle qu'il garde.
    ADD COLUMN derniers_chiffres CHAR(4)
        CONSTRAINT methode_derniers_chiffres CHECK (derniers_chiffres ~ '^[0-9]{4}$'),
    -- La marque, pour l'affichage.
    ADD COLUMN marque TEXT
        CONSTRAINT methode_marque_bornee CHECK (char_length(marque) <= 32),
    -- **L'échéance, parce qu'une carte expire entre deux usages** (FR-006
    -- `@edge`). Sans elle, la Demande partirait et le paiement échouerait plus
    -- tard, quand le prestataire est déjà en route.
    ADD COLUMN expire_mois SMALLINT
        CONSTRAINT methode_mois_valide CHECK (expire_mois BETWEEN 1 AND 12),
    ADD COLUMN expire_annee SMALLINT
        CONSTRAINT methode_annee_valide CHECK (expire_annee BETWEEN 2000 AND 2100);

-- Les quatre vont ensemble : une carte à demi décrite ne s'affiche pas et ne se
-- contrôle pas.
ALTER TABLE methode_paiement ADD CONSTRAINT methode_details_complets CHECK (
    (derniers_chiffres IS NULL) = (expire_mois IS NULL)
    AND (expire_mois IS NULL) = (expire_annee IS NULL)
);

-- **Une seule carte par défaut et par compte.** Un index partiel plutôt qu'une
-- règle applicative : deux écritures concurrentes en poseraient deux, et le
-- service choisirait alors arbitrairement laquelle débiter.
CREATE UNIQUE INDEX methode_paiement_defaut_unique
    ON methode_paiement (utilisateur_id) WHERE par_defaut;
