# Epics & User Stories — Klaar

*Livrable du Scrum Master · TOGAF Phase E · Phase 1 BMAD.
Chaque story est un tour du cercle, dimensionné sur les deux axes (taille wall-clock S/M/L → coût superviseur ; tours → coût tokens).*

```
---
projet: Klaar
persona: Scrum Master (conception)
date: 2026-07-18
version: 2.1 (extension J11-J14, J12 → J12' Tauri/PWA ; équipe corrigée 1 indépendant, durée cœur ~7 mois — sprints à re-timeboxer)
superviseur_validateur: [à valider pour passage Validateur]
signature_humaine: PENDING
brief_source: docs/bmad-livrables/01-Product-Brief.md v0.3
prd_source: docs/bmad-livrables/02-PRD.md v0.3 (68 FR)
architecture_source: docs/bmad-livrables/03-Architecture.md v0.2
---
```

> **Convention de dimensionnement foyer** :
> - **Taille** = wall-clock (coût superviseur en binôme) : **S = 0,5 j** · **M = 0,75 j** · **L = 1 j**
> - **Tours** = nombre d'itérations rouge→vert→bleu (coût tokens modèle, négligeable)
> - **DoD** = tests 4×N verts (`@happy @negative @edge @security`) + quality gate + security gate + doc vivante à jour

---

## Sprint 0 — Fondations *(la story habilitante — précède tout)*

> **Description foyer** : le Sprint 0 livre « la capacité de boucler ». Sa forme aboutie = **délivrabilité reproductible** : un `git clone` + commande agent reconstruit les 4 environnements + postes superviseur + enforcement. Aucune story métier ne peut boucler sans elle (point de concours Gantt).

### Story 0.1 — Bootstrap workspace Cargo monorepo
- **En tant que** équipe · **je veux** un workspace Cargo avec les 9 crates Domain + Application + Infra + API · **afin de** démarrer le dev
- **Critères Gherkin** : `Étant donné` un clone neuf · `Quand` je lance `make bootstrap` · `Alors` tous les crates compilent, `cargo test` passe vert sur un test smoke
- **4×N** : `@happy` build OK · `@negative` crate cassé détecté · `@edge` versions mismatch toolchain · `@security` dépendances bloquées par cargo-deny
- **Couche(s)** : IaC + CI
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : `.tool-versions` figé (Rust 1.85+) · `cargo machete` propre · `make bootstrap` idempotent

