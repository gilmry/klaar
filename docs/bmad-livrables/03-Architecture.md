# Architecture Technique — Klaar

*Livrable de l'Architecte · TOGAF Phases C-D · Phase 1 BMAD.
Organisé par les sept couches foyer. À vérifier intégralement.*

```
---
projet: Klaar
persona: Architecte
date: 2026-07-18
version: 0.2 (extension C11-C14, stack mobile lockée Tauri/PWA, IA matching, API publique, multi-région)
superviseur_validateur: [à valider pour passage Scrum Master]
signature_humaine: PENDING
brief_source: docs/bmad-livrables/01-Product-Brief.md v0.3
prd_source: docs/bmad-livrables/02-PRD.md v0.3 (68 FR)
adrs:
  - ADR-001 Cohérence Rust (cloud + Tauri) — confirmé par décision v0.3 (pas de natif)
  - ADR-002 sqlx CQRS SQL pur
  - ADR-003 actix-web
  - ADR-004 utoipa OpenAPI
  - ADR-005 License propriétaire
  - ADR-006 OpenStreetMap + Valhalla
  - ADR-007 APNs + FCM
  - ADR-008 Stack mobile Tauri/PWA only (NOUVEAU v0.2 — confirmer décision superviseur)
  - ADR-009 Matching IA (à tracer lors de l'activation J13)
  - ADR-010 API publique partenaires (à tracer lors de l'activation J13)
---
```

> **Différenciel v0.1 → v0.2** : extension aux capacités C11-C14 (Brief v0.3 §7, PRD v0.3 FR-045 à FR-068). Stack mobile lockée **Tauri 2.0 + PWA uniquement** (ADR-008 NOUVEAU) — toute mention d'une bascule native React Native ou Flutter est **retirée** comme option planifiée. Les ajouts sont marqués `NOUVEAU Jxx` ou `NOUVEAU v0.2` pour faciliter la relecture.

## Vue d'ensemble

```
                          ┌─────────────────────────────────────┐
                          │       Clients (3 surfaces)          │
                          │  ┌──────────┐ ┌────────┐ ┌────────┐ │
                          │  │  Tauri   │ │  PWA   │ │Astro+  │ │
                          │  │ iOS+And  │ │ public │ │Svelte  │ │
                          │  │          │ │(J12')  │ │ Admin  │ │
                          │  └────┬─────┘ └───┬────┘ └───┬────┘ │
                          └───────┼───────────┼──────────┼──────┘
                                  │           │          │
                                  ▼           ▼          ▼
                          ┌─────────────────────────────────────┐
                          │   Codegen TS (shared client)        │  ← openapi-typescript
                          │   depuis OpenApiDoc utoipa          │
                          └─────────────────┬───────────────────┘
                                            │ HTTPS/JSON + WSS
                                            ▼
        ┌────────────────────────────────────────────────────────────────┐
        │             Backend cloud (Rust, ADR-001)                      │
        │  ┌──────────────────────────────────────────────────────────┐ │
        │  │  Adapters (Infrastructure)                               │ │
        │  │  actix-web · sqlx · Stripe · itsme · OVH S3 · APNs ·     │ │
        │  │  FCM · OSM/Valhalla · Sendgrid · ClamAV · biometric (J12')│ │
        │  │  · ml (J13) · insurance (J13) · authority (J11) · region  │ │
        │  └────────────────────────┬─────────────────────────────────┘ │
        │                           ▼                                   │
        │  ┌──────────────────────────────────────────────────────────┐ │
        │  │  Application (Ports / Use cases / DTOs)                  │ │
        │  └────────────────────────┬─────────────────────────────────┘ │
        │                           ▼                                   │
        │  ┌──────────────────────────────────────────────────────────┐ │
        │  │  Domain (Pure Rust · Invariants in constructors)         │ │
        │  │  Cœur 8 BC + 4 BC d'extension (J11-J14) :                │ │
        │  │  identity · catalog · matching · intervention · payment  │ │
        │  │  messaging · trust · ops · skills(J11) · surge(J13) ·    │ │
        │  │  subscription(J13) · public-api(J13)                     │ │
        │  └──────────────────────────────────────────────────────────┘ │
        └────────────────────────────────────────────────────────────────┘
                                            │
                                            ▼
                          ┌──────────────────────────────────┐
                          │  PostgreSQL + PostGIS (OVH EU)   │
                          │  + S3 (KMS) + Vault              │
                          │  + MLflow registry (J13)         │
                          └──────────────────────────────────┘
```

### Workspace Cargo (monorepo Rust)

```text
klaar/
├── Cargo.toml                    (workspace)
├── crates/
│   ├── klaar-domain/               (pure, no IO)
│   │   ├── klaar-identity/         (cœur)
│   │   ├── klaar-catalog/          (cœur, étendu en J11 pour multi-secteurs)
│   │   ├── klaar-matching/         (cœur, étendu en J13 pour IA ranking)
│   │   ├── klaar-intervention/     (cœur)
│   │   ├── klaar-payment/          (cœur, étendu en J13 pour subscription + assurance)
│   │   ├── klaar-messaging/        (cœur)
│   │   ├── klaar-trust/            (cœur)
│   │   ├── klaar-skills/           (NOUVEAU J11 — attestations compétences)
│   │   ├── klaar-surge/            (NOUVEAU J13 — moteur surge pricing)
│   │   ├── klaar-subscription/     (NOUVEAU J13 — billing récurrent Pro)
│   │   ├── klaar-public-api/       (NOUVEAU J13 — API publique partenaires)
│   │   └── klaar-shared-kernel/    (value objects communs : Money, Geo, Email)
│   ├── klaar-application/          (ports + use cases, par BC)
│   ├── klaar-infrastructure/       (adapters)
│   │   ├── klaar-sqlx-repos/
│   │   ├── klaar-stripe-adapter/
│   │   ├── klaar-itsme-adapter/
│   │   ├── klaar-geo-adapter/      (OSM + Valhalla, ADR-006)
│   │   ├── klaar-push-adapter/     (APNs + FCM, ADR-007)
│   │   ├── klaar-storage-adapter/  (S3 OVH)
│   │   ├── klaar-av-adapter/       (ClamAV)
│   │   ├── klaar-audit-adapter/    (audit_logs WORM)
│   │   ├── klaar-email-adapter/    (Sendgrid / Postmark)
│   │   ├── klaar-biometric-adapter/    (NOUVEAU J12' — FaceID/TouchID via Tauri plugin)
│   │   ├── klaar-ml-adapter/       (NOUVEAU J13 — modèle matching IA + features store)
│   │   ├── klaar-insurance-adapter/ (NOUVEAU J13 — API partenaire assurance BE)
│   │   ├── klaar-authority-adapter/ (NOUVEAU J11 — fédérations sectorielles + KBO-BCE + INASTI)
│   │   └── klaar-region-adapter/   (NOUVEAU J14 — données régionales par ville)
│   └── klaar-api/                  (actix-web + utoipa, ADR-003/004)
├── pwa-public/                   (NOUVEAU J12'/J13 — PWA grand public alternative)
├── tauri-app/                    (Tauri 2.0 mobile, ADR-001 + ADR-008)
│   ├── src-tauri/                (Rust backend embarqué, plugins J12')
│   └── src/                      (Svelte 5 runes)
├── admin-web/                    (Astro + Svelte 5 îlots)
├── migrations/                   (refinery, ADR-002)
├── infra/                        (IaC : salt, GitOps, Terraform)
└── .github/workflows/            (CI/CD + pipeline ML J13)
```

---

## ADR-008 — Stack mobile : Tauri 2.0 + PWA uniquement (pas de bascule native) *(NOUVEAU v0.2)*

- **Statut** : Accepté (override superviseur v0.3)
- **Date** : 2026-07-18
- **Décideur** : Superviseur (décision structurante v0.3), tracé par l'Architecte
- **Superviseur valideur** : PENDING (fichier dédié `/home/user/Klaar/docs/adr/ADR-008-stack-mobile-tauri-pwa-only.md` à créer en Sprint 0)

### Contexte

Le Brief v0.2 envisageait une **bascule native (RN/Flutter)** au jalon J12 (~1000-1600 h, ~100-160 k€) après un MVP Tauri 2.0. Cette option créait :
- une **double codebase** (Rust + TS/Svelte côté Tauri, puis RN-JS ou Flutter-Dart côté natif) — violation de la cohérence Rust (ADR-001) ;
- un **coût massif** (réécriture complète, re-submission stores, E2E natif) ;
- un **risque H-13** (maturité plugin Tauri géoloc background iOS) utilisé comme justification pour ouvrir la porte natif — alors que des paliers moins coûteux existent (PWA foreground permanent, plugin Tauri custom).

La décision structurante v0.3 du superviseur (2026-07-18) **ferme la porte natif**. L'Architecte trace cette décision en ADR-008.

### Décision

La stack mobile reste **Tauri 2.0 + PWA fallback** pour toute la roadmap (J0 → J14, horizon 4-5 ans). **Pas de réécriture native React Native ou Flutter**. Le jalon anciennement « J12 Native premium » devient **J12' Enhancement Tauri/PWA continu** (~100-200 h au lieu de 1000-1600 h).

Toute évolution mobile = enhancement du socle Tauri/PWA :
- plugins Tauri additionnels (biometric, stronghold, deep-linking, geolocation background)
- PWA grand public alternative (FR-055)
- feature parity matrix documentée

### Justification

- **Cohérence Rust (ADR-001) préservée** : 1 seul langage backend cloud + backend embarqué Tauri
- **Sobriété (Manifeste sumak kawsay)** : 1 codebase au lieu de 3 (Rust + TS Svelte + RN-JS/Flutter-Dart)
- **Coût** : économie de ~**800-1400 k€** vs bascule native (devis §4.5)
- **Tauri 2.0 stable** depuis octobre 2024, écosystème plugins mature (notification, push, biometric, stronghold)
- **PWA fallback** garantit un continuum de service même en cas de blocage plugin Tauri spécifique

### Conséquences

#### Positives
- 1 équipe, 1 langage backend (Rust), 1 framework frontend (Svelte 5) — productivité maximale
- SBOM unique (CRA, NIS2), audit sécurité simplifié
- Stories habilitantes PoC réduites (pas de ré-apprentissage natif)
- Conforme au Manifeste (écologie des savoirs, mottainai)

