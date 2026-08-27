# PRD — Klaar

*Livrable du Product Manager · TOGAF Phases B-C · Phase 1 BMAD.*
*Chaque exigence se rattache à une capacité du Brief `01-Product-Brief.md` §7 (traçabilité).*

```
---
projet: Klaar
persona: Product Manager
date: 2026-07-18
version: 0.3 (extension E1-E4 détaillés, 24 nouveaux FR)
superviseur_validateur: [à valider pour passage Architecte]
signature_humaine: PENDING
brief_source: docs/bmad-livrables/01-Product-Brief.md v0.3
---
```

## 1. Résumé exécutif

Klaar est une marketplace mobile (Tauri 2.0 iOS+Android) et web admin (Astro+Svelte) de dépannage et services à la demande multi-secteurs, en pilote RBC. Le MVP couvre 5 secteurs (plomberie, serrurerie, électricité, dépannage auto, livraison objet) sur 8 bounded contexts, avec matching géoloc < 5 min, paiement Stripe Connect (take-rate 18 %, escrow), messagerie photos, notation double-sens. Backend Rust hexagonal + PostgreSQL, hébergement OVHcloud BE/EU, conformité RGPD / NIS2-CyFun / DSP2 / Platform Work dès le Sprint 0.

## 2. Objectifs produit (mesurables)

| Objectif | Cible an 1 | Cible an 3 |
|---|---|---|
| MAU RBC | 25 000 | 300 000 |
| GMV annuel | 4,5 M € | 25 M € |
| Fill rate | > 60 % | > 70 % |
| Time-to-first-match (P50) | < 8 min | < 5 min |
| NPS post-intervention | > 40 | > 50 |
| Cancel rate User | < 15 % | < 10 % |
| LTV/CAC blended | > 3:1 | > 4:1 |
| Payback period | < 12 mois | < 6 mois |
| Conformité CyFun Basic | ✅ | ✅ |
| 0 incident RGPD géoloc déclaré APD | ✅ | ✅ |

## 3. Périmètre — MVP / Hors scope

### MVP (Brief §11)
- **Géographie** : Région de Bruxelles-Capitale (19 communes)
- **Secteurs** : plomberie urgence, serrurerie, électricité, dépannage auto, livraison objet précis
- **Clients** : app Tauri mobile iOS + Android, console admin web Astro+Svelte 5
- **Capacités couvertes** : C1 Identity, C2 Catalog, C3 Matching, C4 Devis, C5 Intervention, C6 Payment, C7 Messaging, C8 Trust, C9 Ops admin, C10 i18n FR/NL/EN

### Hors scope MVP (renvoyé post-MVP, Brief §12)
Bascule native · secteurs supplémentaires · matching IA · surge pricing · subscription pro · assurance intégrée · API publique · extension géographique · PWA web grand public · push rich media · analytics avancé.

### Périmètre d'extension (v0.3, post-MVP au fil de l'eau)

Détail en §7 du présent document (FR-045 à FR-068) :
- **E1 — Densification secteurs** (+5-8 secteurs : chauffage, électroménager, bricolage, jardinage, déménagement, ménage)
- **E2' — Enhancement Tauri/PWA continu** (push rich, biométrie, géoloc background, PWA grand public) — **pas de bascule native**
- **E3 — Intelligence, monétisation & ouverture** (matching IA, surge, subscription, assurance, API publique, analytics)
- **E4 — Expansion géographique** (Anvers, Liège, Gand, Charleroi)

## 4. Glossaire DDD (figé depuis Brief §8)

Repris inchangé depuis Brief §8 + ajouts PRD :

| Terme | Définition |
|---|---|
| **MissionStatus** | `CREATED` → `MATCHED` → `ACCEPTED` → `PROVIDER_EN_ROUTE` → `ON_SITE` → `COMPLETED` → `RELEASED` ; branches : `CANCELLED` (User/Provider), `DISPUTED`, `REFUNDED`, `NO_MATCH` |
| **RequestStatus** | `DRAFT` → `BROADCASTING` → `MATCHED` → `MISSION_CREATED` ; branches : `CANCELLED_USER`, `NO_MATCH`, `EXPIRED` |
| **EscrowStatus** | `PRE_AUTHORIZED` → `CAPTURED` → `RELEASED` ; branches : `REFUNDED_FULL`, `REFUNDED_PARTIAL`, `FROZEN_DISPUTE` |
| **KycStatus** | `PENDING_EMAIL_VERIFY` → `SUBMITTED` → `PENDING_OPS_REVIEW` → `APPROVED` ; branches : `REJECTED`, `EXPIRED`, `ERASED` |
| **SanctionLevel** | `WARNING`, `SUSPENSION_7J`, `SUSPENSION_30J`, `BAN` |
| **DisputeStatus** | `OPENED` → `UNDER_MEDIATION` → `RESOLVED_FULL_REFUND` / `RESOLVED_PARTIAL_REFUND` / `RESOLVED_PROVIDER_FAVOR` / `RESOLVED_USER_FAVOR` |
| **Trace** | Journal immuable des décisions algorithmiques (AI Act Art. 12, Platform Work) |

## 5. Bounded contexts → modules

| BC | Crate Rust | Tables PG principales |
|---|---|---|
| **Identity & Access** (IDN) | `klaar-identity` | `users`, `providers`, `kyc_documents`, `sessions`, `payment_methods` |
| **Catalog** (CTL) | `klaar-catalog` | `sectors`, `skills`, `indicative_prices` |
| **Matching & Dispatch** (MCH) | `klaar-matching` | `requests`, `availabilities`, `matches` |
| **Intervention** (INT) | `klaar-intervention` | `missions`, `mission_statuses`, `evidence_photos` |
| **Payment** (PAY) | `klaar-payment` | `quotes`, `escrows`, `payouts`, `invoices`, `stripe_events` |
| **Messaging** (MSG) | `klaar-messaging` | `conversations`, `messages`, `attachments` |
| **Trust & Moderation** (TRU) | `klaar-trust` | `ratings`, `reviews`, `disputes`, `sanctions`, `mediations` |
| **Ops & Admin** (OPS) | `klaar-ops` | `audit_logs`, `kpi_snapshots`, `regulatory_exports`, `ops_users`, `ops_roles` |

## 6. Exigences fonctionnelles

> **Format foyer renforcé** : chaque FR = préconditions + scénarios Gherkin multi-étapes (avec `Examples`) + 4 classes de tests (`@happy` + `@negative` + `@edge` + `@security`) détaillées en N scénarios chacune + capacité Brief rattachée.

---

### Module : Identity & Access (IDN)

#### FR-001 — Inscription Utilisateur
- **En tant que** particulier · **je veux** créer un compte email + mot de passe · **afin de** pouvoir émettre des Demandes
- **Préconditions** : aucun compte existant avec cet email ; email format RFC 5322 ; mot de passe ≥ 12 chars (NIST 800-63B)
- **Garanties post** : compte `User` créé en `PENDING_EMAIL_VERIFY` ; email de vérification envoyé avec token JWT courte durée (1 h) ; audit log
- **Capacité Brief** : C1

```gherkin
Feature: Inscription Utilisateur
  Background:
    Given le système est opérationnel
    And le rate-limit IP est à 0 inscription sur la dernière heure

  # === @happy ===
  Scenario: Inscription nominale
    Given un visiteur avec email "marie@example.eu" non existant
    When il soumet email + mot de passe "Marie@2026Secure" + locale "fr"
    Then un compte User est créé en statut "PENDING_EMAIL_VERIFY"
    And un email de vérification est envoyé à "marie@example.eu"
    And un audit_log "USER_SIGNUP" est créé
    And l'IP est incrémentée au rate-limit (1/5 tentatives/heure)

  Scenario: Vérification email par token
    Given un User en "PENDING_EMAIL_VERIFY" avec token "abc123" valide < 1 h
    When il GET "/api/v1/auth/verify-email?token=abc123"
    Then son statut passe à "ACTIVE"
    And le token est marqué utilisé (non rejouable)

  # === @negative ===
  Scenario Outline: Validation champs invalides
    Given un visiteur avec email "<email>"
    When il soumet email + mot de passe "<password>"
    Then la réponse est 400 avec code d'erreur "<code>"
    Examples:
      | email              | password           | code                |
      | "invalide"         | "Marie@2026Secure" | EMAIL_MALFORMED     |
      | "marie@example.eu" | "court"            | PASSWORD_TOO_SHORT  |
      | "marie@example.eu" | ""                 | PASSWORD_EMPTY      |
      | ""                 | "Marie@2026Secure" | EMAIL_EMPTY         |

  Scenario: Email déjà existant
    Given un User existant avec email "marie@example.eu"
    When un visiteur soumet un nouvel email "marie@example.eu"
    Then la réponse est 409 avec code "EMAIL_ALREADY_EXISTS"
    And aucun email n'est envoyé (anti-énumération)

  Scenario: Token expiré
    Given un User en "PENDING_EMAIL_VERIFY" avec token "abc123" créé il y a 2 h
    When il GET "/api/v1/auth/verify-email?token=abc123"
    Then la réponse est 410 avec code "TOKEN_EXPIRED"

  # === @edge ===
  Scenario: Double soumission concurrente (race)
    Given 2 requêtes simultanées avec email "marie@example.eu"
    When elles arrivent en parallèle
    Then 1 seule réussit (201), l'autre reçoit 409 "EMAIL_ALREADY_EXISTS"
    And 1 seul email de vérification est envoyé

  Scenario: Email unicode normalisé
    When un visiteur soumet email "JØrgen@Üniverse.eu"
    Then le système normalise en lowercase + NFC "jørgen@üniverse.eu"
    And l'inscription réussit

  Scenario: Locale non supportée
    When un visiteur soumet locale "de"
    Then la réponse est 200 avec locale fallback "fr"
    And un warning "LOCALE_FALLBACK" est loggé

  Scenario: Rate-limit atteint (5/IP/heure)
    Given 5 inscriptions réussies depuis l'IP "1.2.3.4" dans la dernière heure
    When une 6e tentative arrive
    Then la réponse est 429 "RATE_LIMIT_EXCEEDED" avec Retry-After: 3600

  # === @security ===
  Scenario: Password jamais loggé en clair
    When une inscription est soumise
    Then les logs ne contiennent pas le mot de passe en clair
    And le mot de passe est hashé avec argon2id (memory=64 MiB, iterations=3)

  Scenario: Email générique anti-énumération
    Given un email existant OU non existant
    When un attaquant tente de l'énumérer
    Then la réponse est identique (timing + payload) dans les deux cas
    And le système retourne toujours "si cet email existe, un email a été envoyé"

  Scenario: Captcha après 3 échecs IP
    Given 3 échecs de validation depuis l'IP "1.2.3.4"
    When une 4e tentative arrive
    Then un challenge hCaptcha est requis
```

#### FR-002 — Authentification itsme (User + Provider)
- **En tant que** User ou Provider belge · **je veux** m'authentifier via itsme · **afin de** vérifier mon identité légale (eIDAS substantial)
- **Préconditions** : compte pré-existant ; numéro de téléphone BE/EU enregistré ; itsme mobile installé
- **Garanties post** : claim `verified_identity` au niveau eIDAS substantial ajouté au profil
- **Capacité Brief** : C1

```gherkin
Feature: Authentification itsme

  Background:
    Given itsme IdP opérationnel
    And le client a lancé le flow OAuth2 OIDC

  # === @happy ===
  Scenario: itsme complet (User)
    Given un User avec phone BE "+32 470 12 34 56"
    When il déclenche son itsme et valide sur mobile
    Then son profil gagne le claim "verified_identity:substantial"
    And une session est créée (1 h access + 30 j refresh)
    And un audit_log "ITSME_VERIFIED" est créé

  Scenario: itsme en flux Provider (KYC étape 3)
    Given un Provider candidat avec BCE + assurance validés
    When il complète son itsme
    Then son profil passe en "PENDING_OPS_REVIEW"
    And le claim eIDAS substantial est enregistré

  # === @negative ===
  Scenario Outline: Échecs itsme
    Given un User avec phone BE valide
    When itsme retourne "<error>"
    Then l'authentification échoue avec code "<code>"
    And le fallback email est proposé
    Examples:
      | error              | code                |
      | "user_cancelled"   | ITSME_CANCELLED     |
      | "timeout"          | ITSME_TIMEOUT       |
      | "phone_not_bound"  | ITSME_PHONE_MISSING |
      | "server_error"     | ITSME_UNAVAILABLE   |

  Scenario: itsme indisponible
    Given itsme IdP en panne (5xx)
    When un User tente son itsme
    Then la réponse est 503 "ITSME_UNAVAILABLE"
    And le fallback email est proposé automatiquement
    And une alerte ops est levée

  # === @edge ===
  Scenario: itsme déjà lié à un autre compte
    Given un User A avec itsme vérifié (BSN hash X)
    When un User B tente son itsme et obtient le même BSN hash X
    Then la réponse est 409 "ITSME_ALREADY_LINKED"
    And les deux comptes sont flaggés pour review ops (anti-duplicate)

  Scenario: Phone non-BE
    Given un User avec phone français "+33 6 12 34 56 78"
    When il tente son itsme
    Then la réponse est 422 "PHONE_NOT_BE"
    And un fallback email est proposé

  Scenario: Ré-authentification après expiration session
    Given un User avec itsme vérifié mais session expirée
    When il re-tente son itsme
    Then son claim est conservé (pas de re-vérification)
    And une nouvelle session est créée

  # === @security ===
  Scenario: Token itsme JWT validé
    When le callback itsme est reçu avec un JWT
    Then le JWT est validé (signature, audience, expiration, nonce)
    And le nonce CSRF est vérifié contre la session en cours

  Scenario: Pas de stockage BSN en clair
    When le BSN est reçu de itsme
    Then seul un hash argon2id du BSN est stocké
    And le BSN en clair n'est jamais persisté ni loggé

  Scenario: Token à courte durée
    Given un token itsme émis
    Then sa durée de vie est ≤ 60 s
    And un token utilisé ne peut être rejoué
```

#### FR-003 — Onboarding Provider (KYC BCE)
- **En tant que** Provider candidat · **je veux** soumettre mon numéro BCE + assurance RC · **afin de** recevoir des Demandes (Invariants §10.1, §10.8)
- **Préconditions** : compte User authentifié ; numéros BCE au format 10 chiffres ; PDF assurance < 10 Mo
- **Garanties post** : profil `Provider` en `PENDING_OPS_REVIEW` ; BCE cross-checké avec API KBO-BCE publique ; document assurance scanné ; audit log
- **Capacité Brief** : C1

```gherkin
Feature: Onboarding Provider

  Background:
    Given un User authentifié en "ACTIVE"
    And l'API KBO-BCE publique est opérationnelle

  # === @happy ===
  Scenario: Onboarding complet
    Given un User sans profil Provider
    When il soumet BCE "0123.456.789" + upload assurance PDF + Skills ["plomberie", "serrurerie"]
    Then le BCE est validé contre KBO-BCE public (entreprise active, non en faillite)
    And le PDF est scanné (taille, type, antivirus OK)
    And le profil Provider est créé en "PENDING_OPS_REVIEW"
    And un email de notification ops est envoyé

  # === @negative ===
  Scenario Outline: BCE invalide
    When le Provider soumet BCE "<bce>"
    Then la réponse est 400 avec code "<code>"
    Examples:
      | bce            | code              |
      | "123"          | BCE_FORMAT        |
      | "012345678X"   | BCE_FORMAT        |
      | "0000000000"   | BCE_NOT_FOUND     |

  Scenario: BCE en faillite
    Given BCE "0987.654.321" actif mais statut "FAILLITE" KBO-BCE
    When le Provider soumet ce BCE
    Then la réponse est 422 "BCE_BANKRUPT"

  Scenario: Assurance expirée
    Given une attestation assurance avec expiration dans le passé
    When le Provider l'upload
    Then la réponse est 422 "INSURANCE_EXPIRED"
    And un message propose de renouveler

  Scenario: Skill non couvert MVP
    When le Provider sélectionne Skill "chauffage"
    Then la réponse est 422 "SKILL_NOT_MVP"

  # === @edge ===
  Scenario: BCE déjà utilisé
    Given un Provider existant avec BCE "0123.456.789"
    When un nouveau Provider soumet le même BCE
    Then la réponse est 409 "BCE_ALREADY_USED"
    And un review ops est créé (anti-fraude multi-comptes)

  Scenario: Assurance > 10 Mo
    When le Provider upload une assurance de 15 Mo
    Then la réponse est 413 "FILE_TOO_LARGE"
    And un message propose compression

  Scenario: PDF corrompu
    Given un PDF assurance avec magic bytes invalides
    When le Provider l'upload
    Then la réponse est 422 "FILE_CORRUPTED"

  Scenario: KBO-BCE API en panne
    Given l'API KBO-BCE indisponible (timeout)
    When le Provider soumet son BCE
    Then la réponse est 202 "PENDING_BCE_CHECK"
    And un job différé vérifiera le BCE quand l'API reviendra (max 24 h)

  Scenario: Provider annule en cours de review
    Given un Provider en "PENDING_OPS_REVIEW"
    When il annule sa demande
    Then son profil passe en "CANCELLED"
    And un nouvel onboarding nécessite une nouvelle soumission complète

  # === @security ===
  Scenario: Scan antivirus obligatoire
    When un PDF assurance est uploadé
    Then il est scanné par ClamAV avant stockage
    And un PDF infecté est rejeté avec 422 "FILE_INFECTED"

  Scenario: BCE cross-check automatique
    When un BCE est soumis
    Then il est vérifié contre KBO-BCE public via TLS 1.3 + JWT serveur
    And le résultat est journalisé dans audit_log

  Scenario: Audit log non-effaçable
    Given un onboarding Provider avec BCE + assurance
    When le Provider demande son effacement RGPD
    Then ses PII sont anonymisés
    And l'audit_log de l'onboarding est conservé (assertion comptable + CyFun)
```

#### FR-004 — Gestion session et refresh
- **En tant que** User authentifié · **je veux** une session valide 1 h + refresh 30 j · **afin de** ne pas me réauthentifier à chaque usage
- **Préconditions** : authentification réussie (email/itsme)
- **Garanties post** : access token JWT 1 h + refresh token rotatif 30 j en cookie httpOnly Secure SameSite=Lax
- **Capacité Brief** : C1

```gherkin
Feature: Session et refresh

  # === @happy ===
  Scenario: Login nominal
    Given un User "ACTIVE" avec password "Marie@2026Secure"
    When il POST "/api/v1/auth/login" avec credentials corrects
    Then un access token JWT (1 h) est retourné
    And un refresh token rotatif est posé en cookie httpOnly

  Scenario: Refresh nominal
    Given un User avec refresh token valide
    When il POST "/api/v1/auth/refresh"
    Then un nouvel access token est émis
    And l'ancien refresh est invalidé (rotation)
    And un nouveau refresh est posé

  # === @negative ===
  Scenario: Refresh expiré
    Given un refresh token expiré (31 j)
    When il POST "/api/v1/auth/refresh"
    Then la réponse est 401 "REFRESH_EXPIRED"
    And l'User doit se réauthentifier

  Scenario: Refresh révoqué (logout)
    Given un User ayant fait logout
    When son ancien refresh est utilisé
    Then la réponse est 401 "REFRESH_REVOKED"

  # === @edge ===
  Scenario: Réutilisation d'un refresh (rotation = vol détecté)
    Given un refresh token "R1" déjà utilisé (rotation → R2)
    When un attaquant utilise "R1"
    Then la chaîne entière R1→R2 est invalidée
    And l'User est forcé de se réauthentifier
    And une alerte sécurité est levée

  Scenario: User désactive son compte
    Given un User "ERASED"
    When son refresh est utilisé
    Then la réponse est 401 "ACCOUNT_ERASED"

  # === @security ===
  Scenario: Binding UA+IP+device
    Given un refresh token lié à UA="Firefox/120" + IP="1.2.3.4"
    When il est utilisé depuis UA="curl/8" + IP="5.6.7.8"
    Then une alerte "ANOMALY_REFRESH" est levée
    And un challenge itsme est requis pour valider

  Scenario: Cookie Secure + httpOnly + SameSite
    Then tout refresh posé a les attributs Secure + httpOnly + SameSite=Lax
    And il n'est jamais lisible par JavaScript
```

#### FR-005 — Droit à l'effacement RGPD (User + Provider)
- **En tant que** User · **je veux** demander l'effacement de mon compte et données · **afin de** exercer mon droit RGPD Art. 17 (Invariant §10.6)
- **Préconditions** : pas de Mission en cours ; pas de dette paiement
- **Garanties post** : PII anonymisées ; traces géoloc supprimées ; missions archivées (obligation comptable 7 ans) ; compte `ERASED` sous 30 j ; email confirmation
- **Capacité Brief** : C1

```gherkin
Feature: Effacement RGPD

  # === @happy ===
  Scenario: Effacement complet
    Given un User sans Mission en cours ni dette paiement
    When il POST "/api/v1/me/erase" avec confirmation "DELETE"
    Then son compte passe en "ERASED_PENDING"
    And un job d'effacement est programmé sous 30 j
    And un email de confirmation est envoyé

  Scenario: Effacement exécuté (job)
    Given un User "ERASED_PENDING"
    When le job s'exécute
    Then son email est remplacé par "erased_<hash>@dep.local"
    And son password_hash est supprimé
    And ses sessions sont invalidées
    And ses traces géoloc sont supprimées
    And ses Missions sont archivées (anonymisées : user_id → NULL)
    And l'audit_log "USER_ERASED" est créé

  # === @negative ===
  Scenario: Mission en cours
    Given un User avec Mission "EN_ROUTE"
    When il demande l'effacement
    Then la réponse est 409 "MISSION_IN_PROGRESS"
    And l'effacement est différé jusqu'à la fin de la Mission

  Scenario: Dette paiement
    Given un User avec escrow "CAPTURED" mais Mission non libérée
    When il demande l'effacement
    Then la réponse est 409 "PAYMENT_IN_PROGRESS"

  Scenario: Effacement déjà demandé
    Given un User "ERASED_PENDING"
    When il demande l'effacement à nouveau
    Then la réponse est 409 "ERASURE_ALREADY_PENDING"

  # === @edge ===
  Scenario: Réinscription après effacement
    Given un User effacé (email "marie@example.eu")
    When un nouveau User s'inscrit avec "marie@example.eu"
    Then un nouveau compte est créé avec un nouvel ID
    And il n'y a aucun lien avec l'ancien compte (pas de chaînage)

  Scenario: Provider avec historique
    Given un Provider avec 50 Missions libérées
    When il demande l'effacement
    Then ses Missions sont archivées (anonymisées)
    And les factures émises sont conservées (obligation comptable 7 ans)
    And son stripe_account_id est conservé (obligation Stripe)

  # === @security ===
  Scenario: Audit log non-effaçable
    Then l'audit_log de l'User est conservé
    Et seul le hash de l'email y remplace l'email en clair

  Scenario: Window d'annulation 7 j
    Given un User "ERASED_PENDING"
    When il annule sous 7 j
    Then son compte est restauré en "ACTIVE"
    And l'email "ERASURE_CANCELLED" est envoyé
```

#### FR-006 — Gestion méthode paiement User (Stripe customer)
- **En tant que** User · **je veux** enregistrer / supprimer une carte de paiement · **afin de** accélérer mes futures Demandes
- **Préconditions** : User authentifié ; Stripe customer créé à la première Demande
- **Garanties post** : payment_method lié au customer Stripe ; jamais stocké côté Klaar (PCI SAQ-A)
- **Capacité Brief** : C1, C6

```gherkin
Feature: Méthode paiement

  # === @happy ===
  Scenario: Ajout carte
    Given un User sans méthode paiement
    When il ajoute une carte via Stripe Elements (iframe)
    Then la méthode est créée chez Stripe (pm_XXX)
    And seul le pm_XXX est stocké côté Klaar
    And la carte est marquée "default" si première

  Scenario: Suppression carte
    Given un User avec 2 cartes
    When il supprime la carte "default"
    Then la carte est détachée chez Stripe
    And l'autre carte devient "default" automatiquement

  # === @negative ===
  Scenario: Carte refusée par Stripe
    Given un User ajoutant une carte "declined"
    When Stripe refuse
    Then la réponse est 402 "CARD_DECLINED" avec raison

  Scenario: Stripe indisponible
    Given Stripe en panne
    When un User tente l'ajout
    Then la réponse est 503 "STRIPE_UNAVAILABLE"

  # === @edge ===
  Scenario: Carte expirée entre usage
    Given une carte "default" expirée ce mois
    When l'User tente une Demande
    Then la réponse est 402 "CARD_EXPIRED"
    And l'User est invité à mettre à jour

  Scenario: Maximum 5 cartes par User
    Given un User avec 5 cartes
    When il ajoute une 6e
    Then la réponse est 422 "MAX_CARDS_REACHED"

  # === @security ===
  Scenario: Scope PCI SAQ-A strict
    Then aucun PAN (numéro carte) ne transite par les serveurs Klaar
    And seul l'iframe Stripe Elements capture les données carte
    And les webhooks Stripe sont vérifiés (signature)
```

#### FR-007 — Verrouillage compte après N tentatives échouées
- **En tant que** système · **je veux** verrouiller un compte après 5 tentatives échouées · **afin de** mitiger le brute-force (CyFun Basic)
- **Préconditions** : authentification par email/mot de passe
- **Capacité Brief** : C1

```gherkin
Feature: Anti brute-force

  # === @happy ===
  Scenario: Verrouillage après 5 échecs
    Given un User existant
    When 5 logins échouent consécutifs en < 10 min
    Then le compte est verrouillé 15 min
    And un email "SECURITY_ALERT" est envoyé
    And un audit_log "ACCOUNT_LOCKED" est créé

  Scenario: Déverrouillage automatique
    Given un User verrouillé depuis 15 min
    When il tente un login avec password correct
    Then le login réussit
    And le compteur d'échecs est réinitialisé

  # === @negative ===
  Scenario: Login pendant verrouillage
    Given un User verrouillé
    When il tente un login (correct ou non)
    Then la réponse est 423 "ACCOUNT_LOCKED" avec Retry-After

  # === @edge ===
  Scenario: Tentative sur compte inexistant (anti-énumération)
    Given un email inexistant
    When un attaquant tente 5 mots de passe
    Then la même réponse 401 est retournée (pas de fuite d'info)
    And le rate-limit IP est incrémenté

  # === @security ===
  Scenario: Timing constant
    Then la durée de réponse à un login échoué est constante (±50 ms)
    Et aucune information ne fuit sur l'existence du compte
```

---

### Module : Catalog (CTL)

#### FR-008 — Catalogue Secteurs et Skills
- **En tant que** User · **je veux** consulter la liste des Secteurs et Skills couverts · **afin de** choisir la bonne catégorie pour ma Demande
- **Capacité Brief** : C2

```gherkin
Feature: Catalogue

  Background:
    Given le catalogue MVP contient 5 Secteurs (plomberie, serrurerie, électricité, auto, livraison)

  # === @happy ===
  Scenario: Liste complète trilingue
    When un User GET "/api/v1/catalog/sectors?locale=fr"
    Then la réponse contient les 5 Secteurs avec libellés FR
    And chaque Secteur liste ses Skills avec prix indicatifs

  Scenario: Switch NL
    When un User GET "/api/v1/catalog/sectors?locale=nl"
    Then la réponse contient les 5 Secteurs avec libellés NL

  # === @negative ===
  Scenario: Locale non supportée
    When un User GET "?locale=de"
    Then la réponse est 200 avec fallback FR
    And un warning "LOCALE_FALLBACK" est retourné

  # === @edge ===
  Scenario: Catalogue en maintenance
    Given une mise à jour catalogue en cours
    When un User GET le catalogue
    Then la réponse est 503 avec Retry-After: 60

  Scenario: Catalogue vide (init)
    Given aucune donnée catalogue
    When un User GET le catalogue
    Then la réponse est 200 avec liste vide
    And un warning "CATALOG_EMPTY" est loggé

  # === @security ===
  Scenario: Rate-limit lecture
    Given 60 req/min depuis une IP
    When une 61e arrive
    Then la réponse est 429

  Scenario: Cache CDN
    Then le catalogue est mis en cache CDN 5 min (Cache-Control)
    And l'ETag est posé pour revalidation
```