### Story 0.2 — Bootstrap PWA Astro + Svelte (ADR-010)
- **En tant que** équipe · **je veux** `web/` initialisé en Astro + Svelte, installable et fonctionnel hors-ligne · **afin de** démarrer le frontend
- **Critères Gherkin** : `Étant donné` le workspace · `Quand` je lance `make frontend` · `Alors` `web/` build sans erreur, sert un manifeste valide et enregistre son service worker
- **4×N** : `@happy` build + manifeste + SW enregistré · `@negative` build cassé détecté · `@edge` navigateur sans service worker (dégradation, pas d'erreur) · `@security` CSP stricte, aucun script tiers
- **Couche(s)** : Frontend + IaC
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : `npm run build` vert · Web App Manifest + icônes maskables · service worker enregistré · queue offline IndexedDB testée · consomme `@klaar/client`
- *Remplace le bootstrap Tauri 2.0 mobile, retiré par ADR-010. Ce qui était bloqué par l'absence de macOS et de compte développeur ne l'est plus.*

### Story 0.3 — PostgreSQL + PostGIS + migrations refinery
- **En tant que** équipe · **je veux** un PostgreSQL 16 + PostGIS sur OVH BE/EU en dev/integration · **afin de** démarrer les BC stateful
- **Critères Gherkin** : `Étant donné` `docker compose up` · `Quand` l'app démarre · `Alors` les migrations s'appliquent idempotent, extension PostGIS active
- **4×N** : `@happy` migrations OK · `@negative` migration cassée détectée · `@edge` rollback migration · `@security` secrets DB en vault
- **Couche(s)** : IaC + Infra backend
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : `refinery` embarqué dans `klaar-api` · `sqlx-cli` configuré · backup quotidien

### Story 0.4 — CI/CD pipeline (quality + security gate + tests + SBOM)
- **En tant que** équipe · **je veux** un pipeline CI complet · **afin de** garantir la définition de faite (foyer `gates.md`)
- **Critères Gherkin** : `Étant donné` une PR · `Quand` CI tourne · `Alors` fmt + clippy + cargo audit + cargo deny + gitleaks + trivy + tests + SBOM CycloneDX générés
- **4×N** : `@happy` CI verte · `@negative` clippy warning détecté · `@edge` cache Cargo cassé · `@security` secret leak détecté par gitleaks
- **Couche(s)** : CI/CD + Infra backend
- **Taille** : **L** (1 j) · **Tours** : 4
- **DoD** : CI < 10 min · hooks Git DRY avec CI · pre-commit + pre-push actifs

### Story 0.5 — Harnais contrat API (utoipa + schemathesis) — *non optionnel*
- **En tant que** équipe · **je veux** le harnais de contrat API · **afin de** garantir la matérialisation (foyer `contrat-api.md`, ADR-004)
- **Critères Gherkin** : `Étant donné` le backend · `Quand` je lance `make contract-tests` · `Alors` `utoipa` génère `openapi.json`, `schemathesis` fuzz l'API et passe
- **4×N** : `@happy` contrat généré + tests passent · `@negative` endpoint non documenté détecté · `@edge` OpenAPI 3.1 features · `@security` schémas stricts `deny_unknown_fields`
- **Couche(s)** : Infra backend + CI
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : `/api/v1/openapi.json` + `/api/v1/docs` Swagger UI · `openapi-typescript` génère `@klaar/client` · `schemathesis` en CI obligatoire

### Story 0.6 — Codegen TypeScript client partagé (`@klaar/client`)
- **En tant que** équipe · **je veux** un client TS consommé par Tauri + admin · **afin de** DRY les types API
- **Critères Gherkin** : `Étant donné` `openapi.json` · `Quand` CI publie · `Alors` `@klaar/client` package disponible
- **4×N** : `@happy` package publié · `@negative` schéma breaking détecté · `@edge` consommateurs multiples · `@security` pas de secrets dans package
- **Couche(s)** : Frontend + CI
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : package versionné semver · Tauri + admin consomment · breaking change = bump majeur

### Story 0.7a — Terraform provisioning 4 environnements (dev/integration/staging/prod)
- **En tant que** équipe · **je veux** 4 env provisionnés via Terraform · **afin de** respecter `bootstrap-delivrabilite.md`
- **Critères Gherkin** : `Étant donné` un clone neuf · `Quand` je lance `make env-staging` · `Alors` l'env staging est joignable sur OVH BE/EU
- **4×N** : `@happy` env OK · `@negative` quota OVH atteint · `@edge` multi-env parallèle · `@security` secrets en vault
- **Couche(s)** : IaC
- **Taille** : **M** (0,75 j) · **Tours** : 3

### Story 0.7b — salt-ssh durcissement CIS + idempotence
- **En tant que** équipe · **je veux** les serveurs durcis (CIS benchmark) idempotent · **afin de** respecter CyFun Basic
- **Critères Gherkin** : `Étant donné` un serveur Terraform · `Quand` salt-ssh apply · `Alors` durcissement appliqué idempotent (re-apply = 0 diff)
- **4×N** : `@happy` idempotent · `@negative` drift détecté · `@edge` multi-OS · `@security` fail2ban, auditd, SSH durci
- **Couche(s)** : IaC + Sécurité
- **Taille** : **M** (0,75 j) · **Tours** : 3

### Story 0.7c — GitOps (ArgoCD ou Flux) réconciliateur
- **En tant que** équipe · **je veux** GitOps (branche = source de vérité) · **afin de** automatiser le déploiement
- **Critères Gherkin** : `Étant donné` une branche `release/x.y` · `Quand` elle est mergée · `Alors` ArgoCD déploie en staging automatiquement
- **4×N** : `@happy` sync OK · `@negative` drift manuel détecté · `@edge` rollback automatique · `@security` protection branche
- **Couche(s)** : IaC + CI/CD
- **Taille** : **S** (0,5 j) · **Tours** : 2

### Story 0.8 — Observabilité (Prometheus + Loki + Tempo + Sentry EU)
- **En tant que** ops · **je veux** l'observabilité complète · **afin de** détecter incidents et audits
- **Critères Gherkin** : `Étant donné` une requête API · `Quand` elle s'exécute · `Alors` métrique + log + trace générés
- **4×N** : `@happy` signaux collectés · `@negative` pipeline coupé · `@edge` forte charge · `@security` PII jamais loggées
- **Couche(s)** : Infra backend + Monitoring
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : Grafana dashboards (API + DB + costs) · AlertManager règle basique · Sentry EU plugin

### Story 0.9 — Hooks Git RED-first + secrets + format
- **En tant que** équipe · **je veux** des hooks Git locaux DRY avec CI · **afin de** garantir L2 foyer (4 classes obligatoires, RED-first)
- **Critères Gherkin** : `Étant donné` un commit avec code sans test · `Quand` pre-commit s'exécute · `Alors` le commit est bloqué
- **4×N** : `@happy` commit avec test passe · `@negative` commit sans test bloqué · `@edge` fichier binaire ignoré · `@security` secret bloqué par gitleaks
- **Couche(s)** : CI/CD + enforcement
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : hooks pre-commit + pre-push installés via `make hooks` · protection de branche configurée

### Story 0.10 — SBOM CycloneDX + SLSA provenance + reporting incident 24 h
- **En tant que** ops · **je veux** SBOM + provenance + runbook incident · **afin de** respecter CRA (obligations pleines déc. 2027) et NIS2 (reporting 24 h)
- **Critères Gherkin** : `Étant donné` une release · `Quand` CI publie l'image · `Alors` SBOM CycloneDX généré + signature cosign (SLSA) + runbook `incident.md` accessible
- **4×N** : `@happy` SBOM publié · `@negative` dépendance vulnérable détectée · `@edge` chaîne multi-images · `@security` provenance vérifiable
- **Couche(s)** : CI/CD + Sécurité + Documentation
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : `cyclonedx-bom` + `cosign` en CI · runbook NIS2 testé (jeu de rôle ops)

### Story 0.11 — Tile-server OpenStreetMap + Valhalla routing (ADR-006)
- **En tant que** équipe · **je veux** tile-server OSM BE + routing Valhalla auto-hébergés OVH · **afin de** servir les cartes sans dépendance Mapbox
- **Critères Gherkin** : `Étant donné` un User ouvre la carte · `Quand` il navigue · `Alors` les tiles proviennent du tile-server OVH, routing Valhalla calcule le trajet
- **4×N** : `@happy` tiles + routing OK · `@negative` tile-server down · `@edge` MAJ données OSM hebdo · `@security` pas de fuite géoloc vers tiers
- **Couche(s)** : IaC + Infra backend
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : tile-server + Valhalla déployés · MAJ OSM BE hebdo automatisée · P99 latence tiles < 200 ms

### Story 0.12 — Web Push VAPID de bout en bout (ADR-010, amende ADR-007)
- **En tant que** Provider · **je veux** recevoir une notification push dans la PWA · **afin d'** être alerté d'une Demande sans garder l'application ouverte
- **Critères Gherkin** : `Étant donné` un navigateur abonné · `Quand` le backend émet un push · `Alors` le service worker affiche la notification et le clic ouvre la Mission ciblée
- **4×N** : `@happy` push chiffré reçu et affiché · `@negative` abonnement expiré (410 Gone) purgé côté serveur · `@edge` permission refusée, aucun abonnement créé · `@security` charge chiffrée RFC 8291, `Authorization` VAPID signée ES256, clé privée jamais exposée au client
- **Couche(s)** : Infra (`klaar-push-adapter`, Web Push) + Application (port `PushNotifier`) + Frontend (service worker `push` / `notificationclick`)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : chiffrement `aes128gcm` vérifié par vecteurs de test RFC 8291 · JWT VAPID vérifié par sa signature · purge automatique des abonnements morts
- *Remplace le PoC push Tauri, retiré par ADR-010. **Limite assumée** : sur iOS le push n'arrive qu'aux PWA ajoutées à l'écran d'accueil (iOS ≥ 16.4). Non vérifiable ici faute d'appareil ; le protocole, lui, l'est intégralement.*

**Sprint 0 total** : 12 stories · ~10,75 j wall-clock · ~44 tours

> **État au 27/08/2026.** Faites : 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.9, 0.12. Partielles
> avec limites écrites : 0.8 (pas de trace distribuée ni de Sentry), 0.10 (runbook non
> joué). **Bloquées par un prérequis absent, pas par l'effort** : 0.7a/b/c et 0.11
> nécessitent un compte OVH provisionné.
>
> ADR-010 a débloqué 0.2 et 0.12, qui l'étaient par macOS et par des comptes développeur
> payants. Deux des quatre stories restantes du Sprint 0 ont ainsi été levées par une
> décision de conception, pas par du travail supplémentaire.

---

## Epic 1 — Identity & Access (IDN) · Priorité **Must**

### Story 1.1 — Signup User (FR-001) — *faite*
- **En tant que** visiteur · **je veux** créer un compte email + password · **afin de** faire des Demandes
- **Critères Gherkin** : voir PRD FR-001
- **4×N** : PRD FR-001 (4 scénarios × 4 classes)
- **Couche(s)** : Domain (`klaar-identity` : `Utilisateur`, `MotDePasse`, `EmpreinteMotDePasse` argon2id, `JetonVerification`) + Application (ports `UtilisateurRepository`, `EnvoiCourriel`, `JournalAudit`, `Horloge` ; cas d'usage `inscrire`) + Infra (migration V3, `PgUtilisateurRepository`, `PgJournalAudit`, `CourrielJournalise`, limitation de débit) + Frontend (`/inscription`, Svelte 5)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : 4×N verts · gates · i18n FR/NL/EN · email vérification

> **Trois écarts assumés avec FR-001, détaillés dans `klaar/COMPLIANCE.md`.**
> 1. **Le `409 EMAIL_ALREADY_EXISTS` disparaît du contrat.** FR-001 le demande en
>    `@negative` et exige en `@security` une réponse indistinguable que l'adresse existe
>    ou non : les deux ne tiennent pas ensemble. L'inscription répond toujours `202`.
> 2. **Un courriel part dans les deux cas**, alors que FR-001 écrit « aucun email n'est
>    envoyé ». Sans cela, le chemin « adresse déjà prise » se reconnaît au chronomètre.
>    Le message au titulaire ne porte aucun lien.
> 3. **Le jeton de vérification est opaque et haché, pas un JWT.** Un JWT ne peut pas
>    être marqué utilisé sans l'état qu'il prétend éviter, alors que FR-001 exige la
>    non-rejouabilité.
>
> **Non fourni** : le challenge hCaptcha après trois échecs (`@security`), qui suppose un
> tiers et un appel sortant hors périmètre. La limitation de débit — 5 par heure et par
> adresse, en mémoire du processus — est la seule borne d'abus.
>
> **Amende la Story 0.12** au passage : la migration V3 pose la clé étrangère
> `push_subscription.sujet_id → utilisateur.id` que V2 annonçait sans pouvoir l'écrire,
> avec `ON DELETE CASCADE` pour que l'effacement d'un compte emporte ses abonnements.

### Story 1.2 — Email vérification par token (FR-001) — *faite*
- **En tant que** User · **je veux** vérifier mon email · **afin d'** activer mon compte
- **Critères Gherkin** : PRD FR-001 (scénario vérification)
- **4×N** : token invalide / expiré / déjà utilisé / valide
- **Couche(s)** : Application (`verifier_email`, port étendu `consommer_jeton_verification`) + Infra (transaction `FOR UPDATE`) + API (`POST /api/v1/auth/verify-email`) + Frontend (page `/verifier-email`)
- **Taille** : **S** (0,5 j) · **Tours** : 2

> **`POST` et non `GET`**, contrairement au tableau des endpoints du PRD. Les passerelles
> de messagerie d'entreprise visitent les liens des courriels avant leur destinataire pour
> les analyser : un `GET` qui consomme le jeton est consommé par l'antivirus, et
> l'utilisateur trouve un lien déjà utilisé au moment où il clique. Le lien du courriel
> ouvre la page `/verifier-email` de la PWA — statique, sans effet — qui présente ensuite
> le jeton par un `POST`. Un test e2e le vérifie en désactivant JavaScript, comme le ferait
> un tel analyseur.
>
> **Un second clic répond `200 EMAIL_ALREADY_VERIFIED`, pas une erreur.** Recharger la page
> ou rouvrir le courriel est le cas le plus banal du parcours ; y répondre par un refus
> ferait croire à un échec à quelqu'un dont le compte vient d'être activé. Le jeton reste
> consommé une seule fois, et le journal d'audit ne consigne qu'une vérification quel que
> soit le nombre de clics.
>
> Le contrôle « déjà consommé » passe **avant** le contrôle d'expiration : sinon, rouvrir
> un vieux courriel afficherait « lien expiré » à un compte actif depuis des semaines.

### Story 1.3 — Login email + password (FR-004) — *faite*
- **En tant que** User · **je veux** me login · **afin de** démarrer une session
- **4×N** : PRD FR-004 (rotation refresh, binding UA/IP)
- **Couche(s)** : Application (`connecter`, ports `EmetteurJetonAcces` et `SessionRepository`) + Infra (migration V4 `session_refresh`, `PgSessionRepository`, adaptateur `JwtHs256`) + API (`POST /api/v1/auth/login`) + Frontend (page `/connexion`)
- **Taille** : **M** (0,75 j) · **Tours** : 4

> **Adresse inconnue et mot de passe faux sont indistinguables**, réponse et
> temps compris. Une adresse inconnue économiserait la vérification argon2 et
> répondrait en une milliseconde là où un mot de passe faux en prend cinquante :
> une empreinte leurre est donc vérifiée dans le vide, avec les paramètres réels.
> « Compte non vérifié » est distingué, lui (`403`), parce que l'atteindre suppose
> déjà de connaître le bon mot de passe.
>
> **Le jeton d'accès reste en mémoire de l'onglet**, jamais dans `localStorage`
> ni `sessionStorage`, lisibles par tout script donc par une faille XSS. Le
> refresh vit en cookie `HttpOnly` `Secure` `SameSite=Lax`, de chemin restreint à
> `/api/v1/auth` : l'envoyer à chaque appel d'API l'exposerait à toute faille
> d'une autre route.
>
> **Reste à la Story 1.4** : la rotation, la détection de rejeu et le *binding*
> UA/IP que réclame `@security`. La colonne `famille_id` et le `consomme_le`
> existent déjà pour cela ; recharger la page déconnecte encore, faute de
> rafraîchissement.

### Story 1.4 — Refresh token rotatif (FR-004) — *faite*
- **En tant que** User · **je veux** un refresh rotatif · **afin de** garder ma session sans re-login
- **4×N** : refresh valide / expiré / révoqué / réutilisé (vol détecté)
- **Couche(s)** : Application (`rafraichir`, `deconnecter`) + Infra (migration V5, rotation transactionnelle `FOR UPDATE`) + API (`POST /api/v1/auth/refresh` et `/logout`) + Frontend (reprise de session au chargement, renouvellement programmé, déconnexion)
- **Taille** : **M** (0,75 j) · **Tours** : 4

> **Un refresh rejoué coupe toute sa famille.** Chaque présentation consomme le jeton et
> en rend un neuf ; le porteur légitime a donc toujours le dernier. Présenter un jeton déjà
> consommé signifie qu'une copie circule, sans qu'on puisse dire laquelle des deux mains
> est la bonne — les deux sont donc coupées. Le coût est une reconnexion, contre une
> session volée qui durerait trente jours.
>
> **Le *binding* est partiel, et c'est délibéré.** `@security` demande un lien
> « UA + IP + device ». L'agent utilisateur est lié, sous forme d'empreinte, et un
> changement lève `SESSION_CONTEXT_CHANGED` **sans couper la session** : les navigateurs
> changent d'agent à chaque mise à jour, bloquer là-dessus déconnecterait tout le monde
> toutes les quelques semaines. L'adresse IP n'est **pas** liée : un téléphone en change
> plusieurs fois par trajet en passant du wifi aux données mobiles. Le challenge itsme que
> prévoit le scénario n'est pas fourni (contrat itsme hors périmètre) ; l'anomalie est donc
> consignée sans remédiation automatique.
>
> **Ce que cela change pour l'utilisateur** : recharger la page ne déconnecte plus, et le
> jeton d'accès se renouvelle une minute avant d'expirer. La déconnexion coupe la famille
> entière, maillons déjà consommés compris — les laisser vivants rendrait la détection de
> rejeu inopérante après un `logout`.

### Story 1.5 — Auth itsme complet (FR-002)
- **En tant que** User/Provider belge · **je veux** m'authentifier itsme · **afin de** vérifier mon identité eIDAS substantial
- **4×N** : PRD FR-002 (5 scénarios)
- **Couche(s)** : Infra (itsme adapter OIDC) + Application + Frontend
- **Taille** : **L** (1 j) · **Tours** : 6
- **Dépendance** : sandbox itsme à demander (story amont)

### Story 1.6 — Onboarding Provider KYC BCE (FR-003) — *agrégat fait, KYC non fourni*
- **En tant que** Provider candidat · **je veux** soumettre BCE + assurance + Skills · **afin de** recevoir des Demandes
- **4×N** : PRD FR-003 (validation BCE, faillite, doublon)
- **Couche(s)** : Domain (Provider aggregate) + Infra (KBO-BCE API + S3 + ClamAV) + Frontend (wizard onboarding)
- **Taille** : **L** (1 j) · **Tours** : 5

> **Ce qui est vérifiable hors ligne l'est.** Le numéro BCE porte une clé de contrôle — les
> deux derniers chiffres valent `97 - (les huit premiers modulo 97)`. Cette vérification
> attrape ce qui compte le plus souvent : une faute de frappe, deux chiffres intervertis, un
> numéro inventé. Elle ne dit rien de l'existence de l'entreprise, de sa faillite ni de son
> activité, qui demandent l'API de la BCE.
>
> **Le KYC n'est pas fourni, et le type l'impose.** Un prestataire naît `PENDING_KYC` ; le
> seul chemin vers `ACTIVE` réclame une `PreuveKyc`, type opaque sans constructeur littéral.
> Il n'existe que deux façons d'en obtenir une : `depuis_verification_bce`, **qui n'a aucun
> appelant** faute d'adaptateur, et `demonstration`, dont le nom dit ce qu'elle vaut. L'origine
> est conservée en base, et la contrainte `provider_origine_coherente` interdit qu'un
> prestataire actif n'en porte aucune. Un prestataire actif sans contrôle réel se retrouve
> donc par une requête, longtemps après.
>
> **Le peuplement de démonstration est un binaire, pas un endpoint** : une commande hors
> ligne ne s'atteint pas par HTTP, alors qu'une route d'activation, même protégée, serait une
> route qu'on peut oublier d'enlever. Elle refuse de tourner sans `KLAAR_PRESTATAIRES_DEMO=1`
> et journalise ce qu'elle fait.
>
> **Non fourni** : l'attestation d'assurance (stockage objet chiffré + antivirus), le contrôle
> de faillite, le contrôle de doublon d'identité. Le champ `disponible` est un interrupteur
> simple, en attendant les plages horaires de la Story 3.7.
>
> Livre au passage la recherche par rayon que la Story 3.2 attendait : `ST_DWithin` sur une
> `geography`, filtre de compétence en `EXISTS` — joindre dupliquerait la ligne du prestataire
> par compétence, et la limite porterait sur les couples plutôt que sur les prestataires.

### Story 1.7 — Gestion méthode paiement User (FR-006)
- **En tant que** User · **je veux** enregistrer ma carte via Stripe Elements · **afin de** accélérer mes Demandes
- **4×N** : PRD FR-006
- **Couche(s)** : Infra (Stripe adapter) + Frontend (iframe Stripe Elements)
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 1.8 — Verrouillage brute-force (FR-007) — *faite*
- **En tant que** système · **je veux** verrouiller après 5 échecs · **afin de** mitiger brute-force
- **4×N** : PRD FR-007
- **Couche(s)** : Application + Infra
- **Taille** : **S** (0,5 j) · **Tours** : 2

> **Un `423` ne part qu'à qui connaît le mot de passe.** FR-007 demande `423 ACCOUNT_LOCKED`
> « correct ou non », et exige au scénario suivant qu'« aucune information ne fuit sur
> l'existence du compte » : les deux ne tiennent pas ensemble, un `423` sur une adresse au
> hasard révélant qu'elle a un compte. Le mot de passe est donc vérifié d'abord, ce qui
> coûte le même temps dans les deux cas. Un mauvais mot de passe sur un compte verrouillé
> rend exactement la même réponse qu'une adresse inconnue.
>
> **Le verrou en cours n'est jamais prolongé.** Le premier jet le repoussait à chaque
> tentative, ce qui offrait à un tiers le moyen de garder un compte fermé indéfiniment —
> l'attaque même que le verrou prétend arrêter. C'est un test qui l'a montré, le commentaire
> décrivant déjà l'intention que le code ne tenait pas. Un nouveau verrou peut en revanche
> succéder à un verrou expiré si les échecs continuent.
>
> **Une seule alerte par verrouillage**, au franchissement du seuil. Une alerte par échec
> ferait du service un relais de courriels vers une adresse non sollicitée. Un compte
> inexistant n'en déclenche aucune, pour la même raison.
>
> La limitation par adresse IP (5 par heure) tape avant le verrou (5 échecs) : depuis une
> source unique, on ne l'atteint pas. Le verrou vise l'attaque distribuée, ce que les tests
> reproduisent en variant l'adresse source.

### Story 1.9 — RGPD effacement (FR-005) — *faite pour le périmètre existant*
- **En tant que** User · **je veux** effacer mes données · **afin d'** exercer mon droit RGPD Art. 17
- **4×N** : PRD FR-005 (Mission en cours, dette, window 7 j)
- **Couche(s)** : Domain + Application + Infra (job async)
- **Taille** : **L** (1 j) · **Tours** : 5
- **Dépendance** : tous les autres BC doivent supporter l'anonymisation (consommateur de l'événement `UserErased`)

> **Ce qui est effacé, et ce qui ne peut pas encore l'être.** L'article 17 vise les données
> à caractère personnel. Celles d'un compte, dans l'état actuel du code, sont son adresse,
> l'empreinte de son mot de passe, ses jetons de vérification, ses sessions et ses
> abonnements push : toutes disparaissent. Les Missions, factures et traces de
> géolocalisation que décrit FR-005 **n'existent pas encore** — leurs bounded contexts
> arrivent aux Epics 3 et suivants, et l'effacement devra les traiter à ce moment-là. Les
> scénarios `@negative` « Mission en cours » et « dette paiement » sont donc hors d'atteinte
> pour la même raison.
>
> **L'annulation existe bien que FR-005 ne la décrive pas.** Un délai de trente jours n'a de
> raison d'être que s'il est réversible ; sans annulation, ce serait trente jours d'attente
> pour rien. Le compte reste utilisable pendant le délai, faute de quoi son titulaire ne
> pourrait pas se connecter pour annuler sa propre demande.
>
> **La ligne de compte est vidée, pas supprimée.** La supprimer emporterait par cascade les
> entrées du journal d'audit, que le scénario `@security` exige de conserver. L'adresse est
> remplacée par une valeur dérivée de l'identifiant sur le domaine `.invalid`, réservé par
> la RFC 2606 : rien ne peut y être livré, et rien ne permet de remonter à l'adresse
> d'origine.
>
> **Un défaut de concurrence trouvé par un test** : deux exécutions simultanées du job
> effaçaient le même compte deux fois, et le journal d'audit prétendait alors que le droit
> avait été exercé deux fois. La mise à jour est désormais gardée par le statut et passe en
> premier dans la transaction, ce qui sérialise les exécutions concurrentes.
>
> Livre au passage l'extracteur `Authentifie` : premier endpoint protégé, donc premier
> besoin de vérifier un jeton d'accès.

### Story 1.10 — Logout + révocation refresh — *couverte par la Story 1.4*
- **En tant que** User · **je veux** me logout · **afin de** sécuriser ma session
- **4×N** : logout nominal / refresh déjà révoqué / multi-device
- **Couche(s)** : Application + Infra
- **Taille** : **S** (0,5 j) · **Tours** : 2

**Epic 1 total** : 10 stories · ~7,5 j wall-clock · ~38 tours

---

## Epic 2 — Catalog (CTL) · Priorité **Must**

### Story 2.1 — Seed catalogue 5 secteurs MVP + i18n (FR-008) — *faite*
- **En tant que** ops · **je veux** le catalogue seed (plomberie, serrurerie, électricité, auto, livraison) · **afin de** démarrer
- **4×N** : PRD FR-008
- **Couche(s)** : Domain (Sector, Skill entities) + Infra (migration seed)
- **Taille** : **S** (0,5 j) · **Tours** : 2

> **La liste des Skills est une proposition, pas une donnée de conception.** Le PRD nomme
> les cinq secteurs ; il ne dit rien des compétences qu'ils regroupent. Les dix-huit Skills
> amorcés sont tirés des interventions de dépannage courantes à Bruxelles, et sont **à
> valider avec le métier** avant toute mise en service. Ils sont là pour que le catalogue
> existe et se teste.
>
> **Les trois traductions sont obligatoires**, en base comme dans le domaine. Bruxelles est
> officiellement bilingue : une entrée sans néerlandais n'est pas une entrée incomplète,
> c'est une entrée qui ne devrait pas exister. Un test refuse un jeu de données dont plus
> d'un dixième des néerlandais recopie le français — le symptôme habituel d'un « à compléter
> plus tard ».
>
> **L'ordre d'affichage est explicite et non alphabétique** : l'ordre alphabétique change
> d'une langue à l'autre, et le même catalogue apparaîtrait dans un ordre différent selon la
> langue choisie.

### Story 2.2 — API lecture catalogue + cache CDN (FR-008) — *faite*
- **En tant que** User · **je veux** consulter le catalogue · **afin de** choisir mon Secteur
- **4×N** : PRD FR-008 (locale fallback, rate-limit)
- **Couche(s)** : Application + Infra (actix handler)
- **Taille** : **S** (0,5 j) · **Tours** : 2

> **L'avertissement de repli est rendu au client**, et pas seulement journalisé, comme le
> demande `@negative` : c'est au client d'apprendre qu'il n'aura pas la langue qu'il a
> réclamée, pas à l'exploitant de le découvrir dans ses journaux.
>
> **L'`ETag` est calculé sur le contenu servi**, jamais sur une date de mise à jour. Un
> horodatage changerait à chaque redéploiement sans qu'une ligne du catalogue ait bougé, et
> invaliderait tous les caches pour rien. Deux langues donnent deux `ETag` distincts, sans
> quoi un cache servirait le néerlandais à qui demande le français en se croyant correct.
>
> **`Cache-Control: public`** parce que le catalogue est le même pour tout le monde. Un test
> vérifie que la réponse ne contient aucune donnée propre à celui qui l'a demandée — c'est
> la condition qui rend ce `public` légitime.
>
> **Le catalogue en maintenance répond 503 avec `Retry-After`**, via
> `KLAAR_CATALOGUE_MAINTENANCE=1`, ce qui distingue un retrait volontaire d'une panne et
> évite qu'un visiteur tombe sur un catalogue à moitié réécrit.
>
> Le limiteur accepte désormais des quotas nommés : 5 par heure pour les écritures
> sensibles, 60 par minute pour la lecture publique. Les clés restent préfixées par usage,
> sans quoi consulter le catalogue épuiserait le droit de se connecter.

### Story 2.3 — Prix indicatifs par Secteur (FR-009) — *algorithme et exposition faits, données absentes*
- **En tant que** User · **je veux** une fourchette de prix · **afin d'** estimer mon budget
- **4×N** : PRD FR-009 (IQR outliers, lancement sans data)
- **Couche(s)** : Application (job calcul IQR) + Infra
- **Taille** : **M** (0,75 j) · **Tours** : 3

> **Ce qui est livré** : le calcul IQR complet, avec son seuil d'anonymat, la table d'agrégat
> `fourchette_prix`, l'exposition par l'API et l'affichage. Le calcul reproduit l'exemple de
> FR-009 `@edge` — sur `[80, 120, 150, 200, 1000]`, la fourchette rendue est 80–200 — qui
> sert donc de vecteur de test.
>
> **Ce qui manque** : les données. Le job qui alimente la table lit l'historique des
> Missions, qui n'existe pas avant l'Epic 3. La table reste donc vide, et toutes les
> fourchettes sont absentes — ce qui est exactement le scénario `@negative` du FR : au
> lancement, « prix sur devis ». L'état livré **est** l'état attendu à ce stade.
>
> **Un garde-fou surnuméraire retiré.** Le premier jet appliquait le seuil d'anonymat aussi
> **après** exclusion des valeurs aberrantes, et contredisait alors l'exemple de FR-009
> lui-même : cinq Missions dont une aberrante n'en laissent que quatre, et le PRD attend
> pourtant une fourchette. Le seuil porte donc sur l'échantillon d'entrée. Ce que cela
> laisse subsister est écrit dans `COMPLIANCE.md` : au seuil, les bornes publiées sont deux
> prix réellement facturés sur cinq.
>
> La contrainte d'anonymat est reposée par la base (`nb_missions >= 5`), pour qu'aucun chemin
> d'écriture ne puisse la contourner. La mention « prix indicatif, prix final fixé par le
> prestataire » accompagne obligatoirement toute fourchette : sans elle, une fourchette se
> lit comme un devis, et l'écart devient un litige.

### Story 2.4 — Admin catalogue (FR-010, post-MVP ready)
- **En tant que** ops · **je veux** gérer le catalogue · **afin de** l'étendre
- **4×N** : PRD FR-010 (4-eyes principle)
- **Couche(s)** : Application + Frontend (admin web)
- **Taille** : **M** (0,75 j) · **Tours** : 3

**Epic 2 total** : 4 stories · ~2,5 j wall-clock · ~10 tours

---

## Epic 3 — Matching & Dispatch (MCH) · Priorité **Must** (cœur métier)

### Story 3.1 — Soumission Demande (FR-011) — *faite, sauf la précondition de paiement*
- **En tant que** User · **je veux** soumettre une Demande · **afin de** déclencher le matching
- **4×N** : PRD FR-011 (validations, doublon, rate-limit)
- **Couche(s)** : Domain (Request aggregate) + Application + Infra (PostGIS) + Frontend (formulaire)
- **Taille** : **L** (1 j) · **Tours** : 5

> **Le périmètre géographique est un rectangle, pas la Région.** Le contrôle `GEO_OUTSIDE_RBC`
> ramène les dix-neuf communes à un rectangle englobant, qui **sur-accepte** : Kraainem et
> Drogenbos, en Brabant flamand, y tombent. Le choix est délibéré — sur-accepter fait entrer
> quelques Demandes hors périmètre qu'un prestataire refusera, sous-accepter refuserait des
> Bruxellois chez eux. Un test constate cette sur-acceptation plutôt que de la masquer, et
> devra être inversé quand le contour réel viendra (Story 0.11, bloquée faute d'hébergement).
>
> **La précondition « méthode paiement valide » n'est pas tenue**, faute de Story 1.7. Le
> contrôle existe pourtant, avec son port, son `422` et ses tests : il est désactivable par
> `KLAAR_EXIGER_METHODE_PAIEMENT=0`, et l'est dans le déploiement vitrine. Actif par défaut —
> un contrôle de paiement qu'on oublie de rallumer est pire que pas de contrôle, parce que
> personne ne s'en aperçoit.
>
> **Le matching n'est pas déclenché** : FR-011 prévoit un job asynchrone à la création, qui
> appartient aux Stories 3.2 et 3.3. Une Demande naît `BROADCASTING` et y reste, faute de
> prestataires à qui la diffuser. La page le dit à l'utilisateur plutôt que de laisser croire
> qu'un dépanneur est en route.
>
> **Photos non fournies** (`@happy` « Demande avec photos ») : le stockage objet chiffré
> demande un compartiment provisionné, hors périmètre.
>
> Le doublon rend la Demande existante en `200`, et non un `409` : l'utilisateur veut
> retrouver la sienne, pas apprendre qu'il a cliqué deux fois. Il est cherché **avant** le
> quota horaire, sans quoi cinq double-clics se verraient refuser pour excès.

### Story 3.2 — Recherche géoloc multi-Provider (FR-012) — *faite, sans le rating*
- **En tant que** système · **je veux** trouver Providers < 5 km + Skill · **afin de** notifier
- **4×N** : PRD FR-012 (top-10, boundary 5 km, Trace AI Act)
- **Couche(s)** : Application + Infra (PostGIS KNN query) + Domain (Match entity + criteria JSONB)
- **Taille** : **L** (1 j) · **Tours** : 6

> **La réponse à l'AI Act est la signature de la fonction, pas une promesse.** `calculer` ne
> reçoit que trois nombres — distance, ancienneté du contrôle, note éventuelle. Elle ne peut
> pas voir un nom, une adresse, une langue ou une photo, parce qu'on ne les lui donne pas. Un
> biais sur un attribut protégé demanderait d'abord de changer cette signature, ce qui se voit
> à la relecture d'une ligne.
>
> **Le rating de FR-012 n'existe pas** : le bounded context Trust arrive plus tard. Il est
> traité comme **absent** et non comme nul, et son poids est redistribué — sinon un
> prestataire sans historique serait classé derrière un prestataire mal noté, aucun nouveau
> venu ne recevrait jamais rien, et le classement se figerait sur les premiers arrivés.
> L'absence est inscrite dans la ventilation, pour que la trace dise aussi ce qui manquait.
>
> **La trace conserve les écartés**, pas seulement les retenus : ne garder que les retenus la
> rendrait inutile pour la seule personne à qui elle est destinée, le prestataire qui veut
> savoir pourquoi il n'a pas été notifié. Elle est écrite **avant** que les candidats ne
> soient rendus — une notification qu'aucune trace n'explique est ce que l'AI Act interdit.
>
> **Écart avec FR-011** : le matching est lancé dans la requête, alors que le FR le décrit
> asynchrone. Il n'y a pas de file de travaux dans ce périmètre, et un binaire cadencé
> retarderait la diffusion de sa période entière — le contraire de ce qu'on veut sur un
> dépannage. Un échec de matching ne défait pas la Demande.
>
> **Non fourni** : le second tour à rayon élargi (Story 3.6, FR-015), qui donnera son sens au
> motif d'écart `HORS_RAYON` déjà prévu par la trace.

### Story 3.3 — Notification push multi-Provider — *faite*
- **En tant que** Provider · **je veux** être notifié d'une Demande à proximité · **afin de** proposer un Devis
- **4×N** : push delivered / refused / device unreachable / offline sync
- **Couche(s)** : Infra (APNs + FCM adapter)
- **Taille** : **L** (1 j) · **Tours** : 5

> **Une notification s'affiche sur un écran verrouillé**, lisible par quiconque passe à côté
> du téléphone. Elle ne porte donc ni la description du problème, ni l'adresse, ni rien du
> demandeur : seulement le secteur, la distance **arrondie** et l'urgence. Le chiffrement de
> la charge (RFC 8291) n'y change rien — il protège le transit, pas l'affichage, et les deux
> problèmes sont distincts.
>
> La distance est arrondie à la centaine de mètres sous le kilomètre : au mètre près, croisée
> avec la position du prestataire, elle situerait le demandeur chez lui.
>
> **`candidats` et `notifies` sont deux nombres distincts** dans la réponse. Un prestataire
> retenu sans abonnement push verra la Demande en ouvrant l'application ; les confondre ferait
> croire à qui attend que dix personnes ont été réveillées alors que personne n'a rien reçu.
>
> Un abonnement que le service de push déclare disparu (410) est **supprimé**, pas réessayé :
> le garder conserverait une donnée personnelle sans finalité. Une panne de transport, elle,
> n'interrompt pas le tour — les autres candidats n'y sont pour rien.
>
> **Le port `PushNotifier` est devenu asynchrone.** Il avait été défini synchrone au Sprint 0
> « pour rester utilisable par un adaptateur de test » ; il n'a jamais eu d'implémenteur, et
> l'adaptateur réel ne pouvait pas le satisfaire — un envoi push est un appel réseau.
>
> **Non vérifiable ici** : la réception effective sur un appareil, qui demande un service de
> push distant. Le protocole reste vérifié contre les vecteurs du RFC 8291 (Story 0.12).

### Story 3.4 — Acceptation Provider atomic CAS (FR-013) — *faite*
- **En tant que** Provider · **je veux** accepter une Demande · **afin de** devenir attribué
- **4×N** : PRD FR-013 (race, déjà pris, provider busy)
- **Couche(s)** : Application + Infra (Postgres atomic UPDATE...RETURNING)
- **Taille** : **M** (0,75 j) · **Tours** : 4

> **Toute la story tient dans une clause `WHERE`.** Cinq prestataires notifiés
> peuvent toucher « accepter » dans la même seconde. Lire le statut puis
> l'écrire en laisserait passer deux, et deux camionnettes partiraient pour une
> seule fuite. C'est
> `UPDATE demande SET statut='MATCHED' WHERE id=$1 AND statut='BROADCASTING' RETURNING id`
> qui tranche : PostgreSQL sérialise les écritures sur une même ligne, le second
> arrivant ré-évalue la condition après le premier, et ne voit plus rien.
> Vérifié par de vraies acceptations concurrentes, à deux puis à dix, dans
> `crates/klaar-sqlx-repos/tests/mission.rs`.
>
> **La bascule de la Demande et la création de la Mission sont une seule
> transaction.** Une Demande `MATCHED` sans Mission laisserait le demandeur
> devant un statut qui promet une intervention dont personne ne porte la trace.
>
> **« Une Mission à la fois » est tenu par un index unique partiel**, pas par un
> contrôle applicatif : vérifier puis insérer laisserait passer deux
> acceptations simultanées, c'est-à-dire exactement la course que cette story
> ferme. Le refus d'insertion défait toute la transaction, donc une Demande
> convoitée par un prestataire déjà occupé **reste diffusée** — sans quoi il
> l'éteindrait en essayant de la prendre.
>
> **L'éligibilité se vérifie à l'acceptation, pas au matching** (FR-013
> `@security`). La notification reçue il y a trois minutes ne dit rien de l'état
> présent. Le contrôle de statut vient **avant** toute lecture de la Demande :
> sinon les codes rendus à un prestataire suspendu lui diraient quelles Demandes
> existent et lesquelles sont prises.
>
> **Ajout au FR : le secteur est revérifié.** FR-013 ne le demande pas, mais la
> route est ouverte à tout prestataire actif, et un serrurier qui connaît
> l'identifiant d'une Demande de plomberie pouvait la rafler — le demandeur
> aurait vu arriver quelqu'un qui ne sait pas réparer sa fuite. Ce contrôle-là
> arrive après la lecture de la Demande, puisqu'il a besoin du secteur : un
> prestataire actif apprend donc qu'une Demande existe, ce qui ne dit rien de
> son état et ne s'exploite pas — les identifiants sont des UUID v4.
>
> **L'expiration ne se lit pas dans le statut.** Aucune tâche de fond ne fait
> basculer une Demande passé cinq minutes : elle reste `BROADCASTING` en base et
> l'expiration se constate au moment où quelqu'un tente d'agir dessus
> (`Demande::est_acceptable`). C'est une dette assumée, pas un oubli, et elle
> est écrite dans le domaine plutôt que découverte plus tard.
>
> **Le quota est compté par compte et non par adresse** (5/s, FR-013). Une
> flotte derrière une seule sortie NAT ne doit pas s'épuiser mutuellement, et
> changer d'adresse ne doit pas remettre le compteur à zéro. La fenêtre est
> courte parce que le geste l'est : un quota horaire punirait celui qui perd la
> course plusieurs fois de suite alors qu'il n'a rien fait de mal.
>
> **`MATCH_TAKEN` remplace la notification d'origine** au lieu de s'ajouter à
> elle (même `tag`), et ne nomme pas le gagnant : cela le désignerait à quatre
> autres entreprises. `autres_prevenus` est compté à part dans la réponse, pour
> ne pas faire croire que quatre personnes ont été informées quand aucune ne
> l'a été.
>
> **Hors périmètre, et à dire.** La `Mission` n'a qu'un statut, `ASSIGNED` : sa
> machine à états appartient à FR-018 et suivants. Il n'existe aucune interface
> prestataire — l'URL des notifications pointe vers une page qui reste à écrire.
> Aucun paiement n'est engagé à l'acceptation (FR-024 et suivants, bloqués par
> Stripe).

### Story 3.5 — Annulation User avant matching (FR-014) — *faite*
- **En tant que** User · **je veux** annuler ma Demande · **afin de** ne pas être facturé
- **4×N** : PRD FR-014 (annulation en course)
- **Couche(s)** : Domain + Application
- **Taille** : **S** (0,5 j) · **Tours** : 2

> **Le motif est un vocabulaire fermé, pas un texte libre.** FR-014 `@security`
> veut le motif « stocké pour analytics ». Un champ libre inviterait à écrire
> « le plombier d'hier était désagréable, j'habite au 12 rue X » : une donnée
> personnelle non sollicitée, dans un champ dont la finalité annoncée est
> statistique. Cinq codes — résolu seul, trop long, trouvé ailleurs, erreur,
> autre — servent la même analyse et ne peuvent rien laisser fuir. Un motif hors
> vocabulaire est **refusé** et non ramené sur `OTHER` : le ramener
> silencieusement ferait passer une faute de frappe du client pour un choix
> délibéré.
>
> **Le motif vit sur la Demande, pas dans le journal d'audit.** Il disparaît
> donc avec elle quand le compte est effacé (art. 17), sans qu'aucune procédure
> de purge n'ait à s'en souvenir. Dans le journal, il survivrait à
> l'effacement. Une contrainte de base impose en outre qu'un motif n'existe que
> sur une Demande annulée : sans elle, une Demande attribuée pourrait porter le
> motif d'une annulation qui n'a pas eu lieu, et l'analyse compterait des
> annulations imaginaires.
>
> **Écart au FR : 404 et non 403 pour la Demande d'autrui.** FR-014 `@negative`
> demande un 403 `FORBIDDEN`. Distinguer « elle n'existe pas » de « elle n'est
> pas à vous » laisserait apprendre quelles Demandes existent ; la précédence de
> l'anti-énumération est une décision déjà prise sur ce projet, et rendre deux
> codes différents sur deux routes de la même ressource — annulation et
> élargissement — serait de surcroît incohérent.
>
> **La course annulation/acceptation est tranchée par la base** (FR-014
> `@edge`). Les deux écritures portent sur la même ligne et sont chacune une
> seule instruction : PostgreSQL les sérialise. Si l'annulation gagne, le
> prestataire reçoit **410** — la Demande a existé et n'existe plus, ce qui
> n'est pas « quelqu'un d'autre l'a » ; si l'acceptation gagne, le demandeur est
> renvoyé vers FR-023.
>
> **Le statut reste `CANCELLED` et non `CANCELLED_USER`.** À ce stade, une
> Demande n'a qu'un annulateur possible : son auteur. Qualifier n'ajoute rien,
> et la distinction que FR-014 anticipe appartient à la Mission (FR-022,
> FR-023), où les deux parties peuvent annuler.
>
> **« Aucun paiement n'est capturé » est vrai sans effort** : aucun paiement
> n'est jamais capturé, Stripe étant hors du périmètre vitrine (Story 1.7).
>
> **L'avis envoyé aux prestataires ne dit pas pourquoi.** Le motif appartient au
> demandeur ; le porter à dix entreprises en ferait un jugement diffusé, et
> « trouvé ailleurs » se lit vite comme un reproche.

### Story 3.6 — Timeout NO_MATCH + élargir rayon (FR-015) — *faite*
- **En tant que** système · **je veux** annoncer NO_MATCH après 30 s · **afin de** garder l'User informé
- **4×N** : PRD FR-015 (élargissement max 3)
- **Couche(s)** : Application (job cron) + Domain
- **Taille** : **M** (0,75 j) · **Tours** : 3

> **Contradiction du PRD, tranchée sans arbitrage.** FR-013 `@edge` refuse une
> acceptation « après 5 min », FR-015 `@happy` annonce `NO_MATCH` « après 30 s ».
> Trente secondes l'emportent, et le choix ne coûte rien : une règle à trente
> secondes rejette **aussi** tout ce que la règle à cinq minutes rejetait, donc
> elle satisfait les deux scénarios. L'inverse est faux — attendre cinq minutes
> priverait le demandeur de la réponse que FR-015 lui promet en trente
> secondes, alors qu'il est devant une fuite.
>
> **Le délai court depuis le début du tour, pas depuis la création.** Un
> élargissement rouvre une fenêtre entière ; la faire courir depuis `cree_le`
> la rendrait déjà écoulée au moment où on l'offre. D'où la colonne
> `diffuse_depuis`, distincte de `cree_le`.
>
> **L'échelle des rayons s'arrête à vingt kilomètres parce que la Région
> s'arrête là.** 5 → 10 → 15 → 20 km : depuis n'importe quel point de la Région
> de Bruxelles-Capitale, vingt kilomètres la couvrent entièrement, et un
> quatrième élargissement n'atteindrait personne de plus. C'est ce qui borne la
> liste, et non un chiffre rond. `ELARGISSEMENTS_MAX` est **dérivé** de la
> longueur de l'échelle : les deux ne peuvent pas diverger.
>
> **Le quatrième essai annule la Demande** (FR-015 `@security`). Laisser un
> `NO_MATCH` après le refus entretiendrait l'idée que quelque chose peut encore
> arriver ; mieux vaut le dire et rendre au demandeur sa liberté d'appeler
> ailleurs.
>
> **Ajout au FR : le score se normalise sur le rayon du tour.** `calculer`
> prenait le rayon comme constante ; après un élargissement, tout candidat
> au-delà de cinq kilomètres marquait zéro de proximité et le classement du tour
> élargi n'ordonnait plus rien — précisément quand il en a le plus besoin. Le
> test de signature AI Act a échoué à l'ajout du paramètre, ce qui est
> exactement son rôle : `rayon_metres` est un paramètre du **tour**, identique
> pour tous les candidats d'un même tour, donc incapable d'en distinguer aucun.
> Un second test fixe ce raisonnement pour le prochain ajout.
>
> **Deux gardes distinctes, découvertes par les tests.** La relance est un
> compare-and-swap sur le **compteur d'élargissements** et non sur le statut :
> une Demande échue que le balayage n'a pas encore touchée est encore
> `BROADCASTING` et doit pouvoir être relancée, alors que deux clics successifs
> doivent être distingués. Et l'auto-annulation a dû quitter `changer_statut`,
> qui ne part que de `BROADCASTING`, pour un `annuler` qui part aussi de
> `NO_MATCH` — le cas exact du quatrième refus. Les deux défauts ont été
> attrapés par les tests d'intégration, pas par relecture.
>
> **Le balayage est un binaire, pas une tâche de fond.** Même raison que
> `klaar-effacer` : une tâche de fond s'exécute autant de fois qu'il y a
> d'exemplaires du serveur. `klaar-expirer` se lance toutes les dix secondes,
> et son `UPDATE … RETURNING … FOR UPDATE SKIP LOCKED` garantit qu'aucune
> Demande n'est rendue deux fois — donc qu'aucun demandeur n'est notifié deux
> fois. Vérifié par deux balayages réellement concurrents.
>
> **Un retard du balayage ne laisse rien passer.** L'expiration se constate
> aussi à la lecture (`Demande::est_acceptable`), donc aucun prestataire ne peut
> accepter une Demande échue même si le balayage n'est pas encore passé. Ce que
> le balayage apporte, c'est l'**avis** au demandeur. Cela lève la dette
> signalée en Story 3.4.
>
> **Limites assumées.** L'avis de fin de tour part en français quel que soit le
> compte : lire la langue du demandeur demanderait un dépôt de plus au binaire
> pour un message de deux lignes. Et il n'existe toujours pas d'interface : le
> bouton « élargir » que FR-015 décrit reste à écrire, la route existe.

### Story 3.7 — Disponibilité Provider ( Availability CRUD) — *faite*
- **En tant que** Provider · **je veux** gérer ma disponibilité (go/pause) · **afin de** contrôler le flux
- **4×N** : go / pause / busy auto / multi-zone
- **Couche(s)** : Domain (Availability) + Frontend
- **Taille** : **M** (0,75 j) · **Tours** : 3

> **Trois notions distinctes, et c'est tout l'enjeu.** Un prestataire peut être
> écarté du matching pour trois raisons qui n'ont rien à voir : son **statut**
> (en attente de contrôle, suspendu), sa **disponibilité** (« je suis en
> congé »), et son **occupation** (une Mission en cours). Les confondre ferait
> d'une pause une sanction, ou laisserait notifier quelqu'un qui ne peut pas
> répondre. Seule la deuxième se règle ; les deux autres s'affichent, parce
> qu'un prestataire en service et pourtant jamais sollicité conclurait sinon que
> le service est cassé.
>
> **Le « busy auto » comblait un vrai trou.** Depuis la Story 3.4, un
> prestataire déjà en Mission recevait encore des notifications qu'il ne pouvait
> qu'échouer à accepter — et il volait sa place à quelqu'un de libre. Le filtre
> est un `NOT EXISTS` dans la recherche SQL, donc posé par la base plutôt
> qu'appliqué après coup : rapatrier pour écarter ferait porter la limite des
> cent candidats examinés sur des gens inéligibles.
>
> **Le rayon d'intervention est celui du prestataire, distinct de celui du
> tour.** Le tour dit jusqu'où la Demande cherche ; celui-ci dit jusqu'où le
> prestataire accepte d'aller. Les deux s'appliquent. En SQL, le `ST_DWithin`
> élague avec l'index GIST et une comparaison `ST_Distance` affine ensuite : une
> distance qui varie d'une ligne à l'autre ne peut pas passer par l'index.
>
> **Le défaut est le maximum, pas une valeur médiane.** Les prestataires déjà en
> base n'ont jamais exprimé de limite ; leur en prêter une les retirerait du
> service sans qu'ils aient rien demandé. Vingt kilomètres couvrent la Région
> entière, donc ce défaut ne change rien au comportement observé. Le plancher à
> un kilomètre n'est pas cosmétique non plus : en dessous, un prestataire ne
> serait trouvé par presque personne et conclurait que le service ne marche pas.
>
> **`peut_etre_sollicite` a changé de sens**, et deux tests l'ont signalé. Il ne
> regardait que le statut, alors que la base filtrait déjà sur statut **et**
> disponibilité : le domaine mentait sur ce que le système faisait. Il regarde
> désormais les deux. L'occupation reste dehors — le domaine ne connaît pas les
> Missions — et c'est le cas d'usage qui la joint.
>
> **Multi-zone n'est pas livré, et le mot ne veut pas la même chose partout.**
> Dans le PRD, « zone » relève du lancement multi-villes, hors du périmètre
> vitrine. Des zones d'intervention **disjointes** — travailler à Uccle et à
> Schaerbeek mais pas entre les deux — demanderaient un modèle géographique
> autre qu'un point et un rayon. Ce qui est livré est la part actionnable :
> chacun règle sa distance.
>
> **Frontend : `/prestataire`.** Un interrupteur et un curseur. Le rayon
> s'affiche en kilomètres parce que personne ne pense en mètres pour un
> déplacement, et se transmet en mètres parce que c'est l'unité de l'API. Un
> réglage refusé remet le curseur sur la valeur réellement enregistrée : le
> laisser sur la valeur refusée ferait croire qu'elle a pris.

### Story 3.8 — Audit AI Act (Trace immuable + job audit biais semestriel) — *faite*
- **En tant que** ops · **je veux** audit anti-biais · **afin de** respecter AI Act Art. 12
- **4×N** : trace immuable / queryable / audit OK / biais détecté
- **Couche(s)** : Application + Infra (audit_logs) + Documentation
- **Taille** : **M** (0,75 j) · **Tours** : 4

> **Deux des trois axes de biais demandés ne sont pas auditables, et c'est
> volontaire.** FR-012 `@security` réclame un rapport « vérifiant l'absence de
> biais (genre, ethnie estimée, quartier) ».
>
> - Le **genre** n'est pas collecté. L'auditer supposerait de le demander,
>   c'est-à-dire de créer la donnée qui rendrait la discrimination possible.
> - L'**ethnie estimée** suppose de l'estimer, typiquement depuis un nom. C'est
>   exactement la pratique que l'AI Act et le RGPD art. 9 proscrivent : la
>   produire pour vérifier qu'on ne s'en sert pas serait absurde.
>
> La garantie sur ces deux axes est **structurelle et plus forte qu'un audit
> statistique** : `calculer` reçoit quatre nombres et rien d'autre, et ne peut
> discriminer sur un attribut qu'on ne lui donne pas. Le rapport le dit
> explicitement plutôt que de laisser une case vide.
>
> - Le **quartier**, lui, est auditable et compte vraiment. Le score est dominé
>   par la proximité, donc la qualité du service suit la densité de
>   prestataires, donc la géographie. C'est le biais réel, et c'est celui que le
>   rapport mesure : par maille d'environ un kilomètre, le nombre de Demandes,
>   le taux d'attribution, la part sans réponse, et surtout **l'écart entre la
>   maille la mieux servie et la moins bien servie** — le chiffre qui dit s'il y
>   a un problème, et qui ne se lit pas dans une liste de cent mailles.
>
> **k-anonymat, seuil à cinq.** Une maille d'un kilomètre où deux Demandes ont
> été émises désignerait des foyers. Les mailles sous le seuil sont supprimées
> et **leur nombre est annoncé** : les taire ferait passer une couverture
> partielle pour une couverture complète. Elles comptent quand même dans le
> total, le rapport ne prétendant pas que ces Demandes n'existent pas.
>
> **La signature est chaînée, et c'est ce qui la rend utile.** Un HMAC par ligne
> détecte une modification ; il ne dit rien d'une **suppression**, et supprimer
> est exactement ce que ferait quelqu'un voulant effacer un matching
> discriminatoire. Chaque ligne signe donc son contenu **et** la signature de la
> précédente. Prix payé : la tête de chaîne est verrouillée pendant l'écriture,
> donc deux tours de matching simultanés s'y sérialisent — quelques
> millisecondes, écrit plutôt que découvert.
>
> **Portée réelle de la signature.** Elle détecte une altération faite depuis la
> base ; elle ne couvre pas une compromission du serveur, où la clé est lisible
> et permet de resigner. Le WORM que FR-012 demande — stockage tiers avec verrou
> de rétention — lèverait cette limite et demande un compte d'hébergement, hors
> périmètre.
>
> **Limite opérationnelle : la chaîne est globale.** Une rotation de clé casse
> la vérification à partir du premier maillon signé avec la nouvelle. Constaté
> en vérifiant à la main — une clé étrangère donne « rompue à la ligne 66986 »,
> la bonne clé donne « 81 vérifiées, chaîne intacte ». Une rotation demandera de
> conserver l'ancienne clé pour le segment antérieur.
>
> **L'immuabilité est un déclencheur, pas une convention.** `UPDATE` et `DELETE`
> sur `trace_matching` lèvent une exception, y compris ceux venant d'un
> `ON DELETE CASCADE` : supprimer une Demande tracée échoue bruyamment plutôt
> que d'emporter sa trace en silence.
>
> **Tension assumée avec le droit à l'effacement.** L'effacement d'un compte est
> ici une anonymisation, donc aucune cascade ne se déclenche aujourd'hui. Le
> jour où quelqu'un voudra supprimer une ligne de `demande`, le déclencheur
> l'en empêchera, et ce sera la bonne réponse : l'art. 17 §3 b) réserve le cas
> des traitements imposés par une obligation légale, ce qu'est cette trace. Elle
> ne porte du reste ni nom, ni adresse, ni description — deux identifiants, un
> score et une distance.
>
> **La clé est optionnelle, contrairement au secret des jetons.** Sans elle, la
> trace est écrite **non signée** : elle explique toujours une décision, ce que
> l'AI Act exige, alors que l'absence de trace ne s'explique pas. Refuser de
> démarrer priverait le service de sa trace entière pour protéger cette trace.
> Les lignes non signées sont comptées à part dans le rapport — les ranger avec
> les vérifiées produirait un rapport rassurant sans preuve, le pire des
> résultats.
>
> **Les lignes antérieures à la migration restent non signées.** Il n'y a aucune
> façon honnête de leur fabriquer une signature après coup : la produire dirait
> qu'elles ont été scellées à l'écriture, ce qui serait faux.

**Epic 3 total** : 8 stories · ~6,75 j wall-clock · ~32 tours

---

## Epic 4 — Intervention (INT) · Priorité **Must**

### Story 4.1 — Envoi Devis Provider (FR-016)
- **En tant que** Provider attribué · **je veux** envoyer un Devis · **afin de** contractualiser
- **4×N** : PRD FR-016 (montant, délai, prix libre Invariant §10.2)
- **Couche(s)** : Domain (Quote aggregate) + Application + Infra (Stripe pre-auth) + Frontend
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 4.2 — Acceptation Devis User + Escrow capture (FR-017)
- **En tant que** User · **je veux** accepter le Devis · **afin de** déclencher la Mission
- **4×N** : PRD FR-017 (3DS2, fonds insuffisants, devis expiré)
- **Couche(s)** : Application + Infra (Stripe capture) + Frontend
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 4.3 — Machine à états Mission (FR-018) — *faite*
- **En tant que** Provider · **je veux** faire évoluer le statut Mission · **afin de** tracer
- **4×N** : PRD FR-018 (transitions valides/interdites, offline sync)
- **Couche(s)** : Domain (MissionStatus state machine) + Application + Infra (mission_statuses table)
- **Taille** : **L** (1 j) · **Tours** : 5

> **`ASSIGNED` devient `ACCEPTED`.** La Story 3.4 avait nommé l'état initial
> faute que FR-013 le nomme ; FR-018 l'appelle `ACCEPTED` dans toutes ses
> transitions. Aligner coûte une migration et évite d'entretenir un synonyme
> privé que personne ne retrouverait en lisant les deux documents.
>
> **La machine à états est une fonction totale, pas une suite de `if`.**
> `transitions_possibles` énumère ce qui est permis depuis chaque statut, et le
> `match` est exhaustif : ajouter un statut sans dire ce qu'on peut en faire ne
> compile pas. C'est ce qui évitera qu'un état apparaisse un jour sans que
> personne ne se demande d'où on y entre ni comment on en sort.
>
> **Un défaut réel comblé au passage.** Le filtre « déjà en mission » du
> matching ne connaissait que `ASSIGNED` : dès l'ajout des états
> intermédiaires, un prestataire en route aurait de nouveau reçu des Demandes.
> La liste est désormais celle de `StatutMission::occupe_le_prestataire`, et
> l'index partiel de la base suit. Cela lève la note laissée en Story 3.4 :
> `COMPLETED` et `CANCELLED` libèrent le prestataire, ce qui l'aurait sinon
> bloqué à vie.
>
> **La position est facultative, et c'est un choix.** FR-018 `@security` demande
> la géolocalisation sur chaque entrée. L'exiger rendrait l'autorisation de
> localisation de fait obligatoire, alors que quelqu'un sans GPS — cas que
> FR-019 prévoit explicitement — doit pouvoir déclarer qu'il est arrivé. Son
> absence est consignée comme telle, et `hors_zone` ne vaut jamais vrai sans
> position : ne pas savoir où quelqu'un est n'est pas la même chose que le
> savoir ailleurs.
>
> **Sortir de la Région se consigne, ne refuse pas** (FR-018 `@edge`). Un
> prestataire qui coupe par le ring reste en intervention ; c'est à
> l'exploitation d'y regarder, pas au domaine de bloquer. L'alerte est
> journalisée sans la position ni l'identifiant de Mission — le journal
> applicatif n'a pas à dire où se trouve un prestataire.
>
> **Deux horodatages, et pas un.** Celui que le client déclare et celui où le
> serveur reçoit. C'est ce qui permet à une transition faite hors connexion de
> garder sa date au lieu d'être datée du retour du réseau. La tolérance de cinq
> minutes est la borne : au-delà, ce n'est plus un décalage de synchronisation
> mais une date choisie, et une intervention pourrait se prétendre commencée une
> heure plus tôt.
>
> **La bascule et l'entrée d'historique sont une seule transaction**, avec une
> garde sur le statut de départ : deux transitions concurrentes depuis le même
> état ne peuvent pas toutes deux aboutir, sinon l'historique porterait deux
> entrées pour un seul changement. L'historique est append-only, par
> déclencheur, comme la trace de matching — une preuve qu'on peut réécrire n'en
> est pas une.
>
> **Écart au FR : 404 et non 403** pour la Mission d'autrui, par la même
> précédence anti-énumération que les routes de Demande.
>
> **Le périmètre géographique a déménagé dans le shared kernel.** Le bounded
> context Intervention en avait besoin, et la première version le dupliquait —
> ce qui aurait fait diverger deux définitions de la même frontière, et rendu un
> prestataire « hors zone » selon des bornes que le demandeur n'a jamais connues.
>
> **Non livré ici** : le WebSocket que FR-018 mentionne (Story 4.9), la
> validation de fin par le demandeur (FR-021), les pénalités d'annulation
> (FR-022). Le domaine connaît la transition vers `CANCELLED` ; aucune route ne
> l'expose, faute de la règle de pénalités qui doit l'accompagner.

### Story 4.10 — Interface prestataire et suivi demandeur — *faite, hors plan initial*
- **En tant que** Provider · **je veux** voir et prendre les Demandes qui me sont proposées · **afin de** travailler
- **En tant que** User · **je veux** suivre ma Demande · **afin de** savoir si quelqu'un vient
- **4×N** : liste / acceptation / suivi / asymétrie des vues
- **Couche(s)** : Application + Infra + Frontend
- **Taille** : **M** (0,75 j)

> **Ajoutée au plan, et pourquoi.** Jusqu'ici, aucun parcours ne pouvait être
> mené de bout en bout dans un navigateur : accepter une Demande et faire
> avancer une Mission n'existaient qu'en API. Un service dont la valeur ne se
> montre pas ne se vérifie pas non plus, et les notifications pointaient déjà
> vers `/demande?id=…`, une page qui affichait un formulaire vierge.
>
> **L'asymétrie des deux vues est le cœur de la story.** Le prestataire voit,
> avant d'accepter : le secteur, la description, l'urgence, une distance. **Pas
> l'adresse.** Elle ne lui est révélée qu'une fois la Mission à lui, parce qu'il
> doit s'y rendre. Faire l'inverse donnerait à dix entreprises l'adresse d'un
> foyer pour un dépannage que neuf d'entre elles ne feront pas. Ce n'est pas une
> consigne dans un commentaire : `VuePrestataire` **n'a pas de champ de
> position**, et un test vérifie que la réponse HTTP n'en porte aucune trace.
>
> Le demandeur, lui, apprend le **nom de l'entreprise** dès l'attribution.
> Savoir qui va sonner à sa porte est le minimum ; rien d'autre du prestataire
> n'est exposé.
>
> **Les boutons d'étape viennent du serveur.** `GET /missions/{id}` rend
> `suites`, les statuts atteignables. Recopier la machine à états dans
> l'interface la ferait diverger, et l'écran proposerait un bouton que le
> domaine refuse.
>
> **`tour_ecoule` est exposé au demandeur.** Une Demande peut être « en
> diffusion » et son tour écoulé, le balayage passant périodiquement. Afficher
> « recherche en cours » dans ce cas ferait attendre pour rien.
>
> **Le suivi sonde toutes les cinq secondes**, et s'arrête quand la Demande est
> close. C'est une dette assumée : le temps réel appartient au WebSocket de la
> Story 4.9. Un sondage court est honnête et se voit dans les journaux, là où
> une absence de rafraîchissement laisserait croire qu'il ne se passe rien.
>
> **Correction apportée à la Story 3.8 au passage.** La vérification de la
> chaîne de trace repartait toujours de l'origine. Deux conséquences : elle ne
> passerait pas à l'échelle, et une seule ligne signée avec une autre clé — une
> rotation, une vérification manuelle — casse définitivement le rejeu pour tout
> le monde. C'est arrivé sur la base de développement. `verifier_chaine` accepte
> désormais un identifiant de départ, et le rapport annonce que la portée d'une
> fenêtre est plus faible : elle prouve la cohérence de la fenêtre, pas
> qu'aucun maillon n'a disparu avant son début.

### Story 4.11 — Parcours filmés et vitrine publiée — *faite, hors plan initial*
- **En tant que** lecteur du projet · **je veux** voir le service fonctionner · **afin de** juger sur pièces
- **4×N** : narration / rythme / parcours à deux acteurs / publication
- **Couche(s)** : Frontend + CI
- **Taille** : **M** (0,75 j)

> **Une suite à part, et pas un interrupteur sur l'existante.** Les tests de
> `tests/e2e` vérifient : ils vont vite, simulent l'API et n'ont pas à être
> regardés. Ceux de `tests/demo` **montrent** : ils tournent contre le service
> réel — PostgreSQL, l'API, le navigateur —, à vitesse humaine, et leur produit
> est une vidéo. Ralentir la suite de vérification pour la filmer aurait donné
> une barrière lente et des vidéos illisibles.
>
> **Une seconde entre chaque geste, au minimum.** En dessous, l'œil ne suit
> pas : un formulaire se remplit et se soumet dans le même quart de seconde. Le
> temps d'affichage d'une narration est proportionnel à sa longueur, borné entre
> une et cinq secondes. La saisie se fait caractère par caractère, sans quoi un
> champ se remplit d'un coup et masque les validations qui réagissent à la
> frappe.
>
> **La narration est incrustée dans la page**, avec le nom de l'acteur. Sans
> elle, la vidéo montre des clics sans dire ce qu'ils démontrent ; sans
> l'étiquette, deux enregistrements côte à côte sont indéchiffrables.
>
> **Le parcours à deux acteurs est le seul qui prouve la valeur.** Ce qui compte
> n'est pas ce que chacun fait, mais ce que chacun voit *pendant* que l'autre
> agit : le prestataire qui n'a pas l'adresse avant d'accepter et l'obtient
> après, le demandeur qui apprend qui vient puis suit chaque étape sans
> rafraîchir. Deux contextes de navigateur, deux vidéos, publiées côte à côte.
>
> **Site et API sur la même origine.** Un petit serveur sans dépendance sert le
> build et relaie `/api`. Pointer le front sur un autre port aurait demandé du
> CORS — relâcher une garantie de production pour une démonstration — et
> intercepter les appels dans le navigateur aurait montré un chemin réseau qui
> n'existe pas. La troisième voie est la seule qui reproduise le déploiement
> réel, derrière un proxy inverse.
>
> **Deux quotas sont devenus paramétrables**, et c'est un chiffre et non un
> interrupteur : la limitation d'écritures sensibles par adresse et le quota
> horaire de Demandes par compte. Plusieurs parcours se connectent depuis la
> même adresse en quelques minutes, et le second quota est compté en base donc
> survit au redémarrage. Un quota qu'on peut *éteindre* finit éteint en
> production ; un chiffre annoncé au démarrage se remarque.
>
> **Deux défauts réels trouvés en filmant, qu'aucun test simulé ne pouvait
> voir.**
>
> 1. Deux îlots Svelte sur l'espace prestataire appelaient chacun
>    `restaurerSession()` au montage. Le refresh est à usage unique et sa
>    rotation détecte le rejeu : le second appel passait pour un vol, et la
>    famille de jetons entière était révoquée — comportement voulu côté serveur
>    (FR-004), catastrophique quand c'est notre propre page qui le déclenche.
>    Les appels concurrents partagent désormais une seule requête.
> 2. L'indicateur de connexion affirmait « En ligne » avant d'avoir rien
>    vérifié. Sur une page dont les scripts n'ont pas pu être chargés — premier
>    passage hors ligne — l'îlot ne s'hydrate pas et la pastille restait bloquée
>    sur « En ligne » alors que le réseau était coupé. Elle part maintenant d'un
>    état « inconnu ». Un indicateur qui ment sur le réseau est pire que pas
>    d'indicateur.
>
> **La publication est une page écrite à la main**, pas seulement le rapport
> Playwright. Ce dernier est fait pour diagnostiquer un échec : il liste des
> étapes, pas des intentions. Les deux rapports sont publiés à côté.
>
> **Un enregistrement absent est annoncé** sur la page publiée. Montrer cinq
> vidéos sur six sans le dire laisserait croire qu'il n'y en a jamais eu que
> cinq.
>
> **Limites.** La démonstration a besoin d'une base ne contenant que ses propres
> données : sur une base de développement partagée avec la suite de tests, des
> centaines de prestataires posés au centre évincent ceux de la démonstration du
> classement. En intégration continue la base est neuve, et le parcours pose sa
> Demande près de l'atelier plutôt que de modifier les données des autres. La
> géolocalisation est accordée au contexte du navigateur : aucune boîte de
> dialogue système n'est cliquée, et c'est un écart avec un usage réel.

### Story 4.4 — Tracking géoloc temps réel, foreground uniquement (FR-019)
- **En tant que** User · **je veux** voir la position Provider temps réel · **afin de** savoir quand il arrive
- **Périmètre** : tracking **foreground**, PWA ouverte pendant EN_ROUTE (`Geolocation.watchPosition`)
- **Hors périmètre, définitivement** : le suivi en arrière-plan. ADR-010 le classe *won't do* — aucune API web ne le fournit. Ce n'est plus un conditionnel sous gate de PoC, c'est une capacité que le produit n'a pas.
- **4×N** : PRD FR-019 (consentement, DPIA, purge post-Mission)
- **Couche(s)** : Frontend (Geolocation API + carte Svelte) + Infra (WebSocket actix-web-actors)
- **Taille** : **L** (1 j) · **Tours** : 6
- **Prérequis dur** : DPIA géoloc signée **avant** le premier traitement de position réel (RGPD art. 35). Voir `klaar/COMPLIANCE.md`.

### Story 4.5 — Preuves photos BEFORE/AFTER (FR-020)
- **En tant que** Provider · **je veux** prendre photos avant/après · **afin de** documenter
- **4×N** : PRD FR-020 (EXIF, hash, scan antivirus, chiffrement KMS)
- **Couche(s)** : Infra (S3 + ClamAV + KMS) + Frontend (`<input capture>` / MediaDevices)
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 4.6 — Validation fin Mission + libération Escrow (FR-021)
- **En tant que** User · **je veux** valider la fin · **afin de** libérer l'Escrow
- **4×N** : PRD FR-021 (validation manuelle / auto 72 h, > 500 € 4-eyes)
- **Couche(s)** : Application + Infra (transaction atomique SQL)
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 4.7 — Annulation Mission avec pénalités (FR-022)
- **En tant que** User/Provider · **je veux** annuler une Mission · **afin de** sortir d'engagement
- **4×N** : PRD FR-022 (pénalités, forfait déplacement, seuils fraude)
- **Couche(s)** : Domain + Application
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 4.8 — Re-programmation Mission (FR-023)
- **En tant que** User · **je veux** re-programmer · **afin de** ne pas perdre le bénéfice
- **4×N** : PRD FR-023
- **Couche(s)** : Domain + Application
- **Taille** : **S** (0,5 j) · **Tours** : 2

### Story 4.9 — WebSocket statut Mission temps réel
- **En tant que** User · **je veux** le statut live · **afin de** suivre sans refresh
- **4×N** : WebSocket OK / déconnecté / reconnect / multi-device
- **Couche(s)** : Infra (actix-web-actors) + Frontend
- **Taille** : **M** (0,75 j) · **Tours** : 4

**Epic 4 total** : 9 stories · ~7,75 j wall-clock · ~40 tours

---

## Epic 5 — Payment (PAY) · Priorité **Must**

### Story 5.1 — Stripe Connect Onboarding Provider (FR-024)
- **En tant que** Provider · **je veux** configurer Stripe Connect · **afin de** recevoir mes Payouts
- **4×N** : PRD FR-024 (KYC Stripe, IBAN, account déjà lié)
- **Couche(s)** : Infra (Stripe adapter + abstraction PaymentGateway) + Frontend
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 5.2 — Calcul Take-rate + Payout J+2 (FR-025)
- **En tant que** système · **je veux** calculer Take + verser Payout · **afin de** rémunérer
- **4×N** : PRD FR-025 (retry, IBAN clos, remboursement partiel)
- **Couche(s)** : Domain + Application + Infra (Stripe transfer)
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 5.3 — Factures TVA BE signées eIDAS (FR-026)
- **En tant que** Provider · **je veux** facture auto · **afin de** tenir ma compta TVA
- **4×N** : PRD FR-026 (TVA 21/6/12 %, credit note, archivage WORM)
- **Couche(s)** : Application + Infra (générateur PDF + eIDAS + S3 Object Lock)
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 5.4 — Remboursement total/partiel (FR-027)
- **En tant que** ops · **je veux** rembourser · **afin de** résoudre un Litige
- **4×N** : PRD FR-027 (4-eyes > 100 €, Payout exécuté)
- **Couche(s)** : Application + Infra
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 5.5 — Webhooks Stripe (signature + idempotence) (FR-028)
- **En tant que** système · **je veux** traiter webhooks Stripe idempotent · **afin de** garantir cohérence
- **4×N** : PRD FR-028 (signature, ordre inversé, retry)
- **Couche(s)** : Infra (endpoint public + signature verifier + stripe_events table)
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 5.6 — Réconciliation quotidienne (FR-029)
- **En tant que** ops · **je veux** rapport réconciliation Klaar ↔ Stripe · **afin de** détecter écarts
- **4×N** : PRD FR-029 (écart, Stripe indispo)
- **Couche(s)** : Application (job cron) + Infra
- **Taille** : **M** (0,75 j) · **Tours** : 3

**Epic 5 total** : 6 stories · ~5,25 j wall-clock · ~26 tours

---

## Epic 6 — Messaging (MSG) · Priorité **Should**

### Story 6.1 — Conversation in-app User ↔ Provider (FR-030)
- **En tant que** User/Provider · **je veux** échanger messages · **afin de** préciser la Demande
- **4×N** : PRD FR-030 (> 4000 chars, Mission close, offline sync)
- **Couche(s)** : Domain + Application + Infra (actix-web-actors WebSocket) + Frontend
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 6.2 — Photos dans conversation (FR-031)
- **En tant que** User · **je veux** envoyer photos · **afin de** montrer le problème
- **4×N** : PRD FR-031 (> 5 Mo, EXIF strippé, quota 10)
- **Couche(s)** : Infra (S3 + ClamAV) + Frontend
- **Taille** : **M** (0,75 j) · **Tours** : 3

### Story 6.3 — Anti-circumvention (FR-032)
- **En tant que** système · **je veux** bloquer phone/email dans messages · **afin de** empêcher la mise en relation hors plateforme
- **4×N** : PRD FR-032 (regex sophistiquée, faux positifs)
- **Couche(s)** : Application + Domain (modération IA légère)
- **Taille** : **M** (0,75 j) · **Tours** : 4

**Epic 6 total** : 3 stories · ~2,5 j wall-clock · ~12 tours

---

## Epic 7 — Trust & Moderation (TRU) · Priorité **Must**

### Story 7.1 — Notation double-sens symétrique (FR-033)
- **En tant que** User/Provider · **je veux** noter · **afin d'** aider la communauté
- **4×N** : PRD FR-033 (double-sens, > 14 j, déjà noté)
- **Couche(s)** : Domain + Application + Frontend
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 7.2 — Ouverture Litige (FR-034)
- **En tant que** User/Provider · **je veux** ouvrir Litige · **afin de** contester
- **4×N** : PRD FR-034 (> 14 j, motif vide, fraude 2/semaine)
- **Couche(s)** : Domain (Dispute aggregate) + Application
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 7.3 — Sanction auto + manuelle (FR-035)
- **En tant que** ops/système · **je veux** appliquer Sanction · **afin de** protéger la communauté
- **4×N** : PRD FR-035 (seuils, appel, 4-eyes BAN)
- **Couche(s)** : Domain + Application
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 7.4 — Médiation ops workflow (FR-036)
- **En tant que** ops · **je veux** médiater un Litige · **afin de** trancher
- **4×N** : PRD FR-036 (timeout 7 j, escalade 30 j)
- **Couche(s)** : Application + Frontend (admin médiation UI)
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 7.5 — Rating Wilson pondéré (FR-037)
- **En tant que** système · **je veux** calcul Wilson score · **afin de** éviter le biais faible échantillon
- **4×N** : PRD FR-037
- **Couche(s)** : Application (job)
- **Taille** : **S** (0,5 j) · **Tours** : 2

**Epic 7 total** : 5 stories · ~3,5 j wall-clock · ~18 tours

---

## Epic 8 — Ops & Admin (OPS) · Priorité **Must**

### Story 8.1 — KYC review console (FR-038)
- **En tant que** ops · **je veux** valider KYC Providers · **afin de** sécuriser la plateforme
- **4×N** : PRD FR-038 (4-eyes, motif, Provider annule)
- **Couche(s)** : Application + Frontend (admin web console)
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 8.2 — Exports régulateurs RGPD/NIS2/TVA (FR-039)
- **En tant que** ops · **je veux** générer exports signés · **afin de** répondre aux autorités
- **4×N** : PRD FR-039 (période, > 100k lignes asynchrone)
- **Couche(s)** : Application + Infra (PGP + eIDAS signature)
- **Taille** : **L** (1 j) · **Tours** : 5

### Story 8.3 — Dashboard temps réel KPI (FR-040)
- **En tant que** ops · **je veux** dashboard · **afin de** piloter
- **4×N** : PRD FR-040 (backend down, empty state, RBAC)
- **Couche(s)** : Application + Frontend (Svelte 5 dashboard)
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 8.4 — RBAC ops + MFA TOTP (FR-041)
- **En tant que** super-admin · **je veux** gérer ops users + rôles · **afin de** sécuriser admin
- **4×N** : PRD FR-041 (MFA, auto-révocation 90 j)
- **Couche(s)** : Domain + Application + Frontend
- **Taille** : **M** (0,75 j) · **Tours** : 4

### Story 8.5 — Audit log consultable + immuable (FR-042)
- **En tant que** ops · **je veux** consulter audit log · **afin d'** auditer
- **4×N** : PRD FR-042 (WORM, recherche, > 10M lignes)
- **Couche(s)** : Infra (table partitionnée mensuellement) + Frontend
- **Taille** : **M** (0,75 j) · **Tours** : 4

**Epic 8 total** : 5 stories · ~4,25 j wall-clock · ~22 tours

---

## Epic 9 — i18n · Priorité **Must**

### Story 9.1 — i18n FR/NL/EN toutes surfaces (FR-043)
- **En tant que** User · **je veux** choisir ma langue · **afin d'** utiliser l'app confortablement
- **4×N** : PRD FR-043
- **Couche(s)** : Frontend (catalogues compilés) + Backend (emails)
- **Taille** : **M** (0,75 j) · **Tours** : 3

### Story 9.2 — i18n factures + emails (FR-044)
- **En tant que** User/Provider · **je veux** docs dans ma langue · **afin de** comprendre
- **4×N** : PRD FR-044 (mix destinataires)
- **Couche(s)** : Application + Infra (template PDF multilingue)
- **Taille** : **M** (0,75 j) · **Tours** : 3

**Epic 9 total** : 2 stories · ~1,5 j wall-clock · ~6 tours

---

## Epic 10 — E1 Densification secteurs (C11, J11) · Priorité **Post-MVP**

> Activable lorsque le gate **fill rate > 60 %** sur les 5 secteurs pilotes est franchi. Onboarding séquentiel : 1 secteur à la fois, max 2 par an (mitigation H-14). Sous-capacités CBS E1.1-E1.6 — `00-Capability-Breakdown-Estimation.md` §Partie 2 · J11. Référence PRD §7 module E1 (FR-045 à FR-050).

### Story 10.1 — Moteur Skills & attestations réglementées (FR-045)
- **En tant que** Provider candidat sur secteur réglementé · **je veux** attester mes agréments légaux · **afin de** respecter l'Invariant §10.8 (pas d'Intervention sans agrément valide)
- **Critères Gherkin** : voir PRD FR-045
- **4×N** : PRD FR-045 (submission PDF, validation, 2e compétence, échecs, edge fédération, anti-falsification)
- **Couche(s)** : Domain (`Skill`, `SkillAttestation` aggregates) + Application (`SubmitSkillAttestationHandler`) + Infra (`klaar-skills` crate, S3 + ClamAV + KMS) + Frontend (wizard attestation)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : tests 4×N verts · hash SHA-256 WORM · i18n FR/NL/EN · doc vivante (DPIA sectoriel)

