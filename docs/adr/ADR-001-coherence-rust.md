# ADR-001 — Cohérence Rust (backend cloud + Tauri embarqué)

- **Statut** : Accepté
- **Date** : 2026-07-18
- **Décideur** : Superviseur (humain)
- **Contexte** : Projet Klaar — application de dépannage/services à la demande, Région de Bruxelles-Capitale
- **Superviseur valideur** : [à signer]

## Contexte

Klaar vise un client mobile **iOS + Android** avec suivi géolocalisé en arrière-plan du prestataire (cas type Uber). Le framework foyer prescriptionne par défaut une stack **Python LTS / FastAPI** pour le backend cloud (cf. `bmad/BMAD-Conception.md` Étape 0) et un frontend **Astro + îlots Svelte 5**.

Le PoC V2 sera construit avec **Tauri 2.0** (frontend Svelte 5 dans une webview + backend embarqué en Rust pour l'accès aux APIs natives : géolocalisation background, camera, push, filesystem). Tauri lève le verrou iOS Safari (coupe JS en background au bout de ~30 s) qui rendait la PWA inadaptée au suivi temps réel du prestataire.

Le backend embarqué Tauri étant en **Rust**, la question se pose de la langue du backend cloud.

## Décision

**Option A — Cohérence Rust** : le backend cloud **et** le backend embarqué Tauri sont écrits en **Rust**, en architecture **hexagonale** (Domain + Application + Infra adapters), avec DDD au centre.

- Frontend (Tauri webview + admin web) : **Svelte 5 (runes)** — foyer-compliant
- Backend cloud : **Rust (axum ou actix-web)** hexagonal
- Backend embarqué Tauri : **Rust** hexagonal, accès natif (geolocation, camera, push)
- Persistance : **PostgreSQL** (via `sqlx` — décision CQRS SQL pur vs ORM à tracer en ADR-002)
- API : **OpenAPI généré depuis le code** (`utoipa` ou `aide`) — matérialisé, pas seulement décrit (foyer `contrat-api.md`)

## Alternatives écartées

- **Option B — Fidelity foyer (Python cloud + Rust Tauri)** : backend cloud Python/FastAPI (foyer-compliant), backend Tauri Rust. Écartée car :
  - Deux langages backend à maintenir, deux chaînes de build, deux communautés
  - Duplication possible du modèle métier (Domain) entre Python et Rust
  - Le *répondre-de* est plus faible sur la cohérence du domaine
- **PWA pure** : écartée — verrou iOS Safari background, pas de suivi temps réel fiable
- **React Native / Flutter** : écartés pour le PoC V2 — écosystème plus lourd, moins sobre (Manifeste §2 *Sumak kawsay*), pas d'avantage décisif sur le PoC
- **Natif Swift + Kotlin** : écarté — coût +60-80 %, 2 codebases, sans gain pour un PoC

## Conséquences

### Positives
- **1 langage backend** (Rust) → sobriété, performance,empreinte RAM minimale (Manifeste §2)
- **1 modèle métier** partagé entre cloud et Tauri (crate `klaar-domain` commune)
- **Sécurité mémoire** par construction (Rust ownership) — aligné ISO 27001 / CyFun
- **Binaires Tauri légers** (~10× plus sobres qu'Electron)
- **Durabilité** (Manifeste) : Rust LTS, typage fort, compilé, peu de dette technique
- **Hexagonal + DDD + SOLID** (principes langage-agnostiques) intégralement tenus

### Négatives / risques à tracer
- **Déviation foyer** sur la prescription Python/FastAPI → **ADR méthodologique** à tracer au meta-grain (Boucle-de-retroaction.md §meta-grain). Foyer restera compatible sur la forme (hexagonal, DDD, TDD, cycle-dev, gates, ADR, répondre-de).
- **Écosystème Tauri Mobile jeune** (stable depuis oct 2024, ~20 mois) — maturité à surveiller pour plugins natifs (push iOS en particulier)
- **Courbe d'apprentissage Rust** pour l'équipe — investissement wall-clock à anticiper (cf. abaque coût)
- **Moins de tutoriels "IA-ready"** en Rust qu'en Python → tokens modèle potentiellement plus élevés sur les stories complexes
- **Moins de candidats développeurs Rust** sur le marché belge — risque de bus factor à mitiger par pair programming (Manifeste conviction 8)

## Sagesse racine (manifeste)

- **Sumak kawsay** (sobriété) — Rust + Tauri minimisent l'empreinte (RAM, binaire, énergie)
- **Écologie des savoirs** — refus de la monoculture (Python par défaut), choix justifié par le terrain
- **Sept générations** — Rust durable, typage fort, code lisible à 30 ans
- **Mottainai** — 1 langage évite le gaspillage de duplication

## Points irréversibles engagés

- Choix langage backend cloud (Rust) — refactoring croisé coûteux
- Choix framework mobile (Tauri) — réécriture native future possible mais coûteuse
- **Validation humaine** : ✅ Superviseur — *« Pourrai-je en répondre, et devant qui ? »*

## Suivi

- ADR-002 (à venir) : `sqlx` CQRS SQL pur vs ORM (SeaORM/Diesel) pour la persistance
- ADR-003 (à venir) : `axum` vs `actix-web` pour le backend cloud
- ADR-004 (à venir) : génération du contrat OpenAPI (`utoipa` annotation vs `aide` vs spec-first)
- ADR méthodologique (meta-grain foyer) : déviation stack foyer Python → Rust à tracer dans le Manifeste Maury §4.4.11