#### FR-009 — Prix indicatifs par secteur
- **En tant que** User · **je veux** voir un prix indicatif par Secteur · **afin de** estimer mon budget avant la Demande
- **Capacité Brief** : C2

```gherkin
Feature: Prix indicatifs

  # === @happy ===
  Scenario: Prix indicatif affiché
    When un User consulte le Secteur "plomberie"
    Then une fourchette "80-200 €" est affichée (basée sur historique)
    And un disclaimer "prix final défini par le Provider" est visible

  # === @negative ===
  Scenario: Pas d'historique (lancement)
    Given 0 Mission libérée sur le Secteur "serrurerie"
    When un User consulte ce Secteur
    Then aucune fourchette n'est affichée
    And un message "prix sur devis" est affiché

  # === @edge ===
  Scenario: Secteur avec 5 Missions
    Given 5 Missions libérées avec prix [80, 120, 150, 200, 1000] €
    When le prix indicatif est calculé
    Then la fourchette exclut les outliers (IQR)
    And la fourchette "80-200 €" est affichée (pas 80-1000)

  # === @security ===
  Scenario: Anonymisation agrégats
    Then les prix individuels ne sont jamais exposés
    And seules les fourchettes agrégées sont publiques
```

#### FR-010 — Administration catalogue (ops)
- **En tant que** ops admin · **je veux** ajouter / modifier un Secteur post-MVP · **afin de** faire évoluer l'offre
- **Capacité Brief** : C2

```gherkin
Feature: Admin catalogue

  # === @happy ===
  Scenario: Ajout Secteur (post-MVP)
    Given un ops admin authentifié
    When il crée le Secteur "chauffage" avec i18n FR/NL/EN
    Then le Secteur est créé en statut "DRAFT"
    And un ops senior doit l'approuver (4-eyes)

  # === @negative ===
  Scenario: Doublon code Secteur
    When un ops crée "plomberie" (existant)
    Then la réponse est 409

  # === @edge ===
  Scenario: Secteur en production avec Missions actives
    Given un Secteur avec 10 Missions en cours
    When un ops tente de le désactiver
    Then la réponse est 409 "SECTOR_HAS_ACTIVE_MISSIONS"

  # === @security ===
  Scenario: RBAC ops
    Then seul un ops avec rôle "catalog_manager" peut créer un Secteur
    Et 4-eyes principle requis pour publication
```

---

### Module : Matching & Dispatch (MCH)

#### FR-011 — Soumission d'une Demande
- **En tant que** User · **je veux** soumettre une Demande (Secteur + description + géoloc + urgence) · **afin de** déclencher le matching
- **Préconditions** : User authentifié ; méthode paiement valide ; géoloc dans RBC
- **Garanties post** : Demande créée en `BROADCASTING` ; matching déclenché
- **Capacité Brief** : C3

```gherkin
Feature: Soumission Demande

  Background:
    Given un User "ACTIVE" avec carte "default" valide

  # === @happy ===
  Scenario: Demande nominale
    When il POST "/api/v1/requests" avec secteur "plomberie", description, geo [50.83, 4.37], urgence "HIGH", photos []
    Then la Demande est créée en "BROADCASTING"
    And un job matching est déclenché en asynchrone
    And un audit_log "REQUEST_CREATED" est créé

  Scenario: Demande avec photos
    When il soumet une Demande avec 3 photos
    Then les photos sont uploadées (S3 KMS-encrypted)
    And elles sont attachées à la Demande

  # === @negative ===
  Scenario Outline: Validation champs
    When il soumet une Demande avec <champ> invalide
    Then la réponse est 400 avec code "<erreur>"
    Examples:
      | champ               | erreur                |
      | secteur inconnu     | SECTOR_NOT_FOUND      |
      | description vide    | DESCRIPTION_EMPTY     |
      | description > 2 000 | DESCRIPTION_TOO_LONG  |
      | urgence non enum    | URGENCY_INVALID       |
      | geo hors RBC        | GEO_OUTSIDE_RBC       |
      | méthode paiement absente | NO_PAYMENT_METHOD |

  Scenario: User sans carte
    Given un User sans méthode paiement
    When il soumet une Demande
    Then la réponse est 422 "PAYMENT_METHOD_REQUIRED"

  # === @edge ===
  Scenario: Doublon < 5 min
    Given une Demande créée il y a 2 min pour le même Secteur + geo
    When le même User soumet une Demande identique
    Then la réponse est 409 "DUPLICATE_REQUEST"
    Et la Demande existante est retournée

  Scenario: Rate-limit 5 Demandes/User/heure
    Given 5 Demandes créées par le User dans la dernière heure
    When une 6e arrive
    Then la réponse est 429 "RATE_LIMIT_EXCEEDED"

  Scenario: 0 Provider dispo dans 5 km
    Given aucun Provider disponible à < 5 km
    When le User soumet
    Then la Demande est créée en "BROADCASTING"
    And après 30 s sans match → "NO_MATCH"
    And une proposition d'élargir le rayon est affichée

  # === @security ===
  Scenario: Géoloc rough par défaut
    Then la précision par défaut envoyée au matching est ≤ 50 m
    Et la précision fine n'est envoyée au Provider qu'après acceptation

  Scenario: Photos scan antivirus
    Given une photo uploadée
    Then elle est scannée par ClamAV
    Et un fichier infecté est rejeté

  Scenario: Audit trace
    Then la Demande est journalisée dans audit_log
    Et l'IP + UA du User sont conservées (PII, 13 mois)
```

#### FR-012 — Matching géoloc multi-Provider
- **En tant que** système · **je veux** trouver les Providers disponibles à < 5 km avec le Skill requis · **afin de** notifier les candidats
- **Capacité Brief** : C3

```gherkin
Feature: Matching

  Background:
    Given une Demande en "BROADCASTING"
    And PostGIS opérationnel

  # === @happy ===
  Scenario: Matching nominal
    Given 5 Providers disponibles avec Skill "plomberie" à < 5 km
    When le moteur s'exécute
    Then les 5 Providers sont notifiés par push < 30 s
    And un score est calculé pour chacun (distance × rating × KYC date)
    And la Trace contient : score final, critères, providers notifiés

  Scenario: Score transparent
    Given 2 Providers A (proche, rating 4.2) et B (loin, rating 4.9)
    When le score est calculé
    Then A et B reçoivent un score documenté
    And le score n'est pas biaisé par un attribut protégé (AI Act)

  # === @negative ===
  Scenario: Aucun Provider disponible
    Given 0 Provider avec Skill requis à < 5 km
    When le moteur s'exécute
    Then la Demande passe en "NO_MATCH"
    Et une notification "élargir le rayon" est proposée au User

  Scenario: Provider dispo mais KYC suspendu
    Given 3 Providers disponibles mais 2 en "SUSPENSION_7J"
    When le moteur s'exécute
    Then seuls les Providers "APPROVED" sont notifiés

  # === @edge ===
  Scenario: > 50 Providers candidats
    Given 60 Providers disponibles à < 5 km
    When le moteur s'exécute
    Then seuls les top-10 par score sont notifiés
    Et la Trace documente pourquoi les 50 autres ne l'ont pas été

  Scenario: Providers exactement à 5 km (boundary)
    Given un Provider exactement à 5000 m
    Then il est inclus dans le matching (≤)

  Scenario: Provider à 5001 m
    Given un Provider à 5001 m
    Then il est exclu du premier round
    Et il est candidat au second round (rayon élargi)

  Scenario: Doublon Skill (multi-Secteur)
    Given un Provider avec Skills ["plomberie", "chauffage"]
    When une Demande "plomberie" est soumise
    Then il est notifié uniquement sur "plomberie"

  # === @security ===
  Scenario: Trace immuable
    Then chaque exécution du matching génère une Trace
    Et la Trace est signée (HMAC) et stockée WORM
    Et elle est consultable par l'ops admin et l'APD sur demande

  Scenario: Audit biais semestriel
    Given l'historique des matchings sur 6 mois
    When l'audit s'exécute
    Then le rapport vérifie l'absence de biais (genre, ethnie estimée, quartier)
    Et le rapport est envoyé à l'APD si biais détecté

  Scenario: Provider opt-out matching
    Given un Provider en "PAUSE" (manuelle)
    Then il n'est jamais notifié
    Et son rating n'est pas impacté
```

#### FR-013 — Acceptation Provider (1er répondant)
- **En tant que** Provider notifié · **je veux** accepter la Demande · **afin de** devenir le Provider attribué
- **Capacité Brief** : C3

```gherkin
Feature: Acceptation Provider

  Background:
    Given une Demande en "BROADCASTING" avec 5 Providers notifiés

  # === @happy ===
  Scenario: 1er accept gagne
    Given Provider A et Provider B notifiés
    When A accepte en premier (atomic CAS)
    Then A est attribué, la Demande passe "MATCHED"
    Et une Mission est créée
    Et les 4 autres reçoivent "MATCH_TAKEN"

  # === @negative ===
  Scenario: Provider non éligible
    Given Provider C avec Skill "plomberie" mais en "SUSPENSION_7J"
    When C tente d'accepter
    Then la réponse est 403 "PROVIDER_NOT_ELIGIBLE"

  Scenario: Demande déjà matchée
    Given une Demande déjà en "MATCHED"
    When un Provider tardif tente d'accepter
    Then la réponse est 409 "REQUEST_ALREADY_MATCHED"

  # === @edge ===
  Scenario: Race condition (2 simultanés)
    Given 2 Providers A et B tentant d'accepter simultanément
    When les requêtes arrivent en parallèle
    Then 1 seul gagne (Postgres atomic CAS sur request.status)
    Et l'autre reçoit 409

  Scenario: Provider déjà sur Mission
    Given Provider A avec 1 Mission "EN_ROUTE" en cours
    When A tente d'accepter une 2e Demande
    Then la réponse est 409 "PROVIDER_BUSY" (MVP : 1 mission à la fois)

  Scenario: Demande expirée
    Given une Demande "BROADCASTING" depuis > 5 min
    When un Provider tente d'accepter
    Then la réponse est 410 "REQUEST_EXPIRED"

  # === @security ===
  Scenario: Vérification KYC au moment de l'accept
    Given un Provider dont le KYC a été suspendu après notification
    When il tente d'accepter
    Then la vérification s'exécute au moment de l'accept (pas au matching)
    Et la réponse est 403

  Scenario: Rate-limit accept
    Given un Provider spammer 100 accepts/s
    When le rate-limit s'applique
    Then seules 5 req/s sont autorisées par Provider
```

#### FR-014 — Annulation Demande par User (avant matching)
- **En tant que** User · **je veux** annuler ma Demande avant qu'un Provider l'accepte · **afin de** ne pas être facturé
- **Capacité Brief** : C3

```gherkin
Feature: Annulation Demande

  # === @happy ===
  Scenario: Annulation avant matching
    Given une Demande "BROADCASTING"
    When le User l'annule
    Then la Demande passe "CANCELLED_USER"
    Et les Providers notifiés reçoivent "CANCELLED"
    Et aucun paiement n'est capturé

  # === @negative ===
  Scenario: Annulation post-matching
    Given une Demande "MATCHED"
    When le User tente d'annuler
    Then la réponse est 409 "ALREADY_MATCHED"
    Et il doit utiliser FR-023 (annulation Mission)

  Scenario: Annulation par un autre User
    Given une Demande appartenant à User A
    When User B tente d'annuler
    Then la réponse est 403 "FORBIDDEN"

  # === @edge ===
  Scenario: Annulation en course
    Given 2 requêtes : une annulation et un accept Provider simultanés
    When elles arrivent en parallèle
    Then soit l'annulation gagne (Demande → CANCELLED, Provider → 410)
    Soit l'accept gagne (Demande → MATCHED, User doit utiliser FR-023)

  # === @security ===
  Scenario: Audit annulation
    Then toute annulation est journalisée avec motif optionnel
    Et le motif est stocké pour analytics
```

#### FR-015 — Timeout matching (NO_MATCH)
- **En tant que** système · **je veux** annoncer NO_MATCH après 30 s sans accept · **afin de** garder l'User informé
- **Capacité Brief** : C3

```gherkin
Feature: Timeout matching

  # === @happy ===
  Scenario: Timeout nominal
    Given une Demande "BROADCASTING" sans accept depuis 30 s
    When le timeout se déclenche
    Then la Demande passe "NO_MATCH"
    Et le User est notifié avec options : élargir rayon / annuler

  Scenario: Proposition élargir rayon
    Given une Demande "NO_MATCH"
    When le User choisit "élargir à 10 km"
    Then la Demande repasse "BROADCASTING" avec rayon 10 km
    Et le timeout est réinitialisé à 30 s

  # === @edge ===
  Scenario: Timeout + accept tardif
    Given une Demande en timeout (30 s écoulées)
    When un Provider accepte à 31 s
    Then l'accept est rejeté (410 REQUEST_EXPIRED)
    Et la Demande reste "NO_MATCH" ou "CANCELLED"

  # === @security ===
  Scenario: Limite élargissements
    Given un User qui a déjà élargi 3 fois
    When il tente un 4e élargissement
    Then la réponse est 422 "MAX_RADIUS_REACHED"
    Et la Demande est auto-annulée
```

---

### Module : Intervention (INT)

#### FR-016 — Envoi de Devis
- **En tant que** Provider attribué · **je veux** envoyer un Devis (prix + délai + note) · **afin de** contractualiser (Invariant §10.2)
- **Capacité Brief** : C4

```gherkin
Feature: Envoi Devis

  Background:
    Given une Mission "MATCHED" avec Provider A attribué

  # === @happy ===
  Scenario: Devis nominal
    When Provider A POST devis montant 180 € HTVA, délai 45 min, note "remplacement joint"
    Then le Devis est créé avec TVA 21 % → 217.80 € TTC
    Et l'Escrow est pré-autorisé (capture) pour 217.80 €
    Et le User reçoit une notification "DEVIS_RECEIVED"
    Et le Devis expire dans 1 h

  # === @negative ===
  Scenario Outline: Montant invalide
    When Provider A POST devis montant <montant> €
    Then la réponse est 400 avec code "<erreur>"
    Examples:
      | montant | erreur              |
      | 0       | AMOUNT_ZERO         |
      | -10     | AMOUNT_NEGATIVE     |
      | 100000  | AMOUNT_TOO_HIGH     |

  Scenario: Délai > 24 h
    When Provider A POST devis délai "25 h"
    Then la réponse est 422 "DELAY_TOO_LONG"

  Scenario: Provider non attribué
    Given Provider B (non attribué à cette Mission)
    When B tente d'envoyer un devis
    Then la réponse est 403 "NOT_ASSIGNED"

  # === @edge ===
  Scenario: Devis consécutifs (3 max)
    Given 3 Devis déjà refusés par le User
    When Provider A envoie un 4e
    Then la réponse est 422 "MAX_QUOTES_REACHED"
    Et la Mission est annulée (User doit relancer)

  Scenario: User ne répond pas sous 1 h
    Given un Devis créé il y a 1 h 01
    When le timeout s'exécute
    Then le Devis expire
    Et la Mission retourne en "MATCHED" pour un nouveau Devis (ou remise en broadcast après 2e échec)

  Scenario: Devis avec TVA réduite 6 %
    Given une Demande sur logement ≥ 5 ans avec preuve
    When Provider A émet un devis à TVA 6 %
    Then la TVA est calculée à 6 %
    Et la preuve est jointe au Devis (audit fiscal)

  # === @security ===
  Scenario: Prix libre par Provider
    Then le système n'impose JAMAIS le prix
    Et le Provider peut proposer n'importe quel montant dans les bornes légales
    Et un audit confirme le respect de l'Invariant §10.2 (mitigation Platform Work)

  Scenario: Audit price setting
    Then chaque Devis est journalisé avec timestamp + Provider ID + montant
    Et l'absence d'algorithme de fixation de prix est auditable
```

#### FR-017 — Acceptation Devis par User + Escrow
- **En tant que** User · **je veux** accepter le Devis · **afin de** déclencher la Mission et capturer l'Escrow
- **Capacité Brief** : C4, C6

```gherkin
Feature: Acceptation Devis

  Background:
    Given une Mission "MATCHED" avec Devis en attente

  # === @happy ===
  Scenario: Acceptation nominale
    When le User accepte le Devis
    Then Stripe capture l'Escrow (3DS2 si requis)
    Et la Mission passe "ACCEPTED"
    Et le Provider est notifié pour démarrer

  Scenario: Refus Devis par User
    When le User refuse le Devis avec motif "trop cher"
    Then le Devis est marqué "REFUSED_USER"
    Et le Provider peut émettre un nouveau Devis (jusqu'à 3)

  # === @negative ===
  Scenario: 3DS2 échec
    Given un User avec carte nécessitant 3DS2
    When le 3DS2 échoue
    Then la Mission reste en attente
    Et le User a 3 tentatives
    Et après 3 échecs la Mission est annulée

  Scenario: Fonds insuffisants
    When Stripe retourne "insufficient_funds"
    Then la réponse est 402 "INSUFFICIENT_FUNDS"
    Et l'User doit mettre à jour sa carte

  # === @edge ===
  Scenario: Devis expiré pendant accept
    Given un Devis expirant dans 5 s
    When le User accepte à T+10 s
    Then la réponse est 410 "QUOTE_EXPIRED"
    Et l'Escrow n'est pas capturé

  Scenario: Carte expirée entre-temps
    Given une carte "default" valide au Devis
    When la carte expire entre-temps
    Then la capture échoue avec 402 "CARD_EXPIRED"

  # === @security ===
  Scenario: PCI SAQ-A
    Then la capture se fait via Stripe Payment Intent (server-side)
    Et le PAN ne transite jamais par Klaar

  Scenario: Idempotency key
    Given une acceptation avec idempotency key "K1"
    When une 2e requête avec "K1" arrive
    Then la 2e est ignorée (retourne le résultat de la 1e)
```

#### FR-018 — Cycle de vie d'une Mission (machine à états)
- **En tant que** Provider · **je veux** faire évoluer le statut de la Mission · **afin de** tracer l'Intervention
- **Capacité Brief** : C5

```gherkin
Feature: Cycle Mission

  Background:
    Given une Mission "ACCEPTED"

  # === @happy ===
  Scenario Outline: Transitions valides
    Given une Mission en statut "<from>"
    When le Provider change à "<to>"
    Then la transition réussit
    Et un mission_status est journalisé avec timestamp + geo
    Examples:
      | from      | to                  |
      | ACCEPTED  | PROVIDER_EN_ROUTE   |
      | PROVIDER_EN_ROUTE | ON_SITE     |
      | ON_SITE   | COMPLETED           |

  Scenario: Notification User à chaque transition
    When une transition réussit
    Then le User reçoit une notification push
    Et la Mission est mise à jour sur le WebSocket

  # === @negative ===
  Scenario Outline: Transitions interdites
    Given une Mission en "<from>"
    When le Provider tente "<to>"
    Then la réponse est 409 "INVALID_TRANSITION"
    Examples:
      | from      | to                |
      | COMPLETED | PROVIDER_EN_ROUTE |
      | ON_SITE   | ACCEPTED          |
      | CANCELLED | ON_SITE           |

  Scenario: Provider non attribué
    Given un Provider B non attribué
    When B tente de changer le statut
    Then la réponse est 403

  # === @edge ===
  Scenario: Provider offline (Tauri)
    Given un Provider perd connectivité pendant transition
    When il revient online
    Then les transitions sont synchronisées
    Et les timestamps client sont validés (anti-falsification, ±5 min max)

  Scenario: Provider sort de la zone RBC en EN_ROUTE
    Given une Mission "EN_ROUTE" avec Provider à l'extérieur RBC
    When le système détecte la sortie
    Then le statut geo passe "OUT_OF_ZONE"
    Et une alerte ops est levée
    Et last known position est conservée 5 min

  # === @security ===
  Scenario: Trace horodatée
    Then chaque transition génère une entrée dans mission_statuses
    Et l'entrée contient : status, ts (UTC), geo, provider_id
    Et l'entrée est immuable (append-only)
```

#### FR-019 — Tracking géoloc pendant la Mission
- **En tant que** User · **je veux** voir la position temps réel du Provider pendant la Mission · **afin de** savoir quand il arrive (Invariant §10.5)
- **Capacité Brief** : C5

```gherkin
Feature: Tracking géoloc

  Background:
    Given une Mission "PROVIDER_EN_ROUTE"
    And le Provider a consenti au partage géoloc pour cette Mission

  # === @happy ===
  # Baseline MVP : tracking foreground (app au premier plan pendant EN_ROUTE),
  # garanti sur Tauri ET fallback PWA. Background continu = enhancement
  # conditionnel au succès du PoC plugin Tauri Mobile (H-2, Story 0.12).
  Scenario: Diffusion continue (foreground MVP)
    When le Provider déplace, app au premier plan (foreground)
    Then sa position est envoyée au backend toutes les 10 s
    Et le User la reçoit via WebSocket < 2 s
    Et la précision est ≤ 50 m

  Scenario: Diffusion en arrière-plan (enhancement conditionnel PoC Tauri)
    Given le PoC plugin Tauri Mobile géoloc background a réussi (H-2)
    When le Provider déplace, app en arrière-plan
    Then sa position continue d'être envoyée toutes les 10 s
    Et à défaut (PoC échoué), le fallback PWA foreground s'applique

  Scenario: Arrêt à ON_SITE
    When la Mission passe "ON_SITE"
    Then le tracking s'arrête immédiatement
    Et aucune position n'est plus diffusée

  # === @negative ===
  Scenario: Provider refuse partage
    Given un Provider refusant le partage
    When la Mission démarre
    Then le User voit "position non partagée"
    Et un délai estimé textuel est affiché

  Scenario: WebSocket déconnecté
    Given le WebSocket du User coupé
    When le Provider envoie position
    Then la position est persistée
    Et le User reçoit la dernière position au reconnect

  # === @edge ===
  Scenario: Provider perd GPS
    Given un Provider en zone sans GPS
    When la position est perdue > 30 s
    Then le statut passe "POSITION_LOST"
    Et last known position reste affichée

  Scenario: Provider sort zone RBC
    When le Provider sort de la RBC pendant EN_ROUTE
    Then le statut geo passe "OUT_OF_ZONE"
    Et une alerte ops est levée

  # === @security ===
  Scenario: Consentement explicite par Mission
    Then le Provider doit consentir AVANT chaque Mission
    Et le consentement est horodaté dans audit_log
    Et le tracking hors Mission est techniquement impossible (Invariant §10.5)

  Scenario: DPIA Art. 35
    Then la collecte est documentée dans la DPIA
    Et la base légale est "exécution contractuelle" (Art. 6.1.b)
    Et la minimisation est appliquée (précision 50 m, pas 5 m)

  Scenario: Purge post-Mission
    When la Mission passe "COMPLETED"
    Then les positions géoloc sont supprimées sous 24 h
    Et seul le trajet agrégé (distance + durée) est conservé pour analytics

  Scenario: Droit d'accès RGPD
    Given un User ou Provider
    When il demande son données géoloc
    Then il reçoit l'export complet sous 30 j
```

#### FR-020 — Preuves photos horodatées
- **En tant que** Provider · **je veux** prendre des photos avant/après intervention · **afin de** documenter l'état
- **Capacité Brief** : C5

```gherkin
Feature: Preuves photos

  # === @happy ===
  Scenario: Photo avant intervention
    Given une Mission "ON_SITE"
    When le Provider prend une photo "BEFORE"
    Then la photo est uploadée avec EXIF horodatage + géoloc + hash SHA-256
    Et elle est chiffrée (KMS OVH) et stockée S3
    Et elle est visible par User + Provider + Ops

  Scenario: Photo après intervention
    When le Provider prend une photo "AFTER"
    Then la paire BEFORE/AFTER est jointe à la Mission
    Et ces preuves sont utilisées en cas de Litige

  # === @negative ===
  Scenario: EXIF manquant
    Given une photo sans EXIF
    When le Provider tente l'upload
    Then la réponse est 422 "EXIF_REQUIRED"

  Scenario: Photo > 10 Mo
    Given une photo de 12 Mo
    When le Provider tente l'upload
    Then la réponse est 413 "FILE_TOO_LARGE"

  Scenario: Type non image
    Given un PDF au lieu d'une photo
    When le Provider tente l'upload
    Then la réponse est 422 "INVALID_FILE_TYPE"

  # === @edge ===
  Scenario: Quota 5 photos par phase
    Given 5 photos "BEFORE" déjà uploadées
    When le Provider tente une 6e
    Then la réponse est 422 "MAX_EVIDENCE_REACHED"

  Scenario: Pas de caméra (Tauri)
    Given un device sans caméra ou permission refusée
    When le Provider tente une photo
    Then la réponse est 422 "CAMERA_UNAVAILABLE"
    Et une description textuelle alternative est demandée

  # === @security ===
  Scenario: Scan antivirus
    Then chaque photo est scannée ClamAV avant stockage
    Et un fichier infecté est rejeté

  Scenario: Chiffrement at-rest
    Then la photo est chiffrée AES-256-GCM avec clé KMS OVH
    Et la clé est rotée annuellement

  Scenario: Accès restreint
    Then seuls User + Provider + Ops peuvent voir la photo
    Et chaque accès est journalisé
```

#### FR-021 — Validation fin de Mission + libération Escrow
- **En tant que** User · **je veux** valider la fin de Mission · **afin de** libérer l'Escrow au Provider (Invariant §10.4)
- **Capacité Brief** : C5, C6

```gherkin
Feature: Validation fin Mission

  Background:
    Given une Mission "COMPLETED" par Provider avec Escrow "CAPTURED"

  # === @happy ===
  Scenario: Validation manuelle
    When le User valide dans les 72 h
    Then l'Escrow est libéré
    Et le Take (18 %) est calculé
    Et un Payout est programmé J+2
    Et un email est envoyé au Provider

  Scenario: Validation auto 72 h
    Given une Mission "COMPLETED" depuis 72 h sans action User
    When le job s'exécute
    Then l'Escrow est libéré automatiquement
    Et un audit_log "AUTO_RELEASE_72H" est créé

  # === @negative ===
  Scenario: User conteste (ouvre Litige)
    Given une Mission "COMPLETED"
    When le User ouvre un Litige
    Then l'Escrow passe "FROZEN_DISPUTE"
    Et le workflow TRU démarre
    Et le Payout est suspendu

  Scenario: Mission déjà validée
    Given une Mission déjà "RELEASED"
    When le User tente de valider à nouveau
    Then la réponse est 409 "ALREADY_RELEASED"

  # === @edge ===
  Scenario: User injoignable > 72 h
    Given une Mission "COMPLETED" sans action User > 72 h
    Then la libération est automatique
    Et un email récapitulatif est envoyé (même si non lu)
    Et le User peut toujours ouvrir un Litige dans les 14 j

  Scenario: Montant > 500 €
    Given une Mission avec montant > 500 €
    When l'Escrow est libéré
    Then un double-signature ops est requis (4-eyes pour gros montants)
    Et le Payout est différé jusqu'à validation ops

  # === @security ===
  Scenario: Transaction atomique
    Then la libération Escrow + calcul Take + programmation Payout est atomique (transaction SQL)
    Et en cas d'échec, tout est rollback

  Scenario: Audit libération
    Then chaque libération génère un audit_log avec montant, take, payout_id
    Et l'audit_log est conservé 10 ans (comptable + fiscal)
```

#### FR-022 — Annulation Mission par Provider/User avec pénalités
- **En tant que** User ou Provider · **je veux** annuler une Mission en cours · **afin de** sortir d'un engagement impossible
- **Capacité Brief** : C5