### Story 10.2 — Attestation B2V/VR électricité (FR-045)
- **En tant que** Provider électricien · **je veux** soumettre mon B2V/VR · **afin d'** opérer sur le secteur réglementé électricité
- **Critères Gherkin** : PRD FR-045 (scénarios `B2V-2026-12345`, fédération AIB-Vincotte)
- **4×N** : happy B2V valide · negative format/expiré · edge fédération indispo · security anti-corruption BCE↔attestation
- **Couche(s)** : Infra (`klaar-authority-adapter` AIB-Vincotte) + Application + Domain
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : adapter fédération mocké en integration · cross-check job nightly

### Story 10.3 — Attestation plomberie gaz BE (FR-045)
- **En tant que** plombier · **je veux** attester mon agréation gaz naturel PEB · **afin de** respecter la réglementation gaz BE
- **Critères Gherkin** : PRD FR-045 (scénario `agreation_gaz_PEB`)
- **4×N** : happy PEB · negative numéro invalide · edge renouvellement · security hash
- **Couche(s)** : Infra (adapter fédération gaz PEB) + Domain
- **Taille** : **S** (0,5 j) · **Tours** : 2
- **DoD** : réutilise harnais Story 10.1 · i18n

### Story 10.4 — Attestation chauffage Class1 (FR-045)
- **En tant que** chauffagiste · **je veux** attester ma certification Class1 · **afin d'** étendre mon activité chauffage
- **Critères Gherkin** : PRD FR-045 (extension 2e compétence, rating conservé)
- **4×N** : happy Class1 · negative expiré · edge secteur combiné · security
- **Couche(s)** : Infra (adapter fédération Class1) + Domain
- **Taille** : **S** (0,5 j) · **Tours** : 2
- **DoD** : réutilise harnais Story 10.1

