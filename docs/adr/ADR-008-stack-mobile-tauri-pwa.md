# ADR-008 — Stack mobile : Tauri 2.0 + PWA uniquement (pas de réécriture native)

- **Statut** : **Remplacé par [ADR-010](ADR-010-stack-pwa-only.md)** le 27/08/2026 — Tauri est retiré, le client est une PWA Astro + Svelte.
- **Statut d'origine** : Accepté
- **Date** : 2026-07-18
- **Décideur** : Superviseur (décision structurante v0.3), formalisée par l'Architecte
- **Superviseur valideur** : ⏳ PENDING (à contresigner avec l'ensemble v0.3)
- **Livrables impactés** : Brief v0.4 §7/§16/§19, PRD v0.3 module E2', Architecture v0.2 §2, Epics v2.1 Epic 11, CBS v1.2 §Partie 2, Estimateur v2.1 §11, DEVIS §4.5

## Contexte

Le Brief v0.2 posait une **bascule native** au jalon J12 (« Native premium ») : réécriture du client mobile en React Native ou Flutter après un MVP livré en Tauri 2.0, chiffrée **1000-1600 h (100-160 k€)**.

Cette trajectoire créait trois problèmes structurels :

1. **Double codebase à maintenir** sur l'horizon 4-5 ans de la roadmap (Tauri pour l'existant, natif pour le nouveau), estimée 30-50 k€/an de maintenance additionnelle.
2. **Double compétence** à financer sur une équipe d'**un seul indépendant** (Rust + Svelte/Tauri *et* RN/Flutter), alors que le bus factor est déjà de 1 (cf. Validateur C-5).
3. **Réécriture jetant le travail déjà payé** par le client sur les jalons J0-J10 — contraire au principe *mottainai* et au modèle à la carte (chaque jalon payé doit rester acquis).

La question posée au superviseur : la bascule native est-elle **nécessaire**, ou seulement **héritée** d'un réflexe de place ?

## Décision

**La stack mobile reste Tauri 2.0 + PWA pour toute la roadmap (J0 → J14).**

**React Native et Flutter sont explicitement écartés** comme option planifiée. Ils ne subsistent dans les livrables que pour constater l'écart de coût.

Le jalon **J12 « Native premium »** devient **J12' « Enhancement Tauri/PWA continu »** :

| Sous-capacité | FR | Moyen retenu |
|---|---|---|
| E2'.1 Push rich media + deep-linking | FR-051 | Plugins Tauri standard |
| E2'.2 Secure storage + biométrie FaceID/TouchID | FR-052 | `tauri-plugin-biometric` + Stronghold (IOTA) |
| E2'.3 Géoloc background *(conditionnel)* | FR-053 | `tauri-plugin-geolocation`, **sous gate PoC** |
| E2'.4 Hotfix OTA | FR-054 | Tauri Updater |
| E2'.5 PWA grand public *(déplacé depuis E3.6)* | FR-055 | `pwa-public/` |

**Budget** : ~44 h accéléré / 100-200 h prudent, soit **4-20 k€** au lieu de 100-160 k€.

## Alternatives écartées

### Réécriture React Native
Écartée car : double codebase, double compétence, 1000-1600 h, et parité fonctionnelle à reconstruire depuis zéro sur des capacités déjà livrées et payées.

### Réécriture Flutter
Écartée pour les mêmes raisons, avec un facteur aggravant : Dart ajoute un **troisième langage** au projet (Rust backend + TypeScript/Svelte frontend + Dart), contre la cohérence recherchée par l'ADR-001.

### Approche hybride (Tauri + module natif ciblé pour la géoloc background)
Écartée **pour l'instant** : elle rouvrirait une chaîne de build native (Xcode/Gradle, provisioning, signature) pour une seule fonctionnalité, alors que le fallback foreground couvre l'usage principal. À réévaluer si — et seulement si — le PoC E2'.3 échoue **et** que la géoloc background devient une exigence commerciale bloquante.

## Conséquences

### Positives
- **Économie capex 95-155 k€** sur J12 → J12'.
- **Économie opex 150-250 k€ sur 5 ans** (une seule codebase à maintenir).
- **Une seule compétence** à tenir — cohérent avec un intervenant unique (Validateur C-5).
- **Cohérence ADR-001** (Rust partout) préservée sur toute la roadmap.
- **Capital client préservé** : aucun jalon payé n'est jeté par une réécriture.

### Négatives / risques à tracer
- ⚠️ **Le repli natif n'existe plus.** Tauri devient un point de passage obligé sans plan B de stack. Cette décision **aggrave le concern C-2** du Validateur : l'échec du PoC ne peut plus être rattrapé en basculant natif.
- **Géoloc background non garantie** au MVP : dépend du plugin Tauri (gate PoC Story 0.12). En cas d'échec → suivi **foreground uniquement**, UX dégradée. Ce point est **exclu de la garantie de résultat** et communiqué comme tel au client (DEVIS §4.5).
- **Plafond de performance** : si un besoin futur exige des animations 120 fps ou un accès matériel exotique, la décision devra être rouverte — au prix fort, puisque le rattrapage se fera alors sur une base plus large.
- **Dépendance à la trajectoire amont de Tauri Mobile** : l'écosystème mobile de Tauri 2.0 est plus jeune que RN/Flutter. Risque de plugin manquant à traiter au cas par cas.

## Sagesse racine (manifeste)

- **Mottainai** : ne pas jeter 1000-1600 h de travail déjà livré et payé pour reconstruire à l'identique.
- **Sobriété** : une codebase, un langage principal, une chaîne de build.
- **Répondre-de** : la réserve géoloc background est **écrite au client**, pas enfouie ; l'aggravation de C-2 est nommée dans le rapport de validation.
- **Écologie des savoirs** : ne pas exiger d'un intervenant unique qu'il tienne deux écosystèmes mobiles.

## Point irréversible

- Choix de stack mobile : **réversible en théorie, coûteux en pratique** — une bascule native ultérieure coûterait davantage que les 1000-1600 h initiales, la base fonctionnelle à répliquer étant plus large.
- **Gate de réévaluation** : fin S2 (PoC géoloc background). Un échec ne rouvre **pas** le natif ; il acte le fallback foreground.
- **Validation humaine** : ⏳ PENDING — à contresigner par le superviseur avec l'ensemble v0.3.

## Suivi

- **Sprint 0 / S2** — Story 0.12 étendue : PoC push **+ PoC géoloc background**. Gate J12'.
- **Si PoC géoloc échoue** : activer le fallback PWA foreground (Story 11.5), informer le client, ne pas rouvrir le natif.
- **Jalon J12'** (post-MVP, à la carte) : activation des plugins `biometric`, `stronghold`, `deep-linking`, `geolocation`.
- **Indicateur de réouverture** : si ≥ 2 exigences produit nécessitent un accès natif non couvert par un plugin Tauri, porter la réouverture de cet ADR au superviseur.