```gherkin
Feature: Annulation Mission

  Background:
    Given une Mission "ACCEPTED" ou "EN_ROUTE"

  # === @happy ===
  Scenario: Annulation User avant EN_ROUTE
    When le User annule la Mission "ACCEPTED"
    Then la Mission passe "CANCELLED_USER"
    Et l'Escrow est entièrement remboursé
    Et le Provider est notifié

  Scenario: Annulation Provider avant EN_ROUTE
    When le Provider annule "ACCEPTED"
    Then la Mission passe "CANCELLED_PROVIDER"
    Et l'Escrow est entièrement remboursé
    Et le Provider reçoit un penalty (-1 rating, compteur)

  # === @negative ===
  Scenario: Annulation en ON_SITE
    Given une Mission "ON_SITE" (Provider sur place)
    When le User annule
    Then la Mission passe "CANCELLED_USER"
    Et un forfait de déplacement (30 €) est prélevé sur l'Escrow
    Et le reste est remboursé

  Scenario: Annulation après COMPLETED
    Given une Mission "COMPLETED"
    When le User tente d'annuler
    Then la réponse est 409 "MISSION_COMPLETED"
    Et il doit ouvrir un Litige (FR-022 TRU)

  # === @edge ===
  Scenario: Provider annule 3× en 30 j
    Given un Provider avec 2 annulations en 30 j
    When il annule une 3e
    Then un warning automatique est émis
    Et après 3 = SUSPENSION_7J automatique

  Scenario: User annule 5× en 7 j (fraude)
    Given un User avec 5 annulations
    When il tente une nouvelle Demande
    Then la réponse est 422 "USER_FRAUD_FLAG"
    Et un review ops est créé

  # === @security ===
  Scenario: Audit annulation
    Then chaque annulation est journalisée avec motif, timestamps, pénalité
    Et le motif est stocké pour analytics
```

#### FR-023 — Re-programmation Mission
- **En tant que** User · **je veux** re-programmer une Mission annulée · **afin de** ne pas perdre le bénéfice du devis accepté
- **Capacité Brief** : C5

```gherkin
Feature: Re-programmation

  # === @happy ===
  Scenario: Re-programmation par accord commun
    Given une Mission "CANCELLED_PROVIDER" avec User et Provider d'accord
    When le User demande re-programmation
    Then une nouvelle Mission est créée reprenant le Devis initial
    Et les 2 parties doivent valider

  # === @negative ===
  Scenario: Provider a refusé
    Given un Provider qui a refusé la re-programmation
    When le User tente
    Then la réponse est 409 "PROVIDER_DECLINED"

  # === @edge ===
  Scenario: Re-programmation après 7 j
    Given une Mission annulée depuis > 7 j
    When le User tente
    Then la réponse est 410 "RESCHEDULE_EXPIRED"
```

---

### Module : Payment (PAY)

#### FR-024 — Configuration Stripe Connect Provider
- **En tant que** Provider · **je veux** configurer mon compte Stripe Connect (Onboarding Stripe) · **afin de** recevoir mes Payouts
- **Capacité Brief** : C6

```gherkin
Feature: Stripe Connect Onboarding

  Background:
    Given un Provider "APPROVED"

  # === @happy ===
  Scenario: Onboarding Standard complet
    When le Provider clique "Configurer paiements"
    Then il est redirigé vers Stripe Connect Onboarding (Standard account)
    When il complète KYC Stripe
    Then son stripe_account_id est lié à son profil Provider
    Et un test micro-paiement vérifie l'IBAN

  # === @negative ===
  Scenario: Stripe refuse KYC
    Given un Stripe KYC échec
    When le Provider complète
    Then la réponse est 422 "STRIPE_KYC_FAILED"
    Et un ops est notifié pour assistance

  Scenario: IBAN hors BE/EU
    Given un IBAN marocain
    When Stripe le refuse
    Then la réponse est 422 "IBAN_NOT_SUPPORTED"

  # === @edge ===
  Scenario: Provider existe avec account_id
    Given un Provider avec stripe_account_id déjà lié
    When il tente un nouvel onboarding
    Then la réponse est 409 "STRIPE_ALREADY_LINKED"
    Et un lien "reprendre" est proposé

  Scenario: Stripe Onboarding interrompu
    Given un Provider ayant commencé mais pas fini
    When il revient plus tard
    Then il reprend là où il s'est arrêté (Stripe supporte)

  # === @security ===
  Scenario: Abstraction Payment adapter
    Then le code n'appelle jamais Stripe directement depuis le Domain
    Et il passe par un trait `PaymentGateway` (hexagonale)
    Et un adapter Mollie peut substituer Stripe (mitigation H-6)

  Scenario: Clés Stripe en vault
    Then les clés Stripe (secret) sont en HashiCorp Vault / OVH KMS
    Et jamais dans le code ou les env files versionnés
```

#### FR-025 — Calcul Take-rate et libération Payout
- **En tant que** système · **je veux** calculer le Take (18 %) et verser le Payout net au Provider · **afin de** rémunérer plateforme + Provider
- **Capacité Brief** : C6

```gherkin
Feature: Take + Payout

  Background:
    Given une Mission "RELEASED" avec Escrow 217.80 € TTC

  # === @happy ===
  Scenario: Calcul Take + Payout
    When le calcul s'exécute
    Then Take 18 % sur montant HTVA (180 €) = 32.40 € HTVA
    Et TVA 21 % sur Take = 6.80 € → Take TTC 39.20 €
    Et Payout Provider = 217.80 − 39.20 = 178.60 €
    Et un transfer Stripe Connect est créé

  Scenario: Payout différé J+2
    When le transfer Stripe est créé
    Then il est programmé pour J+2 ouvré
    Et un email récapitulatif est envoyé au Provider

  # === @negative ===
  Scenario: Stripe transfer échoue
    Given Stripe indisponible
    When le transfer échoue
    Then 3 retry automatiques (backoff exponentiel)
    Et après 3 échecs, une alerte ops + mise en file manuelle

  Scenario: IBAN Provider clos
    Given un IBAN Provider clos chez Stripe
    When le transfer échoue
    Then la réponse est 422 "IBAN_CLOSED"
    Et le Provider est notifié pour mettre à jour

  # === @edge ===
  Scenario: Remboursement partiel après Litige
    Given un Litige résolu "PARTIAL_REFUND" 50 %
    When l'ajustement s'exécute
    Then 50 % est remboursé au User
    Et 50 % du Take est reversé au Provider
    Et une credit note est générée

  Scenario: Provider BAN en cours de Payout
    Given un Provider BAN pendant que son Payout est en attente
    When le système détecte le BAN
    Then le Payout est gelé
    Et un ops review est créé (argent potentiellement à rembourser users)

  # === @security ===
  Scenario: Idempotency Stripe
    Given un transfer Stripe avec idempotency key "T1"
    When une 2e requête avec "T1" arrive
    Then la 2e est ignorée

  Scenario: Réconciliation quotidienne
    Then chaque jour, un job compare les transfers Klaar ↔ Stripe
    Et tout écart génère une alerte ops
```

#### FR-026 — Génération factures TVA BE
- **En tant que** Provider · **je veux** recevoir une facture auto par Mission · **afin de** tenir ma comptabilité TVA BE
- **Capacité Brief** : C6

```gherkin
Feature: Factures TVA

  # === @happy ===
  Scenario: Facture conforme BE
    Given un Payout effectué
    When la facture est générée
    Then le PDF contient : numéro facture séquentiel, date, émetteur Klaar, destinataire Provider, désignation, montant HTVA, TVA 21 %, TTC, mentions légales BE
    Et le PDF est signé électroniquement (eIDAS)
    Et il est envoyé par email + archivé 7 ans

  # === @negative ===
  Scenario: Taux TVA incorrect
    Given une config TVA erronée
    When la facture est générée
    Then une alerte bloque l'envoi
    Et ops review

  # === @edge ===
  Scenario: TVA réduite 6 % rénovation
    Given une Mission sur logement ≥ 5 ans avec preuve
    When la facture est générée
    Then TVA 6 % est appliquée
    Et la preuve est archivée avec la facture (audit fiscal)

  Scenario: Facture rectificative (credit note)
    Given un remboursement partiel
    When la credit note est générée
    Then elle référence la facture initiale
    Et elle est signée et archivée

  # === @security ===
  Scenario: Signature eIDAS
    Then la facture est signée avec une clé eIDAS qualifiée
    Et la signature est horodatée (TSA)
    Et la révocation de clé est gérée

  Scenario: Archivage WORM 7 ans
    Then les factures sont stockées en S3 Object Lock (WORM)
    Et la rétention est de 7 ans minimum (loi comptable BE)
```

#### FR-027 — Remboursement total / partiel
- **En tant que** ops admin · **je veux** rembourser un User (total ou partiel) · **afin de** résoudre un Litige ou une erreur
- **Capacité Brief** : C6, C8

```gherkin
Feature: Remboursement

  # === @happy ===
  Scenario: Remboursement total
    Given un Litige résolu "FULL_REFUND"
    When l'ops exécute
    Then l'Escrow est entièrement remboursé au User
    Et un credit note est émis au Provider
    Et un audit_log "REFUND_FULL" est créé

  Scenario: Remboursement partiel
    Given un Litige résolu "PARTIAL_REFUND 30 %"
    When l'ops exécute
    Then 30 % est remboursé au User
    Et 70 % est versé au Provider (net de Take proportionnel)

  # === @negative ===
  Scenario: Remboursement > Escrow
    When l'ops tente un remboursement > montant Escrow
    Then la réponse est 422 "REFUND_EXCEEDS_ESCROW"

  # === @edge ===
  Scenario: Remboursement après Payout exécuté
    Given un Payout déjà versé au Provider
    When l'ops tente un remboursement
    Then la réponse est 422 "PAYOUT_EXECUTED"
    Et l'ops doit négocier un reversement manuel avec le Provider

  # === @security ===
  Scenario: 4-eyes principle
    Then tout remboursement > 100 € nécessite validation par 2 ops
    Et un seul ops ne peut pas valider
```

#### FR-028 — Webhooks Stripe (signature, retry, idempotence)
- **En tant que** système · **je veux** traiter les webhooks Stripe de façon idempotente · **afin de** garantir la cohérence
- **Capacité Brief** : C6

```gherkin
Feature: Webhooks Stripe

  # === @happy ===
  Scenario: Webhook payment_intent.succeeded
    Given un webhook Stripe signé
    When il arrive
    Then la signature Stripe est vérifiée
    Et l'événement est stocké dans stripe_events (idempotence par event_id)
    Et l'Escrow correspondant est marqué "CAPTURED"

  # === @negative ===
  Scenario: Signature invalide
    Given un webhook avec signature invalide
    When il arrive
    Then la réponse est 400 "INVALID_SIGNATURE"
    Et l'événement est rejeté

  Scenario: Webhook dupliqué
    Given un webhook déjà traité (event_id existant)
    When il arrive à nouveau
    Then la réponse est 200 OK
    Et aucune action n'est rejouée (idempotence)

  # === @edge ===
  Scenario: Webhook retardé > 1 h
    Given un webhook Stripe envoyé il y a 2 h
    When il arrive
    Then il est traité normalement
    Et le système se synchronise

  Scenario: Ordre des webhooks inversé
    Given 2 webhooks (A capturé, B remboursé) envoyés dans le désordre
    When ils arrivent B avant A
    Then le système réordonne par timestamp Stripe
    Et l'état final est cohérent

  # === @security ===
  Scenario: Endpoint webhook public
    Then l'endpoint "/api/v1/webhooks/stripe" est public (pas d'auth)
    Mais la signature Stripe (HMAC SHA-256) est vérifiée obligatoirement
    Et le secret endpoint est en vault, roté trimestriellement

  Scenario: Rate-limit webhook
    Given un attaquant spam le endpoint
    When il dépasse 100 req/min
    Then la réponse est 429
```

#### FR-029 — Réconciliation quotidienne ops
- **En tant que** ops admin · **je veux** un rapport quotidien de réconciliation Klaar ↔ Stripe · **afin de** détecter les écarts
- **Capacité Brief** : C6, C9

```gherkin
Feature: Réconciliation

  # === @happy ===
  Scenario: Rapport vert
    Given une journée sans écart
    When le job s'exécute à 03h00
    Then un rapport "RECONCILIATION_OK" est généré
    Et il est archivé + envoyé à ops

  # === @negative ===
  Scenario: Écart détecté
    Given 1 transfer Klaar absent de Stripe
    When le job s'exécute
    Then une alerte "RECONCILIATION_MISMATCH" est levée
    Et un ticket ops est créé avec détails

  # === @edge ===
  Scenario: Stripe indisponible pendant réconciliation
    Given Stripe en panne à 03h00
    When le job échoue
    Then il retente à 04h00, 05h00
    Et après 3 échecs, alerte ops
```

---

### Module : Messaging (MSG)

#### FR-030 — Conversation in-app User ↔ Provider
- **Capacité Brief** : C7

```gherkin
Feature: Conversation

  Background:
    Given une Mission "ACCEPTED" avec conversation créée

  # === @happy ===
  Scenario: Message nominal
    When le User envoie "Bonjour, où êtes-vous ?"
    Then le message est persisté
    Et le Provider reçoit push + WebSocket < 2 s

  # === @negative ===
  Scenario: Message > 4 000 chars
    When le User envoie un message de 5 000 chars
    Then la réponse est 422 "MESSAGE_TOO_LONG"

  Scenario: Mission close > 7 j
    Given une Mission "RELEASED" depuis > 7 j
    When le User envoie un message
    Then la réponse est 410 "CONVERSATION_CLOSED"

  # === @edge ===
  Scenario: User ou Provider offline
    Given le destinataire offline
    When un message est envoyé
    Then il est persisté
    Et livré au reconnect

  Scenario: 100 messages / conversation
    Given une conversation avec 99 messages
    When le 100e arrive
    Then la conversation passe en read-only (limite MVP)

  # === @security ===
  Scenario: Anti-circumvention (pas de phone/email)
    Given un message avec " appelez-moi au 04XX"
    When le message est analysé
    Then il est bloqué avec warning "CONTACT_INFO_FORBIDDEN"
    Et après 3 tentatives, le User est flaggé

  Scenario: Scan malware pièces jointes
    Then toute pièce jointe est scannée ClamAV
```

#### FR-031 — Envoi de photos dans la conversation
- **Capacité Brief** : C7

```gherkin
Feature: Photos conversation

  # === @happy ===
  Scenario: Photo nominale
    When le User envoie une photo 3 Mo
    Then elle est uploadée (S3 KMS-encrypted)
    Et affichée dans la conversation

  # === @negative ===
  Scenario: Photo > 5 Mo
    Then la réponse est 413

  Scenario: Type non image
    Then la réponse est 422

  # === @edge ===
  Scenario: EXIF géoloc à stripper
    Given une photo avec EXIF GPS
    When elle est uploadée
    Then l'EXIF GPS est strippé (privacy)
    Et seul le hash + timestamp sont conservés

  Scenario: 10 photos / conversation
    Then la 11e est refusée

  # === @security ===
  Scenario: Scan antivirus
    Then chaque photo est scannée ClamAV
```

#### FR-032 — Blocage contact info (anti-circumvention)
- **Capacité Brief** : C7

```gherkin
Feature: Anti-circumvention

  # === @happy ===
  Scenario: Détection phone
    Given un message "appelez 0470 12 34 56"
    When il est analysé
    Then il est bloqué avec warning

  Scenario: Détection email
    Given un message "contacte moi@exemple.eu"
    When il est analysé
    Then il est bloqué

  # === @negative ===
  Scenario: Faux positif
    Given un message "j'ai 47 ans"
    When il est analysé
    Then il passe (pas un numéro de phone)

  # === @edge ===
  Scenario: Tentative encodage (04/70/12/34/56)
    Given un message avec encodage
    When il est analysé
    Then il est bloqué (regex sophistiquée)

  # === @security ===
  Scenario: Audit blocage
    Then toute tentative est journalisée
    Et 3 tentatives = flag ops
```

---

### Module : Trust & Moderation (TRU)

#### FR-033 — Notation double-sens post-Mission
- **Capacité Brief** : C8

```gherkin
Feature: Notation

  Background:
    Given une Mission "RELEASED" depuis < 14 j

  # === @happy ===
  Scenario: Notation User → Provider
    When le User note 5 ★ + commentaire "Intervention parfaite"
    Then la note est publiée (ou masquée jusqu'à contre-notation)
    Et le rating moyen Provider est mis à jour

  Scenario: Double-sens (symétrie)
    Given User a noté Provider 5 ★
    When Provider note User 4 ★
    Then les 2 notes sont publiées simultanément (anti-représailles)

  # === @negative ===
  Scenario: Note hors [1, 5]
    When le User tente 0 ★ ou 6 ★
    Then la réponse est 422

  Scenario: Commentaire > 500 chars
    When le User tente un commentaire long
    Then la réponse est 422

  Scenario: Commentaire injurieux
    Given un commentaire "espèce de [insulte]"
    When l'IA de modération le détecte
    Then il est bloqué avec raison

  # === @edge ===
  Scenario: Notation après 14 j
    Given une Mission "RELEASED" depuis 15 j
    When le User tente de noter
    Then la réponse est 410 "RATING_WINDOW_CLOSED"

  Scenario: 2e notation même User même Mission
    Given le User a déjà noté
    When il tente une 2e fois
    Then la réponse est 409 "ALREADY_RATED"

  # === @security ===
  Scenario: 1 notation/User/Mission
    Then la contrainte unique (user_id, mission_id) est en base
    Et la tentative de double est techniquement impossible

  Scenario: Anonymisation RGPD
    Given un User effacé
    Then ses commentaires restent mais "anonyme" remplace son nom
```

#### FR-034 — Ouverture de Litige
- **Capacité Brief** : C8

```gherkin
Feature: Litige

  Background:
    Given une Mission "RELEASED" depuis < 14 j

  # === @happy ===
  Scenario: Ouverture par User
    When le User ouvre un Litige avec motif "QUALITY" + preuves (photos, description)
    Then le Litige est créé en "OPENED"
    Et l'Escrow est gelé (si non libéré) ou un remboursement est à programmer
    Et ops est notifié < 1 h

  Scenario: Ouverture par Provider
    When le Provider ouvre un Litige "USER_NO_SHOW"
    Then le Litige est créé en "OPENED"
    Et ops est notifié

  # === @negative ===
  Scenario: Mission > 14 j
    When le User tente d'ouvrir un Litige après 14 j
    Then la réponse est 410 "DISPUTE_WINDOW_CLOSED"

  Scenario: Motif vide
    When le User ouvre sans motif ni preuve
    Then la réponse est 422 "MOTIVE_REQUIRED"

  # === @edge ===
  Scenario: 2 Litiges même User/semaine
    Given un User avec 1 Litige cette semaine
    When il ouvre un 2e
    Then un flag "FRAUD_REVIEW" est levé
    Et ops examine

  Scenario: Litige sur Mission déjà litigée
    Given une Mission avec Litige résolu
    When le User tente un 2e Litige
    Then la réponse est 409 "ALREADY_DISPUTED"

  # === @security ===
  Scenario: Trace immuable
    Then le Litige est journalisé avec preuves, motif, timestamps
    Et l'audit_log est WORM

  Scenario: Preuves chiffrées
    Then les preuves sont chiffrées KMS OVH
    Et accessibles ops + parties uniquement
```

#### FR-035 — Sanction automatique et manuelle
- **Capacité Brief** : C8

```gherkin
Feature: Sanction

  # === @happy ===
  Scenario: Sanction automatique (3 Litiges valides 30 j)
    Given un Provider avec 3 Litiges "RESOLVED_USER_FAVOR" en 30 j
    When le seuil est atteint
    Then une "SUSPENSION_7J" est appliquée
    Et le Provider est notifié + ops pour review

  Scenario: Sanction manuelle (ops)
    Given un Provider avec fraude prouvée
    When l'ops applique "BAN" avec motif
    Then le Provider est désactivé immédiatement
    Et ses Payouts en attente sont gelés

  # === @negative ===
  Scenario: Provider déjà BAN
    Given un Provider "BAN"
    When l'ops tente une nouvelle sanction
    Then la réponse est 409 "ALREADY_BAN"

  # === @edge ===
  Scenario: Auto + manuelle en conflit
    Given une sanction auto "SUSPENSION_7J" en cours
    When l'ops applique "WARNING" (plus léger)
    Then la dernière gagne
    Et l'audit_log documente la contradiction

  Scenario: Sanction puis appel Provider
    Given un Provider "SUSPENSION_7J"
    When il fait appel (7 j)
    Then un ops review l'appel
    Et peut lever ou confirmer

  # === @security ===
  Scenario: Droit de rétractation 7 j
    Then tout Provider sanctionné est notifié
    Et il a 7 j pour contester
    Et un ops humain doit valider la sanction

  Scenario: Audit sanction
    Then chaque sanction est journalisée avec motif, niveau, auteur
    Et l'audit_log est WORM
```

#### FR-036 — Médiation ops avec workflow
- **Capacité Brief** : C8

```gherkin
Feature: Médiation

  # === @happy ===
  Scenario: Médiation complète
    Given un Litige "OPENED"
    When l'ops ouvre le dossier (photos, messages, traces)
    Et l'ops demande des info complémentaires aux parties (24 h)
    Et l'ops tranche "PARTIAL_REFUND 30 %"
    Then le Litige passe "RESOLVED_PARTIAL_REFUND"
    Et l'Escrow est ajusté automatiquement

  # === @negative ===
  Scenario: Partie ne répond pas > 7 j
    Given une demande d'info complémentaire sans réponse 7 j
    When le timeout se déclenche
    Then l'ops tranche sur la base des preuves disponibles

  # === @edge ===
  Scenario: Médiation > 30 j
    Given un Litige ouvert depuis 30 j
    When l'ops n'a pas tranché
    Then une alerte escalade ops senior

  # === @security ===
  Scenario: 4-eyes pour BAN
    Then une sanction "BAN" nécessite 2 ops validateurs
    Et 1 seul ne peut pas BAN
```

#### FR-037 — Calcul rating moyen pondéré
- **Capacité Brief** : C8

**Formule Wilson score** (lower bound, intervalle de confiance 95 %) :

```
Wilson(positives, total) = (p + z²/(2n) − z·sqrt(p·(1−p)/n + z²/(4n²))) / (1 + z²/n)

où :
- p = positives / total = (somme des notes × nombre) / (5 × total)  [note normalisée 0..1]
- n = total de notes
- z = 1.96 (intervalle 95 %)
```

```gherkin
Feature: Rating pondéré

  # === @happy ===
  Scenario: Calcul Wilson score
    Given un Provider avec 10 notes (moyenne 4.2 / 5)
    When le rating est calculé
    Then Wilson lower bound est utilisé (pas la moyenne brute)
    Et un Provider avec 1 note 5 ★ (Wilson ≈ 0.45) n'est pas mieux classé qu'un Provider avec 50 notes 4.5 ★ (Wilson ≈ 0.83)

  # === @edge ===
  Scenario: Provider sans note
    Given un Provider avec 0 note
    When le rating est demandé
    Then "Pas encore noté" est retourné
    Et un prior neutre (0.80 = 4.0/5 en Wilson) est utilisé pour le matching (transparence)

  Scenario: Provider avec 1 note parfaite
    Given 1 note 5 ★
    When le rating est calculé
    Then Wilson = 0.45 (à 95 % CI)
    Et la moyenne brute 5.0 n'est jamais affichée seule

  # === @negative ===
  Scenario: Notes invalides
    Given des notes hors [1, 5]
    When le calcul s'exécute
    Then les notes invalides sont ignorées
    Et un audit_log "RATING_INVALID_IGNORED" est créé

  # === @security ===
  Scenario: Audit calcul
    Then chaque calcul génère un audit_log (timestamp, provider_id, score)
    Et le calcul est rejouable (test property-based sur la formule Wilson)

  Scenario: Property-based test
    Then pour tout ensemble de notes, Wilson ≤ moyenne brute
    Et `proptest` vérifie 1000 jeux aléatoires
```

---

### Module : Ops & Admin (OPS)

#### FR-038 — Console admin KYC review
- **Capacité Brief** : C9

```gherkin
Feature: KYC review

  # === @happy ===
  Scenario: Validation KYC
    Given un Provider en "PENDING_OPS_REVIEW"
    When l'ops ouvre le dossier (BCE, assurance, itsme, profil)
    When l'ops valide
    Then le Provider passe "APPROVED"
    Et il peut configurer Stripe Connect
    Et un email lui est envoyé

  # === @negative ===
  Scenario: Refus sans motif
    When l'ops refuse sans motif
    Then la réponse est 400 "MOTIVE_REQUIRED"

  # === @edge ===
  Scenario: 4-eyes pour refus
    Given un refus
    Then un 2e ops doit valider
    Et après validation, le Provider est notifié

  Scenario: Provider annule pendant review
    Given un Provider qui annule sa demande en cours de review
    When l'ops tente de valider
    Then la réponse est 409 "PROVIDER_CANCELLED"

  # === @security ===
  Scenario: RBAC ops
    Then seul un ops "kyc_reviewer" peut valider/refuser
    Et 4-eyes principle pour refus
```

#### FR-039 — Exports régulateurs (RGPD, NIS2, TVA)
- **Capacité Brief** : C9

```gherkin
Feature: Exports régulateurs

  # === @happy ===
  Scenario: Export RGPD (demande APD)
    Given une demande APD pour un User
    When l'ops génère l'export
    Then un fichier JSON + PDF signé est produit
    Et il contient toutes les PII de l'User
    Et il est chiffré (PGP) et envoyé à l'APD

  Scenario: Export TVA annuel
    When l'ops génère l'export TVA 2026
    Then un CSV conforme SPF Finances est produit
    Et il est signé eIDAS

  # === @negative ===
  Scenario: Période invalide
    When l'ops génère un export avec période incohérente
    Then la réponse est 422

  # === @edge ===
  Scenario: Export > 100 k lignes
    Given un export lourd
    When l'ops le demande
    Then il est asynchrone
    Et l'ops est notifié à la fin

  # === @security ===
  Scenario: Journalisation
    Then chaque export est journalisé (type, période, demandeur)
    Et l'audit_log est WORM

  Scenario: DLP
    Then l'export est chiffré PGP avant envoi
    Et il n'est jamais exposé en clair hors vault
```

#### FR-040 — Dashboard ops temps réel
- **Capacité Brief** : C9

```gherkin
Feature: Dashboard

  # === @happy ===
  Scenario: KPI temps réel
    When l'ops ouvre le dashboard
    Then il voit : MAU, fill rate, GMV, NPS, Litiges en cours, Alertes
    Et refresh 30 s

  # === @negative ===
  Scenario: Backend indispo
    Given backend en panne
    When l'ops ouvre le dashboard
    Then cache last known + bannière alerte

  # === @edge ===
  Scenario: 0 données (lancement)
    When l'ops ouvre le dashboard à J0
    Then empty state guidé
    Et aucun KPI planté

  # === @security ===
  Scenario: RBAC
    Then seul un ops peut accéder
    Et les agrégats sont anonymisés (pas de PII)

  Scenario: Audit consultation
    Then chaque consultation dashboard est journalisée
```

#### FR-041 — Gestion RBAC ops
- **Capacité Brief** : C9

```gherkin
Feature: RBAC ops

  # === @happy ===
  Scenario: Création ops user avec rôle
    Given un super-admin ops
    When il crée un ops "kyc_reviewer"
    Then le nouvel ops peut uniquement valider KYC

  # === @negative ===
  Scenario: Role inexistant
    When on assigne un rôle "super_root"
    Then la réponse est 422

  # === @edge ===
  Scenario: Auto révocation après 90 j inactif
    Given un ops inactif 90 j
    When le job s'exécute
    Then son compte est désactivé
    Et un super-admin doit le réactiver

  # === @security ===
  Scenario: MFA obligatoire
    Then tout ops doit activer MFA (TOTP) à la première connexion
    Et sans MFA = accès bloqué
```

#### FR-042 — Audit log consultable
- **Capacité Brief** : C9

```gherkin
Feature: Audit log

  # === @happy ===
  Scenario: Recherche par acteur
    When l'ops filtre audit_log par "user_id = X"
    Then toutes les actions de cet User sont listées
    Et pagination 50/page

  Scenario: Export audit_log
    When l'ops exporte la période
    Then un CSV signé est généré

  # === @negative ===
  Scenario: Tentative modification
    When l'ops tente de modifier une entrée
    Then la réponse est 403 (WORM)

  # === @edge ===
  Scenario: Audit log > 10 M entrées
    Then la recherche utilise index + partition par mois

  # === @security ===
  Scenario: Lecture seule
    Then l'audit_log est strictement insert-only
    Et même un super-admin ne peut modifier
```

---

### Module : i18n

#### FR-043 — Internationalisation FR/NL/EN
- **Capacité Brief** : C10