### Story 10.5 — Onboarding multi-secteur (FR-046)
- **En tant que** Provider actif · **je veux** étendre à un nouveau secteur · **afin de** diversifier sans recommencer le KYC de base
- **Critères Gherkin** : PRD FR-046
- **4×N** : PRD FR-046 (extension réglementé/non-réglementé, blocages, parallèle, réactivation, anti-circumvention)
- **Couche(s)** : Domain (`ProviderSkill`) + Application (`ExtendProviderToSector`) + Infra + Frontend (wizard extension secteur)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : workflow 4-eyes ops · audit_log par secteur · quota 5 secteurs

### Story 10.6 — Règles KYC par Skill (FR-046)
- **En tant que** système · **je veux** exiger le KYC additionnel spécifique au Skill visé · **afin de** respecter les exigences réglementaires par secteur
- **Critères Gherkin** : PRD FR-046 (scénario secteur réglementé déclenche FR-045)
- **4×N** : happy KYC minime bricolage · negative BASE_KYC_EXPIRED · edge simultané · security anti-resurrection
- **Couche(s)** : Domain (`KycRequirements` per Skill) + Application
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : règles configurables ops · doc vivante par secteur

### Story 10.7 — Catalogue extensible ops (FR-047)
- **En tant que** ops admin · **je veux** ajouter un nouveau secteur au catalogue · **afin de** déployer dans de nouveaux domaines avec gouvernance 4-eyes
- **Critères Gherkin** : PRD FR-047
- **4×N** : PRD FR-047 (création 4-eyes, validations, rollback, période pic, RBAC, intégrité référentielle)
- **Couche(s)** : Domain (`Sector` aggregate étendu) + Application (`AddSectorToCatalog`) + Frontend (admin web catalogue UI) + Infra (audit WORM)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : 4-eyes enforced · gate fill rate check · soft delete seulement

