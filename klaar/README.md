# Klaar — workspace Cargo

Scaffolding **Sprint 0 / Story 0.1** (`docs/bmad-livrables/04-Epics-Stories.md`) : le monorepo Rust compile, la définition de faite du Sprint 0 n'est pas franchie pour autant (CI, Postgres, harnais contrat API, etc. restent à faire — Stories 0.2 à 0.12).

**Statut projet** : ce dépôt est développé en vitrine de la Méthode Foyer, indépendamment de la demande commerciale qui l'a fait naître et qui n'a pas abouti. Aucun provisioning payant (OVH, Stripe, itsme, Apple/Google) n'est activé.

## Structure

Suit `docs/bmad-livrables/03-Architecture.md` §Workspace Cargo, limitée au périmètre MVP (les crates d'extension J11/J12'/J13/J14 — `klaar-skills`, `klaar-surge`, `klaar-subscription`, `klaar-public-api`, `klaar-region`, `klaar-biometric-adapter`, `klaar-ml-adapter`, `klaar-insurance-adapter`, `klaar-authority-adapter`, `klaar-region-adapter` — ne sont pas scaffoldées, elles sont post-MVP et gated) :

- `crates/klaar-shared-kernel` — value objects communs (`Email`, `Geo`, `Money`, `VatRate`, `DistanceMeters`, `Locale`, `HashSha256`), seul crate avec une vraie logique métier à ce stade
- `crates/klaar-{identity,catalog,matching,intervention,payment,messaging,trust}` — les 7 bounded contexts cœur (Domain), stubs en attente d'implémentation epic par epic
- `crates/klaar-application` — ports + use cases (vide à ce stade)
- `crates/klaar-{sqlx-repos,stripe-adapter,itsme-adapter,geo-adapter,push-adapter,storage-adapter,av-adapter,audit-adapter,email-adapter}` — adapters Infrastructure MVP (stubs)
- `crates/klaar-api` — API HTTP (actix-web + utoipa, Story 0.5) : binaire `klaar-api` (serveur), `/api/v1/health`, doc OpenAPI sur `/api/v1/openapi.json`, Swagger UI sur `/api/v1/docs/`, métriques Prometheus sur `/metrics`, logs JSON structurés (Story 0.8)

## Utilisation

```sh
make bootstrap   # build + test, idempotent
make db-up       # Postgres + PostGIS local (docker compose)
make migrate     # applique les migrations refinery (idempotent)
make db-down
```

```sh
make hooks       # installe pre-commit (fmt+clippy+gitleaks si dispo) et pre-push (tests)
```

```sh
make codegen     # régénère packages/klaar-client/src/schema.d.ts depuis l'OpenAPI de klaar-api
```

```sh
docker compose up -d prometheus grafana   # + `cargo run -p klaar-api --bin klaar-api` en parallèle
# Grafana : http://localhost:3000 (auth anonyme activée en dev, admin/klaar_dev_only sinon)
# Prometheus : http://localhost:9090
```

## Stories Sprint 0 faites