```gherkin
Feature: i18n

  # === @happy ===
  Scenario: Changement langue
    Given un User authentifié en "fr"
    When il change pour "nl"
    Then l'UI bascule instantanément
    Et les emails futurs sont en NL
    Et les factures en NL

  # === @negative ===
  Scenario: Langue non supportée
    When le User tente "de"
    Then la réponse est 200 avec fallback FR

  # === @edge ===
  Scenario: Mix (User FR, Provider NL)
    Given une Mission entre User FR et Provider NL
    When un message est envoyé
    Then il n'est PAS auto-traduit (MVP)
    Et chaque partie voit l'UI dans sa langue

  # === @security ===
  Scenario: Strings depuis code
    Then les strings i18n sont compilées dans le binaire
    Et jamais fetchées runtime (pas d'injection)
```

#### FR-044 — i18n factures + emails
- **Capacité Brief** : C10

```gherkin
Feature: i18n documents

  # === @happy ===
  Scenario: Facture dans langue User
    Given un User avec locale "nl"
    When une facture est générée
    Then le PDF est en NL avec mentions légales BE traduites

  Scenario: Email dans langue destinataire
    Given un Provider avec locale "fr"
    When un email de notification est envoyé
    Then il est en FR

  # === @edge ===
  Scenario: Facture mixte
    Given un User NL et Provider FR
    Then chaque partie reçoit la facture dans sa langue
    Et la version officielle (fiscal) est dans la langue du Provider (émetteur légal)
```

---

## 7. Capacités d'extension (E1-E4) — FR-045 à FR-068

> Les 24 FR ci-dessous couvrent les capacités d'extension C11-C14 du Brief v0.3 §7. Ils sont déclenchés au fil de l'eau selon les gates go/no-go, au rythme choisi. Chaque jalon est indépendant des autres.
>
> **Conformité transverse** (rappel) : RGPD (Art. 17, 22, 35), AI Act (Art. 10, 12, 14, 15), Platform Work (loi BE 26 avril 2024 + directive UE 2024/2831), DSP2/SCA, NIS2/CyFun Basic, TVA BE. Invariants Brief §10 intangibles (BCE, prix libre Provider, pas d'exclusivité, escrow, trace immuable, etc.).
>
> **Traceabilité** : chaque FR référence sa capacité Brief (C11/C12/C13/C14) et le jalon associé (J11/J12'/J13/J14).

### Module : E1 — Densification secteurs (C11, J11)

> Activable lorsque le gate **fill rate > 60 % tenu sur les 5 secteurs pilotes** (Brief §19.3) est franchi. Onboarding séquentiel : 1 secteur à la fois, max 2 par an (mitigation H-14). Sous-capacités CBS J11 (E1.1-E1.6) — `00-Capability-Breakdown-Estimation.md` §Partie 2 · J11.

#### FR-045 — Moteur d'attestation compétences réglementées
- **En tant que** Provider candidat sur un secteur réglementé · **je veux** attester mes agréments légaux (B2V BR électricité, agréation gaz naturel PEB, etc.) · **afin de** respecter l'Invariant §10.8 (pas d'Intervention sans assurance/agrément valide)
- **Préconditions** : Provider BCE-validé (FR-003) ; secteur cible est activé au catalogue (FR-047) ; pièce justificative < 10 Mo, format PDF/PNG/JPEG
- **Garanties post** : `Skill` lié au `Provider` avec `credential_kind`, `credential_ref`, `valid_until`, `verified_at` ; statut `PENDING_OPS_REVIEW` ; audit log WORM ; entrée dans le registre sectoriel
- **Capacité Brief** : C11 (J11, sous-cap. E1.1 + E1.3)

```gherkin
Feature: Attestation compétences réglementées

  Background:
    Given le catalogue contient le secteur "electricite" avec skill "B2V_BR"
    And le secteur est marqué "reglemented: true"
    And l'API fédération sectorielle (ex. AIB-Vincotte) est joignable

  # === @happy ===
  Scenario: Attestation B2V BR électricité valide
    Given un Provider BCE-validé "0123.456.789"
    When il soumet son attestation B2V BR "B2V-2026-12345" avec PDF valide + valid_until "2027-12-31"
    Then le skill "B2V_BR" est lié au Provider en statut "PENDING_OPS_REVIEW"
    And le PDF est scanné ClamAV + stocké S3 chiffré KMS
    And un audit_log "SKILL_CREDENTIAL_SUBMITTED" est créé

  Scenario: Vérification automatique auprès de la fédération sectorielle
    Given un Provider avec skill "B2V_BR" en "PENDING_OPS_REVIEW"
    When le job de cross-check interroge l'API AIB-Vincotte
    Then le statut passe à "VERIFIED" si l'attestation est confirmée
    And le champ `verified_at` est horodaté
    And un email de confirmation est envoyé au Provider

  Scenario: Ajout d'une deuxième compétence réglementée
    Given un Provider déjà actif sur "plomberie"
    When il ajoute l'attestation "agreation_gaz_PEB" valide
    Then son périmètre Provider s'étend à "plomberie_gaz"
    And son profil conserve son rating historique (pas de reset)

  # === @negative ===
  Scenario Outline: Attestation invalide
    Given un Provider sur secteur réglementé "<sector>"
    When il soumet une attestation avec "<issue>"
    Then la réponse est 400 avec code "<code>"
    And aucune entrée Skill n'est créée
    Examples:
      | sector       | issue                            | code                       |
      | electricite  | PDF corrompu                     | FILE_CORRUPTED             |
      | electricite  | valid_until dans le passé        | CREDENTIAL_EXPIRED         |
      | plomberie_gaz| numéro d'agrément format invalide| CREDENTIAL_FORMAT          |
      | electricite  | PDF > 10 Mo                      | FILE_TOO_LARGE             |
      | chauffage    | type MIME "application/msword"   | FILE_TYPE_UNSUPPORTED      |

  Scenario: Attestation non confirmée par la fédération
    Given un Provider avec attestation B2V BR numéro "FAKE-9999"
    When le job de cross-check interroge la fédération
    Then le statut passe à "REJECTED"
    And un motif "CREDENTIAL_NOT_FOUND" est journalisé
    And un email demande au Provider de régulariser

  # === @edge ===
  Scenario: Attestation valide côté Provider mais expirée côté fédération
    Given un Provider avec PDF affichant valid_until "2026-12-31"
    And la fédération indique "révoquée depuis 2026-09-15"
    When le job de cross-check s'exécute
    Then le statut passe à "REVOKED"
    And le Provider ne reçoit plus de Demandes sur ce secteur
    And un email de notification est envoyé + task ops créée

  Scenario: Fédération sectorielle indisponible
    Given l'API AIB-Vincotte en panne (timeout 5xx)
    When un Provider soumet une attestation
    Then la réponse est 202 "PENDING_VERIFICATION"
    And un retry exponentiel est planifié (max 72 h)
    And le Provider peut opérer sur secteur "non réglementé" en attendant

  Scenario: Attestation en cours de validité mais BCE du Provider suspendu
    Given un Provider avec B2V BR valide mais BCE "0987.654.321" en "FAILLITE"
    When il tente d'ajouter une nouvelle compétence
    Then la réponse est 422 "BCE_INACTIVE"
    And un review ops anti-fraude est ouvert

  # === @security ===
  Scenario: Anti-falsification PDF (metadata EXIF / hash)
    When un Provider soumet un PDF d'attestation
    Then un hash SHA-256 du fichier est calculé et stocké en WORM
    And toute modification ultérieure du PDF stocké est détectable
    And un PDF dont le hash correspond à un hash blacklisté (fraude connue) est rejeté 422 "CREDENTIAL_BLACKLISTED"

  Scenario: Vérification cross-source (anti-corruption)
    When un job quotidien vérifie les agréments
    Then il compare BCE Provider ↔ attestation ↔ registre fédération
    And toute incohérence déclenche une task ops "REVIEW_CREDENTIAL_MISMATCH"
    And le Provider est suspendu en attendant (mitigation Platform Work + confiance)

  Scenario: Audit log non-effaçable des vérifications
    Given un Provider avec attestation vérifiée
    When il demande son effacement RGPD (FR-005)
    Then ses PII sont anonymisés
    And l'audit_log "SKILL_CREDENTIAL_VERIFIED" est conservé (assertion comptable + CyFun)
    And le hash de l'attestation reste consultable par ops pour analyse rétrospective
```

#### FR-046 — Onboarding Provider multi-secteur (KYC spécifique par Skill)
- **En tant que** Provider déjà actif sur un secteur MVP · **je veux** étendre mon activité à un nouveau secteur post-MVP · **afin de** diversifier mes revenus sans recommencer tout le KYC de base
- **Préconditions** : Provider en statut `APPROVED` sur au moins 1 secteur ; secteur cible activé au catalogue ; KYC de base (BCE, itsme, assurance RC pro générique) déjà validé
- **Garanties post** : nouvelle entrée `ProviderSkill` créée ; KYC additionnel (attestation spécifique, assurance étendue) collecté si requis ; profil ops notifié pour review
- **Capacité Brief** : C11 (J11, sous-cap. E1.2)

```gherkin
Feature: Onboarding multi-secteur Provider

  Background:
    Given un Provider APPROVED sur "plomberie" depuis ≥ 3 mois
    And le secteur "chauffage" est activé au catalogue
    And le Provider a un rating ≥ 4.0 et 0 sanction active

  # === @happy ===
  Scenario: Extension à un secteur non réglementé
    Given "bricolage" est un secteur non réglementé
    When le Provider demande l'extension à "bricolage"
    Then son profil passe en "EXTENSION_REVIEW" pour ce secteur
    And seuls les documents sectoriels minimes sont exigés (portefeuille compétences)
    And un ops est notifié pour validation 4-eyes

  Scenario: Extension à un secteur réglementé avec doc additionnelle
    Given "electricite" exige B2V BR
    When le Provider demande l'extension à "electricite"
    Then le workflow déclenche FR-045 (attestation B2V BR)
    And le Provider ne peut accepter des Demandes "electricite" qu'après validation croisée

  # === @negative ===
  Scenario Outline: Blocage extension
    Given un Provider sur "plomberie"
    When il demande l'extension à "<sector>"
    Then la réponse est "<code>" car "<reason>"
    Examples:
      | sector      | code                       | reason                                          |
      | plomberie   | SKILL_ALREADY_OWNED        | déjà actif sur ce secteur                       |
      | inconnu     | SECTOR_NOT_ACTIVATED       | secteur non encore ouvert au catalogue          |
      | electricite | BASE_KYC_EXPIRED           | assurance RC pro expirée (< 30 jours)           |
      | chauffage   | PROVIDER_RATING_TOO_LOW    | rating < 3.5 sur secteur actuel                 |
      | demenage    | SANCTION_ACTIVE            | sanction SUSPENSION_30J en cours                |

  # === @edge ===
  Scenario: Demandes simultanées d'extension
    Given un Provider sur "plomberie"
    When il demande l'extension à "chauffage" et "electromenager" en parallèle
    Then les deux workflows sont créés indépendamment
    And chaque validation ops est distincte (4-eyes par secteur)

  Scenario: Provider radié en cours d'extension
    Given un Provider en "EXTENSION_REVIEW" pour "chauffage"
    When son BCE passe en "RADIATION" KBO-BCE
    Then l'extension est annulée automatiquement
    And son secteur existant est suspendu (FR-050)
    And un audit_log "PROVIDER_DEREGISTERED" est créé

  Scenario: Réactivation après suspension
    Given un Provider suspendu 7j sur "plomberie" (sanction levée)
    When il demande l'extension à "bricolage"
    Then la demande est refusée 422 "PROVIDER_RECENTLY_SANCTIONED"
    And une période de grâce de 30 jours post-sanction est imposée

  # === @security ===
  Scenario: Anti-circumvention de KYC
    Given un Provider avec sanction BAN annulée via RGPD effacement
    When il tente de recréer un compte et d'étendre à un secteur sensible
    Then le hash de son précédent BSN (argon2id) est comparé
    And un match déclenche une task ops "FRAUD_RESURRECTION_ATTEMPT"

  Scenario: Audit log distinct par secteur
    When un Provider étend à 2 secteurs
    Then chaque ProviderSkill a son propre audit_log
    And les événements sont traçables indépendamment (mitigation AI Act Art. 12)

  Scenario: Quota d'extensions par Provider (anti-dilution)
    Given un Provider avec déjà 5 secteurs actifs
    When il demande un 6e secteur
    Then la demande nécessite validation super_admin (4-eyes renforcé)
    And un avis qualité est requis (rating > 4.5 sur secteurs existants)
```

#### FR-047 — Administration catalogue extensible (ajout secteur post-MVP)
- **En tant que** ops admin · **je veux** ajouter un nouveau secteur au catalogue post-MVP · **afin de** déployer la plateforme dans de nouveaux domaines avec gouvernance 4-eyes
- **Préconditions** : ops admin role `catalog_manager` ou `super_admin` ; secteur inexistant au catalogue ; libellés i18n FR/NL/EN préparés ; gate fill rate > 60 % validée par superviseur
- **Garanties post** : nouveau `Sector` créé avec `code`, `i18n_key`, `reglemented`, `kyc_requirements` ; 2 ops signatures requises (4-eyes) ; libellés i18n FR/NL/EN disponibles ; audit log WORM
- **Capacité Brief** : C11 (J11, sous-cap. E1.4)

```gherkin
Feature: Ajout de secteur au catalogue

  Background:
    Given un ops "catalog_manager" authentifié MFA
    And un draft de secteur "chauffage" préparé (libellés FR/NL/EN, skills, prix indicatifs bootstraps)
    And le gate "fill rate > 60 %" est validé sur les 5 secteurs pilotes

  # === @happy ===
  Scenario: Création complète en 2 étapes (4-eyes)
    Given ops A soumet le secteur "chauffage" en statut "DRAFT"
    When ops B (distinct) approuve le draft
    Then le secteur passe à "ACTIVE"
    And le catalogue public reflète le nouveau secteur en FR/NL/EN
    And un audit_log "SECTOR_ACTIVATED" signé par les 2 ops est créé

  Scenario: Libellés i18n par défaut définis
    Given un secteur "jardinage" créé
    When un User consulte le catalogue
    Then le libellé est "Jardinage" (FR) / "Tuinieren" (NL) / "Gardening" (EN)
    And toute locale non couverte retombe sur EN par défaut

  # === @negative ===
  Scenario Outline: Validation draft secteur invalide
    When ops A soumet un draft avec "<issue>"
    Then la réponse est 400 avec code "<code>"
    Examples:
      | issue                                       | code                    |
      | code secteur déjà existant "plomberie"      | SECTOR_CODE_DUPLICATE   |
      | libellé FR manquant                         | I18N_LABEL_MISSING      |
      | libellé NL vide                             | I18N_LABEL_MISSING      |
      |Skills sans credential_kind sur réglemented | KYC_SPEC_INCONSISTENT   |
      | prix indicatif négatif                      | INDICATIVE_PRICE_INVALID|

  Scenario: Approbation par même ops (anti-circumvention 4-eyes)
    Given ops A a soumis le draft "chauffage"
    When ops A tente d'approuver son propre draft
    Then la réponse est 403 "FOUR_EYES_REQUIRED"
    And le draft reste en "DRAFT"

  Scenario: Gate fill rate non atteint
    Given le fill rate pilote est à 52 % (gate < 60 %)
    When ops tente d'activer un nouveau secteur
    Then la réponse est 422 "GATE_FILL_RATE_NOT_MET"
    And un email super_admin est envoyé

  # === @edge ===
  Scenario: Rollback d'un secteur créé par erreur
    Given un secteur "menage" activé mais 0 Provider et 0 Demande
    When super_admin désactive le secteur
    Then il passe à "DEPRECATED"
    And il n'apparaît plus dans le catalogue public
    And l'audit_log "SECTOR_DEPRECATED" est créé

  Scenario: Secteur avec libellés partiels (FR+NL mais pas EN)
    Given un draft avec FR et NL mais EN manquant
    When ops soumet
    Then la réponse est 422 "I18N_LABEL_MISSING"
    And un warning propose l'auto-traduction assistée (validation humaine requise)

  Scenario: Activation en période de pic (mitigation H-14)
    Given un pic de Demandes détecté (> 1000/jour)
    When ops tente d'activer un nouveau secteur
    Then la réponse est 422 "ACTIVATION_FROZEN_DURING_PEAK"
    Et un email propose une activation différée hors pic

  # === @security ===
  Scenario: RBAC strict sur création secteur
    Given un ops "read_only"
    When il tente de créer un secteur
    Then la réponse est 403 "RBAC_FORBIDDEN"
    And la tentative est journalisée audit_log "RBAC_VIOLATION_ATTEMPT"

  Scenario: Audit immuable des modifications catalogue
    When un secteur est modifié (libellé, prix, skills)
    Then chaque modification est journalisée avec ops_id + horodatage + diff JSON
    And l'historique est conservé en WORM S3 Object Lock pendant 7 ans (CyFun)

  Scenario: Validation contrainte d'intégrité référentielle
    Given un secteur utilisé par ≥ 1 Provider ou Demande actif
    When ops tente de supprimer le secteur (au lieu de déprécier)
    Then la réponse est 409 "SECTOR_IN_USE"
    And seul le statut "DEPRECATED" est autorisé (soft delete)
```

#### FR-048 — Calibration prix indicatifs par secteur (IQR + bootstrapping)
- **En tant que** ops admin · **je veux** initialiser et recalibrer les prix indicatifs par secteur · **afin d'** informer les Users sans imposer de prix aux Providers (Invariant §10.2)
- **Préconditions** : secteur activé ; ≥ 20 missions complétées pour calibration IQR OU seed manuel pour secteur bootstrap
- **Garanties post** : table `indicative_prices` mise à jour avec `median`, `q1`, `q3`, `n_samples`, `last_calibrated_at` ; prix indicatif jamais binding (affiché "à titre indicatif")
- **Capacité Brief** : C11 (J11, sous-cap. E1.5)

```gherkin
Feature: Calibration prix indicatifs

  Background:
    Given un secteur "chauffage" activé
    And le job de calibration tourne chaque nuit à 02:00 UTC

  # === @happy ===
  Scenario: Calibration IQR nominale
    Given ≥ 50 missions "chauffage" complétées sur les 90 derniers jours
    When le job s'exécute
    Then l'IQR (Q1, médiane, Q3) est calculé et stocké
    And les outliers (> 1.5 × IQR hors Q1/Q3) sont exclus
    And la table indicative_prices est mise à jour avec n_samples=50

  Scenario: Bootstrap manuel secteur nouveau
    Given un secteur "demenagement" créé sans historique
    When ops saisit des prix seed (median 250 €, Q1 180, Q3 350)
    Then la table est initialisée avec is_bootstrap=true
    And l'UI User affiche "prix indicatif basé sur données partielles"
    And le seed est conservé jusqu'à ≥ 20 missions réelles

  # === @negative ===
  Scenario Outline: Calibration échouée
    Given un secteur "<sector>"
    When le job s'exécute
    Then la calibration est "<outcome>" car "<reason>"
    Examples:
      | sector        | outcome         | reason                                         |
      | demenagement  | SKIPPED         | n_samples < 20, bootstrap conservé             |
      | plomberie     | PARTIAL         | 30 % outliers détectés (anomalie tarifs)       |
      | inconnu       | ERROR           | secteur inexistant                             |
      | livraison     | STALE_KEEP      | 0 mission sur les 90j, ancienne calibration OK |

  Scenario: Tentative de calibration avec échantillon biaisé
    Given 30 missions "electricite" mais 28 viennent d'un seul Provider
    When le job s'exécute
    Then un warning "CONCENTRATION_RISK" est journalisé
    And l'ops est notifié pour review (anti-manipulation prix)

  # === @edge ===
  Scenario: Surge transitoire non incorporé (anti-learning empoisonné)
    Given un pic de Demandes "plomberie" (rupture canalisation collective)
    When le job s'exécute
    Then les prix indicatifs ne sont PAS mis à jour sur le pic
    And une médiane mobile pondérée (90 jours) filtre les valeurs extrêmes

  Scenario: Recalibration manuelle par ops (override)
    Given des prix manifestement erronés (bug seed)
    When super_admin force la recalibration immédiate
    Then la table est mise à jour + flag is_manual_override=true
    And l'audit_log "PRICE_OVERRIDE_MANUAL" est créé avec motif

  Scenario: Changement de TVA (passage 6 % → 21 % sur un secteur)
    Given le taux TVA "chauffage" passe de 6 % à 21 %
    When le job s'exécute
    Then les prix indicatifs sont recalculés en HT puis TTC au nouveau taux
    And la transition est documentée (date effet, secteur, motif fiscal)

  # === @security ===
  Scenario: Anti-manipulation par Provider (umpush prix)
    Given un Provider qui facture systématiquement × 3 la médiane
    When le job de calibration s'exécute
    Then ses missions sont exclues comme outliers
    And un flag "PRICE_OUTLIER_PROVIDER" est levé sur son profil
    And un review ops est créé (potentiel abus Platform Work)

  Scenario: Prix indicatif jamais contraignant (Invariant §10.2)
    When un User consulte un prix indicatif
    Then l'UI affiche "prix indicatif moyen, non contractuel"
    And le Provider reste libre de proposer n'importe quel prix dans son Devis
    And la plateforme n'applique aucun cap min/max au Devis

  Scenario: Audit des recalibrations (anti-falsification historique)
    When ops consulte l'historique d'un secteur
    Then chaque recalibration est tracée (date, n_samples, médiane, auteur)
    Et l'historique ne peut être ni modifié ni supprimé (WORM)
```

#### FR-049 — Migration et recrutement bulk Providers (BCE + skills mapping)
- **En tant que** ops admin · **je veux** importer en masse des Providers BCE pré-identifiés · **afin d'** accélérer la densification d'un nouveau secteur (mitigation H-4)
- **Préconditions** : fichier CSV < 50 Mo, ≤ 1000 lignes ; colonnes BCE obligatoires ; ops role `bulk_recruiter`
- **Garanties post** : invitations générées avec token JWT 7 jours ; statut `INVITED` (pas `APPROVED`) ; aucun Provider activé sans son consentement explicite
- **Capacité Brief** : C11 (J11, sous-cap. E1.6)

```gherkin
Feature: Recrutement bulk Providers

  Background:
    Given un ops "bulk_recruiter" authentifié MFA
    And un secteur "chauffage" activé

  # === @happy ===
  Scenario: Import CSV nominal
    Given un CSV "providers_chauffage.csv" avec 100 lignes valides (BCE, email, secteur)
    When ops lance l'import
    Then 100 invitations sont créées en statut "INVITED"
    And chaque Provider reçoit un email avec lien token JWT (validité 7 jours)
    And un audit_log "BULK_IMPORT_PROVIDERS" est créé

  Scenario: Provider invité complète son onboarding
    Given un Provider avec token "inv-abc123" valide
    When il clique le lien et complète KYC + itsme
    Then son profil passe par FR-003 + FR-002 normalement
    And l'attribution secteur "chauffage" est pré-cochée (modifiable)

  # === @negative ===
  Scenario Outline: Lignes CSV invalides
    Given un CSV avec lignes erronées
    When ops lance l'import
    Then les lignes invalides sont rejetées avec "<code>"
    Examples:
      | issue                              | code                    |
      | BCE invalide "123"                 | BCE_FORMAT              |
      | BCE déjà actif Provider            | BCE_ALREADY_USED        |
      | email malformé                     | EMAIL_MALFORMED         |
      | secteur non activé                 | SECTOR_NOT_ACTIVATED    |
      | BCE en faillite (KBO-BCE)          | BCE_BANKRUPT            |
      | colonnes manquantes                | CSV_SCHEMA_INVALID      |

  Scenario: Doublons intra-fichier
    Given un CSV avec le même BCE en lignes 12 et 47
    When ops lance l'import
    Then seule la ligne 12 génère une invitation
    And la ligne 47 est marquée "DUPLICATE_INTRA_FILE" dans le rapport

  # === @edge ===
  Scenario: Provider déjà User existant (cross-link)
    Given un CSV avec BCE "0123.456.789" déjà lié à un User (sans profil Provider)
    When ops importe
    Then une invitation est envoyée à l'email User existant
    And à l'acceptation, le profil Provider est greffé sur le User existant (pas de double compte)

  Scenario: Token d'invitation expiré
    Given un token "inv-abc123" créé il y a 8 jours
    When le Provider clique
    Then la réponse est 410 "INVITE_EXPIRED"
    Et un ops peut renvoyer une nouvelle invitation (max 3 rappels)

  Scenario: Provider refuse l'invitation
    Given un Provider invité
    When il clique "Refuser"
    Then son statut passe à "INVITE_DECLINED"
    And son email n'est pas re-spamé (respect RGPD minimisation)

  # === @security ===
  Scenario: Consentement explicite RGPD requis
    When un Provider complète son onboarding après invitation
    Then il doit accepter explicitement les ToS + RGPD
    And son consentement est journalisé avec timestamp + version ToS
    And un opt-in séparé est requis pour marketing (pas de bundling RGPD)

  Scenario: Rate-limit invitations par ops
    Given un ops a déjà envoyé 5000 invitations ce mois
    When il tente un nouvel import
    Then la réponse est 429 "BULK_QUOTA_EXCEEDED"
    And un alerte super_admin est levée (anti-spam)

  Scenario: Audit immuable du mapping Skills
    When ops mappe une colonne CSV "skill_label" → Skill "B2V_BR"
    Then chaque mapping est journalisé (input, output, ops_id)
    And un mapping absurde (CSV "chauffagiste" → Skill "B2V_BR" réservé électricité) est bloqué 422 "SKILL_MAPPING_INVALID"
```

#### FR-050 — Vérification automatique continue des agréments (BCE / INASTI / fédérations)
- **En tant que** système · **je veux** vérifier quotidiennement les agréments et BCE de tous les Providers actifs · **afin de** détecter expirations, suspensions ou radiations sans attendre un incident
- **Préconditions** : job planifié nightly ; API KBO-BCE, INASTI, fédérations sectorielles joignables ; Provider avec `verified_until` < J+30
- **Garanties post** : `ProviderSkill.status` passe à `EXPIRED`/`REVOKED` dès détection ; Provider suspendu sur ce secteur ; audit log ; notification Provider + ops
- **Capacité Brief** : C11 (J11, sous-cap. E1.3)

```gherkin
Feature: Vérification continue des agréments

  Background:
    Given le job nightly "verify_provider_credentials" tourne à 03:00 UTC
    And 200 Providers actifs ont un credential à vérifier

  # === @happy ===
  Scenario: Vérification nominale (tout OK)
    Given un Provider avec BCE actif et B2V BR valide jusqu'à "2027-12-31"
    When le job s'exécute
    Then son statut reste "APPROVED"
    And le champ `last_verified_at` est mis à jour
    Et un audit_log "CREDENTIAL_OK" est créé

  Scenario: Détection d'expiration imminente (≤ 30 jours)
    Given un Provider avec valid_until "2026-08-15" (dans 20 j)
    When le job s'exécute
    Then un email de rappel est envoyé au Provider
    And une task ops "RENEWAL_REMINDER_SENT" est créée
    And le statut reste "APPROVED" jusqu'à expiration effective

  # === @negative ===
  Scenario Outline: Anomalies détectées
    Given un Provider avec "<anomaly>"
    When le job s'exécute
    Then "<action>" est appliquée
    Examples:
      | anomaly                                  | action                                            |
      | BCE KBO-BCE en "FAILLITE"                | PROViDER_SUSPENDED_IMMEDIATE                     |
      | attestation B2V BR expirée hier          | SKILL_STATUS_EXPIRED + suspension secteur        |
      | INASTI statut "radié"                    | PROVIDER_SUSPENDED_IMMEDIATE + review ops        |
      | assurance RC expirée                     | SUSPENSION_7J auto + notification                |
      | agrément PEB "révoqué"                   | SKILL_STATUS_REVOKED + provider BAN review       |

  Scenario: Provider suspendu automatiquement
    Given un Provider avec BCE en faillite
    When le job le détecte
    Then son profil passe à "AUTO_SUSPENDED"
    And il ne reçoit plus de nouvelles Demandes
    And ses Demandes en cours sont signalées aux Users (option annulation sans frais)

  # === @edge ===
  Scenario: API KBO-BCE indisponible (timeout)
    Given l'API KBO-BCE en panne
    When le job s'exécute
    Then les vérifications sont reportées au lendemain
    And un warning "EXTERNAL_API_DEGRADED" est journalisé
    And le statut Provider reste inchangé (pas de faux négatif)

  Scenario: Mise à jour KBO-BCE retardée (BCD pas encore à jour)
    Given un Provider radié hier mais KBO-BCE non encore màj
    When le job s'exécute
    Then il s'appuie sur la dernière donnée KBO-BCE disponible
    And un contrôle croisé INASTI est tenté en complément
    And un flag "DEGRADED_VERIFICATION" est posé sur le Provider

  Scenario: Faible volume à vérifier
    Given 0 Provider à vérifier cette nuit
    When le job s'exécute
    Then il se termine en < 5 s avec log "NO_OP"
    And aucune alerte n'est levée

  # === @security ===
  Scenario: Kill-switch manuel (sécurité)
    Given une fraude massive détectée sur une fédération sectorielle
    When super_admin active le kill-switch "REVOKE_ALL_CREDENTIALS_FEDERATION_X"
    Then tous les Providers avec credential de cette fédération sont suspendus
    And un audit_log "KILL_SWITCH_ACTIVATED" est créé avec motif + signature super_admin
    And une procédure de re-vérification manuelle est enclenchée

  Scenario: Journalisation immuable des vérifications
    When le job vérifie 200 Providers
    Then 200 entrées audit_log sont créées (1 par Provider, indépendamment du résultat)
    And les logs sont chiffrés at-rest + WORM S3 Object Lock
    And toute tentative de modification déclenche une alerte CyFun

  Scenario: Privacy by design (minimisation données fédération)
    When le job interroge la fédération
    Then il ne transmet que le numéro d'agrément (pas le nom/email Provider)
    And la réponse est stockée sous forme de booléen + date (pas de données personnelles additionnelles)
    And un DPIA sectoriel documente ce traitement (RGPD Art. 35)
```

### Module : E2' — Enhancement Tauri/PWA continu (C12, J12')

> Note : ce module remplace le jalon J12 *« Native premium »* originel. Décision superviseur v0.3 : **pas de natif RN/Flutter, Tauri 2.0 + PWA uniquement** (Brief §7 C12, §16, §19.3). Voir ADR-001 et Brief v0.3 §16. Référence `00-Capability-Breakdown-Estimation.md` §Partie 2 · J12' révisée (J12 → J12', budget 100-200 h au lieu de 1000-1600 h).