### Story 10.8 — i18n par défaut nouveaux secteurs (FR-047)
- **En tant que** User · **je veux** les libellés secteur dans ma langue · **afin de** naviguer confortablement
- **Critères Gherkin** : PRD FR-047 (scénario libellés FR/NL/EN par défaut, fallback EN)
- **4×N** : happy 3 locales · negative libellé manquant · edge locale non couverte · security
- **Couche(s)** : Infra (i18n catalogue compilé) + Frontend
- **Taille** : **S** (0,5 j) · **Tours** : 2
- **DoD** : validation libellés FR+NL+EN obligatoire à la création secteur

### Story 10.9 — Calibration prix IQR bootstrapping (FR-048)
- **En tant que** ops admin · **je veux** initialiser et recalibrer les prix indicatifs par secteur · **afin d'** informer les Users sans imposer de prix (Invariant §10.2)
- **Critères Gherkin** : PRD FR-048
- **4×N** : PRD FR-048 (IQR nominale, bootstrap manuel, concentration risque, surge transitoire, override, TVA, anti-manipulation)
- **Couche(s)** : Application (`CalibrateIndicativePrices` job nightly) + Infra (`indicative_prices` table)
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : médiane mobile 90 j · outliers exclus · UI "prix indicatif, non contractuel" · audit WORM

