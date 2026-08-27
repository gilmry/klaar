# ADR-009 — Passage du code en licence MIT et renommage en Klaar

- **Statut** : Accepté — **remplace ADR-005**
- **Date** : 2026-08-27
- **Décideur** : Superviseur (humain)
- **Superviseur valideur** : [à signer]
- **ADR méthodologique associé** : déviation foyer/Manifeste Maury à tracer au meta-grain (seconde déviation, de sens inverse à la première)

## Contexte

ADR-005 avait retenu une licence propriétaire, en s'appuyant sur quatre prémisses commerciales explicites :

1. le matching algorithmique et le moteur de paiement constituent un avantage concurrentiel ;
2. la levée de fonds et l'attractivité investisseurs nécessitent la protectibilité du code ;
3. une publication ouverte exposerait les internals aux concurrents (Pagesdor, Recommandé, TaskRabbit, Uber/Bolt) ;
4. le modèle économique repose sur la take-rate, donc la différenciation technologique compte.

**Ces quatre prémisses ont cessé d'être vraies.** Le devis a été décliné le 27/07/2026 ; il n'y a ni société, ni levée, ni investisseurs, ni take-rate. Le dépôt est développé en vitrine de la Méthode Foyer, indépendamment du prospect d'origine. Une licence propriétaire protège désormais un actif qui ne génère aucun revenu, contre des concurrents qui n'en sont pas.

ADR-005 avait par ailleurs inscrit sa propre réversibilité dans sa justification :

> le choix est **réversible** (on peut toujours open-sourcer plus tard) → louer le réversible

Le présent ADR exerce cette option ; il ne contredit pas ADR-005, il en applique la clause de sortie.

## Décision

**Licence MIT** sur l'intégralité du dépôt public : workspace Rust (19 crates), frontend, IaC, configurations et livrables de conception publiés.

`Cargo.toml` du workspace passe de `LicenseRef-Proprietary` à `MIT` (hérité par les 19 crates via `license.workspace = true`) ; `deny.toml` retire `LicenseRef-Proprietary` de sa liste d'autorisation.

## Alternatives écartées

- **AGPL-3.0** — c'est la licence que la doctrine désigne (ADR-005 §Contexte : « l'AGPL-3.0 est la license canonique pour des dérivés cohérents avec cette doctrine »), et c'est l'équivalent côté code du *share-alike* du Manifeste Maury (CC BY-SA 4.0). Elle préserverait en outre l'optionalité : AGPL → MIT reste possible pour le titulaire des droits, l'inverse non. **Écartée au profit de la portée** : l'objectif retenu est que le code soit repris sans friction — vitrine, réutilisation, réputation — et non qu'il impose sa réciprocité aux dérivés. Arbitrage assumé, voir §Sagesse racine.
- **Maintien du propriétaire** — écarté : plus aucune prémisse d'ADR-005 ne tient.
- **Hybride (cœur fermé, périphérie ouverte)** — écarté pour la même raison qu'en ADR-005 (simplicité), et désormais sans objet puisqu'il n'y a plus de cœur à protéger.

## Conséquences

### Positives

- **Le concern C-5 du rapport de validation est largement résolu.** « Bus factor = 1 vs garantie de résultat » était le seul 🔴 survivant au passage en vitrine, et la mitigation demandée était une « clause de reprise, dépôt de code séquestré ». Un code ouvert rend le séquestre sans objet : plus personne ne peut être pris en otage par un intervenant unique.
- Contribution et relecture externes redeviennent possibles — ce qui répond directement au risque tracé en ADR-005 (« pas de contribution externe → le danger *assez fiable pour qu'on cesse de vérifier* doit être compensé par un enforcement substrat plus strict »).
- Vitrine effective de la Méthode Foyer : un dépôt lisible et exécutable démontre la méthode, un dépôt fermé la revendique seulement.
- Friction nulle avec la réutilisation d'Elevia (déjà MIT), dans les deux sens.

### Négatives / risques à tracer

- **Seconde déviation doctrinale, de sens inverse à la première.** ADR-005 déviait de *Mottainai* et *Ubuntu* en fermant le code. MIT dévie du *share-alike* : il autorise quiconque à fermer un dérivé sans rien rendre. La doctrine pointait AGPL ; le choix de la portée sur la réciprocité est **assumé et tracé ici**, il n'est pas un retour à la doctrine.
- **Irréversibilité.** Contrairement à ADR-005, cet ADR n'est **pas** réversible : la concession MIT est perpétuelle sur toute version publiée. Un ADR ultérieur pourrait durcir la licence des versions *futures*, jamais des versions déjà diffusées. Le « louer le réversible » ne s'applique plus.
- **Domaine réglementé publié.** Le dépôt contient des flux relevant de DSP2 (séquestre), de la loi Platform Work du 26/04/2024 (classification des travailleurs) et du RGPD (géolocalisation). La clause *AS IS* couvre la garantie, pas le fait qu'un tiers déploie ces flux sans DPIA ni analyse de conformité. Voir `COMPLIANCE.md`.
- **Conformité CRA inchangée** : le Cyber Resilience Act s'applique dès lors qu'il y a commercialisation ; il ne s'applique pas du seul fait de publier. Les obligations SBOM et reporting incident 24 h restent portées par les stories 0.10.

## Sagesse racine (manifeste) — arbitrage

MIT sert *Mottainai* (ne rien gâcher : un code fermé sans client est du travail perdu) et *Ubuntu* (relations : la réutilisation crée du lien), mais **abandonne la réciprocité** que le *share-alike* du Manifeste porte. C'est un arbitrage portée/réciprocité, tranché en faveur de la portée, dans un contexte où il n'y a rien à protéger et tout à démontrer.

## Actions liées

- [x] `klaar/LICENSE.md` — texte MIT
- [x] `klaar/Cargo.toml` — `license = "MIT"`
- [x] `klaar/deny.toml` — retrait de `LicenseRef-Proprietary`
- [x] `COMPLIANCE.md` — avertissement domaine réglementé
- [x] ADR-005 marqué *Remplacé par ADR-009*
- [ ] **Purge des documents commerciaux, historique git compris** — voir `COMPLIANCE.md` §Publication
- [x] Renommage `dep` → `klaar` : 19 crates, workspace, paquet npm `@klaar/client`,
      namespace de métriques `klaar_api`, utilisateur et base PostgreSQL, dashboard Grafana,
      workflow CI, hooks, livrables de conception. L'ancien nom était attaché au prospect d'origine.
- [ ] `LICENSE` à ajouter au dépôt `foyer` (public, sans fichier de licence à ce jour)