#### FR-051 — Push rich media avec actions inline + deep-linking
- **En tant que** User/Provider · **je veux** recevoir des notifications enrichies avec actions inline (accepter/refuser un Devis depuis la notif) · **afin de** réagir en < 1 tap sans ouvrir l'app
- **Préconditions** : plugin Tauri `notification` initialisé ; token APNs (iOS) / FCM (Android) enregistré ; permissions notif accordées par l'User
- **Garanties post** : notification reçue avec category (devis, mission, message, payment) ; actions inline mappées à des intents ; deep link ouvre l'écran ciblé ; audit log de délivrance
- **Capacité Brief** : C12 (J12', sous-cap. E2'.1)

```gherkin
Feature: Push rich media + deep-linking

  Background:
    Given le plugin Tauri notification v2 est chargé
    And l'User a accordé les permissions de notification
    And un token APNs/FCM valide est enregistré côté backend

  # === @happy ===
  Scenario: Notification Devis avec actions inline
    Given un Provider a soumis un Devis pour la mission "M-1234"
    When le backend envoie une notif "category=quote_action" avec actions ["ACCEPTER", "REFUSER"]
    Then l'User reçoit la notif en < 10 s (P95)
    And les 2 boutons inline s'affichent
    And un tap "ACCEPTER" déclenche l'API FR-017 sans ouvrir l'app

  Scenario: Deep-linking vers mission spécifique
    Given une notif "category=mission_update" avec payload {"mission_id":"M-1234"}
    When l'User tape la notif
    Then l'app s'ouvre directement sur /mission/M-1234
    And l'auth est vérifiée avant l'affichage (pas de fuite données)

  Scenario: Notification message avec preview tronqué
    Given un Provider envoie un message "Bonjour, je suis à 5 min"
    When le backend push la notif
    Then le preview affiche ≤ 50 chars + "..."
    And le contenu complet nécessite d'ouvrir l'app (anti-fuite RGPD)

  # === @negative ===
  Scenario Outline: Notif échec envoi
    Given une notif "<category>"
    When le backend tente l'envoi
    Then la réponse est "<code>" car "<reason>"
    Examples:
      | category        | code                  | reason                                            |
      | quote_action    | TOKEN_INVALID         | token FCM expiré ou révoqué                       |
      | mission_update  | PERMISSION_DENIED     | User a désactivé les notifs                       |
      | payment         | PAYLOAD_TOO_LARGE     | body > 4 Ko (limite FCM)                          |
      | message         | RATE_LIMIT_PUSH       | > 10 notifs/min vers même device                  |

  Scenario: Action inline invalide
    Given une notif avec action "HACK"
    When le backend tente de l'inclure
    Then la réponse est 422 "ACTION_NOT_WHITELISTED"
    And seules les actions déclarées dans le manifest sont permises

  # === @edge ===
  Scenario: Notif reçue pendant app au 1er plan
    Given l'app est active et l'User consulte la mission "M-1234"
    When une notif "mission_update" pour "M-1234" arrive
    Then aucune bannière n'est affichée (anti-spam)
    And un marqueur in-app "Mise à jour" est posé sur la mission
    And le statut est rafraîchi en WebSocket

  Scenario: Device hors-ligne au moment de l'envoi
    Given un User device éteint
    When le backend envoie la notif
    Then APNs/FCM stocke en file (TTL 24 h par défaut)
    And à la remise, la notif est marquée "DELAYED_REMISE"
    And un ack de réception est attendu dans les 7 j (sinon token marqué suspect)

  Scenario: Multiple devices même User
    Given un User connecté sur iPhone + tablette
    When une notif est envoyée
    Then elle est reçue sur les 2 devices
    And une action inline sur l'un invalide la notif sur l'autre (sync via ack WS)

  # === @security ===
  Scenario: Payload chiffré end-to-end (anti-fuite metadata)
    When le backend envoie une notif
    Then le body visible par APNs/FCM ne contient aucune PII (titre générique)
    And le payload detail est chiffré AES-256-GCM avec clé dérivée user
    And seul le client Tauri peut déchiffrer

  Scenario: Anti-spoofing action inline
    Given une notif avec action "ACCEPTER"
    When le client la déclenche
    Then il doit inclure un JWT short-lived (≤ 30 s) signé par le device
    And le backend vérifie le JWT + nonce avant d'appliquer l'action
    And un replay attack (même JWT réutilisé) est rejeté 401 "NONCE_REPLAY"

  Scenario: Notification sans tracking (RGPD minimisation)
    When une notif est envoyée
    Then aucun pixel espion ni beacon analytics n'est inclus
    And seul l'event "DELIVERED" / "OPENED" est tracé (opt-in User)
    Et un User peut désactiver le tracking "OPENED" dans ses préférences
```

#### FR-052 — Secure storage biométrie (FaceID/TouchID pour refresh token + paiement ≥ 100 €)
- **En tant que** User · **je veux** protéger mes actions sensibles via biométrie native (FaceID/TouchID) · **afin de** sécuriser refresh token et paiements importants même si le device est déverrouillé
- **Préconditions** : plugin Tauri `stronghold` (IOTA) ou `biometric` ; biométrie enrollée sur le device ; opt-in explicite User
- **Garanties post** : refresh token chiffré at-rest dans secure enclave/keystore ; paiement ≥ 100 € exige SCA biométrique (DSP2) ; audit log sans données biométriques (jamais stockées)
- **Capacité Brief** : C12 (J12', sous-cap. E2'.2)

```gherkin
Feature: Secure storage biométrie

  Background:
    Given le plugin Tauri biometric est disponible
    And le device a une biométrie enrollée (FaceID/TouchID)
    And l'User a opt-in pour "Renforcement biométrique"

  # === @happy ===
  Scenario: Refresh token stocké dans secure enclave
    Given un User login réussit
    Then le refresh token est chiffré et stocké dans iOS Keychain / Android Keystore
    And il n'est accessible qu'après validation biométrique
    And un hook rotation (FR-004) détecte toute extraction anormale

  Scenario: Paiement ≥ 100 € exige SCA biométrique
    Given un User accepte un Devis de 250 €
    When il confirme le paiement
    Then le prompt FaceID/TouchID s'affiche
    And à validation, le paiement est autorisé (3DS2 + SCA renforcée DSP2)
    Et un audit_log "BIOMETRIC_SCA_SUCCESS" est créé

  # === @negative ===
  Scenario Outline: Échecs biométriques
    Given un User tente une action sensible
    When la biométrie "<issue>"
    Then la réponse est "<code>"
    Examples:
      | issue                       | code                       |
      | échec reconnaissance        | BIOMETRIC_NOT_RECOGNIZED   |
      | utilisateur annule          | BIOMETRIC_CANCELLED        |
      | biométrie non enrollée      | BIOMETRIC_NOT_ENROLLED     |
      | hardware indisponible       | BIOMETRIC_UNAVAILABLE      |
      | 5 échecs consécutifs        | BIOMETRIC_LOCKED_30S       |

  Scenario: Paiement < 100 € sans biométrie autorisé
    Given un User avec biométrie activée paie un Devis de 50 €
    When il confirme
    Then aucun prompt biométrique n'apparaît (SCA allégée DSP2)
    And le paiement suit le flow 3DS2 standard

  # === @edge ===
  Scenario: User désactive biométrie dans iOS Réglages
    Given un User avec refresh en secure storage
    When il désactive FaceID dans Réglages iOS
    Then l'app détecte le changement au prochain lancement
    And le refresh est invalidé (force re-login)
    And un email "Sécurité : biométrie modifiée" est envoyé

  Scenario: Refresh expiré pendant session biométrique
    Given un User avec refresh stocké biométriquement, expiré depuis 31 jours
    When il tente de refresh
    Then le refresh est refusé (FR-004)
    And un re-login complet est requis

  Scenario: Migration token entre devices
    Given un User change d'iPhone
    When il se loggue sur le nouveau device
    Then le refresh de l'ancien device est invalidé (anti-vol)
    And un email "Nouveau device détecté" est envoyé
    Et le User doit re-valider sa biométrie sur le nouveau device

  # === @security ===
  Scenario: Aucune donnée biométrique stockée serveur
    When un User valide sa biométrie
    Then seul un booléen "biometric_enrolled: true" est stocké profil
    And aucune empreinte/FaceID template n'est stocké ni transmis
    And la vérification est déléguée au secure enclave du device

  Scenario: Anti-circumvention (jailbreak / root)
    Given un device jailbreaké ou rooté
    When l'app démarre
    Then elle détecte le compromission (SafetyNet Attestation sur Android, DT-Tap sur iOS)
    And le refresh en secure storage est verrouillé
    And un re-login + itsme (FR-002) est requis pour continuer

  Scenario: Audit des SCA renforcées
    When un paiement ≥ 100 € est effectué
    Then l'audit_log contient uniquement "BIOMETRIC_SCA_SUCCESS" + timestamp
    And aucune donnée biométrique (template, score) n'est loggée
    Et le log est conservé 13 mois (DSP2 preuve)
```