### Story 10.10 — Bulk import BCE + skills mapping (FR-049)
- **En tant que** ops admin · **je veux** importer en masse des Providers BCE · **afin d'** accélérer la densification d'un nouveau secteur (mitigation H-4)
- **Critères Gherkin** : PRD FR-049
- **4×N** : PRD FR-049 (CSV nominal, complétion onboarding, lignes invalides, doublons, token expiré, consentement RGPD, rate-limit)
- **Couche(s)** : Application (`BulkImportProviders`) + Infra (CSV parser + email sender + KBO-BCE) + Frontend (admin import UI)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : invitations JWT 7 j · consentement explicite · mapping skills audit log

### Story 10.11 — Cross-check automatique BCE/INASTI/fédérations (FR-050)
- **En tant que** système · **je veux** vérifier quotidiennement agréments et BCE · **afin de** détecter expirations/radiations sans attendre un incident
- **Critères Gherkin** : PRD FR-050
- **4×N** : PRD FR-050 (vérification OK, rappel ≤ 30 j, anomalies, suspension auto, API KBO-BCE down, kill-switch, privacy by design)
- **Couche(s)** : Application (`VerifySkillAttestation`, `AutoExpireSkillAttestations` jobs nightly) + Infra (adapters KBO-BCE, INASTI, fédérations) + Domain
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : job nightly 03:00 UTC · 200 Providers vérifiables · kill-switch super_admin · DPIA sectoriel

### Story 10.12 — Audit AI Act extension secteurs (FR-050)
- **En tant que** ops/DPO · **je veux** auditer les décisions d'attestation · **afin de** respecter AI Act Art. 12 et Platform Work
- **Critères Gherkin** : PRD FR-050 (scénarios journalisation immuable, kill-switch, privacy by design)
- **4×N** : happy audit consultable · negative modification tentative · edge fraude fédération · security WORM 5 ans
- **Couche(s)** : Infra (audit_logs partitionné) + Documentation (DPIA sectoriel vivant)
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : export DPO signé eIDAS · doc vivante à jour · k-anonymité

**Epic 10 total** : 12 stories · ~9,5 j wall-clock · ~40 tours

---

## Epic 11 — E2' Enhancement PWA (C12, J12') · Priorité **Post-MVP**

> Remplace le jalon J12 originel « Native premium RN/Flutter ». **ADR-010 retire Tauri** : le client est une PWA Astro + Svelte, et cet epic est re-spécifié en conséquence. Deux des sept stories changent de nature, une disparaît. Référence PRD §7 module E2' (FR-051 à FR-055) — les FR concernés sont à re-rédiger au prochain passage sur le PRD, cet epic fait foi entre-temps.
>
> | FR | ADR-008 (Tauri) | ADR-010 (PWA) |
> |---|---|---|
> | FR-051 push enrichi | plugins Tauri, parité APNs | Web Push, **sans actions inline sur iOS** |
> | FR-052 secure storage + biométrie | `biometric` + Stronghold | **WebAuthn** : couvre l'auth, **pas** le stockage chiffré de secrets |
> | FR-053 géoloc background | conditionnel, sous gate PoC | **won't do** |
> | FR-054 re-submission stores | Tauri Updater + stores | **sans objet**, story retirée |
> | FR-055 PWA grand public | surface secondaire | **c'est le produit** |

### Story 11.1 — Push rich media + actions inline (FR-051)
- **En tant que** User/Provider · **je veux** des notifications enrichies avec actions inline (accepter/refuser un Devis en 1 tap) · **afin de** réagir sans ouvrir l'app
- **Critères Gherkin** : PRD FR-051
- **4×N** : PRD FR-051 (notif Devis actions, deep-link, preview tronqué, échecs envoi, action invalide, device offline, multi-device, payload chiffré, anti-spoofing, no tracking)
- **Couche(s)** : Infra (Web Push, `actions` du `NotificationOptions`) + Frontend (service worker `notificationclick` + handlers) + Application
- **Limite** : les `actions` de la Notification API sont ignorées par Safari/iOS. Sur iOS, la notification ouvre l'application, elle ne permet pas de répondre depuis le shade.
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : P95 delivery < 10 s · JWT short-lived actions inline · payload AES-256-GCM

### Story 11.2 — Deep-linking vers Mission (FR-051)
- **En tant que** User · **je veux** qu'une notif m'ouvre directement la Mission ciblée · **afin de** gagner du temps
- **Critères Gherkin** : PRD FR-051 (scénario deep-link `mission_id`)
- **4×N** : happy ouverture /mission/M-1234 · negative mission inconnue · edge auth required · security pas de fuite données
- **Couche(s)** : Frontend (routeur Astro/Svelte + `clients.openWindow` dans le service worker) + Application
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : auth vérifiée avant affichage · une URL suffit, il n'y a plus ni universal link ni app link à déclarer

