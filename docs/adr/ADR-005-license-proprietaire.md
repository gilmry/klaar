# ADR-005 — License propriétaire du code Klaar

- **Statut** : **Remplacé par [ADR-009](ADR-009-license-mit.md)** le 27/08/2026 — le code passe en licence MIT.
  Conservé pour la traçabilité du raisonnement : les quatre prémisses commerciales ci-dessous ont cessé d'être vraies (devis décliné, pas de société, pas de levée, pas de take-rate).
- **Statut d'origine** : Accepté
- **Date** : 2026-07-18
- **Décideur** : Superviseur (humain)
- **Superviseur valideur** : [à signer]
- **ADR méthodologique associé** : déviation foyer/Manifeste Maury à tracer au meta-grain

## Contexte

Le framework foyer et le Manifeste Maury (CC BY-SA 4.0) valorisent l'open source (Ubuntu, écologie des savoirs). L'AGPL-3.0 est la license canonique pour des dérivés cohérents avec cette doctrine.

Cependant, Klaar est un projet commercial destiné à un marché concurrentiel (marketplace services) où :
- Le code du **matching algorithmique** et du **moteur de paiement** constitue un avantage compétitif;
- La levée de fonds et l'attractivité investisseurs nécessitent la protectibilité du code;
- La publication AGPL exposerait les internals aux concurrents (Pagesdor, Recommandé, TaskRabbit si entrée BE, Uber/Bolt si pivot);
- Le modèle économique repose sur la take-rate — la différenciation technologique compte.

## Décision

**License propriétaire (closed source)** sur l'intégralité du code Klaar (backend Rust hexagonal, Tauri mobile, admin web Astro+Svelte, IaC, configs).

## Alternatives écartées

- **AGPL-3.0 (OSS complet)** : écartée pour les raisons ci-dessus. Le *répondre-de* serait plus fort sur la communauté, mais plus faible sur la viabilité économique du projet.
- **Hybride (core closed + peripherals OSS)** : écartée pour la simplicité initiale. À réévaluer à un jalon de capacité ultérieur si Klaar stabilise des briques mutualisables (ex: composants i18n BE FR/NL/EN, adapters itsme) qui pourraient être open-sourcées sans risque.

## Conséquences

### Positives
- Protectibilité de l'avantage compétitif (matching, paiement, données)
- Attractivité investisseurs (levée fonds plus facile)
- Pas d'obligation de publication des évolutions
- Maîtrise totale de la roadmap

### Négatives / risques à tracer
- **Déviation foyer/Manifeste Maury** : la doctrine pousse AGPL. Cette déviation est **assumée** (dépendance assumée, pas subie — `arbitrage-hybride.md` §4) et tracée au meta-grain (Boucle-de-retroaction.md §meta-grain).
- **Pas de contribution externe** : la communauté ne peut pas relire/améliorer le code → le *danger « assez fiable pour qu'on cesse de vérifier »* doit être compensé par un **enforcement substrat plus strict** (CI, protection de branche, audits sécurité externes annuels).
- **Pas d'effet réseau OSS** : le recrutement de développeurs Rust se fera sur salaire/équité, pas sur la passion communautaire.
- **Conformité CRA** : si Klaar est commercialisé (ce qui est le cas), le **Cyber Resilience Act** s'applique plein régime (obligations SBOM, reporting incident 24 h dès sept. 2026 — angle mort foyer `conformite.md` §5) → stories habilitantes à prévoir.

## Sagesse racine (manifeste) — arbitrage

Cette décision **dévie** de *Mottainai* (partage) et *Ubuntu* (relations) au profit de :
- **Réversibilité** (arbitrage-hybride foyer) : le choix est **réversible** (on peut toujours open-sourcer plus tard) → louer le réversible;
- **Répondre-de** : la viabilité économique du projet est une **condition** de la durée (Manifeste conviction 1 : « préférer un logiciel qui dure 30 ans »). Sans viabilité économique, pas de projet de 30 ans.

## Point irréversible

- License initiale du code : choix **réversible** (open-source possible plus tard), mais les **contributions externes reçues** sous license propriétaire créent une irréversibilité (relicense complexe). → Ne pas accepter de contributions externes avant décision définitive.
- **Validation humaine** : ✅ Superviseur — *« Pourrai-je en répondre, et devant qui ? »*

## Suivi

- Tracer dans le Manifeste Maury §4.4.11 (méta-boucle) : déviation « AGPL canonique → propriétaire pour projet commercial en marché concurrentiel » comme ADR méthodologique partageable (ShareAlike).
- Story habilitante CRA à ajouter au Sprint 0 : SBOM CycloneDX + procédure reporting incident ENISA/CSIRT.
- Audit de sécurité externe annuel pour compenser l'absence de revue communautaire.