#### Négatives / risques assumés
- **Certaines fonctionnalités natives avancées seront absentes** : widgets iOS Today Extension, intégration Siri/Shortcuts, Live Activities, Action Extensions — accepté par le superviseur
- **Risque résiduel H-13** : maturité plugin Tauri géoloc background iOS — à valider en Sprint 0 (Story 0.12 étendue). Mitigations en 3 paliers : (a) PWA foreground permanent, (b) plugin Tauri custom développé en interne, (c) limitation au tracking par Mission active (compatible MVP §11)
- **Stores Apple/Google** : review process identique (Hotfix OTA Tauri Updater pour patch webview < 24 h sans re-review)

### Alternatives écartées

#### React Native (Meta)
Écarté car : double langage (JS/TS + Rust backend), violation ADR-001, écosystème dépendant Meta US, coût réécriture ~500-800 h.

#### Flutter (Google)
Écarté car : langage Dart additionnel (vs Rust + TS), violation ADR-001, taille binaire > 50 Mo (vs Tauri < 50 Mo), dépendance Google.

#### Kotlin/Swift natif pur (double codebase)
Écarté car : double équipe, double SBOM, coût maximal (~1000-1600 h), risque de divergence fonctionnelle iOS/Android.

#### Capacitor (Ionic)
Écarté car : équivalent fonctionnel à Tauri sans bénéfice Rust embarqué ; Tauri 2.0 déjà choisi avec ADR-001.

### Sagesse racine (Manifeste)

- **Sumak kawsay** (bon vivre) : 1 codebase = moins de charge mentale équipe, plus de temps métier
- **Mottainai** (pas de gaspillage) : pas de réécriture d'une UI qui marche en Tauri
- **Répondre-de** (responsabilité) : PWA fallback = filet de sécurité traçable
- **Arbitrage-hybride** : on loue le réversible (PWA enhancement), on possède l'irréversible (pas de reset natif)

### Point irréversible

- **Stack mobile Tauri/PWA only** : **irréversible** pour la roadmap 4-5 ans (J0-J14). Toute évolution se fait dans le périmètre Tauri/PWA. Une bascule native ne pourrait être motivée que par un changement majeur externe (retrait Tauri, Apple interdit webview apps — aucun signal à 3 ans).
- **Validation humaine** : ✅ Superviseur v0.3 (décision locked `stack_mobile`), tracé ADR-008 PENDING signature finale

### Suivi