### Story 11.3 — Authentification forte WebAuthn (FR-052, re-spécifiée par ADR-010)
- **En tant que** User · **je veux** confirmer les actions sensibles (paiement ≥ 100 €) par l'authentificateur de plateforme de mon appareil · **afin de** satisfaire la SCA DSP2
- **Critères Gherkin** : PRD FR-052, **amendés** : le moyen n'est plus le Keychain, c'est une passkey liée au domaine
- **4×N** : `@happy` challenge signé et vérifié serveur · `@negative` signature invalide rejetée · `@edge` appareil sans authentificateur de plateforme (repli mot de passe + OTP) · `@security` anti-rejeu par challenge à usage unique, `userVerification: required`
- **Couche(s)** : Infra (vérification WebAuthn côté serveur) + Frontend (`navigator.credentials`) + Application (hooks refresh + payment)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : aucune donnée biométrique côté serveur (elle ne quitte pas l'appareil) · audit_log 13 mois (DSP2)
- **Régression assumée** : WebAuthn authentifie, il ne **stocke** pas. Le refresh token ne peut plus être gardé dans une enclave sécurisée ; il reste en cookie `HttpOnly` `Secure` `SameSite=Strict`. C'est un niveau de protection inférieur à ce que promettait ADR-008, et il faut le dire.

### ~~Story 11.4 — PoC géoloc background plugin Tauri (FR-053)~~ — **retirée (ADR-010)**
Il n'y a plus de plugin à éprouver. Le suivi en arrière-plan sort du périmètre produit, il ne devient pas « à faire plus tard ». Si l'exigence redevient bloquante, c'est ADR-010 qu'il faut rouvrir, pas cette story.

### Story 11.5 — Suivi foreground robuste (FR-053, re-spécifiée)
- **En tant que** User · **je veux** que le suivi survive à un tunnel, un verrouillage bref ou une perte de réseau · **afin de** ne pas perdre la trace de mon dépanneur
- **Critères Gherkin** : reprise de `watchPosition` au `visibilitychange`, positions mises en file dans IndexedDB tant que le réseau est absent, rejeu ordonné à la reconnexion
- **4×N** : `@happy` reprise transparente après retour au premier plan · `@negative` permission révoquée en cours de Mission (message explicite, pas d'échec silencieux) · `@edge` 15 min hors ligne puis rejeu sans doublon ni désordre · `@security` aucune position conservée après `COMPLETED`
- **Couche(s)** : Frontend (Geolocation API + queue IndexedDB) + Application
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : bandeau « suivi interrompu » visible quand la PWA passe en arrière-plan, jamais de position inventée par interpolation · purge à `COMPLETED` vérifiée

### ~~Story 11.6 — Re-submission stores automatisée (FR-054)~~ — **sans objet (ADR-010)**
Il n'y a plus de store. Le déploiement d'un correctif est une mise en ligne, et le service worker rend la nouvelle version disponible au chargement suivant. FR-054 n'a plus de contenu.

### Story 11.7 — Surface publique et incitation à l'installation (FR-055, re-spécifiée)
- **En tant que** visiteur · **je veux** utiliser Klaar sans rien installer · **afin de** l'essayer avant de l'ajouter à mon écran d'accueil
- **Note** : ce n'est plus une « alternative » à une application native, **c'est le produit**. La story ne porte donc plus que sur la partie qui reste vraie : le parcours d'installation et la dégradation propre.
- **4×N** : `@happy` `beforeinstallprompt` capté et proposé après 2 sessions · `@negative` navigateur sans support (aucune invite, aucune erreur) · `@edge` iOS, où l'invite n'existe pas et où il faut une consigne explicite « Partager → Sur l'écran d'accueil » · `@security` HTTPS/HSTS strict, CSP `strict-dynamic`, service worker servi en même origine
- **Couche(s)** : Frontend + Infra (CDN + HSTS)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : P95 accueil < 3 s · matrice de dégradation par navigateur documentée · le push iOS annoncé comme conditionnel à l'installation, dans l'interface et pas seulement dans la doc

**Epic 11 total** : **5 stories** (2 retirées par ADR-010) · ~4 j wall-clock · ~22 tours

---

## Epic 12 — E3 Intelligence, monétisation & ouverture (C13, J13) · Priorité **Post-MVP**

> Activable après stabilisation des secteurs pilotes (J11 + J12' fructueux). Sous-capacités CBS E3.1-E3.7 — `00-Capability-Breakdown-Estimation.md` §Partie 2 · J13. Conformité AI Act renforcée pour E3.1 (matching IA) et E3.2 (surge) — Brief §15 H-15. Référence PRD §7 module E3 (FR-056 à FR-063).

### Story 12.1 — Matching IA features store (FR-056)
- **En tant que** ops/data engineer · **je veux** un feature store pour le matching IA · **afin de** calculer features distance × rating × fiabilité × prix
- **Critères Gherkin** : PRD FR-056 (features input traçables)
- **4×N** : happy features calculées pour 5 Providers · negative feature manquante · edge cold-start · security anti-poisoning
- **Couche(s)** : Infra (`klaar-ml-adapter` feature store + PostgreSQL read replica) + Domain (`MatchCriteria` étendu)
- **Taille** : **L** (1 j) · **Tours** : 6
- **DoD** : features versionnées · refresh ≤ 1 h · Trace AI Art. 12 enrichie

### Story 12.2 — Modèle ranking Rust candle-core (FR-056)
- **En tant que** système · **je veux** ranker les Providers par modèle IA supervisé en Rust · **afin d'** optimiser le fill rate (au-delà du moteur règles C3)
- **Critères Gherkin** : PRD FR-056
- **4×N** : PRD FR-056 (score IA top 3, fallback règles, anomalies, drift kill-switch, cold-start secteur, Provider nouveau, faible candidats, audit biais, Trace immuable, anti-poisoning, supervision humaine)
- **Couche(s)** : Infra (`candle-core` Rust + `RankProvidersByIA` use case) + Domain + Application
- **Taille** : **L** (1 j) · **Tours** : 6
- **DoD** : modèle versionné MLOps · canary 10 % · kill-switch `DISABLE_IA_MATCHER` · supervision humaine Art. 14

### Story 12.3 — A/B testing matching IA (FR-056)
- **En tant que** ops · **je veux** un A/B testing progressif · **afin de** valider l'IA vs règles avant généralisation
- **Critères Gherkin** : PRD FR-056 (canary 10 % → 50 % → 100 %, kill-switch drift > 20 %)
- **4×N** : happy canary 10 % · negative drift détecté · edge bascule 50 % · security audit
- **Couche(s)** : Application (middleware A/B) + Infra (feature flag service)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : kill-switch auto si drift > 20 % · métriques comparatives dashboard

### Story 12.4 — Audit biais semestriel AI Act (FR-056)
- **En tant que** DPO · **je veux** un audit biais semestriel · **afin de** respecter AI Act Art. 10-15
- **Critères Gherkin** : PRD FR-056 (scénarios audit biais, droit explication User)
- **4×N** : happy rapport équité · negative biais détecté · edge kill-switch · security WORM 5 ans
- **Couche(s)** : Application (`AuditBiasSemestriel` job) + Infra (rapports signés) + Documentation
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : métriques demographic parity/equal opportunity · rapport DPO · kill-switch automatique

### Story 12.5 — Surge rule engine (FR-057)
- **En tant que** plateforme · **je veux** appliquer un coefficient d'urgence variable par zone/heure · **afin d'** équilibrer offre/demande sans imposer de prix (Invariant §10.2)
- **Critères Gherkin** : PRD FR-057
- **4×N** : PRD FR-057 (surge nominal, retour normale, contesté, prix plancher interdit, cap max 3.0, surge négatif discount, jamais imposé au Devis, audit rétrospectif, anti-discrimination géo)
- **Couche(s)** : Domain (`SurgeZone`, `SurgeCoefficient`) + Application (`ApplySurgeToRequest` + rule engine) + Infra (`klaar-surge` crate)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : job 5 min · cap 3.0 · transparence Platform Work · audit Platform Work

### Story 12.6 — Transparence surge UI User (FR-057)
- **En tant que** User · **je veux** voir le coefficient d'urgence affiché · **afin de** comprendre le prix
- **Critères Gherkin** : PRD FR-057 (scénarios "Prix d'urgence ×1.5" + justification)
- **4×N** : happy affichage · negative coef tardif · edge cap atteint · security audit
- **Couche(s)** : Frontend (UI prix avec badge surge) + Application (`DiscloseSurgeToUser`)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : justification horodatée · cache < 5 min · i18n

### Story 12.7 — Contestation surge (FR-057)
- **En tant que** User · **je veux** contester un coefficient · **afin d'** exercer mon droit Platform Work
- **Critères Gherkin** : PRD FR-057 (scénario contestation, refund partiel, nullité bug)
- **4×N** : happy annulation 2 min · negative contestation infondée · edge coef > 5 · security audit
- **Couche(s)** : Application + Domain (`SurgeContestation`)
- **Taille** : **S** (0,5 j) · **Tours** : 2
- **DoD** : annulation sans frais 2 min · refund partiel si prouvé · nullité si bug

### Story 12.8 — Subscription Pro Stripe (FR-058)
- **En tant que** Provider · **je veux** souscrire un abonnement Pro · **afin d'** accéder à Demandes prioritaires, CRM léger et analytics avancées
- **Critères Gherkin** : PRD FR-058
- **4×N** : PRD FR-058 (souscription 29 €, renouvellement auto, paiements échouent, quota dépassé, rétrogradation, BAN, migration Pro→Premium prorata, pas d'exclusivité, audit DSP2, anti-contournement)
- **Couche(s)** : Domain (`Subscription`, `Tier`) + Application (`SubscribeProvider`, `RenewSubscription` job) + Infra (`klaar-subscription` crate + Stripe billing récurrent) + Frontend (wizard souscription)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : Stripe 3DS2 initial + MIT · résiliable sans lock-in ·Invariant §10.3 (pas d'exclusivité)

### Story 12.9 — Quotas Subscription (FR-058)
- **En tant que** système · **je veux** appliquer les quotas par tier · **afin de** différencier Free/Pro/Premium sans bridage core
- **Critères Gherkin** : PRD FR-058 (scénario quota 10/jour Pro, 50 Premium)
- **4×N** : happy quota respecté · negative 429 QUOTA_EXCEEDED · edge reset daily · security anti-évasion
- **Couche(s)** : Application (`ApplyQuotaLimits`) + Infra (Redis)
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : Demandes standards toujours disponibles Free · quota prioritaire only

### Story 12.10 — CRM Provider analytics (FR-058)
- **En tant que** Provider Pro · **je veux** un CRM léger · **afin de** gérer ma clientèle récurrente
- **Critères Gherkin** : PRD FR-058 (scénario accès CRM + comparaison Free/Pro)
- **4×N** : happy CRM actif · negative Free en read-only 30 j · edge migration · security
- **Couche(s)** : Domain (`ProviderCRM`) + Application + Frontend (CRM Provider UI)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : CRM read-only 30 j après rétrogradation · pas de PII externes

### Story 12.11 — Assurance intégrée partenaire (FR-059)
- **En tant que** Provider non couvert · **je veux** souscrire assurance RC pro via Klaar · **afin de** respecter Invariant §10.8 et démarrer rapidement
- **Critères Gherkin** : PRD FR-059
- **4×N** : PRD FR-059 (souscription immédiate, renouvellement, échecs, quote > 1000 €, rétractation 14 j, sinistre Mission, assurance externe valide, mTLS, données minimisées, audit)
- **Couche(s)** : Domain (`InsurancePolicy`) + Application (`SubscribeInsurance`) + Infra (`klaar-insurance-adapter` Baloise/AG + mTLS) + Frontend (wizard souscription assurance)
- **Taille** : **L** (1 j) · **Tours** : 6
- **DoD** : mTLS partenaire · quote < 1000 € auto · rétractation 14 j · DPIA assurance-integrée

### Story 12.12 — API publique OAuth2 (FR-060)
- **En tant que** partenaire tiers · **je veux** intégrer Klaar via API publique OAuth2 · **afin d'** enrichir mes services avec catalogue et historique public
- **Critères Gherkin** : PRD FR-060
- **4×N** : PRD FR-060 (OAuth2 client_credentials, lecture catalogue, requêtes invalides, quota dépassé, versioning, partenaire suspendu, pic trafic, rate-limit DOS, audit, PII absente, mTLS Enterprise)
- **Couche(s)** : Domain (`ApiClient`, `Tier`) + Application (`AuthenticatePartner`) + Infra (`klaar-public-api` crate + OAuth2 server + Redis rate-limit) + Frontend (Swagger UI public)
- **Taille** : **L** (1 j) · **Tours** : 6
- **DoD** : OAuth2 client_credentials · rate-limit par tier · mTLS Enterprise · DPIA api-publique

### Story 12.13 — Rate-limiting tier (FR-060)
- **En tant que** système · **je veux** un rate-limiting strict par tier · **afin de** protéger l'API publique anti-DOS
- **Critères Gherkin** : PRD FR-060 (scénario burst 1000 req/s plafonné 100)
- **4×N** : happy sous quota · negative 429 · edge burst prolongé · security alerte ops
- **Couche(s)** : Infra (Redis + middleware `RateLimitRequest`)
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : retry-after header · alerte burst > 30 min · quota Enterprise 1 M/jour

### Story 12.14 — Documentation Swagger publique + SDK (FR-060)
- **En tant que** partenaire · **je veux** une documentation publique + SDK · **afin d'** intégrer rapidement
- **Critères Gherkin** : PRD FR-060 (OpenAPI public publié, header Deprecation/Sunset)
- **4×N** : happy docs consultables · negative v1 dépréciée · edge sunset 6 mois · security
- **Couche(s)** : Frontend (site docs publique + SDK TS/Python) + Infra (génération utoipa)
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : OpenAPI public versionné · v1 supportée 6 mois minimum · SDK TS + Python

### Story 12.15 — Webhooks partenaires (FR-061)
- **En tant que** partenaire · **je veux** recevoir des events webhook temps réel · **afin de** synchroniser sans polling
- **Critères Gherkin** : PRD FR-061
- **4×N** : PRD FR-061 (mission_completed envoyé, ack < 5 s, échecs livraison, signature invalide, URL injoignable DLQ, multi-env, burst batch, HTTPS obligatoire, secret rotation 90 j, replay attack)
- **Couche(s)** : Domain (`Webhook`, `WebhookEvent`) + Application (`EmitWebhook`) + Infra (`klaar-public-api` webhook emitter + DLQ) + Documentation
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : HMAC SHA-256 · retry 1/5/30 min/4 h/24 h · DLQ · rotation secret 90 j · anti-replay 5 min

### Story 12.16 — Analytics avancé ops (FR-062)
- **En tant que** ops admin · **je veux** des dashboards avancés (funnel, unit economics, heatmap) · **afin d'** identifier secteurs/zones à densifier
- **Critères Gherkin** : PRD FR-062
- **4×N** : PRD FR-062 (funnel fill rate, unit economics, données insuffisantes, export raw forbidden, pic 100 ops, comparaison villes, temps réel vs batch, k-anonymité, RBAC, audit, anti-inference)
- **Couche(s)** : Application (job agrégation nightly) + Infra (PostgreSQL read replica / DuckDB) + Frontend (admin analytics dashboards Svelte 5)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : refresh ≤ 1 h · k-anonymité ≥ 100 · RBAC analytics_viewer/analyst · audit log

### Story 12.17 — Provider dashboard analytics (FR-063)
- **En tant que** Provider · **je veux** un dashboard revenus/ratings/taux acceptation · **afin d'** optimiser mon activité
- **Critères Gherkin** : PRD FR-063
- **4×N** : PRD FR-063 (dashboard mensuel, insight temps réponse, données insuffisantes, Free sans bridage core, multi-secteurs, 1re mission, sanction visible, pas de concurrents identifiés, données personnelles, anti-évasion fiscale, audit)
- **Couche(s)** : Application (agrégation Provider) + Frontend (Provider analytics UI Svelte 5)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : Wilson score · médiane secteur anonymisée · export CSV RGPD Art. 20 · Free core metrics

### Story 12.18 — Pipeline ML CI/CD transverse (FR-056)
- **En tant que** data engineer · **je veux** un pipeline ML CI/CD · **afin de** versionner, entraîner et déployer les modèles IA avec audit
- **Critères Gherkin** : PRD FR-056 (versioning MLOps, anti-poisoning entraînement)
- **4×N** : happy modèle déployé · negative drift détecté en CI · edge rollback modèle · security anti-poisoning
- **Couche(s)** : Infra (`klaar-ml-adapter` pipeline + MLOps CI/CD) + Documentation
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : versioning modèles · CI tests biais · CD canary · kill-switch · doc vivante MLOps

**Epic 12 total** : 18 stories · ~14 j wall-clock · ~75 tours

---

## Epic 13 — E4 Expansion géographique (C14, J14) · Priorité **Post-MVP (par ville)**

> Activable par ville après gate **rentabilité RBC prouvée > 12 mois** (Brief §19.3). Sous-capacités CBS E4.1-E4.3 — `00-Capability-Breakdown-Estimation.md` §Partie 2 · J14. Coût indicatif 13-23 k€/ville. Référence PRD §7 module E4 (FR-064 à FR-068). Les stories ci-dessous sont à reproduire par ville activée.

### Story 13.1 — Process activation ville (FR-064)
- **En tant que** ops admin · **je veux** activer une nouvelle ville dans Klaar · **afin d'** étendre le périmètre géographique (Anvers, Liège, Gand, Charleroi)
- **Critères Gherkin** : PRD FR-064
- **4×N** : PRD FR-064 (activation complète, soft launch, blocages, gate rentabilité, rollback, 2 phases, chevauchement, audit launch, RBAC super_admin, registre APD)
- **Couche(s)** : Domain (`City` aggregate) + Application (`ActivateCity` use case 4-eyes super_admin) + Infra (`klaar-region-adapter`) + Frontend (admin activation UI)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : gate rentabilité check · ≥ 100 Providers BCE · 4-eyes super_admin · audit WORM

### Story 13.2 — Configuration géographique (FR-064)
- **En tant que** ops · **je veux** configurer les limites géographiques de la ville · **afin de** restreindre le matching aux Providers locaux
- **Critères Gherkin** : PRD FR-064 (scénarios activation par quartier, chevauchement frontalière)
- **4×N** : happy config polygons · negative zone hors scope · edge chevauchement · security
- **Couche(s)** : Domain (`CityGeometry` PostGIS) + Application + Frontend (admin geo UI)
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : soft launch par quartier · fallback "Service pas encore disponible"

### Story 13.3 — Campaign marketing régional (FR-065)
- **En tant que** ops · **je veux** lancer une campagne ciblée par ville · **afin d'** atteindre la densité critique ≥ 100 Providers BCE locaux
- **Critères Gherkin** : PRD FR-065 (campaign Meta/Google + landing page + tracking)
- **4×N** : happy campagne ciblée · negative conversion < 2 % · edge pic signups · security anti-fraude BCE
- **Couche(s)** : Frontend (landing page `/pro/<city>`) + Infra (tracking UTM audit) · **Exécution = marketing externe (story catalogue)**
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : tracking UTM audit · pause auto si conversion < 2 % · consentement marketing séparé

### Story 13.4 — Onboarding accéléré Providers locaux (FR-065)
- **En tant que** Provider local invité · **je veux** un parcours simplifié (3 étapes au lieu de 5) · **afin de** démarrer rapidement
- **Critères Gherkin** : PRD FR-065 (scénario onboarding accéléré + KYC prioritaire gratuit)
- **4×N** : happy parcours 3 étapes · negative doublon inter-ville · edge Provider RBC déménage · security anti-fraude
- **Couche(s)** : Frontend (wizard onboarding accéléré) + Application
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : SLA review 24 h · KYC gratuit subventionné · ops dédié par ville

### Story 13.5 — Tiles/routing régionaux (FR-066)
- **En tant que** système · **je veux** étendre tile-server OSM et Valhalla à la nouvelle ville · **afin de** garantir matching géoloc et ETA précis
- **Critères Gherkin** : PRD FR-066
- **4×N** : PRD FR-066 (activation routing, tiles CDN, défaillances, routing incomplet, pic launch, chevauchement routing, piéton vs voiture, pas de PII tiles, backup Mapbox, audit extract OSM)
- **Couche(s)** : Infra (`klaar-geo-adapter` étendu + extract Geofabrik + Valhalla config régional) + IaC (k8s HPA)
- **Taille** : **L** (1 j) · **Tours** : 5
- **DoD** : ETA < 5 % erreur vs Google · P95 tiles < 200 ms · audit provenance OSM

### Story 13.6 — CDN régional OVH (FR-066)
- **En tant que** ops · **je veux** un CDN OVH régional · **afin de** réduire la latence tiles pour la nouvelle ville
- **Critères Gherkin** : PRD FR-066 (scénario tiles servies CDN, fallback Mapbox)
- **4×N** : happy cache hit 90 % · negative CDN down · edge pic · security
- **Couche(s)** : IaC (CDN OVH config régional)
- **Taille** : **S** (0,5 j) · **Tours** : 2
- **DoD** : cache hit 90 % · bande passante backend < 10 Mbps · ADR-006 confirmé

### Story 13.7 — Déclaration APD/GBA régionale (FR-067)
- **En tant que** DPO · **je veux** déclarer l'activation aux APD régionaux · **afin de** respecter les obligations hors RBC
- **Critères Gherkin** : PRD FR-067 (scénario déclaration GBA flamand/APD wallon)
- **4×N** : happy registre obtenu · negative APD non déclarée · edge région frontalière · security WORM 10 ans
- **Couche(s)** : Documentation (DPIA étendu vivant) + Infra (`regulatory_registrations` table) · **Exécution = juridique externe (story catalogue)**
- **Taille** : **S** (0,5 j) · **Tours** : 2
- **DoD** : numéro registre stocké · DPIA validé par DPO · archivage WORM

### Story 13.8 — Conformité TVA régionale (FR-067)
- **En tant que** système · **je veux** appliquer la TVA BE correcte par secteur et région · **afin de** rester conforme fiscalement
- **Critères Gherkin** : PRD FR-067 (TVA 6/12/21 %, bascule taux, intracommunautaire)
- **4×N** : happy TVA 6 % rénovation · negative TVA incorrecte · edge bascule en cours · security audit fiscal
- **Couche(s)** : Application (extension moteur facturation Story 5.3) + Domain (`VatRule` par région)
- **Taille** : **M** (0,75 j) · **Tours** : 3
- **DoD** : TVA BE 21/6/12 % correcte · archivage WORM 10 ans · audit fiscal signé eIDAS

### Story 13.9 — Dashboard multi-villes (FR-068)
- **En tant que** ops admin · **je veux** un dashboard multi-villes comparatif · **afin de** piloter l'expansion géographique
- **Critères Gherkin** : PRD FR-068
- **4×N** : PRD FR-068 (vue d'ensemble, comparaison side-by-side, drill-down par ville)
- **Couche(s)** : Application (extension analytics Story 12.16) + Frontend (`/admin/analytics/cities.astro` Svelte 5)
- **Taille** : **M** (0,75 j) · **Tours** : 4
- **DoD** : ≥ 2 villes activées · sparkline 30 j · RBAC multi_city_viewer

### Story 13.10 — Alertes multi-villes (FR-068)
- **En tant que** ops · **je veux** des alertes sur dérive par ville (fill rate, NPS, GMV) · **afin de** réagir rapidement
- **Critères Gherkin** : PRD FR-068 (scénario alerte dérive + drill-down)
- **4×N** : happy alerte fill rate · negative false positive · edge 1 ville en rollback · security
- **Couche(s)** : Application (règles alerting) + Infra (AlertManager)
- **Taille** : **S** (0,5 j) · **Tours** : 2
- **DoD** : alertes sur seuils configurables · notification ops temps réel

**Epic 13 total (par ville)** : 10 stories · ~7,5 j wall-clock/ville · ~33 tours/ville

---

## Stories transverses

### Documentation Vivante
- **Story T.1** — E2E happy path complet (Playwright + Maestro) — **L** (1 j) · 5 tours
- **Story T.2** — E2E Litige complet — **M** (0,75 j) · 4 tours
- **Story T.3** — E2E Onboarding Provider complet — **M** (0,75 j) · 4 tours

### Stories ITIL (pré-prod)
- **Story T.4** — Runbook incident NIS2 (reporting 24 h) — **M** (0,75 j) · 2 tours
- **Story T.5** — DPIA géoloc document vivant — **S** (0,5 j) · 2 tours
- **Story T.6** — Procédure backup/restore testée mensuellement — **S** (0,5 j) · 2 tours

### ~~Déploiement stores~~ — **retiré (ADR-010)**
- ~~**Story T.15** — Submission App Store + Play Store + provisioning~~ — sans objet : la PWA se distribue par son URL. Les 99 €/an Apple et 25 $ Google sortent du budget, et avec eux le délai de revue entre un correctif et sa mise à disposition.

### Audit juridique (H-3 critique, pre-S5)
- **Story J.1** — Audit juridique Platform Work par avocat BE (loi 26/04/2024 + directive UE 2024/2831 transposée 2 déc 2026) · revue invariants §10.1-10.3 · revue contrat Provider — **M** (0,75 j) · 0 tour (mission externe avocat, ~3 k€ budget à prévoir)

### Émergence (~20 % réserve validée — alignée recommandation foyer)
- **Story T.7 à T.18** — réserve pour imprévu (~12 j wall-clock cumulés, 20 % du total MVP)

**Stories transverses total** : ~12 stories · ~18 j wall-clock · ~36 tours

---

## Estimation du projet

Le décompte par epic (stories, jours wall-clock, tours) ainsi que le chiffrage, les
comparaisons de marché et le modèle de facturation figurent dans les livrables restés
privés — ils relèvent de la relation commerciale, pas de la conception. Ce qui est
opposable ici est le découpage lui-même : **76 stories** pour le cœur MVP, chacune
dimensionnée sur ses deux axes et assortie de ses quatre classes de test.

---

## Sprint Plan MVP — 14 sprints × 2 semaines *(re-timeboxé v2.1, intervenant unique)*

> ⚠️ Le tableau ci-dessous conserve le **séquencement d'origine** à titre historique. Le **plan opposable** est celui du `06-Chef-de-projet.md` §2 — 14 sprints, 40 h et ~20 passes d'agent par sprint, gates par jalon.

| Sprint | Epics / stories | Wall-clock |
|---|---|---|
| S0 (semaines 1-2) | Sprint 0 complet | 8 j |
| S1 (3-4) | Epic 1 IDN stories 1.1-1.5 | 3,5 j |
| S2 (5-6) | Epic 1 IDN stories 1.6-1.10 + Epic 2 CTL | 7,5 j |
| S3 (7-8) | Epic 3 MCH complet | 6,75 j |
| S4 (9-10) | Epic 4 INT stories 4.1-4.5 | 4,75 j |
| S5 (11-12) | Epic 4 INT stories 4.6-4.9 + Epic 5 PAY 5.1-5.2 | 5,5 j |
| S6 (13-14) | Epic 5 PAY 5.3-5.6 + Epic 6 MSG complet | 5,5 j |
| S7 (15-16) | Epic 7 TRU + Epic 9 i18n | 5,0 j |
| S8 (17-18) | Epic 8 OPS + Transverses T.1-T.3 | 7,75 j |
| S9 (19-20) | Transverses T.4-T.14 (émergence + ITIL) + UAT | 5,0 j |

**Durée MVP** : **28 semaines (~7 mois) sur 14 sprints** — re-timeboxé v2.1. L'indépendant étant unique (lead dev = superviseur foyer), la capacité soutenable est de ~80 h/mois, soit **40 h et ~20 passes d'agent par sprint**. Le volume d'effort du cœur donne **14 sprints (S0-S13)**.<!-- 548 h : volume retiré du public avec le chiffrage --> Le séquencement des epics et le chemin critique sont inchangés ; seule la cadence l'est. Découpage détaillé et Gantt par passes d'agent : `06-Chef-de-projet.md` §2.

---

## Suggested Sprint Plan Extension (J11-J14, post-MVP au fil de l'eau)

> Déclenchés au fil de l'eau selon les gates go/no-go de chaque jalon. Pas de calendrier imposé.

| Sprint | Epics / stories | Wall-clock |
|---|---|---|
| S10 (J11a) | E1.1-E1.4 Skills + attestations (FR-045) | 4,5 j |
| S11 (J11b) | E1.5-E1.6 Onboarding multi-secteur (FR-046) | 3 j |
| S12 (J11c) | E1.7-E1.12 Catalogue extensible + bulk import + cross-check (FR-047-050) | 6 j |
| S13 (J12'a) | E2'.1-E2'.2 Push rich + deep-linking (FR-051) | 3,5 j |
| S14 (J12'b) | E2'.3 Biométrie + secure storage (FR-052) | 2 j |
| S15 (J12'c) | E2'.4 PoC géoloc background + fallback PWA (FR-053) | 3 j |
| S16 (J12'd) | E2'.5 Re-submission stores + PWA grand public (FR-054-055) | 3 j |
| S17 (J13a) | E3.1-E3.4 Matching IA (FR-056) | 5 j |
| S18 (J13b) | E3.5-E3.7 Surge pricing (FR-057) | 3 j |
| S19 (J13c) | E3.8-E3.10 Subscription Pro (FR-058) | 3 j |
| S20 (J13d) | E3.11 Assurance intégrée (FR-059) | 2 j |
| S21 (J13e) | E3.12-E3.14 API publique + OAuth2 + SDK (FR-060) | 4 j |
| S22 (J13f) | E3.15 Webhooks partenaires (FR-061) | 2 j |
| S23 (J13g) | E3.16-E3.17 Analytics avancés (FR-062-063) + pipeline ML | 3 j |
| S24+ (J14) | E4 par ville (itératif) | ~7,5 j/ville |

**Durée totale extension** : ~48 j wall-clock classiques (~384 h classiques) au rythme choisi, hors expansion géographique.

---

## Risques spécifiques découpage (H-1, H-2)

- **H-1 scope MVP** : 76 stories est ambitieux pour ~7 mois en **solo** (pas de binôme humain — bus factor 1). Mitigation : stories Should (Epic 6 MSG, Epic 9 i18n) peuvent basculer post-MVP si dépassement.
- **H-2 ~~Tauri Mobile~~ — risque clos par ADR-010.** Il portait sur la maturité des plugins Tauri Mobile sans plan B (concern C-2 du Validateur). Tauri étant retiré, il n'y a plus de plugin dont dépendre. Le risque n'est pas mitigé, il est supprimé — au prix de la géoloc background, désormais hors périmètre.
- **H-3 Platform Work** : jurisprudence APD possible entre S5 et S8 (loi 26 avril 2024 + directive 2 déc 2026) — story habilitante juridique à prévoir.
- **H-9 courbe Rust** : risque de glissement S1-S2 (team freelance découvre la codebase) — pair programming obligatoire (Brief conviction 8).
- **H-13 géoloc background** : **acté comme perdu** (ADR-010). Ce n'est plus un risque à surveiller mais une capacité absente, à annoncer comme telle. Le risque résiduel est commercial : un prospect pour qui le suivi écran éteint est bloquant ne sera pas servi par ce produit.
- **H-14 surcharge KYC** : activation séquentielle secteurs et villes (1 secteur/2 par an max), bulk import capé 1000 lignes.
- **H-15 AI Act matching IA** : canary 10 % → 50 % → 100 %, kill-switch drift > 20 %, audit biais semestriel (Story 12.4).

---

## Questions ouvertes pour le superviseur (avant Validateur)

1. **One mission per Provider** (FR-013) : à confirmer ou assouplir (impact Epic 3 + 4)
2. **Story T.7-T.14 réserve émergence** : 6 j soit ~10 % du total — suffisant ou ajuster à 20 % (12 j) ?
3. **Sprint 0 durée** : 8 j est optimiste pour 9 stories complexes — étendre à 3 semaines ?
4. **Capacité soutenable** : ~80 h/mois pour un intervenant unique sur ~7 mois, est-ce tenable pour le cœur MVP ?
5. ~~**Plan B Tauri**~~ — **question close par ADR-010** : il n'y a plus de Tauri, donc plus de plan B à déclencher. La PWA n'est plus un repli, c'est la stack.
6. **Gate J11 fill rate > 60 %** : seuil validé ou ajuster ? Même question pour gate J14 rentabilité RBC > 12 mois.
7. **Activation E2' (J12')** : budget 100-200 h confirmé vs J12 original 1000-1600 h ?
8. **Partenaires assurance (J13)** : Baloise vs AG vs autre — pré-validation commerciale avant Story 12.11 ?

---

*Dérivé du Manifeste Maury (CC BY-SA 4.0). Méthode Foyer. Version 2.1 — 76 stories cœur + ~45 stories extension pour 14 capacités (J0-J14). Roadmap continue par capacité. Amendé le 27/08/2026 par ADR-010 (bascule PWA, deux stories de l'Epic 11 retirées).*
