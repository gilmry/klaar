-- Story 8.1 — une entreprise refusée ou retirée ne porte aucune origine de
-- contrôle (FR-038).
--
-- **Corrige une contrainte devenue fausse par extension du vocabulaire.** V11
-- disait : « un prestataire en attente ne porte aucune origine, tout autre en
-- porte une ». C'était juste quand les seuls autres statuts étaient `ACTIVE` et
-- `SUSPENDED`, qui supposent tous deux une activation passée. `REJECTED` et
-- `WITHDRAWN` (V32) ne l'ont jamais été : leur imposer une origine de contrôle
-- reviendrait à inscrire au dossier un contrôle qui n'a pas eu lieu.
--
-- La règle correcte est : **l'origine accompagne l'activation, et elle seule.**
ALTER TABLE provider DROP CONSTRAINT provider_origine_coherente;
ALTER TABLE provider ADD CONSTRAINT provider_origine_coherente CHECK (
    (statut IN ('ACTIVE', 'SUSPENDED') AND origine_kyc IS NOT NULL)
    OR (statut IN ('PENDING_KYC', 'REJECTED', 'WITHDRAWN') AND origine_kyc IS NULL)
);