- **0.1** — workspace Cargo (19 crates, `klaar-shared-kernel` avec value objects + 24 tests)
- **0.3** — PostgreSQL 16 + PostGIS local via docker compose, migrations refinery embarquées dans `klaar-api` (binaire `klaar-migrate`), idempotence vérifiée
- **0.9** — hooks Git locaux (`scripts/hooks/`) : pre-commit (fmt + clippy -D warnings + gitleaks si installé), pre-push (tests). **Limite assumée** : ne détecte pas "code sans test" par couverture différentielle (hors scope de ce scaffold) — seul le cargo fmt/clippy/tests est mécaniquement bloquant pour l'instant
- **0.4** — pipeline CI (`.github/workflows/ci.yml`, DRY avec les hooks locaux) : quality gate (fmt, clippy, tests), security gate (cargo-audit, cargo-deny, gitleaks), SBOM CycloneDX en artefact de build. `deny.toml` a fait remonter deux vrais problèmes corrigés au passage : dépendances internes en `path` sans version (« wildcard dependencies ») et absence de licence SPDX déclarée (`license = "MIT"` depuis ADR-009, qui renverse ADR-005)
- **0.5** — harnais contrat API : `klaar-api` sert désormais un vrai endpoint (`/api/v1/health`), sa doc utoipa (`/api/v1/openapi.json`) et Swagger UI (`/api/v1/docs/`), fuzzé par `schemathesis` en CI. **Limites connues** :
  - le check `unsupported_method` de schemathesis est exclu — actix-web renvoie 404 (pas 405) pour une méthode non déclarée sur un chemin donné, limitation de son routage par macro `#[get(...)]`. À corriger (fallback par chemin) quand le contrat aura plusieurs endpoints par route
  - `cargo audit` ignore **RUSTSEC-2026-0258** (h2 < 0.4.16, DoS par frames DATA vides) : vulnérabilité transitive via `actix-http` (toute la branche h2 0.3.x d'actix-web v4 en hérite, pas de version corrigée disponible en amont à ce jour). `klaar-api` n'est pas exposé publiquement à ce stade. **À revoir à chaque mise à jour de dépendance** — `cargo tree -i h2` pour vérifier si un correctif amont existe (même ignore répété dans `deny.toml`, cargo-deny a son propre check advisories indépendant de cargo-audit)
- **0.6** — codegen TS client partagé (`packages/klaar-client`, `@klaar/client`) : `openapi-typescript` génère `src/schema.d.ts` depuis `/api/v1/openapi.json` (`make codegen` ou `bash scripts/codegen.sh`, vérifié en CI). Fichiers générés non commités (`openapi.json`, `schema.d.ts` dans `.gitignore`) — régénérés à la demande depuis la seule source de vérité (le contrat servi par `klaar-api`), pas de package publié pour l'instant (aucun consommateur avant Tauri/admin, Story 0.2)
- **0.10 (partiel)** — SBOM CycloneDX signé (`cosign sign-blob` keyless via OIDC GitHub, pas de clé privée à gérer) et runbook incident NIS2 (`docs/runbook-incident.md`, procédure de notification CCB en 24h/72h/1 mois). **Non fait** : le runbook n'a pas été testé en jeu de rôle (DoD complet) — inapplicable tant qu'il n'y a ni équipe ops ni déploiement réel
- **0.8 (partiel)** — observabilité : métrique + log générés par requête, vérifié en conditions réelles (`/metrics` Prometheus scrapé, requête → ligne de log JSON avec `http.route`/`http.status_code`/`request_id`). Stack locale `docker compose up -d prometheus grafana`, dashboard provisionné automatiquement (`observability/grafana/dashboards/klaar-api.json` : requêtes/s par endpoint, latence p95). **Bug trouvé et corrigé en cours de route** : `tracing-actix-web` crée un span par requête mais rien ne l'imprime sans `fmt::layer().with_span_events(FmtSpan::CLOSE)` explicite — sans ça, aucune requête n'apparaissait dans les logs malgré `TracingLogger` actif. **Risque RGPD identifié et documenté (pas corrigé)** : le root span par défaut loggue `http.client_ip` (IP = donnée personnelle) et `http.user_agent` — sans conséquence tant que `/api/v1/health` est le seul endpoint, mais à corriger avant tout endpoint FR réel (voir commentaire dans `klaar-api/src/main.rs`). **Non fait** : trace distribuée (Tempo), AlertManager, plugin Sentry EU — DoD Story 0.8 complet demande une stack plus lourde que ce qui a du sens à scaffolder sans trafic réel à observer

## CI, premier run réel

Le premier run CI a échoué deux fois avant de passer, corrections gardées ici pour mémoire :
1. `cargo-deny-action` a un input `manifest-path` dédié ; le passer aussi via `arguments` duplique le flag
2. Le job contrat API compilait `klaar-api` à la volée avant de le lancer en arrière-plan puis d'attendre 20 s max : en CI à froid la compilation seule dépasse ce délai. Corrigé en compilant d'abord (`cargo build`), puis en laissant `schemathesis --wait-for-schema=30` gérer l'attente de démarrage du binaire déjà prêt

## Ce qui manque avant que le Sprint 0 soit réellement terminé

Stories 0.2, 0.7, 0.11, 0.12 (Tauri/admin, tile-server, PoC push). Les stories 0.7a/0.7b/0.7c (Terraform, salt-ssh, GitOps) nécessitent un compte OVH provisionné — bloquées tant qu'il n'y a pas de client payant. La Story 0.12 (PoC push) est bloquée par les comptes développeur Apple/Google (payants). Les Stories 0.8 et 0.10 restent partielles (cf. ci-dessus).
