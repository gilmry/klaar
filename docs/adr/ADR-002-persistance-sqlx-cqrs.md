# ADR-002 — Persistance : `sqlx` CQRS SQL pur (vs ORM)

- **Statut** : Accepté
- **Date** : 2026-07-18
- **Décideur** : Architecte (validé superviseur)
- **Superviseur valideur** : ✅ 2026-07-18

## Contexte

PRD §10 modèle de données : **30 tables PostgreSQL** avec contraintes complexes :
- **PostGIS** (`requests.geo`, `availabilities.geo`, `evidence_photos.exif_geo`) — géoloc < 5 km
- **Argent** en cents (BIGINT), TVA en basis points (INT)
- **Audit log** partitionnée mensuellement (scale)
- **Transition d'états** atomicité CAS pour matching (FR-013 race condition)
- **Wilson score** pour rating (FR-037)
- **JSONB** pour `matches.criteria` (Trace AI Act)

L'archétype foyer = **stateful full-stack**. ADR-001 a fixé Rust + PostgreSQL. Reste à choisir entre **CQRS SQL pur (`sqlx`)** et **ORM (`SeaORM` / `Diesel`)**.

## Décision

**`sqlx` CQRS SQL pur** : requêtes SQL explicites dans des **repositories** dédiés par bounded context, sans ORM.

- Migrations : **`sqlx-cli`** ou **`refinery`** (idempotents, versionnés)
- Type-safe queries : `sqlx::query_as!::<T>` (validation compilation)
- Transactions : `sqlx::Transaction<'_, Postgres>` explicites
- Tests : base test **sqlx::test** (rollback auto)

## Alternatives écartées

### `SeaORM` (ORM async, dérivé Diesel)
Écarté car :
- **Magie de mapping** masque les requêtes → N+1 silencieux
- **Abstraction leaky** avec PostGIS (fallback SQL brut pour géoloc)
- **Migration des types argent** maladroite (Decimal vs BIGINT cents)
- **Couche d'indirection** supplémentaire = dette technique (Manifeste §1.2)

### `Diesel` (ORM sync, le plus mature)
Écarté car :
- **Sync-only** : incompatible avec axum async (besoin de `tokio` runtime)
- **Schema macro** rigide, migrations manuelles
- **PostGIS** support communautaire incomplet
- Écosystème davantage tourné vers Elixir-style static typing

### `Cornucopia` (codegen SQL → Rust)
Concurrent légitime, écarté car :
- Plus jeune que `sqlx`, moins de tutoriels "IA-ready"
- Apporte peu sur les requêtes dynamiques (filtering par paramètres)
- **Réévaluable** au prochain jalon de capacité si `sqlx` montre des limites

## Conséquences

### Positives
- **Performance maîtrisée** : requêtes SQL explicites, EXPLAIN visible, pas de magie
- **Alignée hexagonale** : repositories = adapters infrastructure, Domain reste pur
- **PostGIS natif** : `SELECT ... WHERE geo <-> $1 < 5000` écrit explicitement
- **Audit AI Act** : `criteria JSONB` queryable, pas de sérialisation ORM opaque
- **Argent exact** : BIGINT cents sans Decimal type gymnastics
- **Compétence transférable** : SQL reste SQL — l'équipe n'est pas enfermée dans un ORM
- **Migrations versionnées** avec `refinery` ou `sqlx-cli` (DRY avec la CI)

### Négatives / risques à tracer
- **Plus verbeux** que ORM : ~20 % de code SQL en plus
- **Pas de magic migrations** : le team doit écrire ses migrations SQL (investissement wall-clock initial)
- **Type-safe repose sur macros** : `query_as!` nécessite `DATABASE_URL` à la compilation (CI setup)
- **Story habilitante Sprint 0** : configuration `sqlx-cli` + `refinery` + CI data preparing (point de concours Gantt)

## Sagesse racine (manifeste)

- **Mottainai** — refus du gaspillage : SQL explicite est la **forme juste** pour un état riche (PostGIS, argent, JSONB), pas du sur-mesure inutile
- **Écologie des savoirs** — l'équipe maîtrise SQL (compétence universelle) plutôt qu'un ORM spécifique (sous-culture)
- **Sept générations** — `sqlx` durable, langage-agnostique, code lisible dans 30 ans
- **DRY** — repositories partagés par BC, pas de duplication

## Point irréversible

- Choix persistance : **réversible** (refactor SQL ↔ ORM possible mais coûteux)
- Migrations de schéma déjà appliquées en production : **irréversibles** (backup obligatoire)
- **Validation humaine** : ✅ Superviseur

## Suivi

- Sprint 0 : harnais `sqlx` + `refinery` + tests d'intégration DB (story habilitante)
- Si performance insuffisante à J3 (1 000 users) → réévaluer `Cornucopia` ou ajouter couche de cache (Redis)
- ADR-003 axum gère l'async DB pooling via `sqlx::PgPool`