#### FR-053 — Géoloc background permanent via plugin Tauri (fallback PWA foreground documenté)
- **En tant que** Provider en Mission active · **je veux** partager ma position même quand l'app est en arrière-plan · **afin que** le User me suive en continu sans intervention manuelle
- **Préconditions** : PoC plugin Tauri Mobile (Brief §19.3 gate J12') concluant ; permission iOS "Always" / Android "Background location" accordée ; Mission en statut `PROVIDER_EN_ROUTE` ou `ON_SITE` ; opt-in explicite Provider
- **Garanties post** : position Pushed toutes les 15 s en background (vs 3 s foreground) ; fallback PWA foreground documenté ; stop automatique à `COMPLETED` ; DPIA étendu
- **Capacité Brief** : C12 (J12', sous-cap. E2'.3) — mitigation H-13

```gherkin
Feature: Géoloc background permanent

  Background:
    Given le PoC plugin Tauri géoloc background a passé la gate J12'
    And le Provider a opt-in pour "Tracking background missions"
    And une Mission active est en statut "PROVIDER_EN_ROUTE"

  # === @happy ===
  Scenario: Tracking background nominal
    Given un Provider en Mission "M-1234" avec app en arrière-plan
    When le device détecte un changement de position ≥ 50 m
    Then une position est envoyée au backend (batch toutes 15 s)
    And le User voit la position se mettre à jour sur sa carte
    Et l'économie batterie est respectée (GPS low-power mode)

  Scenario: Arrêt automatique à fin de Mission
    Given une Mission "M-1234" passant à "COMPLETED"
    When le backend confirme la fin
    Then le tracking background cesse immédiatement
    And l'os libère la permission active (notif barre status retirée)
    Et aucune position n'est plus collectée (Invariant §10.5)

  # === @negative ===
  Scenario Outline: Refus ou échec tracking
    Given un Provider en Mission
    When "<issue>"
    Then "<outcome>"
    Examples:
      | issue                                       | outcome                                          |
      | permission "Always" refusée iOS             | FALLBACK_FOREGROUND (FR-019) + notif User        |
      | permission révoquée en cours de Mission     | FALLBACK_FOREGROUND immédiat                     |
      | GPS désactivé par l'User                    | Notif "Veuillez réactiver la localisation"       |
      | Batterie < 10 % et mode économie            | Fréquence réduite à 1/min + warning              |

  Scenario: Provider désactive opt-in en cours de Mission
    Given un Provider en Mission active
    When il désactive "Tracking background"
    Then le tracking bascule en foreground immédiatement
    And le User est notifié "Tracking dégradé (foreground uniquement)"
    And le Provider peut re-opt-inner à tout moment

  # === @edge ===
  Scenario: Fallback PWA foreground (PoC plugin insuffisant)
    Given le plugin Tauri géoloc background non concluant sur une combinaison device/OS
    When le device démarre une Mission
    Then l'app force le mode foreground (FR-019)
    And un warning "MODE_DÉGRADÉ" est affiché
    Et un rapport telemetry est envoyé ( amélioration plugin, mitigation H-13)

  Scenario: Device en mode Avion / métro
    Given un Provider en Mission avec pas de réseau pendant 5 min
    When le réseau revient
    Then les positions bufferisées localement sont sync (max 100 points)
    And les positions plus anciennes sont drop avec log "STALE_POSITION_DISCARDED"

  Scenario: Mission longue > 4 h (anti-usure batterie)
    Given une Mission de 5 h avec tracking continu
    When le 4e créneau d'heure est atteint
    Then un warning batterie est affiché au Provider
    And la fréquence est réduite à 30 s
    Et le User est informé du tracking dégradé

  # === @security ===
  Scenario: Aucune collecte hors Mission (Invariant §10.5)
    Given un Provider sans Mission active
    When le plugin background est interrogé
    Then aucune position n'est collectée ni transmise
    Et le job de garde vérifie nightly qu'aucune position n'est reçue hors Mission (audit AI Act Art. 12)

  Scenario: Données chiffrées in-transit + at-rest
    When une position est envoyée au backend
    Then elle transite via TLS 1.3 + payload chiffré AES-256-GCM
    And en DB, elle est chiffrée via KMS OVH
    And à la libération de l'Escrow, les positions sont anonymisées (agrégées secteur) après 90 j

  Scenario: Suppression RGPD Art. 17 étendue au background
    Given un Provider demande son effacement RGPD
    When le job FR-005 s'exécute
    Then toutes ses positions background sont supprimées
    And un certificat de suppression est généré (audit log WORM)
    Et l'API "mes données" exporte puis efface les positions
```

#### FR-054 — Re-submission stores (App Store + Play Store) avec versioning automatique + rollback OTA
- **En tant que** ops release manager · **je veux** publier une nouvelle version sur App Store et Play Store avec rollback OTA rapide · **afin de** réagir en cas de bug critique sans attendre la review Apple/Google
- **Préconditions** : certificats Apple p8 + Android keystore valides ; version semver ; release notes FR/NL/EN ; tests E2E Maestro verts
- **Garanties post** : binaire soumis en simultaneous release ; OTA Tauri Updater activable pour hotfix webview < 24 h ; rollback < 1 h
- **Capacité Brief** : C12 (J12', sous-cap. E2'.4)

```gherkin
Feature: Re-submission stores + rollback OTA

  Background:
    Given un ops "release_manager" authentifié MFA
    And les certificats Apple p8 + Android keystore sont valides (≥ 30 j avant expiration)
    And la version "1.4.0" a passé les gates CI/CD (lint, tests, E2E Maestro)

  # === @happy ===
  Scenario: Submission simultanée App Store + Play Store
    Given un binaire "1.4.0" buildé (Tauri 2.0)
    When ops lance la commande "release submit --version 1.4.0"
    Then 2 soumissions sont créées (App Store Connect + Play Console)
    Et les release notes FR/NL/EN sont jointes
    And un audit_log "RELEASE_SUBMITTED" est créé

  Scenario: Hotfix OTA sur webview (sans re-review)
    Given un bug JS critique détecté post-release
    When ops publie un patch OTA via Tauri Updater
    Then les clients détectent la màj en < 1 h
    And le patch est signé (clé privée Tauri) + vérifié à l'install
    And un audit_log "OTA_PATCH_DEPLOYED" est créé

  # === @negative ===
  Scenario Outline: Submission rejetée
    Given une submission "1.4.0"
    When la review store "<store>" la rejette
    Then l'alerte ops est levée avec motif "<reason>"
    Examples:
      | store      | reason                                        |
      | App Store  | "Guideline 2.1 - Demo credentials required"  |
      | App Store  | "Guideline 5.1.1 - Privacy policy missing"   |
      | Play Store | "Violation permissions background location"  |
      | Play Store | "Target API level 35 required"                |

  Scenario: Rollback Apple lent (review > 6 h)
    Given une submission critique à déployer < 2 h
    When ops tente un rollback direct App Store
    Then la réponse est "ROLLBACK_REVIEW_PENDING"
    And ops active le feature-flag backend "DISABLE_BUGGY_FEATURE" en < 5 min à la place

  # === @edge ===
  Scenario: Version déjà soumise (anti-doublon)
    Given une submission "1.4.0" déjà en review
    When ops tente de resoumettre "1.4.0"
    Then la réponse est 409 "VERSION_ALREADY_IN_REVIEW"
    Et ops doit incrementer en "1.4.1"

  Scenario: Certificat expiré pendant la review
    Given le certificat Apple p8 expire dans 5 jours
    When ops soumet "1.4.0" avec review estimée à 7 jours
    Then un warning "CERT_EXPIRES_DURING_REVIEW" est levé
    Et ops doit renouveler le certificat avant validation

  Scenario: Hotfix OTA échec signature
    Given un patch OTA "1.4.0-webview-fix1" avec signature invalide
    When les clients reçoivent le patch
    Then ils refusent l'install (anti-MITM)
    And un rapport telemetry "OTA_SIGNATURE_INVALID" remonte
    And ops est alerté

  # === @security ===
  Scenario: Signature code obligatoire
    Given un binaire "1.4.0"
    When il est soumis
    Then il doit être signé Apple Developer ID + Android Play App Signing
    Et les clés sont stockées dans OVH KMS (rotation annuelle)
    Et un audit "RELEASE_SIGNED" est journalisé

  Scenario: Pinning de version (anti-rollback malveillant)
    Given un client Tauri "1.4.0"
    When il reçoit une notif OTA "downgrade 1.3.0"
    Then il refuse (anti-rollback attack)
    And un warning "DOWNGRADE_ATTEMPT" remonte
    Et seuls les downgrades signés super_admin avec motif sont autorisés

  Scenario: SBOM CycloneDX joint à chaque release
    When une version est publiée
    Then un SBOM CycloneDX est généré et publié (CRA, NIS2)
    And il liste toutes les dépendances Tauri/Svelte/Rust avec hash
    Et le SBOM est archivé 10 ans (WORM S3)
```

#### FR-055 — PWA grand public alternative (accès navigateur desktop/mobile sans install)
- **En tant que** User sans app installée · **je veux** accéder à Klaar via navigateur · **afin de** tester le service puis installer l'app native si besoin
- **Préconditions** : navigateur récent (Chrome ≥ 100, Safari ≥ 16, Firefox ≥ 100) ; service worker PWA en ligne ; HTTPS obligatoire
- **Garanties post** : accès lecture catalogue + création Demande (sans push background, sans biométrie) ; invite "Installer l'app" après 2 sessions ; feature parity documentée
- **Capacité Brief** : C12 (J12', sous-cap. E2'.5 — déplacé depuis E3.6)

```gherkin
Feature: PWA grand public

  Background:
    Given le domaine "app.dep.be" sert une PWA (HTML/CSS/JS + service worker)
    And le navigateur User supporte les service workers

  # === @happy ===
  Scenario: Accès PWA sans install
    Given un visiteur sur desktop Chrome 120
    When il ouvre "https://app.dep.be"
    Then la page d'accueil se charge en < 3 s (P95)
    Et le catalogue 5 secteurs est visible
    And un bouton "Installer l'app" est proposé (modal PWA)

  Scenario: Création de Demande en PWA
    Given un visiteur authentifié en PWA
    When il soumet une Demande géolocée
    Then la Demande est créée et matchée normalement (FR-011/012)
    Et la notification de match est envoyée par email (push non dispo PWA iOS)

  Scenario: Installation PWA après 2 sessions
    Given un visiteur a visité la PWA 2 fois sur 7 jours
    When il ouvre une 3e fois
    Then une bannière "Installer Klaar sur votre écran d'accueil" s'affiche
    Et l'install génère une icône native (WebAPK sur Android)

  # === @negative ===
  Scenario Outline: Dégradation fonctionnelle PWA
    Given un visiteur en PWA
    When il tente "<action>"
    Then la réponse est "<code>" car "<reason>"
    Examples:
      | action                          | code                  | reason                                          |
      | géoloc background               | FEATURE_NOT_AVAILABLE | PWA ne supporte pas iOS background location     |
      | paiement biométrique           | FEATURE_NOT_AVAILABLE | PWA WebAuthn non configuré sur ce navigateur    |
      | upload photo > 5 Mo             | FILE_TOO_LARGE_PWA    | quota PWA mobile Safari                         |
      | push sans abonnement            | PUSH_NOT_SUBSCRIBED   | User n'a pas activé Web Push                    |

  Scenario: Navigateur non supporté
    Given un visiteur sur IE 11 ou Chrome < 90
    When il ouvre la PWA
    Then une page "Navigateur non supporté" s'affiche
    And un lien "Installer Chrome/Firefox/Safari" est proposé

  # === @edge ===
  Scenario: PWA offline (mode dégradé)
    Given un visiteur en PWA puis hors-ligne
    When il ouvre l'app
    Then le cache service worker affiche la dernière version du catalogue
    Et un bandeau "Hors-ligne - fonctions limitées" est visible
    And la création de Demande est bufferisée jusqu'au retour réseau

  Scenario: Migration PWA → app native
    Given un visiteur utilise la PWA depuis 1 mois
    When il installe l'app Tauri native
    Then son compte est conservé (même email)
    And les tokens PWA sont invalidés (sécurité)
    And l'app native propose les features avancées (push, biométrie)

  Scenario: Feature parity matrix documentée
    Given un User compare PWA vs app native
    When il consulte la matrice
    Then il voit un tableau clair : PWA = features core, app native = + push/biométrie/background
    And la documentation est versionnée (git, audit log)

  # === @security ===
  Scenario: HTTPS obligatoire
    Given une tentative HTTP "http://app.dep.be"
    When le client s'y connecte
    Then le serveur répond 301 vers HTTPS
    And HSTS est posé (max-age 1 an, includeSubDomains)
    Et aucune donnée ne transite en clair

  Scenario: Service worker integrity check
    When le service worker se register
    Then son hash SHA-384 est vérifié par le serveur (SRI)
    Et toute modification non signée bloque l'enregistrement
    Et un CSP strict-dynamic est appliqué (anti-XSS)

  Scenario: Pas d'accès PWA pour ops admin
    Given un ops admin tente d'accéder à /admin via PWA navigateur
    When il s'authentifie
    Then la réponse est 403 "OPS_NATIVE_APP_REQUIRED"
    Et la console admin requiert un navigateur desktop dédié + MFA TOTP (FR-041)
```

### Module : E3 — Intelligence, monétisation & ouverture (C13, J13)

> Activable après stabilisation des secteurs pilotes (J11 + J12' fructueux). Sous-capacités CBS J13 (E3.1-E3.7) — `00-Capability-Breakdown-Estimation.md` §Partie 2 · J13. Conformité AI Act renforcée pour E3.1 (matching IA) et E3.2 (surge) — Brief §15 H-15.

#### FR-056 — Matching IA ranking (features distance × rating × fiabilité × prix)
- **En tant que** système · **je veux** ranker les Providers par modèle IA supervisé · **afin d'** optimiser le fill rate et la qualité de matching (au-delà du matching règles C3)
- **Préconditions** : ≥ 10 000 missions historiques pour entraîner ; modèle versionné en MLOps ; supervision humaine activée (AI Act Art. 14) ; DPIA étendu
- **Garanties post** : score IA calculé par Provider avec feature importance traçable ; fallback règles (C3) en cas de défaillance ; audit biais semestriel ; kill-switch opérationnel
- **Capacité Brief** : C13 (J13, sous-cap. E3.1) — mitigation H-15

```gherkin
Feature: Matching IA ranking

  Background:
    Given le modèle "ia-matcher-v1" est déployé en canary (10 % du trafic)
    And le DPIA "matching-IA-2026" est validé par le DPO
    And la Trace AI Act Art. 12 est active

  # === @happy ===
  Scenario: Score IA calculé pour 5 Providers candidats
    Given une Demande "plomberie" avec 5 Providers disponibles à < 5 km
    When le moteur IA calcule les scores
    Then chaque Provider reçoit un score ∈ [0, 1]
    And le top 3 reçoit la notification (vs 5 en mode règles)
    Et la feature importance est journalisée (distance, rating, fiabilité, prix indicatif)

  Scenario: Fallback règles si IA indisponible
    Given le service IA en panne (timeout 5 s)
    When une Demande arrive
    Then le matching bascule sur le moteur règles (FR-012) en < 1 s
    And un warning "IA_FALLBACK_RULES" est journalisé
    And le User n'observe pas de dégradation visible

  # === @negative ===
  Scenario Outline: Anomalies IA
    Given une Demande
    When le modèle IA "<issue>"
    Then "<action>"
    Examples:
      | issue                            | action                                          |
      | score négatif (bug modèle)       | rejet du score + fallback règles + alerte ops   |
      | score identique pour tous        | fallback règles (suspicion biais)               |
      | latence > 30 s                   | timeout + fallback règles                       |
      | modèle expiré (version < courante)| refus chargement + ops alerté                  |

  Scenario: Modèle drifté (> 20 % erreur prédictive)
    Given un audit continu montre drift > 20 % sur les 7 derniers jours
    When le seuil est franchi
    Then le kill-switch "DISABLE_IA_MATCHER" est activé automatiquement
    And un super_admin doit le réarmer après investigation
    And l'audit_log "KILL_SWITCH_TRIGGERED" est créé

  # === @edge ===
  Scenario: Cold-start nouveau secteur
    Given un secteur "demenagement" sans historique
    When une Demande arrive
    Then l'IA utilise un modèle générique fallback (similaire règles)
    And le cas est marqué "COLD_START" pour ré-entraînement futur

  Scenario: Provider nouveau sans rating
    Given un Provider avec 0 mission historique
    When l'IA doit le scorer
    Then un score neutre (0.5 + bonus "novelty") est appliqué
    And le Provider reçoit les Demandes en priorité pour collecter données (fairness)

  Scenario: Demande avec peu de candidats (1 Provider)
    Given une Demande avec 1 seul Provider à proximité
    When l'IA calcule le score
    Then il n'y a pas de classement (juste 1 notification)
    Et l'IA enregistre l'event "FORCED_MATCH" (pas de choix)

  # === @security ===
  Scenario: Audit biais semestriel AI Act Art. 10-15
    Given 6 mois depuis la dernière audit
    When le job d'audit biais s'exécute
    Then il calcule les métriques d'équité (demographic parity, equal opportunity) par secteur, genre, quartier
    And un rapport est publié en console admin
    And si un biais > seuil est détecté, le DPO est alerté + kill-switch potentiel

  Scenario: Trace immuable de chaque décision IA
    When l'IA prend une décision de matching
    Then la Trace contient : modèle_version, features input, scores, top-k, latence
    And la Trace est chiffrée + WORM S3 Object Lock 5 ans
    Et l'User peut demander "Pourquoi ai-je été matché avec ce Provider ?" (droit explication AI Act)

  Scenario: Anti-poisoning données entraînement
    Given un Provider tente d'injecter de fausses données pour booster son score
    When le pipeline d'entraînement s'exécute
    Then les outliers sont filtrés (z-score, isolation forest)
    And le Provider est flaggé "DATA_SUSPICIOUS"
    Et un review ops anti-fraude est ouvert

  Scenario: Supervision humaine (Art. 14)
    Given le modèle propose un match à risque (Provider rating < 3.0)
    When le score est en zone "risque"
    Then un ops est notifié pour valider manuellement le match
    And l'ops peut override (annuler le match) avec motif journalisé
```

#### FR-057 — Surge pricing par zone et heure (transparence Platform Work)
- **En tant que** plateforme · **je veux** appliquer un coefficient d'urgence variable par zone/heure · **afin d'** équilibrer offre/demande sans imposer de prix au Provider (Invariant §10.2)
- **Préconditions** : seuils de demande/offre configurés ; transparence prix affiché ; justification horodatée ; pas de prix plancher imposé
- **Garanties post** : `surge_coefficient` calculé par zone + heure ; prix indicatif ajusté et affiché "Prix d'urgence ×N" ; Provider reste libre de son Devis ; contestation possible
- **Capacité Brief** : C13 (J13, sous-cap. E3.2) — Platform Work compliance

```gherkin
Feature: Surge pricing transparence

  Background:
    Given le moteur surge tourne toutes les 5 min
    And le rapport offre/demande est calculé par zone (commune) + tranche horaire

  # === @happy ===
  Scenario: Surge nominal sur pic demande
    Given commune "Ixelles" avec 50 Demandes vs 3 Providers dispo à 22 h un dimanche
    When le moteur calcule le coefficient
    Then surge_coefficient = 1.5 (modéré)
    And le prix indicatif est multiplié et affiché "Prix d'urgence ×1.5"
    Et le User voit la justification "Demande élevée, peu de Providers disponibles"

  Scenario: Retour à la normale
    Given surge_coefficient = 1.5 sur Ixelles
    When le ratio offre/demande repasse < seuil
    Then le coefficient redescend à 1.0 en < 15 min
    And l'UI User met à jour le prix indicatif
    Et aucun cached outdated n'est affiché > 5 min

  # === @negative ===
  Scenario Outline: Surge contesté
    Given un User avec surge × 2 sur sa Demande
    When il conteste avec motif "<reason>"
    Then la réponse est "<outcome>"
    Examples:
      | reason                                 | outcome                                            |
      | "Pas d'urgence réelle"                 | User peut annuler sans frais dans les 2 min        |
      | "Coefficient affiché tardivement"      | Refund partiel si prouvé (audit log)               |
      | "Bug calcul (coefficient > 5)"         | Nullité + audit ops immédiat                       |

  Scenario: Surge utilisé pour prix plancher (violation Invariant §10.2)
    Given un ops tente de configurer "prix minimum imposé = 80 €"
    When il soumet la config
    Then la réponse est 422 "PLATFORM_PRICE_FLOOR_FORBIDDEN"
    Et l'audit_log "ANTI_PLATFORM_WORK_VIOLATION" est créé

  # === @edge ===
  Scenario: Cap coefficient maximum
    Given une zone avec demande extrême (1 Provider pour 100 Demandes)
    When le moteur calcule le coefficient
    Then il est capé à 3.0 (protection User)
    Et un warning "SURGE_CAPPED" est journalisé
    Et l'ops peut activer un "mode carence" (suspension temporaire Demande)

  Scenario: Surge négatif (discount)
    Given une zone avec surplus de Providers et peu de Demandees
    When le moteur calcule le coefficient
    Then un coefficient < 1 (ex. 0.8) est affiché "Prix réduit"
    And le Provider peut refuser (liberté prix Invariant §10.2)

  # === @security ===
  Scenario: Surge jamais impose au Devis
    Given un User voit prix indicatif ×1.5
    When le Provider envoie son Devis
    Then le Provider peut mettre n'importe quel prix (hors suggestione)
    And l'User accepte ou refuse librement
    Et la Trace journalise "prix indicatif suggéré = X, Devis envoyé = Y" (audit Platform Work)

  Scenario: Audit rétrospectif du surge
    Given un inspecteur Platform Work demande historique surge 6 mois
    When ops génère l'export
    Then chaque coefficient est traçable (zone, heure, ratio, valeur, auteur si manuel)
    And les coefficients manuels sont distingués des automatiques
    Et l'export est signé eIDAS + WORM

  Scenario: Anti-discrimination géographique
    Given deux communes limitrophes "Anderlecht" (modeste) et "Ixelles" (aisé)
    When le moteur calcule surge
    Then il ne doit pas appliquer systématiquement coefficient > 1 sur Anderlecht (biais classe)
    And une audit biais annuel vérifie l'équité géographique
    Et un coefficient > 1 persistant > 7 jours sur une commune déclenche alerte ops
```

#### FR-058 — Subscription Pro (forfait mensuel Providers)
- **En tant que** Provider · **je veux** souscrire un abonnement Pro · **afin d'** accéder à des Demandes prioritaires, un CRM léger et des analytics avancées
- **Préconditions** : Provider en statut `APPROVED` ; moyens de paiement valides (FR-006) ; transparence prix/quotas ; pas d'exclusivité (Invariant §10.3)
- **Garanties post** : `Subscription` active avec `tier` (free/pro/premium) ; facture mensuelle TVA BE ; quotas appliqués ; résiliable à tout moment (sans lock-in)
- **Capacité Brief** : C13 (J13, sous-cap. E3.3)

```gherkin
Feature: Subscription Pro

  Background:
    Given un Provider APPROVED sur "plomberie"
    And le tier "Pro" coûte 29 €/mois TVA incluse (21 %)

  # === @happy ===
  Scenario: Souscription Pro nominale
    When le Provider sélectionne "Passer Pro"
    Then il paie 29 € via Stripe (3DS2)
    And son profil passe à tier "pro"
    And il reçoit l'accès à : Demandes prioritaires (10/jour), CRM, analytics
    Et une facture mensuelle TVA est envoyée

  Scenario: Renouvellement mensuel automatique
    Given un Provider Pro depuis 30 jours
    When le job billing s'exécute
    Then 29 € sont prélevés via Stripe
    Et une nouvelle facture est générée (FR-026)
    Et un audit_log "SUBSCRIPTION_RENEWED" est créé

  # === @negative ===
  Scenario Outline: Paiement souscription échoue
    Given un Provider tente de passer Pro
    When le paiement "<issue>"
    Then la réponse est "<code>"
    Examples:
      | issue                          | code                       |
      | carte refusée (3DS2 fail)      | PAYMENT_3DS2_FAILED        |
      | IBAN invalide                  | PAYMENT_METHOD_INVALID     |
      | fonds insuffisants             | PAYMENT_INSUFFICIENT_FUNDS |
      | Stripe indisponible            | PAYMENT_PROVIDER_DOWN      |

  Scenario: Quota Demandes prioritaires dépassé
    Given un Provider Pro a utilisé ses 10 Demandes prioritaires aujourd'hui
    When il tente une 11e
    Then la réponse est 429 "QUOTA_PRIORITY_EXCEEDED"
    And un message propose le tier Premium (50 Demandes/jour)
    Et les Demandes standards restent disponibles

  # === @edge ===
  Scenario: Provider rétrograde Free
    Given un Provider Pro depuis 6 mois
    When il annule (résiliation)
    Then son tier passe à "free" à la fin du mois en cours
    Et il ne perd pas ses ratings historiques
    Et les features premium sont désactivées (CRM en read-only 30 jours)

  Scenario: Provider BAN pendant abonnement
    Given un Provider Pro sanctionné BAN
    When la sanction est appliquée
    Then l'abonnement est suspendu
    Et un refund pro-rata est calculé
    Et un audit_log "SUBSCRIPTION_SUSPENDED_BAN" est créé

  Scenario: Migration Pro → Premium (prorata)
    Given un Provider Pro à J15 du cycle
    When il upgrade à Premium (49 €/mois)
    Then il paie le pro-rata : (49-29) × 15/30 = 10 €
    Et son cycle de facturation reste aligné sur la date initiale

  # === @security ===
  Scenario: Pas d'exclusivité (Invariant §10.3)
    Given un Provider Free (sans abonnement)
    When il compare son accès au Free vs Pro
    Then il a accès à toutes les Demandes standards (pas bridé)
    And la diff est uniquement sur : priorité notif, CRM, analytics
    Et aucun contractual exclusivity n'est imposé

  Scenario: Audit facturation récurrent (DSP2)
    Given un Provider Pro depuis 12 mois
    When un inspecteur vérifie la facturation
    Then chaque transaction a sa preuve SCA (3DS2 initial + MIT exemptions)
    Et les receipts sont conservés 13 mois (DSP2)
    Et un export TVA annuel est disponible

  Scenario: Anti-contournement quota
    Given un Provider Pro a atteint son quota
    When il tente de créer un 2e compte pour contourner
    Then le hash BSN (argon2id) détecte le doublon
    And le 2e compte est refusé 409 "PROVIDER_ALREADY_EXISTS"
    Et un review ops anti-fraude est ouvert
```

#### FR-059 — Assurance intégrée (API partenaire insurance BE)
- **En tant que** Provider non couvert ou sous-assuré · **je veux** souscrire assurance RC pro via Klaar · **afin de** respecter Invariant §10.8 et démarrer rapidement
- **Préconditions** : partenariat API signé avec assureur BE (ex. Baloise, AG); BCE Provider valide ; secteur identifié ; quote < 1000 € TTC
- **Garanties post** : `InsurancePolicy` créée et liée au Provider ; attestation valide immédiate ; facture automatique ; audit log ; cancellation possible
- **Capacité Brief** : C13 (J13, sous-cap. E3.4)

```gherkin
Feature: Assurance intégrée

  Background:
    Given un partenariat API "Baloise RC Pro" est activé
    And un Provider APPROVED sur "plomberie" sans assurance valide

  # === @happy ===
  Scenario: Souscription assurance immédiate
    When le Provider sélectionne "Souscrire assurance RC pro via Klaar"
    Then un appel API à Baloise génère un quote (300 €/an)
    And à validation, la police est émise + attestation PDF signée eIDAS
    And le Provider peut recevoir des Demandes secteur réglementé immédiatement

  Scenario: Renouvellement annuel automatique
    Given un Provider avec police Baloise expirant dans 30 jours
    When le job renewal s'exécute
    Then un email de rappel est envoyé
    And à défaut de réponse, le renouvellement est automatique (opt-out)
    Et la nouvelle attestation remplace l'ancienne

  # === @negative ===
  Scenario Outline: Souscription échoue
    When l'API assurance retourne "<issue>"
    Then la réponse est "<code>"
    Examples:
      | issue                          | code                          |
      | "BCE non éligible (jeune)"     | INSURANCE_BCE_NOT_ELIGIBLE    |
      | "Secteur non couvert"          | INSURANCE_SECTOR_EXCLUDED     |
      | "Sinistres récents (> 3)"      | INSURANCE_CLAIMS_HISTORY      |
      | "API assureur en panne"        | INSURANCE_PARTNER_DOWN        |

  Scenario: Quote > 1000 € (refus auto)
    Given un Provider avec profil risqué
    When le quote revient à 1500 €
    Then le système refuse l'auto-souscription (manual review requis)
    Et un ops est notifié "HIGH_RISK_INSURANCE_QUOTE"

  # === @edge ===
  Scenario: Provider annule dans les 14 jours (droit rétractation)
    Given un Provider avec police émise depuis 5 jours
    When il exerce son droit de rétractation
    Then la police est annulée
    And un refund complet est émis
    Et le Provider doit trouver une autre assurance (sinon suspendu)

  Scenario: Sinistre déclaré pendant Mission
    Given un Provider avec police Klaar active en Mission "M-1234"
    When un sinistre est ouvert par l'User (litige QUALITY grave)
    Then le sinistre est notifié à l'assureur (webhook)
    And la médiation ops FR-036 s'applique
    Et la procédure assurance prend le relais (DAB)

  Scenario: Assurance externe déjà valide
    Given un Provider avec attestation Baloise externe (hors Klaar)
    When Klaar détecte via FR-050 que l'assurance est valide
    Alors la proposition "Souscrire via Klaar" est masquée
    Et un message "Votre assurance externe est valide jusqu'à X" s'affiche

  # === @security ===
  Scenario: API partenaire authentifiée (mTLS)
    When Klaar appelle l'API Baloise
    Then la connexion utilise mTLS (client cert + serveur cert)
    Et le token JWT est signé ES256 et valide < 5 min
    Et les échanges sont chiffrés TLS 1.3

  Scenario: Données minimisées transmises
    When Klaar transmet le profil Provider à Baloise
    Then seuls BCE, secteur, ratings agrégés (anonymisés), historique sinistres Klaar sont transmis
    And aucune donnée User ou Mission spécifique n'est partagée
    Et un DPIA "assurance-integree" documente ce partage (RGPD Art. 35)

  Scenario: Audit immuable des souscriptions
    When une police est émise
    Then un audit_log "INSURANCE_POLICY_ISSUED" est créé
    Et le PDF attestation est stocké WORM S3 Object Lock 10 ans
    Et la révocation éventuelle est tracée (date, motif, signataire)
```

#### FR-060 — API publique partenaires (OpenAPI public)
- **En tant que** partenaire tiers (CRM, annuaire, marketplace) · **je veux** intégrer Klaar via API publique · **afin d'** enrichir mes services avec le catalogue et l'historique public
- **Préconditions** : partenaire enregistré ; OAuth2 client_credentials ; tier (free/pro/enterprise) ; signature contractual ; capping quotas
- **Garanties post** : access token OAuth2 émis ; endpoints publics documentés (catalogue read, mission status anonymisé, sector availability) ; rate-limit par tier ; audit log complet
- **Capacité Brief** : C13 (J13, sous-cap. E3.5)

```gherkin
Feature: API publique partenaires

  Background:
    Given un partenaire "Pagesdor Pro" avec client_id "client_abc"
    And un tier "Pro" (10 000 req/jour)
    And l'OpenAPI public "/docs/openapi.json" est publié

  # === @happy ===
  Scenario: Authentification OAuth2 client_credentials
    When le partenaire POST "/oauth/token" avec client_id + client_secret
    Then un access token JWT (1 h) est retourné
    And un refresh n'est pas requis (client_credentials flow)

  Scenario: Lecture catalogue public
    Given un partenaire avec token valide
    When il GET "/api/v1/public/catalog/sectors"
    Then la liste des secteurs actifs est retournée (FR/NL/EN)
    And les prix indicatifs sont inclus
    And la réponse est mise en cache CDN 5 min

  # === @negative ===
  Scenario Outline: Requête invalide
    Given un partenaire avec token valide
    When il "<action>"
    Then la réponse est "<code>"
    Examples:
      | action                                  | code                       |
      | GET endpoint privé "/admin/api/..."     | 403 RBAC_FORBIDDEN         |
      | GET sans Authorization header           | 401 MISSING_TOKEN          |
      | token expiré                            | 401 TOKEN_EXPIRED          |
      | signature JWT invalide                  | 401 TOKEN_INVALID          |

  Scenario: Quota journalier dépassé
    Given un partenaire a fait 10 000 req aujourd'hui
    When il tente une 10 001e
    Then la réponse est 429 "RATE_LIMIT_EXCEEDED" avec Retry-After: 86400
    And un email alerte ops est envoyé

  # === @edge ===
  Scenario: Versioning API (v1 dépréciée)
    Given une v2 d'API disponible
    When le partenaire appelle v1
    Then la réponse inclut header "Deprecation: true" + "Sunset: <date>"
    Et la v1 reste supportée 6 mois minimum (contrat API foyer)

  Scenario: Partenaire suspendu en cours d'utilisation
    Given un partenaire "Bad Actor" suspendu par ops
    When son token est utilisé
    Then la réponse est 401 "PARTNER_SUSPENDED"
    Et un audit_log "PARTNER_API_SUSPENDED" est créé

  Scenario: Pic de trafic (CDN cache hit)
    Given une campagne marketing partenaire génère 1000 req/s
    When les req arrivent
    Then le CDN absorbe 90 % en cache hit
    And le backend ne reçoit que 100 req/s (dégradé gracieux)
    Et le service reste disponible pour les clients Tauri

  # === @security ===
  Scenario: Rate-limit strict anti-DOS
    Given un partenaire tente 1000 req/s (burst)
    When le rate-limit s'applique
    Then il est plafonné à 100 req/s (tier Pro)
    Et les req au-delà sont 429
    Et une alerte ops est levée si burst > 30 min

  Scenario: Audit complet des accès partenaires
    When un partenaire appelle n'importe quel endpoint
    Then l'audit_log contient : partner_id, endpoint, params, response_size, latency
    Et les logs sont conservés 13 mois (RGPD Art. 30)
    Et un export est disponible pour le partenaire lui-même (transparence)

  Scenario: Aucune donnée PII dans endpoints publics
    When le partenaire GET "/api/v1/public/sectors/availability"
    Then la réponse ne contient aucun nom/email/téléphone Provider
    And seules des agrégations sont renvoyées (count dispo par zone)
    Et la DPIA "api-publique" documente cette minimisation

  Scenario: mTLS optionnel pour tier Enterprise
    Given un partenaire Enterprise "BigCorp"
    When il configure mTLS (client cert)
    Then son auth est renforcée (double factor)
    Et le quota Enterprise est étendu (1 M req/jour)
    Et la facturation est mensuelle (post-paiement)
```

#### FR-061 — Webhooks partenaires (events mission_completed, provider_available, sector_added)
- **En tant que** partenaire · **je veux** recevoir des events webhook en temps réel · **afin de** synchroniser mon système sans polling
- **Préconditions** : partenaire enregistré avec URL webhook HTTPS ; signature HMAC SHA-256 ; retry exponential backoff ; quotas par event
- **Garanties post** : event envoyé < 30 s après déclencheur ; ack attendu < 5 s ; retry max 5 (24 h) ; dead-letter queue après échec
- **Capacité Brief** : C13 (J13, sous-cap. E3.5 — extension)

```gherkin
Feature: Webhooks partenaires

  Background:
    Given un partenaire avec URL webhook "https://partner.be/hooks/dep"
    And un secret partagé HMAC (rotation 90 jours)

  # === @happy ===
  Scenario: Event mission_completed envoyé
    Given une Mission "M-1234" passe à "COMPLETED"
    When l'event est trigger
    Then un POST est envoyé à l'URL partenaire
    And le body contient {"event":"mission_completed","mission_id":"M-1234","sector":"plomberie","ts":"..."}
    And le header "X-Klaar-Signature: sha256=..." est présent (HMAC)

  Scenario: Ack rapide < 5 s
    Given un event envoyé
    When le partenaire répond 200 OK
    Then l'event est marqué "DELIVERED" dans la DLQ log
    And aucun retry n'est planifié

  # === @negative ===
  Scenario Outline: Échec livraison webhook
    Given un event envoyé à "https://partner.be/hooks/dep"
    When la réponse est "<status>"
    Then "<action>"
    Examples:
      | status | action                                                  |
      | 404    | Désactivation auto URL + email partenaire              |
      | 500    | Retry exponentiel (max 5 : 1, 5, 30 min, 4 h, 24 h)   |
      | timeout| Retry avec même backoff                                |
      | 403    | Désactivation immédiate + alerte ops                   |

  Scenario: Signature invalide côté partenaire
    Given un event reçu par le partenaire
    When le partenaire ne vérifie pas la signature HMAC
    Then il est vulnérable à spoofing
    Et la doc API Klaar marque cette étape obligatoire (foyer contrat-api.md)

  # === @edge ===
  Scenario: URL webhook injoignable (DNS failure)
    Given une URL avec DNS cassé
    When 5 retries successifs échouent
    Then l'event va en dead-letter queue (DLQ)
    And un email partenaire est envoyé "Webhook failing 5 times"
    And après 30 j en DLQ, l'event est drop (audit log)

  Scenario: Partenaire a 2 URLs (multi-env : prod + preprod)
    Given un partenaire avec URLs prod et preprod
    When un event est envoyé
    Then il est dupliqué sur les 2 URLs
    Et la signature est identique (anti-tampering)
    Et l'URL preprod est taguée "test_mode"

  Scenario: Burst d'events (mission bulk complete)
    Given 100 missions complétées en 5 min (pic)
    When les events sont envoyés
    Then ils sont batch (10 events par POST max)
    Et le débit est régulé (≤ 100 events/min par partenaire)
    Et la DLQ absorbe les échecs

  # === @security ===
  Scenario: URL webhook HTTPS obligatoire
    Given un partenaire configure "http://insecure.be/hook"
    When il tente d'enregistrer l'URL
    Then la réponse est 422 "WEBHOOK_MUST_BE_HTTPS"
    Et un warning "INSECURE_URL_ATTEMPT" est journalisé

  Scenario: Secret rotation 90 jours
    Given un secret partenaire en place depuis 89 jours
    When la rotation s'applique
    Then un nouveau secret est généré (32 bytes aléatoire)
    Et l'ancien reste valide 7 jours (grace period)
    Et un email partenaire notifie le changement

  Scenario: Replay attack détectée
    Given un attaquant intercepte un event et le rejoue
    When le partenaire vérifie le timestamp
    Alors le header "X-Klaar-Timestamp" doit être < 5 min
    Et un event > 5 min est rejeté (anti-replay)
    Et le partenaire peut exiger idempotence via "X-Klaar-Event-Id"
```

#### FR-062 — Analytics avancé ops (funnel, unit economics, density heatmap)
- **En tant que** ops admin · **je veux** des dashboards avancés (funnel fill rate, unit economics par secteur, density heatmap) · **afin d'** identifier les secteurs et zones à densifier
- **Préconditions** : data warehouse dédié (PostgreSQL read replica ou DuckDB embedded) ; agrégations nightly ; accès RBAC `analytics_viewer` minimum
- **Garanties post** : dashboards refresh ≤ 1 h ; export CSV/JSON signé ; agrégations ≥ 100 utilisateurs (RGPD k-anonymité)
- **Capacité Brief** : C13 (J13, sous-cap. E3.7)

```gherkin
Feature: Analytics avancé ops

  Background:
    Given un ops "analytics_viewer" authentifié
    And le job nightly "aggregate_metrics" a tourné à 04:00 UTC

  # === @happy ===
  Scenario: Dashboard fill rate par secteur
    When ops ouvre "/admin/analytics/funnel"
    Then le funnel Demandes → Matched → Quote → Accepted → Completed s'affiche par secteur
    Et les conversions sont calculées sur 30 jours glissants
    Et la heatmap géographique montre les zones à faible fill rate

  Scenario: Unit economics par secteur
    When ops ouvre "/admin/analytics/unit-economics"
    Then GMV, take-rate effectif, CAC, LTV par secteur sont affichés
    Et le payback period est calculé
    Et un comparison y/y est disponible

  # === @negative ===
  Scenario Outline: Données insuffisantes
    When ops consulte "<metric>"
    Then "<outcome>"
    Examples:
      | metric                          | outcome                                          |
      | fill rate secteur "demenagement"| "Données insuffisantes (< 20 missions)"          |
      | heatmap < 100 Users zone        | "Heatmap masquée (RGPD k-anonymité)"             |
      | LTV secteur nouveau             | "Calcul impossible, données < 90 jours"          |

  Scenario: Export forbidden si PII
    When ops tente d'exporter raw Users
    Then la réponse est 403 "EXPORT_RAW_USERS_FORBIDDEN"
    Et seul le DPO peut générer un export RGPD Art. 15 (FR-039)

  # === @edge ===
  Scenario: Pic de trafic frontend (100 ops simultanés)
    Given 100 ops consultent le dashboard en même temps
    When le backend charge
    Then la latence reste < 2 s (cache + replica dédié)
    Et le CPU backend ne dépasse pas 70 %

  Scenario: Comparaison 2 villes
    Given 2 villes activées (Bruxelles, Anvers)
    When ops compare les KPIs
    Then un side-by-side est affiché
    Et les écarts sont calculés (delta %)
    Et un export PDF est généré (board reporting)

  Scenario: Données temps réel vs batch
    Given un User veut le fill rate live
    When il consulte le dashboard
    Then les KPIs temps réel (5 min) sont affichés
    Et un badge "live" est visible
    Et les KPIs stabilisés (batch nightly) sont dans un onglet séparé

  # === @security ===
  Scenario: K-anonymité RGPD (≥ 100 individus)
    When ops consulte une heatmap
    Then les cellules avec < 100 Users sont floutées (agrégation)
    And il est impossible de re-identifier un User spécifique
    Et le paramètre k=100 est configurable (DPO approval required pour baisser)

  Scenario: RBAC analytics_viewer vs analyst
    Given un ops "analytics_viewer"
    When il tente d'accéder à "/admin/analytics/raw-sql"
    Then la réponse est 403 "RBAC_FORBIDDEN"
    Et seul "analyst" (avec audit renforcé) peut exécuter SQL ad-hoc

  Scenario: Audit log des consultations
    When ops consulte un dashboard sensible
    Then un audit_log "ANALYTICS_VIEWED" est créé (ops_id, dashboard, filtres)
    Et les exports sont tracés (avec destinataire si email)
    Et le log est chiffré + WORM 13 mois

  Scenario: Anti-inference (combinaison de filtres)
    Given un ops applique filtres "secteur=plomberie + commune=X + date=Y"
    When le résultat contient < 50 missions
    Then le système bloque l'affichage détaillé
    Et un warning "INFERENCE_RISK" est journalisé
    Et le DPO est notifié si cela se reproduit (3 fois/semaine)
```

#### FR-063 — Analytics Provider dashboard (revenus, ratings, taux acceptation)
- **En tant que** Provider · **je veux** un dashboard de revenus/ratings/taux acceptation · **afin d'** optimiser mon activité et ma réputation
- **Préconditions** : Provider APPROVED ; données aggrégées ≥ 7 jours ; accès via app Tauri ou PWA
- **Garanties post** : dashboard personnalisé (revenus J/7j/30j, ratings, temps moyen réponse, taux acceptation) ; comparaison vs médiane secteur ; pas de données concurrentielles (autre Providers anonymisés)
- **Capacité Brief** : C13 (J13, sous-cap. E3.7)

```gherkin
Feature: Analytics Provider dashboard

  Background:
    Given un Provider APPROVED sur "plomberie" depuis 90 jours
    And il a réalisé ≥ 20 missions

  # === @happy ===
  Scenario: Dashboard mensuel
    When le Provider ouvre "/provider/analytics"
    Then son revenu net (après take-rate 18 %) s'affiche pour J/7j/30j
    Et son rating moyen (Wilson) est affiché + comparaison vs médiane secteur
    Et son taux acceptation (Demandes acceptées / reçues) est calculé

  Scenario: Insight temps de réponse
    Given le Provider a un temps moyen réponse > 15 min
    When il consulte le dashboard
    Then une suggestion "Réduisez votre temps de réponse à < 5 min pour booster votre rating" est affichée
    Et un comparatif vs top 10 % du secteur est montré

  # === @negative ===
  Scenario Outline: Données insuffisantes
    Given un Provider avec "<state>"
    When il consulte "<metric>"
    Then la réponse est "<outcome>"
    Examples:
      | state                        | metric              | outcome                                       |
      | 0 mission                    | revenu              | "Données non disponibles"                     |
      | 5 missions                   | rating Wilson       | "Calcul à partir de 10 missions"              |
      | secteur nouveau              | comparaison secteur | "Pas encore de médiane secteur"               |

  Scenario: Provider Free (sans abonnement Pro)
    Given un Provider Free
    When il consulte "/provider/analytics"
    Then les métriques de base sont disponibles (revenu, rating)
    Et les insights avancés (CRM, comparaison top 10 %) sont en upgrade
    Et aucun bridage des métriques core (Invariant §10.3)

  # === @edge ===
  Scenario: Provider multi-secteurs
    Given un Provider sur "plomberie" + "electricite"
    When il consulte le dashboard
    Then les métriques sont séparées par secteur
    Et un total blended est disponible
    Et il peut filter par secteur

  Scenario: Provider nouveau (première mission)
    Given un Provider a réalisé sa 1re mission hier
    When il consulte le dashboard
    Then un message "Bienvenue ! Données en cours de constitution" est affiché
    Et les métriques partielles (1 mission) sont visibles
    Et un comparatif vs médiane sera disponible à partir de 5 missions

  Scenario: Provider avec制裁 récente
    Given un Provider avec SUSPENSION_7J levée
    When il consulte son dashboard
    Then la sanction est visible (transparence)
    Et un onglet "Sanctions" avec motif, date, et recours est disponible
    Et l'impact rating est documenté

  # === @security ===
  Scenario: Pas de données concurrents identifiés
    When le Provider consulte la comparaison vs secteur
    Then les autres Providers sont anonymisés (médiane, top 10 %, percentiles)
    And aucune info individuelle n'est visible
    Et l'audit_log "PROVIDER_COMPETITOR_INFERERENCE_BLOCKED" est journalisé si tentative

  Scenario: Données personnelles du Provider lui-même
    When le Provider consulte son dashboard
    Then il voit SES propres données détaillées (revenus, ratings, missions)
    Et il peut exporter en CSV (RGPD Art. 20 portabilité)
    Et l'export contient toutes ses données non anonymisées

  Scenario: Anti-évasion fiscale
    Given l'administration fiscale BE demande les revenus Provider X
    When ops génère l'export fiscal annuel (FR-039)
    Then les revenus bruts + take-rate + payouts sont inclus
    Et le document est signé eIDAS + WORM
    Et l'audit log trace la demande (mandat légal requis)

  Scenario: Audit des accès dashboard
    When le Provider consulte son dashboard
    Then l'audit_log "PROVIDER_DASHBOARD_VIEWED" est créé
    Et les exports sont tracés
    Et le Provider peut voir son propre historique d'accès (transparence)
```

### Module : E4 — Expansion géographique (C14, J14)

> Activable par ville après gate **rentabilité RBC prouvée** (Brief §19.3). Sous-capacités CBS J14 (E4.1-E4.3) — `00-Capability-Breakdown-Estimation.md` §Partie 2 · J14. Coût indicatif 13-23 k€/ville (130-230 h).

#### FR-064 — Activation ville (process ops)
- **En tant que** ops admin · **je veux** activer une nouvelle ville dans Klaar · **afin d'** étendre le périmètre géographique (Anvers, Liège, Gand, Charleroi)
- **Préconditions** : gate rentabilité RBC validée par superviseur ; ≥ 100 Providers locaux BCE ; tile-server OSM + Valhalla étendus (FR-066) ; conformité régionale validée (FR-067)
- **Garanties post** : ville ajoutée au catalogue `cities` ;Providers locales activés ; launch plan coordonné ; audit log + go-live daté
- **Capacité Brief** : C14 (J14, sous-cap. E4.3)

```gherkin
Feature: Activation ville

  Background:
    Given un ops "city_launch_manager" authentifié MFA
    And la gate "rentabilité RBC > 12 mois" est validée
    And ≥ 100 Providers locaux BCE pour la ville cible

  # === @happy ===
  Scenario: Activation complète (Anvers)
    Given tous les prérequis sont validés (FR-066 tiles, FR-067 conformité, ≥ 100 Providers)
    When ops lance "activate_city --city=anvers --go-live=2027-03-01"
    Then la ville "anvers" est ajoutée à la table `cities`
    Et les Providers locaux reçoivent l'accès au catalogue complet
    Et un audit_log "CITY_ACTIVATED" est créé

  Scenario: Pré-launch (soft launch)
    Given une ville "gand" en activation
    When ops active en "soft launch" (10 % du trafic cible)
    Then les Demandes sont limitées à un quartier pilote (ex. centrum)
    Et un monitoring étroit est appliqué (fill rate, ratings)
    Et le go-live full est conditionnel à des KPIs

  # === @negative ===
  Scenario Outline: Activation bloquée
    When ops tente d'activer la ville "<city>"
    Then la réponse est 422 "<code>" car "<reason>"
    Examples:
      | city     | code                          | reason                                            |
      | anvers   | PROVIDER_DENSITY_INSUFFICIENT | < 100 Providers BCE locaux                        |
      | gand     | TILES_NOT_DEPLOYED            | FR-066 non validé pour cette ville                |
      | liege    | COMPLIANCE_GAP                | FR-067 déclaration APD régionale non faite        |
      | paris    | CITY_OUT_OF_SCOPE             | hors Belgique (périmètre BE uniquement)           |

  Scenario: Gate rentabilité non atteinte
    Given la RBC n'a pas atteint la rentabilité (LTV/CAC < 2:1)
    When ops tente d'activer une 2e ville
    Then la réponse est 422 "GATE_PROFITABILITY_NOT_MET"
    Et un email super_admin est envoyé

  # === @edge ===
  Scenario: Activation rollback (soft launch échec)
    Given une soft launch "gand" avec fill rate < 30 % sur 30 jours
    When ops décide un rollback
    Then la ville passe en "PAUSED"
    Et les Users reçoivent un email "Service temporairement indisponible à Gand"
    Et les Providers locaux restent dans la base (pas de purge)

  Scenario: Activation en 2 phases (communes périphériques plus tard)
    Given une ville "anvers" activée en centrum
    When ops étend aux communes périphériques (Berchem, Deurne)
    Then une activation progressive par quartier est appliquée
    Et chaque quartier a son propre seuil Provider density

  Scenario: Chevauchement de zone (ville frontalière)
    Given une ville "Charleroi" à 50 km de "Namur" (future)
    When un User à la frontière émet une Demande
    Then le système détecte la zone chevauchement
    Et propose les 2 catalogues villes si disponibles
    Et sinon fallback "Service pas encore disponible"

  # === @security ===
  Scenario: Audit complet du launch
    When une ville est activée
    Then l'audit_log contient : city, go_live_date, provider_count, ops_id, super_admin_id
    Et le document de décision (gate validation) est attaché
    Et le log est chiffré + WORM (CyFun Basic)

  Scenario: RBAC city_launch_manager vs super_admin
    Given un ops "city_launch_manager"
    When il tente d'activer sans validation super_admin
    Then la réponse est 403 "SUPER_ADMIN_APPROVAL_REQUIRED"
    Et l'activation d'une ville = décision irréversible nécessitant 4-eyes

  Scenario: Registre APD/GBA mis à jour
    When une nouvelle ville est activée
    Then le registre APD/GBA Bruxelles est complété (pour la partie BE RBC)
    Et si ville hors RBC, le registre APD régional correspondant est créé
    Et le DPO valide la mise à jour DPIA
```

#### FR-065 — Recrutement Providers régionaux (campaign + onboarding accéléré)
- **En tant que** ops · **je veux** lancer une campagne de recrutement ciblée par ville · **afin d'** atteindre la densité critique (≥ 100 Providers BCE locaux)
- **Préconditions** : budget marketing validé ; ciblage BCE par commune ; message FR/NL/EN localisé ; landing page dédiée
- **Garanties post** : campagne créée avec KPIs (impressions, conversions, cost/Provider) ; bulk invitations (FR-049) ; suivi cohorte
- **Capacité Brief** : C14 (J14, sous-cap. E4.2)

```gherkin
Feature: Recrutement Providers régionaux

  Background:
    Given un ops "marketing_manager" authentifié MFA
    And un budget "10000 €" alloué à la campagne "Anvers Q1 2027"

  # === @happy ===
  Scenario: Campagne ciblée par commune
    Given un ciblage "BCE Anvers + secteur plomberie"
    When ops lance la campagne
    Then des ads Meta/Google ciblent 500 BCE identifiés
    Et une landing page "dep.be/pro/anvers" est servie
    Et un tracking UTM est posé (audit)

  Scenario: Onboarding accéléré pour la campagne
    Given un Provider clique l'ad
    When il démarre l'onboarding
    Then un parcours simplifié (3 étapes au lieu de 5) est proposé
    Et le KYC prioritaire est gratuit (subventionné)
    Et un ops dédié "onboarding_anvers" l'accompagne (SLA 24 h)

  # === @negative ===
  Scenario Outline: Campagne inefficace
    Given une campagne "Anvers"
    When "<metric>" est mesuré
    Then "<outcome>"
    Examples:
      | metric                       | outcome                                            |
      | conversion < 2 %             | Pause auto + review ops                            |
      | cost/Provider > 200 €        | Pause + alerte super_admin                         |
      | fraud rate (BCE fake) > 5 %  | Blocage campagne immédiat + audit                  |
      | rating nouveau Providers < 3 | Pause + investigation qualité onboarding           |

  Scenario: Budget dépassé
    Given une campagne avec budget 10 000 €
    When le spend atteint 10 000 €
    Then la campagne est automatiquement paused
    Et un email ops "BUDGET_EXHAUSTED" est envoyé
    Et aucun overspend n'est possible

  # === @edge ===
  Scenario: Pic d'inscriptions pendant campagne
    Given 200 Provider signups en 1 semaine
    When ops review la file KYC
    Then une priorisation "campagne Anvers" est appliquée
    Et un ops renfort est notifié (mitigation H-14 surcharge KYC)
    Et les SLA restent tenus (≤ 48 h review)

  Scenario: Provider déjà actif RBC, déménage à Anvers
    Given un Provider RBC avec BCE déménage à Anvers
    When il met à jour son adresse
    Then son profil est automatiquement activé sur Anvers
    Et son rating historique est conservé
    Et il apparaît dans les 2 villes (anti-friction)

  Scenario: Doublon Provider entre villes
    Given un Provider déjà invité à Bruxelles reçoit une invitation Anvers
    When il tente de créer un 2e compte
    Then le hash BSN détecte le doublon
    Et l'invitation est fusionnée (un seul compte)
    Et un warning "DUPLICATE_INVITE" est journalisé

  # === @security ===
  Scenario: Anti-fraude BCE (campaign phishing)
    Given une vague de BCE fake détectée pendant campagne
    When le rate d'échec KYC > 10 %
    Then la campagne est auto-paused
    Et une investigation anti-fraud est ouverte
    Et les comptes frauduleux sont BAN automatiquement

  Scenario: RGPD consentement marketing séparé
    Given un Provider invité via campagne
    When il finalise son onboarding
    Then il doit opt-in séparément pour marketing Klaar
    Et aucun spam cross-partenaires n'est permis
    Et un retrait de consentement est possible à tout moment (1 clic)

  Scenario: Audit financier de la campagne
    When ops consulte le ROI campagne
    Then chaque euro dépensé est tracé (ad, conversion, payout Provider)
    Et le cost/Provider est calculé (CAC Provider)
    Et le LTV Provider estimé est comparé au CAC (viabilité)
```

#### FR-066 — Tiles/routing régionaux (extension tile-server OSM + Valhalla par ville)
- **En tant que** système · **je veux** étendre le tile-server OSM et le moteur de routing Valhalla à la nouvelle ville · **afin de** garantir un matching géoloc et un ETA précis
- **Préconditions** : extract OSM régional (Geofabrik) ; tile-server déployé (sub-grid dédié ou global BE) ; tests routing OK
- **Garanties post** : tiles et routing disponibles pour la nouvelle ville ; ETA < 5 % d'erreur vs Google ; fallback Mapbox si besoin
- **Capacité Brief** : C14 (J14, sous-cap. E4.1)

```gherkin
Feature: Tiles/routing régionaux

  Background:
    Given le tile-server OSM global "Belgium" est déployé
    Et Valhalla routing est opérationnel pour la Belgique

  # === @happy ===
  Scenario: Activation routing pour Anvers
    Given un extract OSM "anvers-latest.osm.pbf" importé
    When ops lance "validate_routing --city=anvers"
    Then 50 routes tests aléatoires sont calculées en < 1 s
    Et l'ETA moyen est < 5 % d'erreur vs référence Google Maps
    Et un rapport QA est généré

  Scenario: Tiles servies en CDN
    Given un User ouvre la carte Anvers
    When il pan/zoom
    Then les tiles chargent en < 200 ms (P95)
    Et le cache CDN absorbe 90 % des hits
    Et la bande passante backend reste < 10 Mbps

  # === @negative ===
  Scenario Outline: Défaillance tiles/routing
    Given une ville "<city>"
    When "<issue>"
    Then "<outcome>"
    Examples:
      | issue                                | outcome                                          |
      | extract OSM > 30 jours (stale)       | Warning + re-extract déclenché                   |
      | Valhalla timeout > 5 s               | Fallback Mapbox API (payant, ADR-001)            |
      | tile-server en panne                 | Fallback OSM public (limité) + alerte ops         |
      | ETA > 20 % erreur vs Google          | Re-calibration Valhalla requise                  |

  Scenario: Routing incomplet (zone nouvelle non cartographiée)
    Given un quartier neuf à Anvers non dans OSM
    When un Provider tente d'y accéder
    Then le routing affiche "Zone incomplète, ETA approximatif"
    Et un warning est journalisé pour mise à jour OSM

  # === @edge ===
  Scenario: Pic de requêtes routing (campaign launch)
    Given un launch Anvers génère 1000 routing/s pendant 1 h
    When la charge est mesurée
    Then Valhalla scale horizontalement (k8s HPA)
    Et la latence reste < 1 s
    Et le CPU moyen < 70 %

  Scenario: Chevauchement routing 2 villes
    Given un trajet Bruxelles → Anvers (50 km)
    When le routing est calculé
    Then il traverse les 2 zones tiles
    Et le moteur unifie les resultats (pas de coupure)
    Et l'ETA est cohérent sur l'ensemble du trajet

  Scenario: Routing piéton vs voiture
    Given un Provider à pied et un autre en voiture
    When l'ETA est calculé
    Then 2 profils routing distincts sont utilisés
    Et l'UI affiche l'ETA correct selon le mode
    Et le matching C3 tient compte du mode (FR-012)

  # === @security ===
  Scenario: Aucune donnée User dans tiles
    When le tile-server sert les tuiles
    Then aucune PII n'est embedded
    Et les Demandes/Providers en cours ne sont pas visibles sur les tiles publiques
    Et seules les positions agrégées (heat) sont superposées au dashboard ops

  Scenario: Backup Mapbox (DRP)
    Given Valhalla en panne prolongée
    When ops active le fallback Mapbox
    Alors les coûts Mapbox sont tracés (alerte budget)
    Et l'ADR-001 (Mapbox vs OSM souveraineté) est référencé
    Et un retour à Valhalla est planifié (post-mortem)

  Scenario: Audit extract OSM (provenance)
    When un extract OSM est importé
    Then sa source (Geofabrik, date, hash SHA-256) est journalisée
    Et toute modification manuelle est tracée
    Et le registre CyFun documente la chaîne d'approvisionnement
```

#### FR-067 — Conformité régionale (déclaration APD/GBA régionale si hors RBC, TVA régionale)
- **En tant que** ops/legal · **je veux** respecter les obligations régionales hors RBC · **afin de** rester conforme lors de l'expansion géographique
- **Préconditions** : activation ville planifiée ; analyses APD/GBA régionale (Flandre, Wallonie) ; TVA BE appliquée ; DPIA étendu
- **Garanties post** : registre APD régional mis à jour ; TVA BE 21/6 % correcte ; pas de transfert international ; audit log
- **Capacité Brief** : C14 (J14, sous-cap. E4.3)

```gherkin
Feature: Conformité régionale

  Background:
    Given une ville hors RBC ciblée (ex. Anvers = Région flamande)
    Et le DPO est consulté
    Et la DPIA "Klaar-geoloc" est étendue

  # === @happy ===
  Scenario: Déclaration APD flamande (Gegevensbeschermingsautoriteit)
    Given une activation Anvers planifiée
    When le DPO soumet la déclaration au GBA flamand
    Then un numéro de registre est obtenu
    Et il est stocké dans `regulatory_registrations`
    Et un audit_log "APD_REGIONAL_REGISTERED" est créé

  Scenario: TVA BE correcte par secteur
    Given un User à Anvers paie un Devis "plomberie rénovation"
    When la facture est générée
    Then TVA 6 % s'applique (rénovation logement ≥ 5 ans, BE-wide)
    Et la mention légale flamande est ajoutée
    Et l'archivage WORM 10 ans est respecté

  # === @negative ===
  Scenario Outline: Non-conformité détectée
    Given une activation ville "<city>"
    When "<issue>"
    Then "<outcome>"
    Examples:
      | issue                                       | outcome                                          |
      | city     | APD régionale non déclarée          | 422 BLOCK_ACTIVATION + alerte DPO               |
      | anvers   | TVA incorrecte (21 % au lieu de 6 %)| Refactor facturation + audit fiscal            |
      | liege    | Données hébergées hors EU           | Blocage activation (souveraineté Brief §14)    |
      | gand     | i18n NL manquant pour certains libellés | 422 BLOCK_ACTIVATION + complète i18n        |

  Scenario: Transfert international détecté
    Given un sous-traitant cloud hors EU (ex. AWS US)
    When le DPIA l'identifie
    Then le contrat est migré vers OVHcloud EU
    Et le transfert est journalisé (RGPD Art. 44-49)
    Et le DPO valide post-migration

  # === @edge ===
  Scenario: Région frontalière (3 régions BE)
    Given un User à Bruxelles, Provider à Anvers
    When une Mission traverse 2 régions
    Then les règles les plus strictes s'appliquent (RGPD Bruxelles + GBA flamand)
    Et les 2 registres APD sont informés en cas d'incident

  Scenario: Bascule TVA en cours de Mission (taux change)
    Given une Mission en cours quand le taux TVA passe de 21 % à 23 %
    When la facture est générée
    Then le taux applicable est celui en vigueur à la date de prestation
    Et un audit fiscal documente la transition

  Scenario: Provider non-résident BE (intracommunautaire)
    Given un Provider néerlandais avec BCE BE secondaire
    When il opère à Anvers
    Then son traitement fiscal suit les règles intracommunautaires
    Et une mention TVA spécifique est appliquée
    Et le reporting MOSS n'est pas requis (B2C local BE)

  # === @security ===
  Scenario: Registres APD immuables
    Given un inspecteur APD demande historique des déclarations
    When ops génère l'export
    Then chaque déclaration est tracée (date, région, numéro, signataire)
    Et l'export est signé eIDAS
    Et les originaux sont archivés WORM 10 ans

  Scenario: DPIA étendu par ville
    When une nouvelle ville est activée
    Then le DPIA "Klaar-geoloc" est mis à jour (spécificités régionales)
    Et le document est validé par le DPO
    Et la version est journalisée dans `regulatory_documents`

  Scenario: Anti-évasion RGPD (data localisation)
    When une donnée User est créée
    Then elle est stockée sur OVHcloud BE/EU (Gravelines/Roubaix)
    Et aucun replica hors EU n'est autorisé
    Et un audit trimestriel vérifie la localisation (CyFun Basic)
```

#### FR-068 — Dashboard multi-villes (KPIs par ville, comparaison, alertes)
- **En tant que** ops admin · **je veux** un dashboard multi-villes comparatif · **afin de** piloter l'expansion géographique (KPIs par ville, comparaison, alertes)
- **Préconditions** : ≥ 2 villes activées ; données agrégées par ville (FR-062) ; RBAC `multi_city_viewer`
- **Garanties post** : vue multi-villes avec KPIs par ville ; comparaison side-by-side ; alertes sur dérive (fill rate, NPS, GMV) ; drill-down par ville
- **Capacité Brief** : C14 (J14, sous-cap. E4.3 — extension analytics)

```gherkin
Feature: Dashboard multi-villes

  Background:
    Given un ops "multi_city_viewer" authentifié MFA
    Et 2 villes activées (Bruxelles, Anvers)

  # === @happy ===
  Scenario: Vue d'ensemble multi-villes
    When ops ouvre "/admin/analytics/cities"
    Then un tableau affiche par ville : MAU, fill rate, GMV, NPS, providers actifs
    Et une sparkline 30 jours est visible par KPI
    Et la comparaison Bruxelles vs Anvers est calculée (delta %)

  Scenario: Alertes sur dérive
    Given fill rate Anvers < 40 % sur 7 jours (seuil 60 %)
    When le job alerting s'exécute
    Then un email + Slack ops "ALERT_FILL_RATE_ANVERS" est envoyé
    Et la cellule concernée dans le dashboard passe en rouge
    Et un lien "Investiguer" propose un drill-down

  # === @negative ===
  Scenario Outline: Données insuffisantes
    When ops consulte "<view>"
    Then la réponse est "<outcome>"
    Examples:
      | view                              | outcome                                          |
      | ville activée depuis < 7 jours    | "Données insuffisantes" + bouton "Activer alertes" |
      | comparaison 1 ville seule         | "Activez une 2e ville pour la comparaison"       |
      | drill-down < 100 Users            | "Drill-down bloqué (RGPD k-anonymité)"           |

  Scenario: Alerte non-actionnable (bruit)
    Given une alerte "NPS Bruxelles < 30" mais n=12 seulement
    When le seuil statistique n'est pas atteint
    Then l'alerte est masquée (anti-faux positif)
    Et un badge "n insuffisant" est affiché dans le dashboard

  # === @edge ===
  Scenario: Nouvelle ville ajoutée en cours de consultation
    Given ops consulte le dashboard
    When ops active "Liège" (FR-064)
    Then le dashboard rafraîchit et inclut Liège
    Et un message "Liège en warmup, KPIs partiels pendant 30 jours" s'affiche
    Et les comparaisons excluent Liège jusqu'à maturité

  Scenario: Comparaison trimestrielle (board reporting)
    Given un board meeting trimestriel
    When ops génère le rapport
    Then un PDF board-ready est créé (1 page par ville + 1 page comparaison)
    Et le rapport est signé eIDAS
    Et les chiffres sont extraits du data warehouse

  Scenario: Drill-down ville → commune → quartier
    Given ops clique sur "Anvers"
    When il drill-down
    Then il voit les KPIs par commune (Berchem, Deurne, etc.)
    Et par quartier (Centrum, Zurenborg)
    Et un heatmap géographique est affiché

  # === @security ===
  Scenario: K-anonymité RGPD renforcée multi-villes
    When ops consulte un sous-ensemble (1 commune, 1 semaine)
    Then si n < 100 Users, la cellule est floutée
    Et la combinaison de filtres ne permet pas la ré-identification
    Et un warning "INFERENCE_RISK" est journalisé si tentative

  Scenario: RBAC multi_city_viewer vs city_launch_manager
    Given un ops "multi_city_viewer"
    When il tente d'activer une ville depuis le dashboard
    Then la réponse est 403 "RBAC_FORBIDDEN"
    Et seul "city_launch_manager" + super_admin peuvent activer (FR-064)

  Scenario: Audit des consultations sensibles
    When ops consulte un dashboard multi-villes
    Then un audit_log "MULTI_CITY_VIEWED" est créé (ops_id, filtres, ville focus)
    Et les exports sont tracés (avec destinataire si email)
    Et le log est chiffré + WORM 13 mois

  Scenario: Anti-spying (données concurrentielles)
    Given un partenaire tiers tente d'accéder au multi-villes
    When il appelle l'endpoint
    Then la réponse est 403 "PARTNER_FORBIDDEN"
    Et seul ops interne a accès à la vue comparative
    Et aucune donnée concurrentielle inter-Providers n'est exposée
```

---

## 8. Synthèse v0.3

- **44 FR cœur** (FR-001 à FR-044, C1-C10) — inchangés depuis v0.2
- **24 FR extension** (FR-045 à FR-068, C11-C14) — NOUVEAU v0.3
- **Total** : 68 FR, tous Gherkin 4×N (happy/negative/edge/security)
- **Périmètre** : 14 capacités, roadmap 4-5 ans

### 8.1 Synthèse par module d'extension

| Module | Capacité Brief | FR | Sous-capacités CBS |
|---|---|---|---|
| **E1 — Densification secteurs** | C11 (J11) | FR-045 à FR-050 (6 FR) | E1.1-E1.6 (~76 h accéléré / 520-820 h prudent) |
| **E2' — Enhancement Tauri/PWA continu** | C12 (J12') | FR-051 à FR-055 (5 FR) | E2'.1-E2'.5 (~44 h accéléré / 100-200 h prudent, **au lieu de J12 native 1000-1600 h**) |
| **E3 — Intelligence, monétisation & ouverture** | C13 (J13) | FR-056 à FR-063 (8 FR) | E3.1-E3.5/E3.7 (~112 h accéléré / 500-810 h prudent) |
| **E4 — Expansion géographique** | C14 (J14) | FR-064 à FR-068 (5 FR) | E4.1-E4.3 (~60 h accéléré / 130-230 h prudent, par ville) |
| **Total extension v0.3** | | **24 FR** | **~232 h accéléré / 1120-1830 h prudent (+ J14 à la ville)** |

> **Chiffrage à deux branches (v0.3, aligné CBS v1.2 §Partie 2)** : la branche **accélérée** correspond aux heures réelles facturables en binôme agent IA, mesurées à la maille story (04-Epics v2.0, Epics 10-13). La branche **prudente** conserve les priors CBS v1.1 comme plafond de risque, applicable si la vélocité foyer ne se confirme pas en S1-S2. Le chiffrage communiqué expose le scénario **neutre** et jamais la seule branche accélérée (cf. concern C-7 du Validateur).

### 8.2 Synthèse consolidée PRD v0.3

| Vue | FR count | Scénarios BDD (4 classes × N) |
|---|---|---|
| MVP (FR-001 à FR-044) | 44 | ~456 scénarios (synthèse §14 originelle) |
| Extension E1 (FR-045 à FR-050) | 6 | 6 × 4 × 3 = ~72 |
| Extension E2' (FR-051 à FR-055) | 5 | 5 × 4 × 3 = ~60 |
| Extension E3 (FR-056 à FR-063) | 8 | 8 × 4 × 3 = ~96 |
| Extension E4 (FR-064 à FR-068) | 5 | 5 × 4 × 3 = ~60 |
| **Total PRD v0.3** | **68 FR** | **~744 scénarios BDD** |

> Calibrage foyer : `total stories ≈ scénarios BDD ÷ 4 tags` = **~186 stories estimées** (44 MVP + 24 extension). Cohérent avec Brief §18 (~200 stories projet complet).

### 8.3 Conformité et traçabilité (rappel)

- **RGPD** : FR-005 (effacement), FR-039 (exports), FR-045/050/053/059/060/062/067 (DPIA étendus, k-anonymité, minimisation)
- **AI Act** : FR-012 (Trace matching), FR-056 (matching IA + audit biais semestriel Art. 10-15), FR-057 (surge)
- **Platform Work** (loi BE 26 avril 2024 + directive UE 2024/2831) : Invariants §10.1-10.3 + FR-016/017/048/057 (audit price-setting, liberté prix)
- **DSP2/SCA** : FR-017/024/052/058 (3DS2 + biométrie SCA renforcée ≥ 100 €)
- **NIS2/CyFun Basic** : FR-041/042/054/066/067 (MFA ops, audit WORM, SBOM, reporting incident 24 h, souveraineté hébergement)
- **TVA BE** : FR-026/048/058/067 (21 % normal, 6 % rénovation)

### 8.4 Décisions structurantes v0.3 à valider par le superviseur

1. **Stack mobile lockée Tauri 2.0 + PWA** (pas de RN/Flutter) — Brief §16, ADR-001
2. **Module E2' substitue J12 originel** (économie de budget majeure)
3. **Gate go/no-go fill rate > 60 %** avant activation E1 (Brief §19.3)
4. **Gate rentabilité RBC > 12 mois** avant activation E4 (Brief §19.3)
5. **PWA grand public** (FR-055) déplacé depuis E3.6 vers E2'.5 (cohérent avec décision Tauri/PWA only)
6. **Surge pricing jamais prix plancher** (Invariant §10.2 respecté au niveau code)

---

## 9. Exigences non-fonctionnelles

### 9.1 Performance
- API P99 < 500 ms (read), < 1 s (write) — cible 287 req/s à maturité (Brief §18)
- Matching complet < 30 s ; Time-to-first-match < 5 min (cible 3 ans)
- WebSocket géoloc latence < 2 s
- App Tauri : cold start < 3 s, RAM < 200 Mo, binaire < 50 Mo
- Console admin : TTI < 2 s

### 9.2 Sécurité (CyFun Basic visé)
- MFA ops admin (TOTP) obligatoire
- Chiffrement at-rest (AES-256-GCM + KMS OVH) + in-transit (TLS 1.3)
- Secrets en HashiCorp Vault ou OVH KMS
- Audit log immuable (WORM S3 Object Lock)
- SBOM CycloneDX généré à chaque release (CRA)
- Security gate CI : `cargo-audit` (Rust vulns), `cargo-deny` (licences + advisories), `gitleaks` (secrets), `trivy` (images)
- Pen-test annuel externe (compensation ADR-005 closed source)

### 9.3 Disponibilité
- SLA MVP : 99,5 % (43 h/an downtime toléré)
- SLA v1 : 99,9 %
- Rollback automatique si tests déploiement échouent
- RPO < 1 h, RTO < 4 h
- Backups quotidiens + restore testé mensuellement

### 9.4 Sobriété (Manifeste §2)
- Binaires Rust distroless (< 50 Mo)
- Images multi-stage build
- Aucune dépendance spéculative (YAGNI)
- OVHcloud BE/EU datacenters (PUE < 1,3, énergie décarbonée)
- Graceful degradation (cache, queue) sous charge

### 9.5 Accessibilité
- Console admin : WCAG 2.1 AA
- App Tauri mobile : VoiceOver (iOS) + TalkBack (Android) — a11y Tauri 2.0
- Contrastes AA minimum, tailles de police ≥ 16 px

### 9.6 Conformité (mapping foyer conformite.md)
- **RGPD** : DPIA avant tracking géoloc, registre APD/GBA Bruxelles, DPO à nommer, droit d'accès/effacement (FR-005, FR-039)
- **AI Act** : Trace matching (Art. 12), supervision humaine (Art. 14), audit biais semestriel (FR-012)
- **NIS2/CyFun** : mesures techniques gates foyer, audit biennal, reporting incident 24 h
- **DSP2/SCA** : 3DS2 via Stripe
- **PCI-DSS** : scope SAQ-A via Stripe Elements
- **Platform Work** (loi BE 26 avril 2024 + directive UE 2024/2831) : invariants §10.1-10.3 + audit price-setting (FR-016)
- **CRA** : SBOM CycloneDX, reporting incident 24 h dès sept. 2026 (angle mort foyer — story habilitante dédiée Sprint 0)

## 10. Frontend UX

### 10.1 Parcours par persona

**P1 Marie (User)** :
1. Onboarding email + itsme (2 min)
2. Accueil : "De quoi avez-vous besoin ?" → 5 cartes secteurs
3. Saisie Demande (description, photos, géoloc, urgence)
4. Attente matching (< 5 min, écran live)
5. Réception Devis → acceptation (3DS2)
6. Suivi Mission (statut, géoloc, chat)
7. Validation + notation

**P2 Karim (Provider)** :
1. Onboarding BCE + assurance + itsme + Stripe Connect (3 jours ouvrés)
2. Dashboard : Demandes à proximité
3. Acceptation → envoi Devis
4. Cycle Mission (statut, photos, géoloc)
5. Payout J+2 visible

**P3 Samira (Ops)** :
1. Login MFA
2. File KYC review
3. Dashboard KPI temps réel
4. Litiges à traiter
5. Exports régulateurs

### 10.2 Arborescence pages (wireframes textuels)

**App mobile (Tauri)** :
```
/login (email, itsme)
/onboarding (User | Provider)
/home (5 cartes secteurs)
/request/new (formulaire demande)
/request/{id} (live matching + status)
/mission/{id} (chat, géoloc, photos)
/mission/{id}/rate (notation)
/profile (langue, RGPD effacement, payment methods)
/provider/dashboard (liste demandes, stats)
/provider/onboarding (KYC steps)
/provider/payouts (historique)
```

**Console admin (Astro+Svelte)** :
```
/login (MFA)
/dashboard (KPI temps réel)
/kyc/pending (file validation)
/kyc/{provider_id} (dossier détaillé)
/missions (liste filtrable)
/disputes (file médiation)
/payouts (suivi Stripe Connect)
/reconciliation (réconciliation quotidienne)
/exports (RGPD, NIS2, TVA)
/audit (audit log)
/settings (RBAC, sanctions)
```

### 10.3 i18n
- FR (default), NL, EN — toutes surfaces
- Catalog i18n chargé depuis code (pas de fetch runtime)
- Détection langue navigateur + persistance profil

## 11bis. Contrat API (OpenAPI) · *API-first : de premier rang, écrit avant le code*

### 11bis.1 Outils (matérialisation foyer `contrat-api.md`)
- **Annotation exhaustive** : macro `utoipa` (Rust) sur tous les handlers
- **Codegen client** : `openapi-typescript` (admin web) + `openapi-generator` TypeScript (Tauri mobile)
- **Désérialisation stricte** : `serde(deny_unknown_fields)` sur tous les DTOs Rust
- **Contract tests CI** : `schemathesis` run sur l'OpenAPI généré à chaque PR
- **Plan B** : si `utoipa` insuffisant, `aide` (axum-native) — ADR-004

### 11bis.2 Versioning
- URL : `/api/v1/...`
- Rupture majeure = `/api/v2/...` + maintien v1 6 mois minimum
- Rupture = point irréversible → validation humaine + ADR

### 11bis.3 Endpoints par BC (synthèse)

| BC | Méthode | Endpoint | Auth | FR lié |
|---|---|---|---|---|
| IDN | POST | `/api/v1/auth/signup` | Public | FR-001 |
| IDN | GET | `/api/v1/auth/verify-email` | Public (token) | FR-001 |
| IDN | POST | `/api/v1/auth/login` | Public | FR-004 |
| IDN | POST | `/api/v1/auth/refresh` | Refresh cookie | FR-004 |
| IDN | POST | `/api/v1/auth/logout` | Bearer | FR-004 |
| IDN | GET | `/api/v1/auth/itsme/start` | Public | FR-002 |
| IDN | GET | `/api/v1/auth/itsme/callback` | Public (state) | FR-002 |
| IDN | GET | `/api/v1/me` | Bearer | — |
| IDN | PATCH | `/api/v1/me` (locale, etc.) | Bearer | FR-043 |
| IDN | DELETE | `/api/v1/me` (RGPD) | Bearer | FR-005 |
| IDN | POST | `/api/v1/me/payment-methods` | Bearer | FR-006 |
| IDN | DELETE | `/api/v1/me/payment-methods/{pm_id}` | Bearer | FR-006 |
| IDN | POST | `/api/v1/providers/onboarding` | Bearer | FR-003 |
| CTL | GET | `/api/v1/catalog/sectors` | Bearer | FR-008 |
| CTL | GET | `/api/v1/catalog/sectors/{id}` | Bearer | FR-009 |
| MCH | POST | `/api/v1/requests` | Bearer (User) | FR-011 |
| MCH | GET | `/api/v1/requests/{id}` | Bearer | FR-011 |
| MCH | DELETE | `/api/v1/requests/{id}` | Bearer (User) | FR-014 |
| MCH | POST | `/api/v1/requests/{id}/accept` | Bearer (Provider) | FR-013 |
| MCH | POST | `/api/v1/requests/{id}/expand-radius` | Bearer (User) | FR-015 |
| INT | POST | `/api/v1/missions/{id}/quote` | Bearer (Provider) | FR-016 |
| INT | POST | `/api/v1/missions/{id}/accept-quote` | Bearer (User) | FR-017 |
| INT | POST | `/api/v1/missions/{id}/refuse-quote` | Bearer (User) | FR-017 |
| INT | PATCH | `/api/v1/missions/{id}/status` | Bearer (Provider) | FR-018 |
| INT | WS | `/api/v1/missions/{id}/track` | Bearer (User) | FR-019 |
| INT | POST | `/api/v1/missions/{id}/evidence` | Bearer (Provider) | FR-020 |
| INT | POST | `/api/v1/missions/{id}/complete` | Bearer (Provider) | FR-018 |
| INT | POST | `/api/v1/missions/{id}/validate` | Bearer (User) | FR-021 |
| INT | POST | `/api/v1/missions/{id}/cancel` | Bearer | FR-022 |
| INT | POST | `/api/v1/missions/{id}/reschedule` | Bearer | FR-023 |
| PAY | POST | `/api/v1/providers/stripe-onboarding` | Bearer (Provider) | FR-024 |
| PAY | POST | `/api/v1/webhooks/stripe` | Stripe signature | FR-028 |
| PAY | POST | `/admin/api/v1/refunds` | Bearer (Ops) | FR-027 |
| PAY | GET | `/admin/api/v1/reconciliation` | Bearer (Ops) | FR-029 |
| MSG | GET | `/api/v1/conversations/{mission_id}` | Bearer | FR-030 |
| MSG | POST | `/api/v1/conversations/{mission_id}/messages` | Bearer | FR-030 |
| MSG | POST | `/api/v1/conversations/{mission_id}/attachments` | Bearer | FR-031 |
| MSG | WS | `/api/v1/conversations/{mission_id}` | Bearer | FR-030 |
| TRU | POST | `/api/v1/missions/{id}/ratings` | Bearer | FR-033 |
| TRU | POST | `/api/v1/disputes` | Bearer | FR-034 |
| OPS | GET | `/admin/api/v1/kyc/pending` | Bearer (Ops) | FR-038 |
| OPS | POST | `/admin/api/v1/kyc/{provider_id}/review` | Bearer (Ops) | FR-038 |
| OPS | GET | `/admin/api/v1/dashboard` | Bearer (Ops) | FR-040 |
| OPS | GET | `/admin/api/v1/audit-logs` | Bearer (Ops) | FR-042 |
| OPS | POST | `/admin/api/v1/exports` | Bearer (Ops) | FR-039 |
| OPS | POST | `/admin/api/v1/sanctions` | Bearer (Ops) | FR-035 |
| OPS | POST | `/admin/api/v1/mediations/{dispute_id}` | Bearer (Ops) | FR-036 |
| OPS | POST | `/admin/api/v1/rbac/users` | Bearer (Super-Admin) | FR-041 |
| CTL | POST | `/admin/api/v1/catalog/sectors` | Bearer (Ops) | FR-010 |

**Total** : ~50 endpoints MVP.

## 11. Documentation Vivante (Test-Driven Emergence)

### 11.1 Flux critiques à couvrir en E2E (3 scénarios maîtres)

1. **Happy path complet** : User signup → itsme → Demande → Matching → Devis → Accept + Escrow → Mission cycle (EN_ROUTE → ON_SITE → COMPLETED) → Validation → Payout → Notation double-sens
2. **Litige complet** : User signup → Demande → Mission → Litige "QUALITY" → Médiation ops → Remboursement partiel → Sanction Provider → Notation
3. **Onboarding Provider complet** : Provider signup → KYC BCE → Assurance → itsme → Validation ops (4-eyes) → Stripe Connect → 1ère Demande acceptée → 1er Payout

### 11.2 Alignement BDD ↔ E2E

Chaque flux critique = 1 `.feature` BDD Gherkin = 1 test E2E (Playwright pour admin web, Maestro ou Detox pour Tauri mobile). Le test E2E **film l'exécution** (Documentation Vivante foyer) et le résultat est publié dans la doc interne.

## 12. Modèle de données (entités DDD → tables PostgreSQL)

> Archétype **stateful** → persistance requise. Décision **ORM vs CQRS SQL pur** = ADR-002 (Architecte).

### 12.1 Schéma SQL principal (synthèse)

```sql
-- BC Identity
CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email CITEXT UNIQUE NOT NULL,
  password_hash TEXT,  -- argon2id, NULL si itsme-only
  locale TEXT NOT NULL DEFAULT 'fr' CHECK (locale IN ('fr','nl','en')),
  status TEXT NOT NULL CHECK (status IN ('PENDING_EMAIL_VERIFY','ACTIVE','ERASED_PENDING','ERASED')),
  erased_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX ON users (email) WHERE erased_at IS NULL;

CREATE TABLE providers (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id),
  bce_number TEXT UNIQUE NOT NULL,
  insurance_ref TEXT NOT NULL,
  insurance_expires_at DATE NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('PENDING_OPS_REVIEW','APPROVED','REJECTED','SUSPENDED','BAN','CANCELLED')),
  stripe_account_id TEXT,
  rating_wilson FLOAT,  -- calculé par job
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE kyc_documents (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  provider_id UUID NOT NULL REFERENCES providers(id),
  type TEXT NOT NULL CHECK (type IN ('BCE_PROOF','INSURANCE','ITSME_PROOF')),
  s3_key TEXT NOT NULL,
  status TEXT NOT NULL,
  validated_by UUID REFERENCES ops_users(id),
  validated_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE sessions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id),
  refresh_token_hash TEXT NOT NULL,
  ua TEXT, ip INET,
  expires_at TIMESTAMPTZ NOT NULL,
  revoked_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE payment_methods (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id),
  stripe_pm_id TEXT NOT NULL,
  is_default BOOLEAN NOT NULL DEFAULT false,
  brand TEXT, last4 TEXT, exp_month INT, exp_year INT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- BC Catalog
CREATE TABLE sectors (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code TEXT UNIQUE NOT NULL,
  i18n_key TEXT NOT NULL,
  is_active BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE skills (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  sector_id UUID NOT NULL REFERENCES sectors(id),
  code TEXT NOT NULL,
  i18n_key TEXT NOT NULL,
  requirement_label TEXT,
  UNIQUE (sector_id, code)
);

CREATE TABLE provider_skills (
  provider_id UUID REFERENCES providers(id),
  skill_id UUID REFERENCES skills(id),
  evidence_s3_key TEXT,
  validated_at TIMESTAMPTZ,
  PRIMARY KEY (provider_id, skill_id)
);

CREATE TABLE indicative_prices (
  sector_id UUID REFERENCES sectors(id),
  p25_eur INT, p75_eur INT,  -- IQR
  sample_size INT,
  computed_at TIMESTAMPTZ,
  PRIMARY KEY (sector_id)
);

-- BC Matching (PostGIS)
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE TABLE requests (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES users(id),
  sector_id UUID NOT NULL REFERENCES sectors(id),
  description TEXT NOT NULL CHECK (length(description) BETWEEN 1 AND 2000),
  geo GEOGRAPHY(POINT, 4326) NOT NULL,
  urgency TEXT NOT NULL CHECK (urgency IN ('LOW','MEDIUM','HIGH')),
  status TEXT NOT NULL CHECK (status IN ('DRAFT','BROADCASTING','MATCHED','MISSION_CREATED','CANCELLED_USER','NO_MATCH','EXPIRED')),
  radius_m INT NOT NULL DEFAULT 5000,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX ON requests USING GIST (geo);

CREATE TABLE availabilities (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  provider_id UUID NOT NULL REFERENCES providers(id),
  geo GEOGRAPHY(POINT, 4326) NOT NULL,
  radius_m INT NOT NULL DEFAULT 5000,
  status TEXT NOT NULL CHECK (status IN ('AVAILABLE','PAUSED','BUSY')),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX ON availabilities USING GIST (geo);

CREATE TABLE matches (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  request_id UUID NOT NULL REFERENCES requests(id),
  provider_id UUID NOT NULL REFERENCES providers(id),
  score FLOAT NOT NULL,
  criteria JSONB NOT NULL,  -- pour audit AI Act
  notified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  accepted_at TIMESTAMPTZ,
  status TEXT NOT NULL CHECK (status IN ('NOTIFIED','ACCEPTED','TAKEN','DECLINED')),
  UNIQUE (request_id, provider_id)
);

-- BC Intervention
CREATE TABLE missions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  request_id UUID NOT NULL REFERENCES requests(id),
  provider_id UUID NOT NULL REFERENCES providers(id),
  status TEXT NOT NULL CHECK (status IN ('MATCHED','ACCEPTED','PROVIDER_EN_ROUTE','ON_SITE','COMPLETED','RELEASED','CANCELLED_USER','CANCELLED_PROVIDER','DISPUTED','REFUNDED','NO_MATCH')),
  escrow_id UUID,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  completed_at TIMESTAMPTZ,
  released_at TIMESTAMPTZ
);

CREATE TABLE mission_statuses (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL REFERENCES missions(id),
  status TEXT NOT NULL,
  ts TIMESTAMPTZ NOT NULL,
  geo GEOGRAPHY(POINT, 4326),
  source TEXT NOT NULL CHECK (source IN ('PROVIDER','SYSTEM','OPS'))
);

CREATE TABLE evidence_photos (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL REFERENCES missions(id),
  phase TEXT NOT NULL CHECK (phase IN ('BEFORE','AFTER')),
  s3_key TEXT NOT NULL,
  hash_sha256 TEXT NOT NULL,
  exif_ts TIMESTAMPTZ NOT NULL,
  exif_geo GEOGRAPHY(POINT, 4326),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- BC Payment
CREATE TABLE quotes (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL REFERENCES missions(id),
  amount_htva_cents BIGINT NOT NULL CHECK (amount_htva_cents > 0),
  vat_rate_bp INT NOT NULL,  -- basis points (2100 = 21%)
  vat_amount_cents BIGINT NOT NULL,
  ttl_seconds INT NOT NULL DEFAULT 3600,
  expires_at TIMESTAMPTZ NOT NULL,
  accepted_at TIMESTAMPTZ,
  refused_at TIMESTAMPTZ,
  status TEXT NOT NULL CHECK (status IN ('PENDING','ACCEPTED','REFUSED_USER','EXPIRED')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE escrows (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL REFERENCES missions(id),
  stripe_payment_intent TEXT NOT NULL,
  amount_cents BIGINT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('PRE_AUTHORIZED','CAPTURED','RELEASED','FROZEN_DISPUTE','REFUNDED_FULL','REFUNDED_PARTIAL')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  released_at TIMESTAMPTZ
);

CREATE TABLE payouts (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL REFERENCES missions(id),
  provider_id UUID NOT NULL REFERENCES providers(id),
  gross_cents BIGINT NOT NULL,
  take_cents BIGINT NOT NULL,
  net_cents BIGINT NOT NULL,
  stripe_transfer_id TEXT,
  status TEXT NOT NULL CHECK (status IN ('PENDING','EXECUTED','FAILED','FROZEN')),
  executed_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE invoices (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL REFERENCES missions(id),
  number TEXT UNIQUE NOT NULL,  -- séquentiel légal
  pdf_s3_key TEXT NOT NULL,
  signed_pdf_s3_key TEXT NOT NULL,
  period DATE NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE stripe_events (
  id UUID PRIMARY KEY,  -- == Stripe event_id, idempotence
  type TEXT NOT NULL,
  payload JSONB NOT NULL,
  processed_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- BC Messaging
CREATE TABLE conversations (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL UNIQUE REFERENCES missions(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE messages (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  conversation_id UUID NOT NULL REFERENCES conversations(id),
  author_id UUID NOT NULL REFERENCES users(id),
  body TEXT CHECK (length(body) BETWEEN 1 AND 4000),
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE attachments (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  message_id UUID NOT NULL REFERENCES messages(id),
  s3_key TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('image/jpeg','image/png','image/webp')),
  hash_sha256 TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- BC Trust
CREATE TABLE ratings (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL REFERENCES missions(id),
  author_id UUID NOT NULL REFERENCES users(id),
  target_id UUID NOT NULL,
  score INT NOT NULL CHECK (score BETWEEN 1 AND 5),
  comment TEXT CHECK (length(comment) BETWEEN 1 AND 500),
  is_public BOOLEAN NOT NULL DEFAULT false,  -- symétrie
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (mission_id, author_id)  -- 1 rating/User/Mission
);

CREATE TABLE disputes (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  mission_id UUID NOT NULL REFERENCES missions(id),
  opened_by UUID NOT NULL REFERENCES users(id),
  motive TEXT NOT NULL CHECK (motive IN ('QUALITY','PRICE','DELAY','NO_SHOW','OTHER')),
  evidence_keys JSONB NOT NULL DEFAULT '[]',
  status TEXT NOT NULL CHECK (status IN ('OPENED','UNDER_MEDIATION','RESOLVED_FULL_REFUND','RESOLVED_PARTIAL_REFUND','RESOLVED_PROVIDER_FAVOR','RESOLVED_USER_FAVOR')),
  opened_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  resolved_at TIMESTAMPTZ
);

CREATE TABLE sanctions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  provider_id UUID NOT NULL REFERENCES providers(id),
  level TEXT NOT NULL CHECK (level IN ('WARNING','SUSPENSION_7J','SUSPENSION_30J','BAN')),
  reason TEXT NOT NULL,
  is_auto BOOLEAN NOT NULL DEFAULT false,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  expires_at TIMESTAMPTZ,
  appeal_window_until TIMESTAMPTZ
);

CREATE TABLE mediations (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  dispute_id UUID NOT NULL REFERENCES disputes(id),
  ops_user_id UUID NOT NULL REFERENCES ops_users(id),
  decision TEXT NOT NULL,
  decided_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- BC Ops
CREATE TABLE ops_users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email CITEXT UNIQUE NOT NULL,
  password_hash TEXT NOT NULL,
  mfa_secret TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('kyc_reviewer','dispute_mediator','super_admin','read_only')),
  is_active BOOLEAN NOT NULL DEFAULT true,
  last_login_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE audit_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  actor_id UUID,  -- User, Provider ou Ops
  actor_type TEXT NOT NULL CHECK (actor_type IN ('USER','PROVIDER','OPS','SYSTEM')),
  action TEXT NOT NULL,
  target TEXT,
  payload JSONB,
  ip INET,
  ua TEXT,
  ts TIMESTAMPTZ NOT NULL DEFAULT NOW()
) PARTITION BY RANGE (ts);  -- partition mensuel pour scale
CREATE INDEX ON audit_logs (actor_id, ts DESC);

CREATE TABLE kpi_snapshots (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  kpi_key TEXT NOT NULL,
  value FLOAT NOT NULL,
  ts TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE regulatory_exports (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  type TEXT NOT NULL CHECK (type IN ('RGPD_ACCESS','TVA_ANNUAL','NIS2_INCIDENT','CUSTOM')),
  period_start DATE NOT NULL,
  period_end DATE NOT NULL,
  s3_key TEXT NOT NULL,
  signed_pdf_s3_key TEXT NOT NULL,
  requested_by UUID NOT NULL REFERENCES ops_users(id),
  generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### 12.2 Notes
- **PostGIS** pour `requests.geo`, `availabilities.geo`, `evidence_photos.exif_geo` (recherche < 5 km efficacement)
- **Timestamps** UTC TIMESTAMPTZ, jamais local
- **Soft delete** via `erased_at` pour RGPD
- **Audit log** dans table partitionnée par mois (scale)
- **Argent** en cents (BIGINT) — jamais FLOAT pour l'argent
- **TVA** en basis points (INT) — précision exacte
- **WORM** : `audit_logs`, `invoices`, `regulatory_exports` stockés en S3 Object Lock

## 13. Intégrations externes

| Service | Rôle | Mode | Risque lock-in |
|---|---|---|---|
| **Stripe Connect** | Paiement, Escrow, Payout, 3DS2 | API REST + webhooks signés | Moyen (PSP alt Mollie BE) |
| **itsme** | Auth eIDAS substantial | OAuth2 OIDC | Faible (standard eIDAS) |
| **KBO-BCE public API** | Vérification BCE Provider | REST (SPF Économie) | Aucun (public) |
| **OVHcloud Object Storage (S3)** | Photos, documents KYC, factures | S3 API + KMS | Faible (S3 = standard) |
| **OVHcloud KMS** | Chiffrement at-rest | KMS API | Moyen (rotation manuelle) |
| **Apple Push Notification Service** | Notifications iOS (Tauri) | HTTP/2 + JWT | Aucun (standard Apple) |
| **Firebase Cloud Messaging** | Notifications Android (Tauri) | API v1 | Faible (MVP pourrait utiliser UnifiedPush) |
| **Mapbox / OpenStreetMap** | Cartes, géocoding, routing | API REST | ADR (Mapbox payant vs OSM souverain) |
| **Sendgrid / Postmark** | Transactional emails | API REST | Faible (SMTP standard) |
| **Sentry** | Error tracking | SDK | Faible (alternatives OSS) |
| **ClamAV** | Antivirus uploads | Daemon local | Aucun (OSS) |

## 14. Contraintes et hypothèses

Repris depuis Brief §14, complétés :

- **RGPD DPIA obligatoire avant tout tracking géoloc** (story habilitante Sprint 0)
- **Audit log non-effaçable** (RGPD comptable + AI Act + CyFun)
- **TVA BE** : 21 % défaut, 6 % rénovation (preuve à valider ops), 12 % isolation
- **itsme** requis pour tout Provider (renforce KYC)
- **Stripe Connect Standard** account (contrôle KYC maximal)
- **Multilingue** toutes surfaces (Invariant §10.9)
- **Hypothèse** : 200+ Providers recrutés avant lancement public (mitigation H-4)
- **Hypothèse** : CyFun Basic atteignable dès Sprint 0 (mitigation H-5)
- **Hypothèse** : `utoipa` Rust suffisant pour OpenAPI exhaustif (à valider en Sprint 0)
- **Hypothèse** : Tauri 2.0 plugins (géoloc background, push iOS) stables (mitigation H-2/H-13 ; **v0.3 : plan B RN/Flutter retiré** — décision superviseur, fallback = PWA foreground permanent ou plugin Tauri custom)

## 15. Critères de succès MVP

Le MVP est réussi si à 12 mois post-lancement :

| Critère | Seuil |
|---|---|
| MAU RBC | ≥ 10 000 |
| Fill rate | ≥ 50 % |
| NPS post-intervention | ≥ 30 |
| 0 incident RGPD géoloc déclaré APD | ✅ |
| Conformité CyFun Basic auditée | ✅ |
| 200+ Providers actifs | ✅ |
| LTV/CAC blended | ≥ 2:1 |
| GMV mensuel | ≥ 200 k €/mois |

**Échec MVP** si l'un manque à 12 mois → gate review, pivot ou arrêt.

---

## Synthèse de la matrice 4×N

| Module | FR count | Scénarios BDD (4 classes × N scénarios) |
|---|---|---|
| IDN | 7 | 7 × 4 × 3 = ~84 |
| CTL | 3 | 3 × 4 × 2 = ~24 |
| MCH | 5 | 5 × 4 × 3 = ~60 |
| INT | 8 | 8 × 4 × 3 = ~96 |
| PAY | 6 | 6 × 4 × 3 = ~72 |
| MSG | 3 | 3 × 4 × 2 = ~24 |
| TRU | 5 | 5 × 4 × 2 = ~40 |
| OPS | 5 | 5 × 4 × 2 = ~40 |
| i18n | 2 | 2 × 4 × 2 = ~16 |
| **Total MVP** | **44 FR** | **~456 scénarios BDD** |

> Calibrage foyer : `total stories ≈ scénarios BDD ÷ 4 tags` = **~114 stories MVP**. Plus réaliste que le Brief §18 (~200 stories projet complet) qui sous-estimait la granularité des tests.

---

## Questions ouvertes pour le superviseur (à valider avant Architecte)

1. **Take-rate** : 18 % validé, mais fourchette A/B 15-22 % ?
2. **Push Tauri Mobile** : vérif maturité plugin en Sprint 0 (H-2)
3. **Mapbox vs OSM** : ADR coût vs souveraineté ( Brief §16)
4. **TVA 6 % rénovation** : workflow de validation preuve — à préciser avec le client
5. **One mission per Provider** : politique MVP stricte (FR-013), à valider ou assouplir ?
6. **Limite messages/conversation** : 100 messages MVP — suffisant ou bloquant pour cas complexes ?
7. **Auto-libération 72 h** : acceptable ou risque juridique (User peut se retourner) ?

---

*Dérivé du Manifeste Maury (CC BY-SA 4.0). Méthode Foyer. Version 0.3 — 68 FR (44 cœur + 24 extension), tous Gherkin 4×N. En attente de validation superviseur (signature humaine PENDING) avant passage à l'Architecte.*
