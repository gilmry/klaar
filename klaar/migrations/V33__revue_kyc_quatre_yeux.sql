-- Story 8.1 — la règle des quatre yeux ne vaut que pour le refus (FR-038).
--
-- **Corrige une contrainte trop large posée en V32.** Elle interdisait
-- `second_ops = premier_ops` pour toute revue, y compris une validation — qui
-- est pourtant close par son unique auteur, puisqu'elle n'attend personne. La
-- contrainte refusait donc toute validation, ce qu'un test d'intégration a
-- montré.
--
-- La règle qu'il fallait écrire est plus étroite, et c'est celle du FR : un
-- **refus** ne prend effet que confirmé par un autre compte. Valider et refuser
-- ne coûtent pas le même prix — une validation trop généreuse se corrige par
-- une suspension au premier incident ; un refus injuste ne se corrige pas,
-- l'entreprise est déjà partie voir ailleurs.
ALTER TABLE revue_kyc DROP CONSTRAINT revue_quatre_yeux;
ALTER TABLE revue_kyc ADD CONSTRAINT revue_quatre_yeux CHECK (
    decision = 'APPROVE'
    OR second_ops IS NULL
    OR second_ops <> premier_ops
);
