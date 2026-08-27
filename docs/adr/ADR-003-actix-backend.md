# ADR-003 — Framework web backend : `actix-web` (vs `axum`)

- **Statut** : Accepté (override superviseur)
- **Date** : 2026-07-18
- **Décideur** : Superviseur (override proposition Architecte)
- **Superviseur valideur** : ✅ 2026-07-18

## Contexte

ADR-001 a fixé Rust pour le backend cloud. Le choix du framework web est structurant :
- ~50 endpoints API REST (PRD §9bis.3)
- 2 endpoints WebSocket (tracking géoloc FR-019, messagerie FR-030)
- Async DB pooling (via ADR-002 `sqlx`)
- Hexagonal : handlers = adapters, logique dans Application layer
- Middleware : JWT auth, rate-limiting, request ID, structured logging

L'Architecte avait proposé `axum` ; le superviseur **impose `actix-web`**. Ce choix engage le projet — *répondre-de* porté par le superviseur, l'Architecte trace.

## Décision

**`actix-web`** (v4+).

- Version : **actix-web 4.x** (stable, mature)
- Runtime : `tokio` (compatible via `actix-rt`)
- HTTP server : `actix-http` (performances top-tier Techempower)
- Middleware : système natif actix-web (`wrap()`)
- WebSocket : `actix-web-actors` (chat FR-030, tracking FR-019)
- OpenAPI : voir ADR-004 (`utoipa` framework-agnostic, intégré via macros directes + `utoipa-swagger-ui` feature `actix-web`)

## Alternatives écartées

### `axum` (proposition initiale Architecte)
Écartée par le superviseur car :
- **Maturité relative** vs actix-web (axum 0.7 = 2024 ; actix-web 4.x en production depuis 2022)
- **Benchmarks** légèrement inférieurs à actix-web (5-10 %)
- **Écosystème actix** éprouvé en production à grande échelle
- Préférence superviseur pour un framework plus ancien et battle-tested (répondre-de : fiabilité avant modernité)

### `poem`, `rocket`, `warp`
Écartées (raisons identiques à la proposition initiale : communauté EU/US restreinte, macros invasives, courbe raide).

## Conséquences

### Positives
- **Maturité éprouvée** : actix-web utilisé en production par de nombreux acteurs, documentation riche
- **Performance** : top-tier Techempower (parmi les plus rapides du marché), marge pour 287 req/s cible et au-delà
- **Système d'extracteurs type-safe** : `web::Path<Uuid>`, `web::Json<DTO>`, `web::Data<AppState>`
- **Middleware mature** : CORS, rate-limit, traces, auth JWT — tous disponibles
- **WebSocket via `actix-web-actors`** : pattern actor éprouvé pour les connexions longue durée (tracking temps réel, chat)
- **Écosystème actix** : `actix-cors`, `actix-files`, `actix-identity`, `actix-web-httpauth` prêts à l'emploi
- **Intégration utoipa** : `utoipa` framework-agnostic, fonctionne via `#[utoipa::path]` + `utoipa-swagger-ui` feature `actix-web` (ADR-004 inchangé dans son principe)
- **tokio-compatible** : `sqlx`, `reqwest` (Stripe, itsme), `tokio-postgres` fonctionnent nativement

### Négatives / risques à tracer
- **Modèle actor** (`actix` framework) : paradigme distinct, complexité (notamment pour WebSocket actors)
- **Double runtime potentiel** : actix-rt vs tokio pur — utiliser `actix-rt` comme runtime principal (qui wrap tokio) pour éviter la friction
- **Breaking changes** entre versions majeures (4.x stable, 5.x à venir) : pinning strict, migration documentée
- **Code légèrement plus verbeux** que axum extracteurs pour certains cas
- **Story habilitante Sprint 0** : configurer actix + actix-web-actors pour WebSocket + tests d'intégration

## Sagesse racine (manifeste)

- **Sept générations** — actix-web mature, code lisible dans 30 ans, communauté établie
- **Répondre-de** — le superviseur assume le choix d'un framework éprouvé plutôt qu'émergent : la fiabilité long-courrier prime sur la modernité
- **Écologie des savoirs** — actix-web = compétences largement disponibles sur le marché Rust (recrutement plus facile que axum)

## Point irréversible

- Choix framework web : **réversible** (refactor possible mais coûteux — extraction hexagonale facilite le swap des adapters API)
- **Validation humaine** : ✅ Superviseur (override)

## Suivi

- Sprint 0 : bootstrap `actix-web` 4.x + `actix-web-actors` pour WebSocket + extracteurs custom (JWT auth via `actix-web-httpauth`, request ID)
- Monitoring : `tracing-actix-web` + `tracing-subscriber` + OpenTelemetry vers Loki/Tempo
- Si > 1000 req/s requis (J4+) : réévaluer scaling horizontal + load balancer
- Story habilitante : PoC actix-web + WebSocket + sqlx Pool + 1 endpoint healthcheck en Sprint 0