- Sprint 0 : Story 0.12 étendue — **PoC push + PoC géoloc background** (Brief §19.3 gate J12')
- Si PoC géoloc background iOS concluant → activation plugin standard J12'
- Si PoC partiel → palier (a) PWA foreground, (b) plugin custom, (c) limitation par Mission active
- Métrique monitoring : taux de livraison push ≥ 95 %, précision géoloc ≤ 50 m, crash-free sessions ≥ 99 %

---

## Couche 1 — Domain *(SOLID : SRP, DIP — racine : Ubuntu)*

> **Pure Rust, aucune dépendance vers l'extérieur.** Aucune IO, aucune macro serde côté logique (seulement sur DTOs isolés). Tout invariant codé dans les constructeurs.

### 1.1 Value Objects (`klaar-shared-kernel`)

```rust
// Pas de clonage sans validation ; pas de état invalide possible
pub struct Email(String);
impl Email {
    pub fn parse(s: &str) -> Result<Self, EmailError> {
        // RFC 5322 + normalisation NFC lowercase
        ...
    }
}

pub struct Geo { lat: f64, lon: f64 }  // validated [-90, 90] × [-180, 180]
pub struct Money { cents: i64 }         // jamais FLOAT
pub struct VatRate { basis_points: u16 } // 2100 = 21%
pub struct DistanceMeters(u32);
pub struct Locale(String);              // 'fr', 'nl', 'en' uniquement
pub struct HashSha256([u8; 32]);
```

### 1.2 Entities par BC (cœur)

Chaque BC expose ses entités + agrégats. Exemples :

**`klaar-identity`** :
```rust
pub struct User {
    id: UserId,
    email: Email,
    password_hash: Option<PasswordHash>,  // None si itsme-only
    locale: Locale,
    status: UserStatus,
    erased_at: Option<DateTime<Utc>>,
}

pub enum UserStatus { PendingEmailVerify, Active, ErasedPending, Erased }

impl User {
    pub fn signup(email: Email, password: Password, locale: Locale) -> Result<Self, DomainError> { ... }
    pub fn verify_email(&mut self) -> Result<(), DomainError> { ... }
    pub fn erase(&mut self) -> Result<(), DomainError> { ... }
    pub fn is_active(&self) -> bool { ... }
}

pub struct Provider {
    id: ProviderId,
    user_id: UserId,
    bce: BceNumber,
    insurance: Insurance,
    status: ProviderStatus,
    stripe_account_id: Option<StripeAccountId>,
    skills: Vec<SkillId>,
}

impl Provider {
    // Invariant §10.1 : BCE obligatoire
    pub fn onboard(user: User, bce: BceNumber, insurance: Insurance) -> Result<Self, DomainError> { ... }
    // Invariant §10.2 : pas de prix imposé (vérifié par l'absence de méthode set_price)
}
```

**`klaar-matching`** :
```rust
pub struct Request {
    id: RequestId,
    user_id: UserId,
    sector: SectorId,
    description: Description,  // length 1..=2000
    geo: Geo,
    urgency: Urgency,
    status: RequestStatus,
    radius_m: DistanceMeters,
}

pub enum RequestStatus {
    Draft, Broadcasting, Matched, MissionCreated,
    CancelledUser, NoMatch, Expired,
}

pub struct Match {
    id: MatchId,
    request_id: RequestId,
    provider_id: ProviderId,
    score: f64,
    criteria: MatchCriteria,  // sérialisé en JSONB pour audit AI Act
    status: MatchStatus,
}
```

**`klaar-intervention`** :
```rust
pub struct Mission {
    id: MissionId,
    request_id: RequestId,
    provider_id: ProviderId,
    status: MissionStatus,
    escrow_id: Option<EscrowId>,
    // transitions validées par la machine à états (FR-018)
}

impl Mission {
    pub fn transition(&mut self, to: MissionStatus, source: TransitionSource) -> Result<MissionEvent, DomainError> {
        // validation machine à états
    }
}
```

**`klaar-payment`** : Quote, Escrow, Payout, Invoice — règles métier TVA, Take rate, remboursement.

### 1.3 Domain Events

```rust
// Émis par les agrégats, consommés par l'Application layer (outbox pattern)
pub enum DomainEvent {
    UserSignedUp { user_id: UserId, email: Email },
    ProviderApproved { provider_id: ProviderId },
    RequestCreated { request_id: RequestId, sector: SectorId },
    MatchAccepted { request_id: RequestId, provider_id: ProviderId },
    MissionCompleted { mission_id: MissionId },
    EscrowReleased { escrow_id: EscrowId, payout_id: PayoutId },
    DisputeOpened { dispute_id: DisputeId, mission_id: MissionId },
    // NOUVEAU v0.2 — events d'extension
    SkillCredentialSubmitted { provider_id: ProviderId, skill_id: SkillId },         // J11
    SkillCredentialVerified { provider_id: ProviderId, authority: AttestationAuthority },
    SurgeApplied { zone_id: SurgeZoneId, multiplier: f64, request_id: RequestId },   // J13
    SubscriptionActivated { provider_id: ProviderId, plan: SubscriptionPlan },       // J13
    ApiPartnerAuthenticated { client_id: ApiClientId, scopes: Vec<ApiScope> },       // J13
    CityActivated { city_code: String, go_live_at: DateTime<Utc> },                  // J14
}
```

### 1.4 Invariants codés dans les constructeurs

Chaque invariant du Brief §10 est traduit en une méthode ou un type :

| Invariant Brief | Mécanisme code |
|---|---|
| §10.1 BCE obligatoire | `Provider::onboard` refuse sans BCE |
| §10.2 Prix libre par Provider | Pas de méthode `Request::set_price` ; seul `Quote` est émis par Provider |
| §10.3 Pas d'exclusivité | Aucun champ `is_exclusive` dans Provider |
| §10.4 Escrow à l'acceptation | `Mission::accept_quote` atomique avec `Escrow::capture` |
| §10.5 Pas de tracking hors mission | `MissionTracker` activé uniquement entre `Accepted` et `OnSite` |
| §10.6 Droit à l'effacement | `User::erase` produit `UserErased` event → consumers anonymisent |
| §10.7 Trace immuable | `AuditLog::append` only, pas de `update`/`delete` |
| §10.8 Assurance RC requise | `Provider::activate_skill(Skill::Electricity)` vérifie B2V BR |
| §10.9 Multilingue | Type `Locale` + tous les DTOs i18n |

### 1.5 Entités d'extension *(NOUVEAU v0.2 — J11/J13/J14)*

#### `klaar-skills` (J11 — FR-045 à FR-050)

Attestations de compétences réglementées : B2V BR électricité, agréation gaz PEB, etc. Cross-check auprès des fédérations sectorielles.

```rust
pub struct SkillAttestation {
    id: SkillAttestationId,
    provider_id: ProviderId,
    skill_id: SkillId,
    authority: AttestationAuthority,  // B2V_BR, VG_BE, Plomberie_gaz, etc.
    delivered_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    verification_status: VerificationStatus,
    document_hash: HashSha256,        // PDF justificatif, anti-falsification
}

pub enum AttestationAuthority {
    B2VBR,           // Habilitation électrique BR (Basse Tension)
    VGBE,            // Brevet Vérification Générale Belgique
    PlomberieGaz,    // Agréation plomberie gaz BE
    ChauffageClass1, // Chauffage classe 1 BE
    PEBGaz,          // Agréation PEB gaz naturel (Région Wallonne/Bruxelles)
    Custom(String),  // Pour futurs secteurs (chauffagiste, etc.)
}

pub enum VerificationStatus {
    Pending,
    AutoVerified { source: String, checked_at: DateTime<Utc> },
    OpsValidated { ops_user_id: UserId, validated_at: DateTime<Utc> },
    Rejected { reason: String },
    Expired,
    Revoked { reason: String, revoked_at: DateTime<Utc> },
}

impl SkillAttestation {
    // Invariant §10.8 : pas d'Intervention sans assurance/agrément valide
    pub fn is_valid_at(&self, at: DateTime<Utc>) -> bool { ... }
    pub fn submit(provider: ProviderId, skill: SkillId, doc_hash: HashSha256) -> Self { ... }
    pub fn verify(&mut self, source: AuthoritySource, at: DateTime<Utc>) -> Result<(), DomainError> { ... }
    pub fn revoke(&mut self, reason: String, at: DateTime<Utc>) -> Result<(), DomainError> { ... }
}
```

#### `klaar-surge` (J13 — FR-057)

Moteur de surge pricing par zone et heure. **Invariant Platform Work** : le surge n'est jamais imposé au Devis Provider (Invariant §10.2), il agit uniquement sur le prix indicatif affiché.

```rust
pub struct SurgeZone {
    id: SurgeZoneId,
    geo: Polygon,                  // PostGIS
    active: bool,
    multiplier: f64,               // 0.5 à 2.0 (1.0 = nominal), capé à 3.0
    reason: SurgeReason,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
}

pub enum SurgeReason {
    Weather { event: String },
    HighDemand { requests_count: u32 },
    LowSupply { providers_count: u32 },
    Manual { ops_user_id: UserId },
}

impl SurgeZone {
    // Cap protection User : jamais > 3.0
    pub fn new(geo: Polygon, multiplier: f64, reason: SurgeReason) -> Result<Self, DomainError> { ... }
}

// Invariant Platform Work : surge doit être transparent et jamais contraignant
pub struct SurgeDisclosure {
    zone_id: SurgeZoneId,
    multiplier_applied: f64,
    shown_to_user_at: DateTime<Utc>,
    user_accepted: bool,
    request_id: RequestId,
}

impl SurgeDisclosure {
    // Anti-Platform Work : trace obligatoire si surge != 1.0
    pub fn disclose(zone: SurgeZoneId, mult: f64, req: RequestId) -> Self { ... }
}
```

#### `klaar-subscription` (J13 — FR-058)

Forfaits Pro / Premium pour Providers. **Invariant §10.3** : aucun bridage des Demandes standards (Free = accès complet ; Pro = + priorité, CRM, analytics).

```rust
pub struct ProSubscription {
    id: SubscriptionId,
    provider_id: ProviderId,
    plan: SubscriptionPlan,         // Free, Pro29, Pro99
    status: SubscriptionStatus,
    current_period_end: DateTime<Utc>,
    stripe_subscription_id: StripeSubscriptionId,
    quotas: SubscriptionQuotas,
}

#[derive(Clone, Copy)]
pub enum SubscriptionPlan {
    Free,
    Pro29,   // 29 €/mois : 10 priorités/jour + CRM
    Pro99,   // 99 €/mois : 50 priorités/jour + analytics avancé
}

pub struct SubscriptionQuotas {
    priority_requests_per_month: u32,
    crm_contacts: u32,
    analytics_retention_days: u32,
}

impl ProSubscription {
    // Anti lock-in : résiliable à tout moment, effet en fin de mois
    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), DomainError> { ... }
    // Invariant §10.3 : Free doit rester pleinement fonctionnel
    pub fn can_access_standard_requests(&self) -> bool { true } // toujours true
}
```

#### `klaar-public-api` (J13 — FR-060 à FR-061)

API publique partenaires : exposes catalogue read, mission status anonymisé, sector availability. OAuth2 client_credentials.

```rust
pub struct ApiClient {
    id: ApiClientId,
    partner_name: String,
    client_id: String,                  // OAuth2 client_credentials
    client_secret_hash: PasswordHash,   // jamais en clair
    tier: ApiTier,
    rate_limit_per_minute: u32,
    scopes: Vec<ApiScope>,
    webhook_url: Option<Url>,           // FR-061
    webhook_secret_hash: Option<PasswordHash>,
    status: ApiClientStatus,
}

pub enum ApiTier {
    Free { max_requests_per_month: u32 },
    Pro { max_requests_per_month: u32, webhooks_enabled: bool },
    Enterprise { unlimited: bool, sla: bool, mtls_required: bool },
}

pub enum ApiScope {
    CatalogRead,
    SectorAvailability,
    MissionStatusAnonymized,
    WebhookMissionCompleted,
    WebhookSectorAdded,
}

pub enum ApiClientStatus { Active, Suspended { reason: String }, Revoked }
```

#### `klaar-region` (J14 — FR-064 à FR-068)

Expansion géographique multi-villes. Activable après gate rentabilité RBC.

```rust
pub struct City {
    code: CityCode,                  // "brussels", "antwerp", "liege", "ghent", "charleroi"
    name_i18n: I18nString,
    geo: Polygon,                    // périmètre administratif
    status: CityStatus,
    activated_at: Option<DateTime<Utc>>,
    apd_registry_number: Option<String>,  // déclaration APD régionale
}

pub enum CityStatus { Pending, SoftLaunch { quadrant: String }, Live, Paused, Deprecated }

impl City {
    // Gate Brief §19.3 : rentabilité RBC prouvée avant activation
    pub fn activate(&mut self, at: DateTime<Utc>, apd_number: String) -> Result<(), DomainError> { ... }
    pub fn pause(&mut self, reason: String) -> Result<(), DomainError> { ... }
}
```

---

## Couche 2 — Application *(ISP, OCP, DRY)*

> **Ports (traits Rust) + Use cases + DTOs.** Orchestration sans logique métier (qui est dans Domain).

### 2.1 Ports (interfaces)

```rust
// Repositories (lecture/écriture agrégats)
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn load(&self, id: &UserId) -> Result<Option<User>, RepoError>;
    async fn save(&self, user: &User) -> Result<(), RepoError>;
}

#[async_trait]
pub trait ProviderRepository: Send + Sync { ... }
#[async_trait]
pub trait RequestRepository: Send + Sync {
    async fn find_providers_within(&self, geo: &Geo, radius: DistanceMeters, sector: SectorId) -> Result<Vec<ProviderCandidate>, RepoError>;
}

// Gateways (services externes)
#[async_trait]
pub trait PaymentGateway: Send + Sync {
    async fn create_payment_intent(&self, amount: Money, customer: StripeCustomerId) -> Result<PaymentIntentId, GatewayError>;
    async fn capture(&self, intent: PaymentIntentId) -> Result<(), GatewayError>;
    async fn transfer(&self, to: StripeAccountId, amount: Money, idem: IdempotencyKey) -> Result<TransferId, GatewayError>;
}

#[async_trait]
pub trait IdentityProvider: Send + Sync {
    async fn start_itsme_flow(&self, callback: Url) -> Result<AuthUrl, IdpError>;
    async fn verify_itsme_callback(&self, code: AuthCode) -> Result<VerifiedIdentity, IdpError>;
}

#[async_trait]
pub trait GeolocationService { ... }      // OSM/Valhalla abstraction (ADR-006)
#[async_trait]
pub trait PushNotifier { ... }            // APNs/FCM abstraction (ADR-007)
#[async_trait]
pub trait ObjectStorage { ... }           // S3 abstraction
#[async_trait]
pub trait AntivirusScanner { ... }        // ClamAV abstraction
#[async_trait]
pub trait AuditLogger { ... }             // append-only
```

### 2.2 Ports d'extension *(NOUVEAU v0.2 — J11/J12'/J13/J14)*

#### E1 — Skills (J11)

```rust
#[async_trait]
pub trait SkillAttestationRepository: Send + Sync {
    async fn load(&self, id: &SkillAttestationId) -> Result<Option<SkillAttestation>, RepoError>;
    async fn save(&self, attestation: &SkillAttestation) -> Result<(), RepoError>;
    async fn find_by_provider(&self, provider: &ProviderId) -> Result<Vec<SkillAttestation>, RepoError>;
    async fn find_expiring_before(&self, at: DateTime<Utc>) -> Result<Vec<SkillAttestation>, RepoError>;
}

#[async_trait]
pub trait CrossCheckAuthorityGateway: Send + Sync {
    // Fédérations sectorielles (AIB-Vincotte, etc.), KBO-BCE, INASTI
    async fn verify_attestation(&self, authority: &AttestationAuthority, credential_ref: &str) -> Result<AuthorityVerification, GatewayError>;
    async fn check_bce_status(&self, bce: &BceNumber) -> Result<BceStatus, GatewayError>;
    async fn check_inasti_status(&self, national_registry_hash: &HashSha256) -> Result<InastiStatus, GatewayError>;
}
```

#### E3 — Matching IA (J13)

```rust
#[async_trait]
pub trait RankingModel: Send + Sync {
    /// Score un provider pour une demande donnée.
    /// Retourne un score ∈ [0, 1] + la feature importance (transparence AI Act Art. 13).
    async fn score(&self, provider: &ProviderCandidate, request: &Request) -> Result<RankingScore, ModelError>;
}

pub struct RankingScore {
    value: f64,
    features: Vec<FeatureContribution>,  // pour explicabilité
    model_version: String,                // MLflow
}

// 2 implémentations dans klaar-ml-adapter :
// - RuleBasedRanking (MVP, déjà là, distance × rating × KYC date)
// - LearnedRanking (J13, features store + inférence modèle supervisé)
```

#### E3 — Surge pricing (J13)

```rust
#[async_trait]
pub trait SurgeZoneRepository: Send + Sync {
    async fn find_active_at(&self, geo: &Geo, at: DateTime<Utc>) -> Result<Option<SurgeZone>, RepoError>;
    async fn save(&self, zone: &SurgeZone) -> Result<(), RepoError>;
}

pub trait SurgeRuleEngine: Send + Sync {
    /// Calcule le coefficient de surge pour une zone/demande.
    /// Capé à 3.0 (protection User). 1.0 = nominal. < 1.0 = discount autorisé.
    fn compute_multiplier(&self, demand_count: u32, supply_count: u32, weather: Option<WeatherEvent>) -> f64;
}
```

#### E3 — Subscription (J13)

```rust
#[async_trait]
pub trait SubscriptionRepository: Send + Sync {
    async fn load(&self, id: &SubscriptionId) -> Result<Option<ProSubscription>, RepoError>;
    async fn find_by_provider(&self, provider: &ProviderId) -> Result<Option<ProSubscription>, RepoError>;
    async fn save(&self, subscription: &ProSubscription) -> Result<(), RepoError>;
}

#[async_trait]
pub trait BillingGateway: Send + Sync {
    // Stripe Subscriptions (différent de Stripe Connect Payments)
    async fn create_subscription(&self, provider: &ProviderId, plan: SubscriptionPlan) -> Result<StripeSubscriptionId, GatewayError>;
    async fn cancel_at_period_end(&self, sub: &StripeSubscriptionId) -> Result<(), GatewayError>;
    async fn upgrade(&self, sub: &StripeSubscriptionId, new_plan: SubscriptionPlan, prorate: bool) -> Result<(), GatewayError>;
}
```

#### E3 — API publique (J13)

```rust
#[async_trait]
pub trait ApiClientRepository: Send + Sync {
    async fn load(&self, id: &ApiClientId) -> Result<Option<ApiClient>, RepoError>;
    async fn find_by_client_id(&self, client_id: &str) -> Result<Option<ApiClient>, RepoError>;
    async fn save(&self, client: &ApiClient) -> Result<(), RepoError>;
}

#[async_trait]
pub trait OAuth2Server: Send + Sync {
    // client_credentials flow (pas d'impersonation user)
    async fn issue_access_token(&self, client: &ApiClient, scopes: &[ApiScope]) -> Result<AccessToken, OAuthError>;
    async fn introspect(&self, token: &str) -> Result<TokenIntrospection, OAuthError>;
}

#[async_trait]
pub trait WebhookEmitter: Send + Sync {
    // FR-061 — events sortants vers partenaires
    async fn emit(&self, client: &ApiClient, event: WebhookEvent) -> Result<(), WebhookError>;
}
```

#### E4 — Region adapter (J14)

```rust
#[async_trait]
pub trait RegionRepository: Send + Sync {
    async fn load(&self, code: &CityCode) -> Result<Option<City>, RepoError>;
    async fn save(&self, city: &City) -> Result<(), RepoError>;
    async fn list_active(&self) -> Result<Vec<City>, RepoError>;
}

#[async_trait]
pub trait RegionalTilesGateway: Send + Sync {
    async fn validate_routing(&self, city: &CityCode) -> Result<RoutingQaReport, GatewayError>;
    async fn import_osm_extract(&self, source_url: Url, hash: HashSha256) -> Result<(), GatewayError>;
}
```

### 2.3 Use cases (Commands & Queries — CQRS léger)

Chaque FR du PRD = ≥ 1 use case. Exemples cœur :

```rust
// FR-001 Inscription User
pub struct SignupUserCommand { email: Email, password: Password, locale: Locale }
pub struct SignupUserHandler<U: UserRepository, A: AuditLogger> { users: U, audit: A }
impl SignupUserHandler<U, A> {
    pub async fn execute(&self, cmd: SignupUserCommand) -> Result<UserId, UseCaseError> {
        let user = User::signup(cmd.email, cmd.password, cmd.locale)?;
        self.users.save(&user).await?;
        self.audit.append(AuditEvent::UserSignup { id: user.id }).await?;
        Ok(user.id)
    }
}

// FR-012 Matching
pub struct FindProvidersForRequestQuery { request_id: RequestId }
pub struct FindProvidersForRequestHandler<R: RequestRepository, T: AuditLogger> { ... }

// FR-021 Validation fin Mission + libération Escrow
pub struct ValidateMissionCommand { mission_id: MissionId, user_id: UserId }
pub struct ValidateMissionHandler<M, E, P, A> { ... }
```

### 2.4 Use cases d'extension *(NOUVEAU v0.2 — J11/J12'/J13/J14)*

| Use case | BC | FR | Ports dépendants |
|---|---|---|---|
| `SubmitSkillAttestation` | skills | FR-045 | `SkillAttestationRepository`, `ObjectStorage` (PDF), `AntivirusScanner`, `AuditLogger` |
| `VerifySkillAttestation` | skills | FR-045 / FR-050 | `CrossCheckAuthorityGateway`, `SkillAttestationRepository`, `AuditLogger` |
| `AutoExpireSkillAttestations` (job nightly) | skills | FR-050 | `SkillAttestationRepository`, `CrossCheckAuthorityGateway` |
| `ExtendProviderToSector` | identity + skills | FR-046 | `ProviderRepository`, `SkillAttestationRepository` |
| `AddSectorToCatalog` | catalog | FR-047 | `SectorRepository`, `AuditLogger` (4-eyes) |
| `CalibrateIndicativePrices` (job nightly) | catalog | FR-048 | `MissionRepository`, `IndicativePriceRepository` (IQR) |
| `BulkImportProviders` | identity | FR-049 | `ProviderRepository`, `EmailSender` |
| `RankProvidersByIA` | matching | FR-056 | `RankingModel`, `RequestRepository`, `AuditLogger` |
| `AuditBiasSemestriel` (job) | matching | FR-056 | `RankingModel`, `AuditLogger`, `EmailSender` (DPO) |
| `ApplySurgeToRequest` | surge | FR-057 | `SurgeZoneRepository`, `SurgeRuleEngine` |
| `DiscloseSurgeToUser` | surge | FR-057 | `AuditLogger` (transparence Platform Work) |
| `SubscribeProvider` | subscription | FR-058 | `SubscriptionRepository`, `BillingGateway` |
| `RenewSubscription` (job) | subscription | FR-058 | `SubscriptionRepository`, `BillingGateway` |
| `ApplyQuotaLimits` | subscription | FR-058 | `SubscriptionRepository` |
| `SubscribeInsurance` | payment | FR-059 | `InsuranceProvider`, `AuditLogger` |
| `AuthenticatePartner` | public-api | FR-060 | `OAuth2Server`, `ApiClientRepository` |
| `RateLimitRequest` (middleware) | public-api | FR-060 | `ApiClientRepository`, Redis |
| `EmitWebhook` | public-api | FR-061 | `WebhookEmitter`, `AuditLogger` |
| `ActivateCity` | region | FR-064 | `RegionRepository`, `AuditLogger` (4-eyes super_admin) |
| `LoadCityTiles` | region | FR-066 | `RegionalTilesGateway` |
| `DeclareToRegionalAuthority` | region | FR-067 | `RegionRepository`, `AuditLogger` |

### 2.5 DTOs (Data Transfer Objects)

```rust
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]   // ADR-004 — strict
pub struct SignupUserRequest {
    pub email: String,
    pub password: String,
    pub locale: Locale,
}

#[derive(Serialize, ToSchema)]
pub struct UserDto {
    pub id: Uuid,
    pub email: String,
    pub locale: Locale,
    pub status: UserStatusDto,
}

// NOUVEAU v0.2 — DTOs d'extension
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitSkillAttestationRequest {       // FR-045
    pub skill_id: SkillId,
    pub authority: AttestationAuthority,
    pub credential_ref: String,
    pub valid_until: Option<DateTime<Utc>>,
    pub document_base64: String,                  // PDF/PNG/JPEG < 10 Mo
}

#[derive(Serialize, ToSchema)]
pub struct SurgeDisclosureDto {                   // FR-057 — transparence Platform Work
    pub zone_id: SurgeZoneId,
    pub multiplier: f64,
    pub reason: SurgeReasonDto,
    pub user_message: I18nString,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreatePartnerClientRequest {           // FR-060
    pub partner_name: String,
    pub tier: ApiTier,
    pub scopes: Vec<ApiScope>,
    pub webhook_url: Option<Url>,
}
```

---

## Couche 3 — Infrastructure backend *(LSP, DIP)* · *stateful*

> **Adapters** : implémentations concrètes des Ports. Peuvent être remplacées sans toucher Domain/Application (hexagonale).

### 3.1 Persistance (`klaar-sqlx-repos`, ADR-002)

```rust
pub struct SqlxUserRepository { pool: PgPool }
#[async_trait]
impl UserRepository for SqlxUserRepository {
    async fn load(&self, id: &UserId) -> Result<Option<User>, RepoError> {
        let row = sqlx::query_as!(UserRow, "SELECT * FROM users WHERE id = $1 AND erased_at IS NULL", id.0)
            .fetch_optional(&self.pool).await?;
        row.map(User::from).transpose()
    }
    async fn save(&self, user: &User) -> Result<(), RepoError> {
        sqlx::query!("UPDATE users SET email = $1, status = $2 WHERE id = $3", ...)
            .execute(&self.pool).await?;
        Ok(())
    }
}
```

- Migrations : `refinery` embarqué dans le binaire (`klaar-api`), exécutées au démarrage
- Tests : `sqlx::test` (rollback automatique transactionnel)
- Pool : `PgPoolOptions::new().max_connections(20).connect(&db_url)`
- **NOUVEAU v0.2** : tables d'extension créées au fil de l'eau (`skill_attestations`, `surge_zones`, `surge_disclosures`, `pro_subscriptions`, `api_clients`, `webhook_deliveries`, `cities`, `regulatory_registrations`) — migrations conditionnelles par feature flag d'activation de jalon

### 3.2 API HTTP/WS (`klaar-api`, ADR-003 actix-web)

```rust
// main.rs
HttpServer::new(|| {
    App::new()
        .wrap(TracingLogger::default())
        .wrap(Cors::permissive())
        .wrap(IdentityService::new())
        .service(
            web::scope("/api/v1")
                .service(handlers::signup_user)
                .service(handlers::create_request)
                .route("/missions/{id}/track", web::get().to(handlers::track_mission_ws))
                .service(handlers::serve_openapi)
                .service(SwaggerUi::new("/docs/{_:.*}").url("/openapi.json", ApiDoc::openapi())),
        )
        // NOUVEAU J13 — scope API publique partenaires (séparation logique)
        .service(
            web::scope("/api/v1/public")
                .wrap(RateLimitLayer::new(/* per-tier */))
                .service(handlers::public_catalog_sectors)
                .service(handlers::public_sector_availability),
        )
        .app_data(web::Data::new(AppState { db: pool.clone(), stripe: stripe.clone(), itsme, ... }))
})
.bind(("0.0.0.0", 8080))?
.run()
.await
```

### 3.3 Adapters externes

| Port | Adapter | Crate |
|---|---|---|
| `PaymentGateway` | Stripe Connect REST + webhook verifier | `klaar-stripe-adapter` |
| `IdentityProvider` | itsme OIDC | `klaar-itsme-adapter` |
| `GeolocationService` | OSM + Valhalla auto-hébergés (ADR-006) | `klaar-geo-adapter` |
| `PushNotifier` | APNs + FCM (ADR-007) | `klaar-push-adapter` |
| `ObjectStorage` | OVH S3 (`aws-sdk-s3`) + KMS | `klaar-storage-adapter` |
| `AntivirusScanner` | ClamAV daemon (`clamav-client`) | `klaar-av-adapter` |
| `AuditLogger` | PostgreSQL `audit_logs` table (WORM-like, partitionnée) | `klaar-audit-adapter` |
| `EmailSender` | Sendgrid / Postmark | `klaar-email-adapter` |
| `BiometricAuthenticator` *(NOUVEAU J12')* | Tauri plugin `biometric` + Stronghold | `klaar-biometric-adapter` |
| `RankingModel` *(NOUVEAU J13)* | Rust ML (`candle-core`) ou API Python sidecar (FastAPI) — ADR-009 à trancher | `klaar-ml-adapter` |
| `InsuranceProvider` *(NOUVEAU J13)* | API partenaire (AXA BE, Baloise, Ethias) | `klaar-insurance-adapter` |
| `AuthorityVerifier` *(NOUVEAU J11)* | Fédérations sectorielles (AIB-Vincotte) + KBO-BCE + INASTI | `klaar-authority-adapter` |
| `RegionConfigProvider` *(NOUVEAU J14)* | PostgreSQL table `regions` + Valhalla extract | `klaar-region-adapter` |
| `CrossCheckAuthorityGateway` *(NOUVEAU J11)* | Co-consommateur de `klaar-authority-adapter` | `klaar-authority-adapter` |
| `BillingGateway` *(NOUVEAU J13)* | Stripe Subscriptions REST | `klaar-stripe-adapter` (extension) |
| `OAuth2Server` *(NOUVEAU J13)* | `oxideck` ou impl maison (JWT ES256) | `klaar-api` (middleware) |
| `WebhookEmitter` *(NOUVEAU J13)* | `reqwest` + HMAC SHA-256 + retry exponential | `klaar-public-api-adapter` (dans `klaar-api`) |
| `SurgeRuleEngine` *(NOUVEAU J13)* | Impl pure Rust (pas d'IO) | `klaar-surge` (domain) |

---

## Couche 4 — Frontend *(hexagonale light)* · *full-stack*

> **3 clients** consommant l'API via le **même client TypeScript codegen** depuis `OpenApiDoc` (ADR-004). **Stack mobile lockée Tauri 2.0 + PWA** pour toute la roadmap (ADR-008 NOUVEAU).

### 4.1 Codegen TypeScript partagé

```bash
# CI génère le client depuis le backend
openapi-typescript http://klaar-api.local/api/v1/openapi.json -o ./packages/klaar-client/schema.d.ts
openapi-generator-cli generate -g typescript-axios -i http://... -o ./packages/klaar-client/axios
```

Le package `@klaar/client` est publié en interne (Verdaccio ou git submodule) et consommé par :
1. `admin-web/` (Astro + Svelte 5)
2. `tauri-app/src/` (Svelte 5 runes dans webview)
3. `pwa-public/` (NOUVEAU J12'/J13 — PWA grand public alternative)

> **Note v0.2** : la mention « Futur PWA / RN (post-MVP) » figurant en v0.1 est **retirée** — l'option React Native est explicitement écartée par décision superviseur v0.3 (ADR-008). La PWA est désormais un livrable planifié J12'/J13 (`pwa-public/`).

### 4.2 Tauri mobile (`tauri-app/`) — stack unique confirmée *(mis à jour v0.2)*

La stack mobile reste **Tauri 2.0** pour toute la roadmap (J0-J14). Décision superviseur v0.3 (ADR-008). **Aucune réécriture native RN/Flutter** n'est planifiée.

```text
tauri-app/
├── src-tauri/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs             (Tauri runtime)
│   │   ├── commands/           (handlers Tauri appelés depuis JS)
│   │   │   ├── camera.rs       (FR-020 photos preuves)
│   │   │   ├── geolocation.rs  (FR-019 tracking — foreground + J12' background)
│   │   │   ├── push.rs         (FR notifs — ADR-007)
│   │   │   ├── secure_storage.rs (refresh token)
│   │   │   ├── biometric.rs    (NOUVEAU J12' — FaceID/TouchID, FR-052)
│   │   │   └── deep_link.rs    (NOUVEAU J12' — FR-051 deep-linking)
│   │   └── plugins/
│   │       ├── geolocation/             (foreground MVP ; background = plugin Tauri conditionnel PoC H-2/H-13, fallback PWA)
│   │       ├── push-notifications/      (APNs + FCM, ADR-007)
│   │       ├── biometric/               (NOUVEAU J12' — plugin standard Tauri)
│   │       └── stronghold/              (NOUVEAU J12' — secure storage IOTA)
│   └── tauri.conf.json         (permissions granulaires, ADR-001 + ADR-008)
├── src/                        (Svelte 5 runes)
│   ├── routes/
│   ├── lib/
│   │   ├── api/                (client @klaar/client wrapper)
│   │   ├── stores/             (runes : $state, $derived)
│   │   ├── components/
│   │   └── features/
│   │       ├── matching-explainability/ (NOUVEAU J13 — feature importance IA, FR-056)
│   │       └── surge-disclosure/        (NOUVEAU J13 — transparence surge, FR-057)
│   └── app.html
└── package.json
```

**Évolution continue (pas de réécriture)** :
- **J12' Enhancement** : ajout plugins Tauri (`biometric`, `stronghold`, `deep-linking`, `geolocation` background conditionnel). Hotfix OTA via Tauri Updater (FR-054).
- **J13** : ajout composants IA UI (explicabilité matching, transparence surge), vues analytics Provider (FR-063), workflows souscription assurance (FR-059) et abonnement Pro (FR-058).
- **J14** : ajout sélecteur de ville + tiles régionaux + routing Valhalla étendu.

**Communication Svelte ↔ Rust embarqué** :
- Tauri `invoke('command_name', args)` pour accès natifs (camera, geo, push, secure storage, biometric)
- HTTPS direct vers backend cloud pour le reste (via client `@klaar/client`)

### 4.3 Admin web (`admin-web/`)

```text
admin-web/
├── src/
│   ├── pages/                  (Astro pages statiques)
│   │   ├── login.astro
│   │   ├── dashboard.astro
│   │   ├── kyc/[id].astro
│   │   ├── disputes.astro
│   │   ├── analytics/          (NOUVEAU J13 — FR-062 funnel, unit economics)
│   │   │   ├── funnel.astro
│   │   │   ├── unit-economics.astro
│   │   │   └── cities.astro    (NOUVEAU J14 — FR-068 multi-villes)
│   │   ├── catalog/            (NOUVEAU J11 — FR-047 ajout secteurs)
│   │   │   └── sectors.astro
│   │   └── skills/             (NOUVEAU J11 — FR-045/050 validation attestations)
│   │       └── attestations.astro
│   ├── components/             (îlots Svelte 5 interactifs)
│   │   ├── KycReviewer.svelte
│   │   ├── DashboardKpi.svelte
│   │   ├── DisputeMediator.svelte
│   │   ├── SkillAttestationReviewer.svelte  (NOUVEAU J11)
│   │   ├── SurgeMonitor.svelte              (NOUVEAU J13)
│   │   └── CityLaunchConsole.svelte         (NOUVEAU J14)
│   └── lib/api/                (client @klaar/client)
├── astro.config.mjs            (output: 'server' pour sessions ops)
└── package.json
```

### 4.4 State management (Svelte 5 runes)

```typescript
// store user réactif
let currentUser = $state<UserDto | null>(null);
let isAuthenticated = $derived(currentUser !== null);

// requête API réactive
async function loadMission(id: string) {
    const { data } = await depClient.getMission(id);
    mission.set(data);
}

// NOUVEAU J13 — store transparence surge
let surgeDisclosure = $state<SurgeDisclosureDto | null>(null);
let surgeAccepted = $derived(surgeDisclosure?.user_accepted ?? false);
```

### 4.5 i18n (C10)

- Catalog `fr.json`, `nl.json`, `en.json` compilés dans le bundle Svelte
- Détection navigateur + persistance locale User
- Aucun fetch runtime (sécurité ADR-005)
- **NOUVEAU J11/J14** : libellés secteurs et villes chargés depuis catalogue API (cache CDN 5 min)

### 4.6 PWA grand public *(NOUVEAU J12'/J13 — `pwa-public/`)*

PWA alternative pour accès navigateur (desktop et mobile) **sans installation d'app**. Stack :

```text
pwa-public/
├── src/
│   ├── routes/                 (SvelteKit ou Vite SPA)
│   ├── lib/
│   │   ├── api/                (client @klaar/client — endpoints publics + auth user)
│   │   ├── sw/                 (service worker offline-first)
│   │   └── components/
│   ├── app.html
│   └── manifest.webmanifest    (PWA installable, shortcuts, share target)
├── vite.config.ts
└── package.json
```

- **Vite + Svelte 5 (runes)** — partage maximal de composants avec `tauri-app/src/`
- **Service Worker offline-first** (cache API + tiles OSM + shell app)
- **Manifest PWA complet** : installable, `shortcuts` (1-tap "Plomberie urgence"), `share_target` (partage photos depuis galerie)
- **Fallback foreground pour géoloc** (Geolocation API standard W3C)

**Limites PWA connues (acceptées, ADR-008)** :
- iOS Safari : géoloc background coupée après ~30 s (verrou Apple) — fallback foreground dégradé assumé
- Push iOS : supporté via Push API depuis iOS 16.4 (Web Push compatible)
- Pas de secure enclave — fallback `sessionStorage` + vérification biométrique via **WebAuthn** (FR-052 partiel)
- Pas de caméra native avancée — `<input type="file" capture>` standard

**Feature parity matrix documentée** (FR-055) : PWA = catalogue + Demande + suivi Mission + chat (sans push background iOS, sans biométrie native). App Tauri native = + push, biométrie, background location, deep-linking.

---

## Couche 5 — IaC

### 5.1 Les 4 environnements (foyer `bootstrap-delivrabilite.md`)

| Env | But | Tier | Mapping branche GitFlow |
|---|---|---|---|
| `dev` | Local développeur | `docker compose` | — |
| `integration` | Tests automatisés | `docker compose` sur VM OVH | `develop` |
| `staging` | Pré-prod, données anonymisées | `K3s` single-node OVH | `release/*` |
| `production` | Live | `K3s` HA 2-nodes OVH BE/EU (Gravelines) | `main` |

### 5.2 Outils

- **Terraform** : provisioning OVH (instances, S3 buckets, KMS, DNS, load balancer)
- **salt-ssh** (idempotent) : configuration + **durcissement** (CIS benchmarks, fail2ban, auditd)
- **GitOps** : branche = source de vérité, ArgoCD ou Flux réconcilie en continu
- **Docker multi-stage** → image **distroless** en production (surface d'attaque minimale, ~50 Mo)
- **Rollback automatique** si tests post-déploiement échouent

### 5.3 Mapping ISO 27001 → IaC (extrait, foyer architecture.template.md)

| Contrôle ISO 27001 | Mise en œuvre IaC |
|---|---|
| A.5 (politiques) | `infra/salt/policies/` versionné, audits biennaux |
| A.8 (gestion actifs) | Inventaire Terraform déclaratif (`infra/inventory.tf`) |
| A.9 (contrôle accès) | Keycloak ou auth interne ops, MFA TOTP, RBAC strict (FR-041) |
| A.10 (cryptographie) | KMS OVH (clés rotées annuellement), TLS 1.3 partout |
| A.12 (sécurité exploitation) | salt-ssh durcissement CIS, fail2ban, auditd, log shipping Loki |
| A.13 (communications) | mTLS inter-services (K3s CNI), VPC OVH privé |
| A.14 (acquisition dev) | SBOM CycloneDX (CRA), `cargo-deny` licences |
| A.16 (gestion incidents) | Runbooks `infra/runbooks/`, AlertManager → ops, reporting 24 h NIS2 |
| A.17 (continuité) | Backups quotidiens S3 cross-region, restore testé mensuellement |
| A.18 (conformité) | Audit log WORM, exports régulateurs (FR-039) |

### 5.4 Souveraineté (Brief §14)

- **OVHcloud BE/EU** (Gravelines France + Limbourg Belgium) — données personnelles jamais hors EU
- **Clé KMS** OVH (jamais AWS/GCP)
- **Stripe EU** (Irlande) — conformité DSP2
- **itsme** = service belge, souveraineté eIDAS EU
- **NOUVEAU J13** : partenaire assurance BE (Baloise / AXA BE / Ethias) — mTLS + minimisation données (DPIA `assurance-integree`)

### 5.5 Multi-région *(NOUVEAU J14)*

Quand la capacité E4 (expansion géographique) est activée :

- **Replication PostgreSQL read-replica par ville** (latence lecture < 50 ms) — read-replica Anvers/Liège/Gand/Charleroi synchronisés depuis primaire Bruxelles
- **Tile-server OSM régional par zone** (Belgique entière dès J0 via Geofabrik extract ; refresh mensuel ; villes BE dédiées en J14 avec extracts locaux)
- **CDN OVH par région** (PoP par datacenter OVH BE/EU)
- **Déclaration APD régionale** si hors RBC (FR-067) : GBA flamand (Anvers, Gand), APD wallon (Charleroi, Liège)
- **Valhalla routing** : config étendue par ville (FR-066), fallback Mapbox API si panne (payant, ADR-006)
- **Runbook `infra/runbooks/city-activation.md`** : procédure opérationnelle d'activation d'une nouvelle ville (gate rentabilité + tiles + APD + Provider density)

---

## Couche 6 — CI/CD

### 6.1 Pipeline GitHub Actions (ou Forgejo Actions si on préfère souveraineté)

```yaml
# .github/workflows/ci.yml (synthèse)
on: [pull_request, push]

jobs:
  quality-gate:
    - rustup install
    - cargo fmt --check
    - cargo clippy -- -D warnings
    - cargo machete                   # dépendances inutilisées

  security-gate:
    - cargo audit                     # vulnérabilités connues
    - cargo deny check                # licences + advisories
    - gitleaks .                      # secrets dans le code
    - trivy fs .                      # filesystem scan

  tests:
    - cargo test --workspace          # unitaires + intégration (sqlx::test)
    - pnpm --filter admin-web test
    - pnpm --filter tauri-app test
    - pnpm --filter pwa-public test   # NOUVEAU J12'/J13

  contract-tests:
    - cargo build --bin klaar-api
    - schemathesis run http://localhost:8080/api/v1/openapi.json

  e2e-tests:
    - docker compose up -d
    - playwright test                 # admin web
    - maestro test tauri-app/e2e/     # mobile

  build-images:
    - docker build -f Dockerfile.distroless -t klaar-api:$SHA .
    - docker build -f Dockerfile.tauri-ios -t klaar-mobile-ios:$SHA .

  sbom:
    - cyclonedx-bom -i Cargo.lock -o sbom-api.json --format json

  deploy-staging:                     # uniquement sur push release/*
    - if: startsWith(github.ref, 'refs/heads/release/')
    - kubectl apply -f infra/staging/

  deploy-production:                  # tag only, validation humaine
    - if: startsWith(github.ref, 'refs/tags/v')
    - kubectl apply -f infra/production/
```

### 6.2 Hooks Git (DRY avec la CI)

- **Pre-commit** : `cargo fmt`, `cargo clippy`, `gitleaks`, `svelte-check`
- **RED-first hook** : bloque tout commit qui ajoute du code sans test (foyer L2)
- **Pre-push** : rejoue la CI complète localement (anti-filet)

### 6.3 Protection de branche (GitHub/Gitea)

- `main`, `develop`, `release/*` : **reviews obligatoires** (2 validateurs dont 1 ops senior)
- **CI verte obligatoire** pour merge
- **Cowork release** : merge sur `main` requiert un rapport de release signé GO par superviseur humain (foyer L2)
- **Pas de force-push** sur `main`

### 6.4 SBOM + Provenance (CRA, angle mort foyer)

- **CycloneDX** généré à chaque release → publié avec l'image Docker
- **SLSA provenance** : signature cosign + attestation build
- Reporting incident 24 h (NIS2/CRA) : procédure dans `infra/runbooks/incident.md`

### 6.5 Pipeline ML *(NOUVEAU J13)*

Quand la capacité E3 matching IA est activée :

```text
┌─────────────────┐   ┌──────────────────┐   ┌─────────────────┐
│ Features store  │ → │ Training (Rust   │ → │ Validation biais │
│ (PostgreSQL     │   │  candle-core ou  │   │ (Art. 10-15 AI  │
│  + materialized │   │  Python sidecar) │   │  Act)           │
│  views)         │   │                  │   └────────┬────────┘
└─────────────────┘   └──────────────────┘            │
                                                       ▼
┌─────────────────┐   ┌──────────────────┐   ┌─────────────────┐
│ A/B testing     │ ← │ Model registry   │ ← │ Déploiement     │
│ progressif      │   │ (MLflow)         │   │ Canary 10 %     │
│ (10 % → 50 %    │   │ versionning      │   │ → 50 % → 100 %  │
│  → 100 %)       │   │                  │   │                 │
└─────────────────┘   └──────────────────┘   └─────────────────┘
```

- **Training pipeline** : features extraction (Python sidecar ou Rust `candle-core`) → training → validation biais (demographic parity, equal opportunity) → déploiement
- **Audit biais semestriel automatisé** (Art. 12 AI Act) — job cron + rapport publié en console admin + alerte DPO si biais > seuil
- **Model registry** : versionning **MLflow** (auto-hébergé OVH, souveraineté)
- **A/B testing progressif** : 10 % → 50 % → 100 % du trafic, kill-switch auto si drift > 20 % (FR-056)
- **Données d'entraînement** : anti-poisoning (z-score, isolation forest), flag `DATA_SUSPICIOUS` sur Providers tentant d'injecter de fausses données
- **Trace AI Act Art. 12** : `matches.criteria JSONB` enrichi en J13 avec `model_version`, `features`, `score`, `latency` (5 ans WORM S3 Object Lock)

---

## Couche 7 — Monitoring & Sécurité

### 7.1 Observabilité

| Signal | Outil | Sink |
|---|---|---|
| Métriques | `metrics` crate + `metrics-exporter-prometheus` | Prometheus → Grafana |
| Logs structurés | `tracing` + `tracing-subscriber` JSON | Loki |
| Traces distribuées | `tracing-opentelemetry` | Tempo |
| Errors | Sentry SDK | Sentry EU (souveraineté) |
| Uptime | Blackbox exporter | Prometheus AlertManager |
| **Features drift** *(NOUVEAU J13)* | MLflow + custom metrics | Prometheus → Grafana (dashboard `matching-ia-drift`) |
| **Partner API rate-limit** *(NOUVEAU J13)* | tower-http + Redis | Prometheus (compteur 429 par partner) |

### 7.2 Alertes (foyer convergence-iac.md trigger) — *mis à jour v0.2*

AlertManager déclenche le **platform engineer** sur :
- API 5xx > 1 % sur 5 min
- p99 latence > 2 s sur 5 min
- Escrow non libéré > 72 h (job bloqué)
- Écart réconciliation Klaar ↔ Stripe > 0
- Densité Providers < 50 à Bruxelles (fill rate dégrade)
- **Densité Providers < 30 dans toute ville activée en J14** *(NOUVEAU J14)*
- **Surge coefficient > 1.5 sur 30 min** *(NOUVEAU J13 — alerte transparence Platform Work)*
- **Biais matching IA > seuil** *(NOUVEAU J13 — audit semestriel AI Act, kill-switch potentiel)*
- **API publique partenaire : rate-limit déclenché > 10 fois/min** *(NOUVEAU J13)*
- **Subscription churn > 5 % sur 30 j** *(NOUVEAU J13)*
- CyFun : tentative accès non autorisé, secret leak Git, sortie zone géo Provider

### 7.3 Sécurité runtime

- **Permissions Tauri** : allowlist stricte par permission (caméra, géoloc, push, filesystem restreint, **biométrie J12'**, **deep-linking J12'**)
- **Sandbox conteneur** : montages read-only, allowlist egress (pas d'outbound libre)
- **WAF** : Cloudflare ou OVH WAF sur l'endpoint `/api/*` + `/api/v1/public/*` (J13)
- **Rate-limiting** : tower-http + Redis pour distribué (par IP user, par ApiClient partenaire J13)
- **Audit log** : table `audit_logs` partitionnée mensuellement, **append-only** (RLS PostgreSQL + trigger empêchant UPDATE/DELETE)
- **DPIA géoloc** : document vivant `docs/privacy/dpia-geolocation.md`, review annuelle — **étendu en J12'** (background) et **J13** (matching IA)
- **mTLS** *(NOUVEAU J13)* : partenaire assurance BE + tier Enterprise API publique (FR-060)

### 7.4 RGPD & AI Act — *mis à jour v0.2*

- **Registre APD/GBA** : tenu à jour, tout traitement documenté. **NOUVEAU J14** : registre régional (Flandre, Wallonie) si ville hors RBC
- **DPO** : à nommer (interne ou externe)
- **Trace AI Act** : `matches.criteria JSONB` queryable, audit semestriel anti-biais. **NOUVEAU J13** : enrichi avec `model_version`, features, latence
- **Supervision humaine** : médiation litige par ops (jamais auto-décision pour sanction > SUSPENSION_7J). **NOUVEAU J13** : override ops obligatoire pour matchs à risque (Provider rating < 3.0)
- **Reporting incident RGPD** : sous 72 h à l'APD, procédure `runbooks/rgpd-incident.md`
- **DPIA étendus** *(NOUVEAU v0.2)* : `dpia-geolocation` (J12' background), `dpia-matching-ia` (J13), `dpia-api-publique` (J13), `dpia-assurance-integree` (J13)

---

## Contrat API (OpenAPI) · *API-first : écrit avant le code, versionné*

> ADR-004 (`utoipa`) — le contrat est **généré depuis le code**, mais **la spec est le point de vérité pour les consumers**. Toute rupture est un point irréversible.

### Endpoint public

- `GET /api/v1/openapi.json` — spec OpenAPI 3.0.x
- `GET /api/v1/docs` — Swagger UI interactif
- `GET /api/v1/health` — healthcheck (no auth)
- `GET /api/v1/ready` — readiness (incluant DB + Stripe + itsme)

### Versioning (rappel PRD §9bis.2)

- URL `/api/v1/...`
- Rupture majeure = `/api/v2/...` + maintien v1 6 mois minimum + ADR + validation humaine
- Header `Deprecation: true` + `Sunset: <date>` obligatoires en cas de dépréciation (FR-060)

### API publique partenaires *(NOUVEAU J13 — `/api/v1/public/...`)*

Quand la capacité E3 API publique est activée :

- **Endpoints dédiés sous `/api/v1/public/...`** (séparation logique vs endpoints user/ops)
- **Authentification OAuth2 `client_credentials`** (pas d'impersonation user)
- **Rate-limiting par tier** (Free / Pro / Enterprise) — Redis distribué
- **Documentation Swagger publique** auto-générée (utoipa) sur `https://docs.dep.be/partners` (FR/EN)
- **SDK partenaires** : TypeScript (npm `@klaar/partner-sdk`), Python (PyPI `klaar-partner`) — codegen depuis OpenAPI via `openapi-generator-cli`
- **Webhooks sortants** (FR-061) : `mission_completed`, `provider_available`, `sector_added` — signature HMAC SHA-256, retry exponential (1, 5, 30 min, 4 h, 24 h), dead-letter queue
- **Endpoints exposés** (lecture seule, minimisation PII) :
  - `GET /api/v1/public/catalog/sectors` — catalogue actif (FR/NL/EN)
  - `GET /api/v1/public/sectors/{code}/availability` — dispo agrégée par zone (k-anonymité ≥ 100)
  - `GET /api/v1/public/missions/{id}/status` — statut mission anonymisé (pas de PII)
  - `POST /oauth/token` — issue access token (1 h, no refresh)

---

## Glossaire DDD → mapping code — *mis à jour v0.2*

| Terme métier | Type Rust | BC | Table PG |
|---|---|---|---|
| Demande | `Request` (entity) | matching | `requests` |
| Prestataire | `Provider` (aggregate) | identity | `providers` |
| Utilisateur | `User` (aggregate) | identity | `users` |
| Secteur | `Sector` (entity) | catalog | `sectors` |
| Compétence | `Skill` (entity) | catalog | `skills` |
| Disponibilité | `Availability` (entity) | matching | `availabilities` |
| Matching | `Match` (entity) | matching | `matches` |
| Devis | `Quote` (aggregate) | payment | `quotes` |
| Intervention | `Mission` (aggregate) | intervention | `missions` |
| Escrow | `Escrow` (entity) | payment | `escrows` |
| Take | champ `take_cents` dans `Payout` | payment | `payouts` |
| Payout | `Payout` (entity) | payment | `payouts` |
| Litige | `Dispute` (aggregate) | trust | `disputes` |
| Trace | `AuditLog` (entity, append-only) | ops | `audit_logs` |
| MissionStatus | `enum MissionStatus` | intervention | CHECK constraint |
| Preuve | `EvidencePhoto` (entity) | intervention | `evidence_photos` |
| Sanction | `Sanction` (entity) | trust | `sanctions` |
| **Attestation compétence** *(NOUVEAU J11)* | `SkillAttestation` (aggregate) | skills | `skill_attestations` |
| **Autorité d'attestation** *(NOUVEAU J11)* | `AttestationAuthority` (enum) | skills | CHECK constraint |
| **Zone surge** *(NOUVEAU J13)* | `SurgeZone` (aggregate) | surge | `surge_zones` |
| **Divulgation surge** *(NOUVEAU J13)* | `SurgeDisclosure` (entity) | surge | `surge_disclosures` |
| **Abonnement Pro** *(NOUVEAU J13)* | `ProSubscription` (aggregate) | subscription | `pro_subscriptions` |
| **Client API partenaire** *(NOUVEAU J13)* | `ApiClient` (aggregate) | public-api | `api_clients` |
| **Webhook delivery** *(NOUVEAU J13)* | `WebhookDelivery` (entity) | public-api | `webhook_deliveries` |
| **Police assurance** *(NOUVEAU J13)* | `InsurancePolicy` (aggregate) | payment (extension) | `insurance_policies` |
| **Ville** *(NOUVEAU J14)* | `City` (aggregate) | region | `cities` |
| **Enregistrement réglementaire** *(NOUVEAU J14)* | `RegulatoryRegistration` (entity) | region | `regulatory_registrations` |

---

## Stratégie de tests

> Foyer `cycle-dev.md` + `gates.md` + `conformite.md`. Pyramide : **unit (Domain) >> integration (Application + Infra) > E2E (full-stack)**.

### Pyramide

```text
                    ┌───────┐
                    │  E2E  │   ~5 % (3 flux critiques + extension J11-J14, Maestro/Playwright)
                    └───────┘
                ┌─────────────┐
                │ Integration │ ~25 % (sqlx::test, actix-web::test, Stripe mock, AIB-Vincotte mock)
                └─────────────┘
        ┌─────────────────────────┐
        │      Unit (Domain)      │ ~70 % (invariants, value objects, machines à états, surge rules)
        └─────────────────────────┘
```

### Matrice 4×N (héritée de `cycle-dev.md`)

| Classe | BDD (Gherkin) | TDD (unitaire) | Conditionnement archétype Klaar (stateful full-stack) |
|---|---|---|---|
| `@happy` | Chemin nominal (PRD §6 + FR-045 à FR-068) | Invariants constructeurs (Domain), use cases nominaux | Tous |
| `@negative` | Entrées invalides (PRD §6 + extension) | `Result::Err` attendu, validation DTOs | Tous |
| `@edge` | Bornes (vide, max, concurrence, transitions d'état) | Property-based (`proptest`), concurrence (`loom`), transitions machine à états, IQR outliers (FR-048) | **stateful** : intégration DB `sqlx::test`, CAS atomique Postgres |
| `@security` | Abus, injection, autorisation, isolation | RBAC, injection SQL (`sqlx` query_as!), XSS (Svelte auto-escape), CSP | **full-stack** : CSP/XSS Svelte 5, DAST sur flux E2E critiques ; **API-first** : abus contrat (schemathesis) ; **NOUVEAU J13** : biais matching IA, anti-poisoning |

### Couverture cible par couche

| Couche | Couverture cible | Outil |
|---|---|---|
| Domain | **95 %** (le plus critique) | `cargo-tarpaulin` + `cargo-llvm-cov` |
| Application | 85 % | `cargo-tarpaulin` |
| Infrastructure | 70 % (mocks Stripe/itsme/AIB-Vincotte/assurance) | `cargo-tarpaulin` + testcontainers |
| Frontend | 60 % (logique critique seulement) | `vitest` (Svelte 5) |
| **ML pipeline (J13)** *(NOUVEAU)* | Validation biais + non-régression | `proptest` sur features + tests A/B |
| E2E | 3 flux maîtres + 1 flux extension | Playwright + Maestro |

### Test-Driven Emergence (Documentation Vivante)

- 3 `.feature` Gherkin maîtres (PRD §9.1) : happy path, litige, onboarding Provider
- **NOUVEAU J11** : 1 feature extension `skill_attestation_verification.feature`
- **NOUVEAU J13** : 1 feature extension `matching_ia_explainability.feature` + `surge_transparency.feature`
- **NOUVEAU J14** : 1 feature extension `city_activation.feature`
- Chaque `.feature` = 1 test E2E Playwright (admin) + 1 test Maestro (mobile)
- **Film des exécutions E2E** publié en CI → Documentation Vivante consultable par ops + dev

---

## API · Sécurité & RGPD

### Authentification & autorisation

- **JWT access** (1 h) + **refresh rotation** (30 j) — FR-004
- **itsme eIDAS substantial** pour Providers (FR-002)
- **MFA TOTP** pour ops (FR-041)
- **RBAC** 4 rôles ops : `kyc_reviewer`, `dispute_mediator`, `super_admin`, `read_only` — **étendu en J11/J13/J14** : `catalog_manager`, `bulk_recruiter`, `analytics_viewer`, `marketing_manager`, `city_launch_manager`, `release_manager`
- **NOUVEAU J12'** : SCA biométrique pour paiements ≥ 100 € (FR-052, DSP2)
- **NOUVEAU J13** : OAuth2 `client_credentials` pour partenaires API publique

### RGPD

- **DPIA** `docs/privacy/dpia-geolocation.md` avant tout tracking (Invariant §10.5) — étendu J12' background, J13 matching IA
- **Registre APD/GBA** Bruxelles tenu à jour — **étendu J14** registres régionaux (Flandre, Wallonie)
- **Droit d'accès** : export JSON + PDF signé sous 30 j (FR-039)
- **Droit à l'effacement** : FR-005 (PII anonymisées, comptes archivés 7 ans pour obligation comptable). **NOUVEAU J12'** : étendu aux positions géoloc background (FR-053).
- **Minimisation géoloc** : précision 50 m, purge post-Mission 24 h
- **K-anonymité** *(NOUVEAU J13)* : agrégats analytics ≥ 100 individus (FR-062/063/068), `INFERENCE_RISK` warning

### AI Act

- **Trace matching** : `matches.criteria JSONB`, queryable (Art. 12). **NOUVEAU J13** : enrichi `model_version`, `features`, `latency` (FR-056).
- **Supervision humaine** : médiation litige, sanction > SUSPENSION_7J (Art. 14). **NOUVEAU J13** : override ops pour matchs à risque.
- **Audit biais semestriel** : `infra/audits/ai-bias-YYYY-Hx.md` + job automatisé (FR-056)
- **Kill-switch** : `DISABLE_IA_MATCHER` activable auto (drift > 20 %) ou manuel (FR-056)

### NIS2 / CyFun (Belgique)

- **CyFun Basic** visé dès le Sprint 0
- **Reporting incident 24 h** au CCB
- **Biennal audit** externe
- **NOUVEAU J13** : SBOM CycloneDX du pipeline ML + reporting incident lié au modèle (biais critique)

---

## ADR (Architecture Decision Records)

### ADR-001 — Cohérence Rust (cloud + Tauri embarqué) *(racine : écologie des savoirs, sumak kawsay)*
Validé superviseur 2026-07-18. Voir `docs/adr/ADR-001-coherence-rust.md`. **Confirmé par décision v0.3** : pas de natif RN/Flutter (ADR-008 renforce l'ADR-001).

### ADR-002 — `sqlx` CQRS SQL pur (vs ORM) *(racine : mottainai, écologie des savoirs)*
Validé superviseur 2026-07-18. Voir `docs/adr/ADR-002-persistance-sqlx-cqrs.md`.

### ADR-003 — Framework backend `actix-web` (vs `axum`) *(racine : sept générations, répondre-de — override superviseur)*
Validé superviseur 2026-07-18 (override). Voir `docs/adr/ADR-003-actix-backend.md`.

### ADR-004 — Contrat API `utoipa` (vs `aide`) *(racine : sept générations, DRY)*
Validé superviseur 2026-07-18. Voir `docs/adr/ADR-004-openapi-utoipa.md`. **Étendu v0.2** : `/api/v1/public/...` (J13) sous le même contrat utoipa.

### ADR-005 — License propriétaire *(racine : répondre-de, réversibilité)*
Validé superviseur 2026-07-18. Voir `docs/adr/ADR-005-license-proprietaire.md`.

### ADR-006 — Cartographie : OpenStreetMap + Valhalla (auto-hébergés) *(confirmé v0.2)*
Décision retenue : **OSM + Valhalla** (souveraineté, coût 0). Plan B Mapbox API en cas d'insuffisance routing (payant, alerte budget). Étendu en J14 (FR-066 tiles/routing régionaux). Voir `docs/adr/ADR-006-cartographie-osm.md`.

### ADR-007 — Push notifications : APNs + FCM *(validé)*
Plan B UnifiedPush (Android) ou capacitor (iOS) si plugin Tauri push instable. Voir `docs/adr/ADR-007-push-apns-fcm.md`. **Étendu v0.2** : push rich media + deep-linking + actions inline (FR-051, J12').

### ADR-008 — Stack mobile : Tauri 2.0 + PWA uniquement *(NOUVEAU v0.2)*
Voir section dédiée ci-dessus. Statut : Accepté (override superviseur v0.3), fichier dédié `/home/user/Klaar/docs/adr/ADR-008-stack-mobile-tauri-pwa-only.md` PENDING création en Sprint 0.

### ADR-009 — Matching IA : Rust ML (`candle-core`) vs Python sidecar (FastAPI) *(À TRACER en Sprint 0 ou à l'activation J13)*
Open question §Questions ouvertes #2. Critères : performance brute (Rust), écosystème ML mature (Python), fréquence de ré-entraînement, souveraineté (les deux OK avec OVH auto-hébergé).

### ADR-010 — API publique : monétisation freemium vs pay-per-call *(À TRACER à l'activation J13)*
Open question §Questions ouvertes #3. Influence le pricing tier (Free / Pro / Enterprise) et la facturation Enterprise mensuelle.

*Chaque ADR : décision · contexte · alternatives écartées · conséquences · (racine sagesse si éclairant) · point irréversible · validation humaine.*

---

## Synthèse — Capacités d'extension dans l'architecture *(NOUVEAU v0.2)*

| Capacité | Brief | PRD FR | Crates/modules impactés | ADR associé | Activation |
|---|---|---|---|---|---|
| **C11 Densification secteurs** | v0.3 §7 | FR-045 à FR-050 | `klaar-skills` (NOUVEAU), `klaar-catalog` (extension), `klaar-authority-adapter` (NOUVEAU) | ADR-001 (Rust cohérence) | **J11** (gate fill rate > 60 %) |
| **C12' Enhancement Tauri/PWA** | v0.3 §7 | FR-051 à FR-055 | `tauri-app` (plugins biometric/stronghold/deep-link/geo-bg), `pwa-public/` (NOUVEAU), `klaar-biometric-adapter` (NOUVEAU) | **ADR-008 (NOUVEAU v0.2)** | **J12'** (gate MAU + besoins UX, PoC plugin géoloc bg) |
| **C13 IA/monétisation/ouverture** | v0.3 §7 | FR-056 à FR-063 | `klaar-matching` (extension IA), `klaar-surge` (NOUVEAU), `klaar-subscription` (NOUVEAU), `klaar-public-api` (NOUVEAU), `klaar-ml-adapter` (NOUVEAU), `klaar-insurance-adapter` (NOUVEAU) | ADR-009 (à tracer), ADR-010 (à tracer) | **J13** (gate base Providers + demande partenaires) |
| **C14 Expansion géo** | v0.3 §7 | FR-064 à FR-068 | `klaar-region-adapter` (NOUVEAU), `klaar-geo-adapter` (extension routing régional), multi-région IaC | ADR-006 confirmé | **J14** (gate rentabilité RBC prouvée > 12 mois) |

> Toutes les extensions sont activées **au fil de l'eau** selon les gates go/no-go du DEVIS §4.5. Le client peut s'arrêter à n'importe quel jalon (roadmap continue Brief §19). Aucune dépendance technique irréversible n'est créée tant que la capacité n'est pas activée — les crates existent en placeholder (feature flag `cargo feature j11`/`j12_prime`/`j13`/`j14`).

---

## Synthèse *répondre-de*

L'architecture Klaar v0.2 suit strictement la doctrine foyer :
- **Hexagonale + DDD + SOLID** : Domain pure, dépendances vers l'intérieur — **12 BC** (8 cœur + 4 extension J11/J13/J14)
- **TDD + BDD + Documentation Vivante** : 4 classes `@happy @negative @edge @security` sur **68 FR** (44 cœur + 24 extension)
- **Contrat API matérialisé** (ADR-004 + ADR-001 cohérence Rust) — étendu `/api/v1/public/*` J13
- **Enforcement 3 anneaux** : permissions (opencode) + plugin + substrat (hooks git, CI, protection branche, sandbox)
- **Convergence IaC** : salt-ssh idempotent + GitOps + distroless + rollback — **multi-région J14**
- **Bootstrap reproductible** : `git clone` + commande agent = 4 environnements + postes superviseur
- **Sobriété** : Rust + Tauri (binaires légers), OVHcloud BE/EU (énergie décarbonée) — **renforcée par ADR-008** (pas de natif, 1 codebase)
- **Souveraineté** (arbitrage-hybride) : OVH EU + itsme BE + Stripe EU + clé KMS OVH + MLflow auto-hébergé (J13)

---

## Questions ouvertes pour le superviseur (à valider avant Scrum Master)

1. **ADR-006 Mapbox vs OSM** : arbitrage coût (Mapbox payant) vs souveraineté (OSM) — **résolu v0.2 : OSM + Valhalla retenu**. Reste à valider performances routing en Sprint 0 (PoC)
2. **ADR-007 push Tauri Mobile** : vérifier maturité plugins (H-2 Brief) — **validé 2026-07-18, plan B UnifiedPush tracé**
3. **CI/CD hébergeur** : GitHub Actions (dépendance US) vs Forgejo Actions (souveraineté) — ADR méthodologique
4. **Errors tracking Sentry EU** : souveraineté OK mais dépendance SaaS — alternative OSS GlitchTip ?
5. **Workspace Cargo** : un seul monorepo (proposé) vs multi-repos par BC — ADR si multi-repo
6. **ArgoCD vs Flux** : GitOps choice — ADR en Sprint 0
7. **Auth ops interne** : Keycloak (lourd) vs auth native PostgreSQL (léger) — ADR
8. **ADR-008 Stack mobile Tauri/PWA only** *(NOUVEAU v0.2)* : confirmer le tracé formel — fichier dédié `/home/user/Klaar/docs/adr/ADR-008-stack-mobile-tauri-pwa-only.md` à créer en Sprint 0
9. **ADR-009 Matching IA** *(NOUVEAU v0.2)* : Rust ML (`candle-core`) vs Python sidecar (FastAPI) — arbitrage performance vs écosystème ML. À trancher au plus tard au moment d'activer J13.
10. **ADR-010 API publique** *(NOUVEAU v0.2)* : monétisation freemium (Free / Pro / Enterprise) vs pay-per-call pur ? Influence la conception des quotas et du billing Enterprise mensuel.
11. **Plugin Tauri géoloc background iOS** *(NOUVEAU v0.2)* : faut-il développer un plugin custom (effort, mitiga­tion H-13 palier b) ou se contenter de PWA foreground comme fallback durable (palier a) ? Story 0.12 étendue au Sprint 0 doit trancher.
12. **Insurance partner BE** *(NOUVEAU v0.2)* : pré-selectionner 1-2 partenaires (AXA BE, Baloise, Ethias) en amont de J13 — impacte DPIA `assurance-integree` et contrat API mTLS.
13. **Authority Verifier** *(NOUVEAU v0.2)* : peut-on automatiser la cross-check BCE/INASTI/fédérations sectorielles (APIs publiques existent-elles en Belgique) ? Si non, validation ops manuelle systématique (coût récurrent).
14. **Activation feature flags jalons** *(NOUVEAU v0.2)* : adopter `cargo feature j11/j12_prime/j13/j14` (compilation conditionnelle des crates d'extension) vs runtime feature flags (toutes les crates compilées, activation via config) ? Préférence foyer pour compile-time (YAGNI, sobriété).

---

*Dérivé du Manifeste Maury (CC BY-SA 4.0). Méthode Foyer. Version 0.2 — Architecture étendue aux capacités C11-C14, ADR-008 stack mobile Tauri/PWA only, 2 ADR à tracer (009 IA, 010 API publique). En attente de validation superviseur (signature humaine PENDING) avant passage au Scrum Master.*
