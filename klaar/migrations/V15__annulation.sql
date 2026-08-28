-- Story 3.5 — annulation par le demandeur (FR-014).

-- Motif d'annulation, vocabulaire fermé.
--
-- **Pas un texte libre.** FR-014 veut le motif « pour analytics ». Un champ
-- libre inviterait à écrire « le plombier d'hier était désagréable, j'habite au
-- 12 rue X » : une donnée personnelle non sollicitée, dans un champ dont la
-- finalité annoncée est statistique. Cinq codes servent la même analyse et ne
-- peuvent rien laisser fuir. La contrainte les impose, pour qu'une insertion
-- directe ne puisse pas ouvrir la porte que le domaine ferme.
--
-- Le motif vit sur la Demande et non dans un entrepôt d'analyse séparé : il
-- disparaît donc avec elle quand le compte est effacé (art. 17), sans qu'aucune
-- procédure de purge n'ait à s'en souvenir.
ALTER TABLE demande ADD COLUMN motif_annulation TEXT
    CONSTRAINT demande_motif_connu CHECK (
        motif_annulation IS NULL
        OR motif_annulation IN ('RESOLVED_ITSELF', 'TOO_SLOW', 'FOUND_ELSEWHERE', 'MISTAKE', 'OTHER')
    );

-- Un motif ne se comprend que sur une Demande annulée. Sans cette contrainte,
-- une Demande attribuée pourrait porter le motif d'une annulation qui n'a pas
-- eu lieu, et l'analyse compterait des annulations imaginaires.
ALTER TABLE demande
    ADD CONSTRAINT demande_motif_si_annulee CHECK (
        motif_annulation IS NULL OR statut = 'CANCELLED'
    );
