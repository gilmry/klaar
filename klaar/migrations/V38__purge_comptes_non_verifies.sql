-- Index de la purge des comptes jamais vérifiés (Story 1.1, FR-001 `@security`).
--
-- Le balayage tourne toutes les dix secondes. Sans index, chacun de ses
-- passages lit la table `utilisateur` en entier, y compris les comptes actifs
-- qui forment l'écrasante majorité des lignes et que la purge ne peut de toute
-- façon pas toucher. Le coût grandit avec le nombre d'inscrits, pour une
-- requête qui la plupart du temps ne trouve rien.
--
-- Partiel sur le seul statut concerné : un compte vérifié quitte
-- `PENDING_EMAIL_VERIFY` et sort de l'index, si bien que celui-ci reste de la
-- taille du flux d'inscriptions en cours et non du fichier des comptes.
--
-- Sur `cree_le`, parce que c'est la borne de la purge et l'ordre de son
-- `LIMIT` : l'index sert la sélection et le tri d'un seul parcours.
CREATE INDEX utilisateur_non_verifie_cree_le_idx
    ON utilisateur (cree_le)
    WHERE statut = 'PENDING_EMAIL_VERIFY';
