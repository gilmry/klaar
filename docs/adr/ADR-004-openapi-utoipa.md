# ADR-004 — Contrat API OpenAPI : `utoipa` (vs `aide`)

- **Statut** : Accepté
- **Date** : 2026-07-18
- **Décideur** : Architecte (validé superviseur)
- **Superviseur valideur** : ✅ 2026-07-18

## Contexte

PRD §9bis.1 exige un **contrat API OpenAPI matérialisé** (foyer `contrat-api.md`, anti-piège OpenMajor) :
- Annotation exhaustive sur les ~50 endpoints
- Codegen client TypeScript (admin web + Tauri mobile)
- Désérialisation stricte `serde(deny_unknown_fields)`
- Contract tests CI (`schemathesis`)
- Source unique de vérité (pas de spec OpenAPI manuelle divergeant du code)

ADR-003 a fixé `actix-web` (override superviseur). Reste à choisir l'outil de génération OpenAPI.

## Décision

**`utoipa`** (v5+) + **`utoipa-swagger-ui`** (feature `actix-web`).

- Annotation par macros `#[derive(ToSchema)]` sur les DTOs
- `#[utoipa::path(...)]` sur les handlers actix-web (framework-agnostic)
- Génération runtime : `OpenApiDoc` servie sur `/api/v1/openapi.json` + `/api/v1/docs` (Swagger UI via `utoipa-swagger-ui` avec feature `actix-web`)
- Codegen client : `openapi-typescript` (admin web) + `openapi-generator -g typescript-axios` (Tauri)
- Contract tests : `schemathesis run` sur `openapi.json` à chaque PR (CI)

## Alternatives écartées

### `aide` (axum-native, no macros)
Concurrent légitime, écarté car :
- **Plus jeune** (v0.13, écosystème restreint)
- **Moins de tutoriels** "IA-ready" → tokens modèle + élevés (Brief H-9)
- **Macros Kakko** moins exhaustives que `utoipa` sur les cas edge (oneOf, allOf)
- Réévaluable si `utoipa` montre des limites sur OpenAPI 3.1 features

### Spécification OpenAPI manuelle (spec-first)
Écartée car :
- **Dérive garantie** entre spec et code (anti-pattern OpenMajor que foyer `contrat-api.md` dénonce)
- Maintenance double, codegen Rust depuis spec moins mature que annotation
- Plus lent en itération

### `poem`/` poem-openapi`
Écarté car ADR-003 a écarté poem.

## Conséquences

### Positives
- **Single source of truth** : le code Rust génère l'OpenAPI, jamais l'inverse
- **Codegen TS** : admin web (Astro+Svelte) et Tauri mobile partagent le même client
- **Désérialisation stricte** : `#[serde(deny_unknown_fields)]` sur les DTOs → rejet des payloads non conformes (anti-0-day)
- **Contract tests CI** : `schemathesis` fuzz l'API contre la spec à chaque PR
- **Swagger UI intégré** : `/api/v1/docs` facilite le développement ops
- **OpenAPI 3.0.x** support stable, écosystème large (Postman, Hoppscotch, Stoplight)

### Négatives / risques à tracer
- **Macros** peuvent ralentir la compilation (incremental compilation activée)
- **OpenAPI 3.1** support en cours d'amélioration (oneOf recursif encore imparfait) — workaround : `schema(Example)` pour les edge cases
- **Migrations de breaking changes** : tout changement de signature API = vérification de non-régression CI

## Sagesse racine (manifeste)

- **Sept générations** — contrat API documenté est lisible dans 30 ans même si le runtime change
- **DRY** — une seule définition (DTO Rust), consommée par backend + codegen + tests
- **Écologie des savoirs** — `utoipa` standard de-facto, compétences transférables
- **Mottainai** — refus du gaspillage : pas de spécification manuelle divergente

## Point irréversible

- Rupture de contrat API (changement majeur de version) = **irréversible** (consumers à mettre à jour)
- **Validation humaine** obligatoire pour tout bump `/v1` → `/v2` + ADR

## Suivi

- Sprint 0 : harnais `utoipa` + `utoipa_axum` + `openapi-typescript` + `schemathesis` CI (story habilitante — non optionnelle foyer `contrat-api.md`)
- Story de correction structurelle : si projet pré-existant sans ce harnais → blocker backlog
- Audit semestriel : coverage OpenAPI = 100 % des endpoints exposés
