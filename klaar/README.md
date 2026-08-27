# Klaar — workspace Cargo

Monorepo de Klaar : un workspace Cargo (backend Rust, architecture hexagonale) et une PWA Astro + Svelte dans `web/`.

**Statut projet** : ce dépôt est développé en vitrine de la Méthode Foyer, indépendamment du prospect d'origine (devis décliné le 27/07/2026). Aucun provisioning payant (OVH, Stripe, itsme) n'est activé — et depuis **ADR-010**, plus aucun n'est requis côté client : la bascule PWA a retiré Tauri et, avec lui, les comptes développeur Apple/Google.

## Structure

Suit `docs/bmad-livrables/03-Architecture.md` §Workspace Cargo, limitée au périmètre MVP (les crates d'extension J11/J12'/J13/J14 — `klaar-skills`, `klaar-surge`, `klaar-subscription`, `klaar-public-api`, `klaar-region`, `klaar-biometric-adapter`, `klaar-ml-adapter`, `klaar-insurance-adapter`, `klaar-authority-adapter`, `klaar-region-adapter` — ne sont pas scaffoldées, elles sont post-MVP et gated) :

- `crates/klaar-shared-kernel` — value objects communs (`Email`, `Geo`, `Money`, `VatRate`, `DistanceMeters`, `Locale`, `HashSha256`), seul crate avec une vraie logique métier à ce stade
- `crates/klaar-{identity,catalog,matching,intervention,payment,messaging,trust}` — les 7 bounded contexts cœur (Domain), stubs en attente d'implémentation epic par epic
- `crates/klaar-application` — ports + use cases (vide à ce stade)
- `crates/klaar-{sqlx-repos,stripe-adapter,itsme-adapter,geo-adapter,push-adapter,storage-adapter,av-adapter,audit-adapter,email-adapter}` — adapters Infrastructure MVP (stubs)
- `web/` — **PWA Astro + Svelte** (Story 0.2, ADR-010) : coquille installable, service worker, queue d'écritures hors-ligne IndexedDB. Remplace le `tauri-app/` prévu par ADR-008
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
make frontend        # installe et construit la PWA (web/)
make frontend-test   # tests unitaires de la queue hors-ligne (vitest + fake-indexeddb)
cd web && npx playwright test          # tests e2e dans un vrai navigateur
```

> Là où le CDN de Playwright est inaccessible (il répond 403 depuis certaines
> régions), installer un Chrome système et lancer les e2e avec
> `KLAAR_PLAYWRIGHT_CHANNEL=chrome`. Le navigateur diffère alors de celui de la
> CI, ce qu'il faut savoir en lisant un résultat local.

```sh
make msrv        # vérifie que le rust-version déclaré compile réellement
```

```sh
cargo run -p klaar-api --bin klaar-vapid   # génère un couple de clés Web Push
```

> Le PostgreSQL de développement écoute sur **5433** et non 5432, pour ne pas
> entrer en conflit avec un autre PostgreSQL déjà en écoute sur le poste.
> `KLAAR_PG_PORT` permet d'en choisir un autre.

```sh
docker compose up -d prometheus grafana   # + `cargo run -p klaar-api --bin klaar-api` en parallèle
# Grafana : http://localhost:3000 (auth anonyme activée en dev, admin/klaar_dev_only sinon)
# Prometheus : http://localhost:9090
```

## Stories Sprint 0 faites

- **0.1** — workspace Cargo (19 crates, `klaar-shared-kernel` avec value objects + 24 tests). La MSRV déclarée était **fausse** : `Cargo.toml` annonçait 1.85 alors que l'arbre de dépendances exige 1.88 (`actix-web` 4.15, `time`, `icu_*`). Corrigée à 1.88, vérifiée par `cargo +1.88.0 check`, et tenue par un job CI dédié — une MSRV jamais compilée n'est qu'une décoration
- **0.2** — **PWA Astro + Svelte** (`web/`, ADR-010) : coquille installable (manifeste, icônes dont une maskable, service worker), page hors-ligne, et queue d'écritures IndexedDB rejouée à la reconnexion. 12 tests unitaires sur les 4 classes (`vitest` + `fake-indexeddb`, IndexedDB réel et non bouchonné) et 7 tests e2e dans un vrai navigateur. Deux défauts trouvés par ces tests plutôt que par relecture :
  - le composant d'état se fiait à `navigator.onLine`, **qui reste à `true` quand le serveur est injoignable** — il affichait donc « En ligne » à un utilisateur hors ligne. Remplacé par une sonde réseau réelle
  - la queue reprise d'Elevia suppose des écritures idempotentes côté serveur, ce qui est vrai chez Elevia (des upserts) et **faux ici** : rejouer « accepter un Devis » déclencherait deux séquestres. Chaque écriture porte désormais une clé d'idempotence tirée à la mise en file, et un refus définitif est écarté au lieu d'être rejoué sans fin
- **0.3** — PostgreSQL 16 + PostGIS local via docker compose, migrations refinery embarquées dans `klaar-api` (binaire `klaar-migrate`), idempotence vérifiée
- **0.9** — hooks Git locaux (`scripts/hooks/`) : pre-commit (fmt + clippy -D warnings + gitleaks si installé), pre-push (tests). **Limite assumée** : ne détecte pas "code sans test" par couverture différentielle (hors scope de ce scaffold) — seul le cargo fmt/clippy/tests est mécaniquement bloquant pour l'instant
- **0.4** — pipeline CI (`.github/workflows/ci.yml`, DRY avec les hooks locaux) : quality gate (fmt, clippy, tests), security gate (cargo-audit, cargo-deny, gitleaks), SBOM CycloneDX en artefact de build. `deny.toml` a fait remonter deux vrais problèmes corrigés au passage : dépendances internes en `path` sans version (« wildcard dependencies ») et absence de licence SPDX déclarée (`license = "MIT"` depuis ADR-009, qui renverse ADR-005)
- **0.5** — harnais contrat API : `klaar-api` sert désormais un vrai endpoint (`/api/v1/health`), sa doc utoipa (`/api/v1/openapi.json`) et Swagger UI (`/api/v1/docs/`), fuzzé par `schemathesis` en CI. **Limites connues** :
  - le check `unsupported_method` de schemathesis est exclu — actix-web renvoie 404 (pas 405) pour une méthode non déclarée sur un chemin donné, limitation de son routage par macro `#[get(...)]`. À corriger (fallback par chemin) quand le contrat aura plusieurs endpoints par route
  - `cargo audit` ignore **RUSTSEC-2026-0258** (h2 < 0.4.16, DoS par frames DATA vides) : vulnérabilité transitive via `actix-http` (toute la branche h2 0.3.x d'actix-web v4 en hérite, pas de version corrigée disponible en amont à ce jour). `klaar-api` n'est pas exposé publiquement à ce stade. **À revoir à chaque mise à jour de dépendance** — `cargo tree -i h2` pour vérifier si un correctif amont existe (même ignore répété dans `deny.toml`, cargo-deny a son propre check advisories indépendant de cargo-audit)
- **0.6** — codegen TS client partagé (`packages/klaar-client`, `@klaar/client`) : `openapi-typescript` génère `src/schema.d.ts` depuis `/api/v1/openapi.json` (`make codegen` ou `bash scripts/codegen.sh`, vérifié en CI). Fichiers générés non commités (`openapi.json`, `schema.d.ts` dans `.gitignore`) — régénérés à la demande depuis la seule source de vérité (le contrat servi par `klaar-api`), pas de package publié pour l'instant (le premier consommateur est `web/`, Story 0.2)
- **0.12** — **Web Push VAPID** (`klaar-push-adapter`, ADR-010) : chiffrement `aes128gcm` (RFC 8188/8291) et authentification VAPID (RFC 8292) assemblés au-dessus des primitives RustCrypto, plus les endpoints `GET /api/v1/push/cle-publique`, `POST` et `DELETE /api/v1/push/abonnements`, la table `push_subscription` et le service worker qui affiche les notifications. Remplace le PoC push Tauri, que des comptes développeur payants bloquaient.
  Écrire ce protocole plutôt que de prendre une bibliothèque ne se défend que par la preuve : **le message d'exemple du RFC 8291 est reproduit octet pour octet**, et chaque valeur intermédiaire de la dérivation (secret ECDH, IKM, clé de contenu, nonce) est comparée à celle publiée en annexe A. Une erreur de dérivation produit un chiffré parfaitement bien formé que seul le navigateur destinataire rejetterait, silencieusement : c'est exactement ce qu'une bibliothèque n'exposant pas ces valeurs ne permettrait pas de vérifier. La bibliothèque `web-push` a par ailleurs été écartée pour une raison indépendante — sans publication depuis février 2025, elle dépend encore de `http` 0.2 et de `hyper` 0.14.
  6 tests e2e livrent un vrai push à un vrai navigateur (`ServiceWorker.deliverPushMessage` du protocole DevTools). **Non vérifiable ici** : la livraison depuis un service de push distant, et iOS, qui ne délivre qu'aux PWA ajoutées à l'écran d'accueil.
- **Défaut RGPD du Sprint 0 corrigé** : le span racine journalisait `http.client_ip` et `http.user_agent`. La première tentative de correction ne marchait pas — `root_span!` renseigne ces champs lui-même, et les journaux contenaient toujours l'IP alors que le code semblait correct à la lecture. Le span est désormais construit champ par champ, et un test inspecte les journaux réellement émis plutôt que la configuration.
- **0.10 (partiel)** — SBOM CycloneDX signé (`cosign sign-blob` keyless via OIDC GitHub, pas de clé privée à gérer) et runbook incident NIS2 (`docs/runbook-incident.md`, procédure de notification CCB en 24h/72h/1 mois). **Non fait** : le runbook n'a pas été testé en jeu de rôle (DoD complet) — inapplicable tant qu'il n'y a ni équipe ops ni déploiement réel
- **0.8 (partiel)** — observabilité : métrique + log générés par requête, vérifié en conditions réelles (`/metrics` Prometheus scrapé, requête → ligne de log JSON avec `http.route`/`http.status_code`/`request_id`). Stack locale `docker compose up -d prometheus grafana`, dashboard provisionné automatiquement (`observability/grafana/dashboards/klaar-api.json` : requêtes/s par endpoint, latence p95). **Bug trouvé et corrigé en cours de route** : `tracing-actix-web` crée un span par requête mais rien ne l'imprime sans `fmt::layer().with_span_events(FmtSpan::CLOSE)` explicite — sans ça, aucune requête n'apparaissait dans les logs malgré `TracingLogger` actif. **Risque RGPD identifié ici, corrigé depuis** : le root span par défaut journalisait `http.client_ip` (IP = donnée personnelle) et `http.user_agent` — voir le point dédié ci-dessus et `crates/klaar-api/src/telemetry.rs`. **Non fait** : trace distribuée (Tempo), AlertManager, plugin Sentry EU — DoD Story 0.8 complet demande une stack plus lourde que ce qui a du sens à scaffolder sans trafic réel à observer

## Epic 1 — Identity & Access

- **1.1** — **Inscription** (FR-001) : `POST /api/v1/auth/signup`, page `/inscription`, compte créé en `PENDING_EMAIL_VERIFY` avec jeton de vérification valable une heure, journal d'audit, limitation à 5 tentatives par heure et par adresse. Domaine dans `klaar-identity` (`MotDePasse` ≥ 12 caractères sans règle de composition — NIST SP 800-63B —, empreinte argon2id 64 MiB / 3 itérations, `JetonVerification`), cas d'usage dans `klaar-application`, migration V3.

  **FR-001 se contredit, et il a fallu trancher.** Son scénario `@negative` réclame un
  `409 EMAIL_ALREADY_EXISTS` sur une adresse déjà prise ; son scénario `@security` réclame
  une réponse « identique (timing + payload) » que l'adresse existe ou non. Les deux ne
  peuvent pas être vrais : le `409` fait de l'inscription un moyen de tester la présence de
  n'importe quelle adresse. **L'anti-énumération l'emporte**, le `409` sort du contrat, et
  la réponse est toujours `202 SIGNUP_ACCEPTED`. Tenir l'indistinguabilité demande plus que
  de renvoyer le même corps : le mot de passe est haché *avant* que la base soit interrogée,
  et un courriel part dans les deux cas — sinon le chemin « déjà prise » est plus court d'un
  envoi et se reconnaît au chronomètre. Le message adressé au titulaire ne porte aucun lien.

  **Le jeton de vérification n'est pas un JWT**, contrairement à ce qu'écrit FR-001. Un JWT
  se vérifie sans état côté serveur, ce qui interdit de le marquer utilisé — alors que le
  même FR exige qu'il ne soit pas rejouable. Jeton opaque de 32 octets, conservé haché, à
  usage unique : même écriture en base, sans la surface d'attaque d'un JWT.

  **Deux défauts trouvés par les tests, pas par relecture** :
  - le commentaire de `Email` annonçait une normalisation NFC qui n'existait pas. En
    l'écrivant, le test a montré que `ø` (U+00F8) **n'a aucune décomposition canonique** :
    `o` + U+0338 n'en est pas une écriture alternative, et NFC ne les confond pas, à raison.
    Le test dit désormais ce que la normalisation fait *et* ce qu'elle ne fait pas
  - le formulaire lisait `navigator.language` : les tests Playwright, dont le Chromium
    annonce `en-US`, affichaient un refus en anglais au milieu d'une page en français. La
    langue est désormais celle déclarée par la page

  **Non fourni** : le challenge hCaptcha après trois échecs (`@security` de FR-001),
  qui suppose un tiers et un appel sortant. La limitation de débit vit en mémoire du
  processus — suffisant à un exemplaire, insuffisant derrière plusieurs instances. Détails
  et régimes concernés dans `COMPLIANCE.md`.

  **Amende la Story 0.12** : la migration V3 pose enfin la clé étrangère
  `push_subscription.sujet_id → utilisateur.id` que V2 annonçait sans pouvoir l'écrire,
  en `ON DELETE CASCADE` — un abonnement orphelin continuerait à notifier un compte effacé.

## CI, premier run réel

Le premier run CI a échoué deux fois avant de passer, corrections gardées ici pour mémoire :
1. `cargo-deny-action` a un input `manifest-path` dédié ; le passer aussi via `arguments` duplique le flag
2. Le job contrat API compilait `klaar-api` à la volée avant de le lancer en arrière-plan puis d'attendre 20 s max : en CI à froid la compilation seule dépasse ce délai. Corrigé en compilant d'abord (`cargo build`), puis en laissant `schemathesis --wait-for-schema=30` gérer l'attente de démarrage du binaire déjà prêt

## Ce qui manque avant que le Sprint 0 soit réellement terminé

Stories 0.7a/0.7b/0.7c (Terraform, salt-ssh, GitOps) et 0.11 (tile-server OSM + Valhalla) : elles nécessitent un compte OVH provisionné, donc restent bloquées tant qu'il n'y a pas de client payant. Ce n'est pas un manque d'effort, c'est un prérequis qui n'existe pas ici.

La Story 0.12 (push) l'était aussi, pour des comptes développeur payants ; **ADR-010 l'a débloquée** en remplaçant le PoC Tauri par Web Push VAPID, qui se vérifie intégralement en local. Les Stories 0.8 et 0.10 restent partielles (cf. ci-dessus).
