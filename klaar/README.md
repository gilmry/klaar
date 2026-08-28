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

- **1.2** — **Vérification d'adresse** (FR-001) : `POST /api/v1/auth/verify-email`, page `/verifier-email`, jeton consommé une seule fois et compte passé en `ACTIVE` dans la même transaction.

  **`POST` et non le `GET` qu'annonce le PRD.** Les passerelles de messagerie d'entreprise
  visitent les liens des courriels avant leur destinataire, pour les analyser : un `GET`
  qui consomme le jeton est consommé par l'antivirus, et l'utilisateur trouve un lien déjà
  utilisé au moment où il clique. Le lien ouvre donc une page statique de la PWA, qui
  présente ensuite le jeton par un `POST`. Un test e2e le vérifie **en désactivant
  JavaScript**, comme le ferait un tel analyseur : la page se charge, aucun appel ne part.

  **Un second clic répond `200 EMAIL_ALREADY_VERIFIED`.** Recharger la page ou rouvrir le
  courriel est le cas le plus banal du parcours ; y répondre par une erreur ferait croire à
  un échec à quelqu'un dont le compte vient d'être activé. Le jeton n'est consommé qu'une
  fois et le journal d'audit ne consigne qu'une vérification, quel que soit le nombre de
  clics. Le contrôle « déjà consommé » passe avant celui d'expiration, sans quoi rouvrir un
  vieux courriel afficherait « lien expiré » à un compte actif depuis des semaines.

  `FOR UPDATE` sur la ligne du jeton : deux présentations simultanées n'activent qu'une
  fois, ce qu'un test vérifie en les lançant réellement en parallèle. Le jeton est retiré de
  la barre d'adresse dès sa lecture — il resterait sinon dans l'historique, dans les
  captures d'écran et dans le `Referer` de tout lien suivi depuis cette page.

- **1.3** — **Connexion** (FR-004) : `POST /api/v1/auth/login`, page `/connexion`, jeton d'accès JWT d'une heure et refresh de 30 jours en cookie.

  **Adresse inconnue et mot de passe faux sont indistinguables**, réponse *et* temps de
  réponse compris. Une adresse inconnue économiserait la vérification argon2 et répondrait
  en une milliseconde là où un mot de passe faux en prend cinquante : le chronomètre
  distinguerait ce que la réponse tait. Une empreinte leurre est donc vérifiée dans le
  vide, avec les paramètres réellement employés — un `sleep` fixe ne suivrait pas les
  paramètres et sa régularité se repérerait. « Compte non vérifié » est distingué, lui
  (`403`), parce que l'atteindre suppose déjà de connaître le bon mot de passe.

  **Le jeton d'accès ne quitte pas la mémoire de l'onglet.** Ni `localStorage` ni
  `sessionStorage` : les deux sont lisibles par tout script de la page, donc par une seule
  faille XSS. Le refresh, lui, vit en cookie `HttpOnly` `Secure` `SameSite=Lax`, de chemin
  restreint à `/api/v1/auth` — l'envoyer à chaque appel d'API l'exposerait à toute faille
  d'une autre route. Deux tests e2e le vérifient dans un vrai navigateur, dont un qui lit
  `document.cookie` pour confirmer que le refresh n'y apparaît pas.

  L'algorithme de vérification du JWT est fixé explicitement plutôt que lu dans l'en-tête du
  jeton : un jeton annonçant `alg: none` est refusé, ce qu'un test vérifie avec un jeton
  forgé à la main. `KLAAR_JWT_SECRET` est obligatoire au démarrage — en générer un à la
  volée invaliderait toutes les sessions à chaque redémarrage, sans que personne ne
  comprenne pourquoi.

  **Limite assumée jusqu'à la Story 1.4** : pas encore de rotation du refresh ni de
  détection de rejeu. Un refresh volé reste utilisable jusqu'à son expiration, et recharger
  la page déconnecte. Les colonnes `famille_id` et `consomme_le` existent déjà pour cela.

- **1.4** — **Refresh rotatif et détection de vol** (FR-004) : `POST /api/v1/auth/refresh` et `/logout`, reprise de session au chargement, renouvellement programmé une minute avant expiration.

  **Un refresh rejoué coupe toute sa famille.** Chaque présentation consomme le jeton et en
  rend un neuf : le porteur légitime a donc toujours le dernier. Présenter un jeton déjà
  consommé signifie qu'une copie circule — et rien ne permet de dire laquelle des deux mains
  est la bonne, d'où la coupure des deux. Le coût est une reconnexion, contre une session
  volée qui durerait trente jours. Vérifié en conditions réelles : après le rejeu, le
  refresh courant du porteur légitime répond `REFRESH_REVOKED`.

  **Le *binding* est partiel, délibérément.** FR-004 demande un lien « UA + IP + device ».
  L'agent utilisateur est lié, sous forme d'empreinte, et un changement lève
  `SESSION_CONTEXT_CHANGED` **sans couper la session** : les navigateurs changent d'agent à
  chaque mise à jour, bloquer là-dessus déconnecterait tout le monde toutes les quelques
  semaines sans qu'aucun vol n'ait eu lieu. L'adresse IP n'est pas liée du tout : un
  téléphone en change plusieurs fois par trajet entre wifi et données mobiles. La protection
  réelle est la rotation, qui ne dépend d'aucune de ces heuristiques.

  Rotation et coupure se font dans une transaction, la ligne verrouillée par `FOR UPDATE` :
  deux onglets qui rafraîchissent en même temps sérialisent, sinon le second obtiendrait un
  refresh que le premier rejeu ferait passer pour un vol. Le nouveau maillon hérite du
  contexte **d'origine** et non de celui présenté — sinon un voleur ferait glisser
  l'empreinte attendue vers la sienne à chaque rotation, et l'anomalie cesserait d'être
  signalée.

  Côté PWA, recharger la page ne déconnecte plus. Un test e2e le vérifie sans jamais lire le
  cookie, qui reste `HttpOnly`.

- **1.8** — **Verrouillage anti-brute-force** (FR-007) : cinq échecs dans une fenêtre glissante de dix minutes ferment le compte quinze minutes, avec audit `ACCOUNT_LOCKED` et alerte au titulaire.

  **Un `423` ne part qu'à qui connaît déjà le mot de passe.** FR-007 le demande « correct ou
  non », et exige au scénario suivant qu'aucune information ne fuite sur l'existence du
  compte : un `423` sur une adresse au hasard révélerait qu'elle a un compte. Le mot de
  passe est donc vérifié d'abord — même coût dans les deux cas — et un mauvais mot de passe
  sur un compte verrouillé rend exactement la réponse d'une adresse inconnue.

  **Un défaut trouvé par un test, pas par relecture** : le premier jet repoussait la fin du
  verrou à chaque nouvelle tentative, ce qui permettait à un tiers de garder un compte fermé
  indéfiniment — l'attaque même que le verrou prétend arrêter. Le commentaire décrivait déjà
  l'intention ; la condition manquait dans le code. Un verrou expiré peut en revanche être
  suivi d'un nouveau si les échecs continuent, et un test le fixe explicitement.

  Une seule alerte par verrouillage, au franchissement du seuil : une alerte par échec ferait
  du service un relais de courriels vers une adresse non sollicitée. Un compte inexistant
  n'en déclenche aucune, pour la même raison, et ne crée aucune ligne en base.

  La limitation par adresse IP (5 par heure) tape avant le verrou : depuis une source unique
  on ne l'atteint pas, comme le montre le contrôle manuel (cinq `401` puis un `429`). Le
  verrou vise l'attaque distribuée, que les tests reproduisent en variant l'adresse source.

- **1.10** — **Déconnexion et révocation** : livrée avec la Story 1.4.

- **1.9** — **Effacement RGPD** (FR-005, art. 17) : `POST /api/v1/me/erase` avec confirmation `DELETE`, annulation possible pendant trente jours, exécution par le binaire `klaar-effacer` à planifier.

  **Ce qui est effacé, et ce qui ne peut pas encore l'être.** Adresse, empreinte du mot de
  passe, jetons, sessions et abonnements push disparaissent. Les Missions, factures et
  traces de géolocalisation que décrit FR-005 n'existent pas encore : leurs contextes
  arrivent aux Epics 3 et suivants, et c'est écrit dans `COMPLIANCE.md` plutôt que passé
  sous silence.

  **La ligne de compte est vidée, pas supprimée** : la supprimer emporterait par cascade les
  entrées du journal d'audit, que le scénario `@security` exige de conserver. L'adresse
  devient une valeur dérivée de l'identifiant sur le domaine `.invalid`, réservé par la
  RFC 2606 — rien n'y sera jamais livré, et rien ne permet de remonter à l'origine.

  **L'annulation n'est pas dans FR-005 et en découle** : un délai de trente jours n'a de
  raison d'être que s'il est réversible. Le compte reste donc utilisable pendant l'attente,
  faute de quoi son titulaire ne pourrait pas se connecter pour annuler sa propre demande.

  **Un défaut de concurrence trouvé par un test** : deux exécutions simultanées du job
  effaçaient le même compte deux fois, et le journal d'audit prétendait alors que le droit
  avait été exercé deux fois. La mise à jour est gardée par le statut et passe en premier
  dans la transaction, ce qui sérialise les exécutions concurrentes.

  Livre au passage l'extracteur `Authentifie` — premier endpoint protégé. Un extracteur et
  non un middleware : le type dans la signature du handler **est** la déclaration que la
  route est protégée, alors qu'une route ajoutée hors du périmètre d'un middleware serait
  publique sans que rien ne le signale.

## Epic 2 — Catalogue

- **2.1** — **Catalogue MVP trilingue** (FR-008) : cinq secteurs et dix-huit Skills amorcés par migration, bounded context `klaar-catalog`.

  **La liste des Skills est une proposition, pas une donnée de conception.** Le PRD nomme
  les secteurs et ne dit rien des compétences qu'ils regroupent : celles-ci viennent des
  interventions de dépannage courantes à Bruxelles et restent à valider avec le métier.
  C'est écrit dans la migration, pour que personne ne les prenne pour un acquis.

  Les trois traductions sont obligatoires, en base comme dans le domaine. Bruxelles est
  officiellement bilingue : une entrée sans néerlandais n'est pas une entrée incomplète,
  c'est une entrée qui ne devrait pas exister. Un test refuse un jeu de données dont plus
  d'un dixième des néerlandais recopie le français — le symptôme habituel d'un
  « à compléter plus tard ». L'ordre d'affichage est explicite et non alphabétique, sans
  quoi le même catalogue apparaîtrait dans un ordre différent selon la langue.

- **2.2** — **API de lecture** (FR-008) : `GET /api/v1/catalog/sectors?locale=`, page `/catalogue`.

  **L'`ETag` porte sur le contenu servi, jamais sur une date de mise à jour** : un
  horodatage changerait à chaque redéploiement sans qu'une ligne ait bougé, et invaliderait
  tous les caches pour rien. Deux langues donnent deux `ETag` distincts — sinon un cache
  servirait le néerlandais à qui demande le français en se croyant correct.

  **`Cache-Control: public, max-age=300`**, parce que le catalogue est identique pour tout
  le monde. Un test vérifie que la réponse ne contient aucune donnée propre au demandeur :
  c'est la condition qui rend ce `public` légitime.

  L'avertissement de repli de langue est **rendu au client** et pas seulement journalisé :
  c'est à lui d'apprendre qu'il n'aura pas la langue demandée. Un catalogue vide répond 200
  avec une liste vide — un état de démarrage, pas une panne. `KLAAR_CATALOGUE_MAINTENANCE=1`
  fait répondre 503 avec `Retry-After`, ce qui distingue un retrait volontaire d'une panne.

- **2.3** — **Prix indicatifs** (FR-009) : calcul IQR, seuil d'anonymat, table d'agrégat, exposition par l'API et affichage.

  **Ce qui est livré, et ce qui manque.** L'algorithme est complet et reproduit l'exemple de
  FR-009 mot pour mot — sur `[80, 120, 150, 200, 1000]`, la fourchette rendue est 80–200 et
  non 80–1000 — qui sert donc de vecteur de test. Le job qui alimenterait la table lit
  l'historique des Missions, absent avant l'Epic 3 : la table reste vide et toutes les
  fourchettes affichent « prix sur devis ». C'est exactement le scénario `@negative` du FR :
  l'état livré **est** l'état attendu à ce stade.

  **Une fourchette est faite de deux prix réels.** Son minimum et son maximum sont des
  montants qu'un prestataire a facturés : publier une fourchette de deux Missions revient à
  publier ces deux prix. D'où le seuil de cinq, qui n'est pas une précaution statistique
  mais la condition pour que l'agrégat en soit un. Ce qu'il ne supprime pas est écrit dans
  `COMPLIANCE.md` : au seuil, deux prix sur cinq restent exposés.

  **Un garde-fou surnuméraire retiré, par un test.** Le premier jet appliquait le seuil aussi
  après exclusion des aberrations, et contredisait alors l'exemple du PRD : cinq Missions
  dont une aberrante n'en laissent que quatre. Le seuil porte sur l'échantillon d'entrée.

  La mention « prix indicatif, prix final fixé par le prestataire » accompagne
  obligatoirement toute fourchette — sans elle, une fourchette se lit comme un devis, et
  l'écart devient un litige.

- **1.6 (partielle)** — **Prestataires** (FR-003) : agrégat `Provider`, numéro BCE avec sa clé de contrôle, compétences, recherche par rayon.

  **Ce qui est vérifiable hors ligne l'est.** Un numéro BCE porte une clé : les deux derniers
  chiffres valent `97 - (les huit premiers modulo 97)`. Cette vérification attrape une faute
  de frappe ou deux chiffres intervertis — un test le montre en inversant deux chiffres d'un
  numéro valide. Elle ne dit rien de l'existence de l'entreprise, qui demande l'API de la BCE.

  **Le KYC n'est pas fourni, et le type l'impose.** Un prestataire naît `PENDING_KYC` ; le
  seul chemin vers `ACTIVE` réclame une `PreuveKyc`, type opaque sans constructeur littéral.
  Deux fabriques seulement : `depuis_verification_bce`, **qui n'a aucun appelant** faute
  d'adaptateur, et `demonstration`, dont le nom dit ce qu'elle vaut. L'origine est conservée
  en base et une contrainte interdit qu'un actif n'en porte aucune, de sorte qu'un
  prestataire non contrôlé se retrouve par une requête longtemps après.

  Le peuplement de démonstration est un **binaire, pas un endpoint** : une commande hors
  ligne ne s'atteint pas par HTTP, alors qu'une route d'activation serait une route qu'on
  peut oublier d'enlever.

  **Un défaut de montage trouvé par les tests** : mes numéros BCE de test étaient
  déterministes et entraient en collision avec les lignes de l'exécution précédente. La
  contrainte d'unicité faisait son travail ; c'est le montage qui présumait une base vierge.

## Epic 3 — Demandes et matching

- **3.1** — **Soumission d'une Demande** (FR-011) : `POST /api/v1/requests`, page `/demande`, position PostGIS, détection de doublon, quota horaire par compte.

  **Le périmètre est un rectangle, pas la Région.** Le contrôle `GEO_OUTSIDE_RBC` ramène les
  dix-neuf communes à un rectangle englobant, qui **sur-accepte** : Kraainem et Drogenbos,
  en Brabant flamand, y tombent. Sur-accepter fait entrer quelques Demandes hors périmètre
  qu'un prestataire refusera ; sous-accepter refuserait des Bruxellois chez eux. Un test
  **constate** cette sur-acceptation au lieu de la masquer, et devra être inversé le jour où
  le contour réel arrivera (Story 0.11).

  **L'ordre des arguments de `ST_MakePoint` est vérifié par un test** qui relit la position
  depuis PostGIS. C'est le genre de détail qu'aucun test unitaire n'attrape et qui place
  Bruxelles au large de la Somalie sans qu'aucune contrainte ne s'en aperçoive.

  Le doublon rend la Demande existante en `200` plutôt qu'un `409` : l'utilisateur veut
  retrouver la sienne, pas apprendre qu'il a cliqué deux fois. Il est cherché **avant** le
  quota horaire, sans quoi cinq double-clics se verraient refuser pour excès. La position
  n'est demandée qu'à l'envoi : une invite de géolocalisation à l'arrivée est refusée par
  réflexe, et ce refus est définitif dans plusieurs navigateurs.

  **Deux limites assumées** : la précondition « méthode de paiement » est désactivée dans ce
  déploiement, faute de compte Stripe — le contrôle existe et est actif par défaut, sa
  désactivation est journalisée au démarrage. Et le matching n'est pas déclenché : une
  Demande naît `BROADCASTING` et y reste, ce que la page dit à l'utilisateur plutôt que de
  laisser croire qu'un dépanneur est en route.

- **3.2** — **Matching géolocalisé** (FR-012) : recherche par rayon, score explicable, trace AI Act.

  **La réponse à l'AI Act est la signature de la fonction, pas une promesse dans un
  commentaire.** `calculer` ne reçoit que trois nombres — distance, ancienneté du contrôle,
  note éventuelle. Elle ne peut pas voir un nom, une adresse, une langue ou une photo, parce
  qu'on ne les lui donne pas. Un biais sur un attribut protégé demanderait d'abord de changer
  cette signature, ce qui se voit à la relecture d'une ligne.

  **Le rating que FR-012 nomme n'existe pas encore.** Il est traité comme absent et non comme
  nul, et son poids est redistribué : sinon un prestataire sans historique serait classé
  derrière un prestataire mal noté, aucun nouveau venu ne recevrait rien, et le classement se
  figerait sur les premiers arrivés. L'absence est inscrite dans la ventilation — la trace dit
  aussi ce qui manquait.

  **La trace conserve les écartés**, pas seulement les retenus : ne garder que les retenus la
  rendrait inutile pour la seule personne à qui elle est destinée. Elle est écrite avant que
  les candidats ne soient rendus, et un échec d'écriture annule le tour.

  **Deux défauts de montage trouvés par les tests** : une propriété que j'avais affirmée sur
  le score — « cinq étoiles valent l'absence de note » — était fausse, et le calcul avait
  raison ; et une migration ajoutant une contrainte échouait sur les lignes déjà en base,
  faute de les reprendre d'abord.

- **3.3** — **Notification des candidats** : les prestataires retenus reçoivent un push.

  **Une notification s'affiche sur un écran verrouillé**, lisible par quiconque passe à côté
  du téléphone. Elle ne porte ni la description du problème, ni l'adresse, ni rien du
  demandeur : le secteur, la distance arrondie et l'urgence. Le chiffrement RFC 8291 n'y
  change rien — il protège le transit, pas l'affichage, et les deux problèmes sont distincts.
  La distance est arrondie à la centaine de mètres : au mètre près, croisée avec la position
  du prestataire, elle situerait le demandeur chez lui.

  **`candidats` et `notifies` sont deux nombres distincts** dans la réponse. Un prestataire
  retenu sans abonnement push verra la Demande en ouvrant l'application ; les confondre ferait
  croire à qui attend que dix personnes ont été réveillées alors que personne n'a rien reçu.

  Le port `PushNotifier` est devenu asynchrone au passage. Il avait été défini synchrone au
  Sprint 0 « pour rester utilisable par un adaptateur de test » ; il n'a jamais eu
  d'implémenteur, et l'adaptateur réel ne pouvait pas le satisfaire — un envoi push est un
  appel réseau.

- **3.4** — **Acceptation par le premier répondant** : `POST /api/v1/requests/{id}/accept`.

  **Toute la story tient dans une clause `WHERE`.** Cinq prestataires notifiés
  peuvent toucher « accepter » dans la même seconde. Lire le statut puis
  l'écrire en laisserait passer deux, et deux camionnettes partiraient pour une
  seule fuite. C'est
  `UPDATE demande SET statut='MATCHED' WHERE id=$1 AND statut='BROADCASTING' RETURNING id`
  qui tranche : PostgreSQL sérialise les écritures sur une même ligne. Vérifié
  par de vraies acceptations concurrentes, à deux puis à dix.

  La bascule de la Demande et la création de la Mission sont **une seule
  transaction** : une Demande `MATCHED` sans Mission promettrait une
  intervention dont personne ne porte la trace.

  « Une Mission à la fois » est tenu par un **index unique partiel**, pas par un
  contrôle applicatif — vérifier puis insérer laisserait passer deux
  acceptations simultanées. Le refus défait toute la transaction, donc une
  Demande convoitée par un prestataire déjà occupé reste diffusée.

  L'éligibilité **et le secteur** sont revérifiés à l'acceptation. Le secteur ne
  figurait pas dans le FR : la route est ouverte à tout prestataire actif, et un
  serrurier qui connaît l'identifiant d'une Demande de plomberie pouvait la
  rafler.

  **L'expiration ne se lit pas dans le statut.** Aucune tâche de fond ne fait
  basculer une Demande passé cinq minutes : elle reste `BROADCASTING` en base, et
  l'expiration se constate quand quelqu'un tente d'agir dessus. Dette assumée,
  écrite dans le domaine.

  La `Mission` n'a qu'un statut, `ASSIGNED` : sa machine à états appartient à
  FR-018. Il n'existe pas encore d'interface prestataire.

- **3.5** — **Annulation par le demandeur** : `DELETE /api/v1/requests/{id}`,
  motif facultatif en paramètre.

  **Le motif est un vocabulaire fermé de cinq codes, pas un texte libre.** Le
  FR veut le motif « pour analytics » ; un champ libre inviterait à écrire
  « le plombier d'hier était désagréable, j'habite au 12 rue X », c'est-à-dire
  une donnée personnelle non sollicitée dans un champ dont la finalité annoncée
  est statistique. Un motif hors vocabulaire est **refusé** et non ramené sur
  `OTHER` : le ramener silencieusement ferait passer une faute de frappe pour un
  choix délibéré.

  Le motif vit sur la Demande et non dans le journal d'audit : il s'efface donc
  avec elle quand le compte est effacé (art. 17). Une contrainte de base impose
  qu'un motif n'existe que sur une Demande annulée.

  **Écart au FR : 404 et non 403** pour la Demande d'autrui. Distinguer « elle
  n'existe pas » de « elle n'est pas à vous » laisserait apprendre quelles
  Demandes existent, et l'élargissement répond déjà ainsi.

  Après attribution, l'annulation est refusée : le prestataire est peut-être
  déjà en route, et c'est la Mission qu'il faut alors annuler (FR-023). La
  course annulation/acceptation est tranchée par PostgreSQL ; si l'annulation
  gagne, le prestataire reçoit un 410 — la Demande a existé et n'existe plus, ce
  qui n'est pas « quelqu'un d'autre l'a ».

  L'avis envoyé aux prestataires ne dit pas pourquoi : le motif appartient au
  demandeur, et « trouvé ailleurs » diffusé à dix entreprises se lit vite comme
  un reproche.

- **3.6** — **Fin de tour et élargissement** : `POST /api/v1/requests/{id}/expand-radius`,
  plus le binaire `klaar-expirer`.

  **Contradiction du PRD tranchée sans arbitrage.** FR-013 refusait une
  acceptation après cinq minutes, FR-015 annonce `NO_MATCH` après trente
  secondes. Trente secondes l'emportent, et le choix ne coûte rien : cette règle
  rejette **aussi** tout ce que la règle à cinq minutes rejetait. L'inverse est
  faux — attendre cinq minutes priverait le demandeur de la réponse que FR-015
  lui promet en trente secondes, alors qu'il est devant une fuite.

  Le délai court depuis le **début du tour** et non depuis la création : un
  élargissement rouvre une fenêtre entière, sinon la deuxième chance serait déjà
  écoulée au moment où on l'offre.

  **L'échelle s'arrête à vingt kilomètres parce que la Région s'arrête là.**
  5 → 10 → 15 → 20 km : depuis n'importe quel point de la Région de
  Bruxelles-Capitale, vingt kilomètres la couvrent entièrement. Le quatrième
  essai **annule** la Demande (FR-015) : laisser un `NO_MATCH` entretiendrait
  l'idée que quelque chose peut encore arriver.

  Le score se normalise désormais sur le rayon du tour. Sans cela, après un
  élargissement, tout candidat au-delà de cinq kilomètres marquait zéro de
  proximité et le classement n'ordonnait plus rien. Le test de signature AI Act
  a échoué à l'ajout du paramètre, ce qui est son rôle : `rayon_metres` vaut
  pareil pour tous les candidats d'un même tour, donc il n'en distingue aucun.

  `klaar-expirer` est un binaire à lancer toutes les dix secondes, pas une tâche
  de fond — même raison que `klaar-effacer`. Son `UPDATE … RETURNING …
  FOR UPDATE SKIP LOCKED` garantit qu'aucun demandeur n'est prévenu deux fois.
  Un retard du balayage ne laisse rien passer : l'expiration se constate aussi à
  la lecture, donc personne ne peut accepter une Demande échue entre deux
  passages.

  Limite : l'avis de fin de tour part en français quel que soit le compte, et il
  n'existe toujours pas d'interface prestataire ni de bouton « élargir ».

- **3.7** — **Disponibilité du prestataire** : `GET`/`PATCH
  /api/v1/providers/me/availability`, plus la page `/prestataire`.

  **Trois notions distinctes, et c'est tout l'enjeu.** Un prestataire peut être
  écarté du matching pour trois raisons sans rapport : son **statut** (en
  attente de contrôle, suspendu), sa **disponibilité** (« je suis en congé »),
  et son **occupation** (une Mission en cours). Les confondre ferait d'une pause
  une sanction. Seule la deuxième se règle ; les deux autres s'affichent, parce
  qu'un prestataire en service et pourtant jamais sollicité conclurait sinon que
  le service est cassé.

  Le « busy auto » comblait un vrai trou ouvert par la Story 3.4 : un
  prestataire déjà en Mission recevait des notifications qu'il ne pouvait
  qu'échouer à accepter, et volait sa place à quelqu'un de libre.

  Le **rayon d'intervention** est celui du prestataire, distinct de celui du
  tour : le tour dit jusqu'où la Demande cherche, celui-ci jusqu'où le
  prestataire accepte d'aller. Le défaut est le maximum, pas une valeur
  médiane — les fiches existantes n'ont jamais exprimé de limite, leur en prêter
  une les retirerait du service sans qu'elles aient rien demandé.

  `peut_etre_sollicite` a changé de sens, et deux tests l'ont signalé : il ne
  regardait que le statut alors que la base filtrait déjà sur statut **et**
  disponibilité. Le domaine mentait sur ce que le système faisait.

  **Multi-zone n'est pas livré.** Des zones d'intervention disjointes
  demanderaient un modèle géographique autre qu'un point et un rayon ; dans le
  PRD, « zone » désigne par ailleurs le lancement multi-villes, hors périmètre.

- **3.8** — **Trace immuable et audit anti-biais** : binaire `klaar-audit-biais`
  (`make audit-biais`), déclencheur d'immuabilité, signature chaînée.

  **Deux des trois axes de biais demandés ne sont pas auditables, et c'est
  volontaire.** Le **genre** n'est pas collecté : l'auditer supposerait de le
  demander, donc de créer la donnée qui rendrait la discrimination possible.
  L'**ethnie estimée** suppose de l'estimer, typiquement depuis un nom, ce qui
  est exactement la pratique que l'AI Act et le RGPD art. 9 proscrivent. La
  garantie sur ces deux axes est structurelle et plus forte qu'un audit
  statistique : `calculer` reçoit quatre nombres et ne peut discriminer sur un
  attribut qu'on ne lui donne pas. Le rapport le dit plutôt que de laisser une
  case vide.

  Le **quartier**, lui, est le biais réel : le score est dominé par la
  proximité, donc la qualité du service suit la densité de prestataires. Le
  rapport le mesure par maille d'environ un kilomètre, et sort surtout l'**écart
  entre la maille la mieux servie et la moins bien servie** — le chiffre qui dit
  s'il y a un problème. Seuil de k-anonymat à cinq Demandes, et le nombre de
  mailles supprimées est annoncé : les taire ferait passer une couverture
  partielle pour complète.

  **La signature est chaînée, et c'est ce qui la rend utile.** Un HMAC par ligne
  détecte une modification mais ne dit rien d'une suppression — or supprimer est
  exactement ce que ferait quelqu'un voulant effacer un matching
  discriminatoire. Prix payé : la tête de chaîne est verrouillée pendant
  l'écriture, donc deux tours de matching simultanés s'y sérialisent.

  **Portée réelle** : la signature détecte une altération faite depuis la base,
  pas une compromission du serveur où la clé est lisible. Et la chaîne étant
  globale, une rotation de clé casse la vérification à partir du premier maillon
  signé avec la nouvelle.

  **La clé est optionnelle**, contrairement au secret des jetons : sans elle la
  trace est écrite non signée, ce qui explique toujours une décision, alors que
  l'absence de trace ne s'explique pas. Les lignes non signées sont comptées à
  part — les ranger avec les vérifiées produirait un rapport rassurant sans
  preuve.

  L'immuabilité est un **déclencheur**, pas une convention : supprimer une
  Demande tracée échoue bruyamment plutôt que d'emporter sa trace par cascade.

- **4.3** — **Machine à états de la Mission** : `PATCH /api/v1/missions/{id}/status`.

  `ASSIGNED` devient **`ACCEPTED`** : la Story 3.4 avait nommé l'état initial
  faute que FR-013 le nomme, FR-018 l'appelle ainsi dans toutes ses transitions.
  Aligner coûte une migration et évite un synonyme privé.

  **La machine à états est une fonction totale, pas une suite de `if`** :
  `transitions_possibles` énumère ce qui est permis depuis chaque statut, et le
  `match` exhaustif fait qu'ajouter un état sans dire ce qu'on en fait ne
  compile pas.

  **Un défaut réel comblé** : le filtre « déjà en mission » du matching ne
  connaissait que `ASSIGNED`, si bien qu'un prestataire en route aurait de
  nouveau reçu des Demandes. `COMPLETED` et `CANCELLED` le libèrent, ce qui
  l'aurait sinon bloqué à vie — la note laissée en 3.4 est levée.

  **La position est facultative** : l'exiger rendrait l'autorisation de
  localisation de fait obligatoire, alors que quelqu'un sans GPS doit pouvoir
  déclarer qu'il est arrivé. Sortir de la Région se **consigne** et ne refuse
  pas ; l'alerte est journalisée sans la position, le journal n'ayant pas à dire
  où se trouve un prestataire.

  **Deux horodatages** : celui du client et celui du serveur. Une transition
  faite hors connexion garde sa date, dans une tolérance de cinq minutes —
  au-delà, une intervention pourrait se prétendre commencée une heure plus tôt.

  La bascule et l'entrée d'historique sont une seule transaction, avec garde sur
  l'état de départ. L'historique est append-only par déclencheur.

  Non livré : la validation de fin (FR-021), les pénalités
  d'annulation (FR-022) — le domaine connaît la transition vers `CANCELLED`,
  aucune route ne l'expose faute de la règle qui doit l'accompagner.

- **4.10** — **Interface prestataire et suivi demandeur** (hors plan initial) :
  `GET /api/v1/requests/{id}`, `GET /api/v1/providers/me/requests`,
  `GET /api/v1/missions/{id}`, plus les écrans correspondants.

  **Ajoutée parce qu'aucun parcours ne pouvait être mené de bout en bout dans un
  navigateur** : accepter une Demande et faire avancer une Mission n'existaient
  qu'en API, et les notifications pointaient déjà vers `/demande?id=…`, une page
  qui affichait un formulaire vierge.

  **L'asymétrie des deux vues est le cœur de la story.** Le prestataire voit,
  avant d'accepter : le secteur, la description, l'urgence, une distance. **Pas
  l'adresse.** Elle ne lui est révélée qu'une fois la Mission à lui. Ce n'est pas
  une consigne dans un commentaire — le type qui porte cette vue n'a pas de champ
  de position, et un test vérifie que la réponse HTTP n'en porte aucune trace.

  Le demandeur apprend le **nom de l'entreprise** dès l'attribution : savoir qui
  va sonner à sa porte est le minimum.

  Les boutons d'étape viennent du serveur (`suites`) : recopier la machine à
  états dans l'interface la ferait diverger.

  Le suivi sonde et s'arrête quand la Demande est close. Depuis la Story 4.9, une
  socket double ce sondage et le ralentit à trente secondes tant qu'elle vit.

- **7.1 / 7.5** — **Notation double sens et réputation Wilson** (FR-033, FR-037) :
  `POST /api/v1/missions/{id}/rating`, `GET .../ratings`.

  **Les deux notes se dévoilent ensemble.** C'est la seule protection contre les
  représailles : si celle du demandeur s'affichait d'abord, le prestataire
  ajusterait la sienne. Elles se publient quand les deux existent, ou quand la
  fenêtre de quatorze jours se ferme.

  **La réputation est une borne basse de Wilson, pas une moyenne.** Un
  prestataire avec une seule note de cinq étoiles afficherait 5,0 et passerait
  devant quelqu'un qui en a cinquante à 4,5 ; la borne basse répond à la bonne
  question — « quelle est la note la plus basse compatible avec ce qu'on a
  observé ». Une note isolée est peu informative, et le calcul le dit.

  **Un prior neutre pour les non-notés, adopté après avoir été écarté à tort.**
  Le premier réflexe avait été de laisser le score redistribuer le poids de la
  note absente, au motif que c'était plus honnête que d'inventer une réputation.
  C'est faux : redistribuer revient à noter le prestataire sur sa propre moyenne
  des autres critères, donc à lui prêter la meilleure note compatible avec son
  profil. À distance égale, un compte tout neuf passait devant un artisan à
  cinquante avis. L'inconnu vaut désormais quatre étoiles, et la trace d'audit
  consigne qu'un classement s'est fait sur une note prêtée.

  La boucle du matching se referme : le score réservait un emplacement pour la
  note depuis FR-012, il est rempli.

- **4.7** — **Annuler une intervention en cours** (FR-022) :
  `POST /api/v1/missions/{id}/cancel`, une seule route pour les deux parties.

  **Qui annule est déduit du jeton**, et c'est cela qui détermine le coût. Le
  demandeur qui renonce récupère son argent, moins trente euros si le
  prestataire était **déjà sur place** — en route, rien n'est dû, parce que rien
  ne dit qu'il était arrivé. Le prestataire qui se désiste rend tout, et son
  désistement est compté : **trois en trente jours suspendent son compte**, ce
  que l'écran annonce avant le clic.

  Le forfait est borné par ce qui était engagé : sans cela, une annulation sur
  une Mission sans devis accepté produirait un remboursement négatif,
  c'est-à-dire une dette inventée.

  **La suspension suit l'annulation mais n'en fait pas partie** : son échec ne
  défait pas une annulation déjà prononcée. La Mission est close, c'est le fait
  principal, et le compteur se rattrape au désistement suivant.

  Non livré : le remboursement effectif (Stripe), le signalement de fraude du
  demandeur à cinq annulations en sept jours, et la levée automatique de la
  suspension au bout de sept jours.

- **4.2 / 4.6** — **Accepter un devis, puis valider la fin** (FR-017, FR-021) :
  `POST /api/v1/missions/{id}/accept-quote`, `refuse-quote`, `validate`.

  **Livrées sans Stripe, et c'est écrit.** L'accord et la validation sont
  enregistrés ; la capture et le virement rejoindront l'Epic 5. Un accord sans
  capture est un état honnête — le devis dit ce qui a été convenu — là où
  attendre le compte aurait laissé le demandeur devant un devis qu'il ne peut ni
  accepter ni refuser.

  **La répartition est celle du PRD, au centime près** : 180 € HTVA à 21 % font
  217,80 € ; la commission de 18 % sur le HTVA fait 32,40 €, sa TVA 6,80 €, donc
  39,20 € TTC ; il reste 178,60 € au prestataire. L'invariant « la somme des
  parts fait le total » est gravé par une contrainte : une erreur d'arrondi
  échouera à l'écriture plutôt que de se retrouver dans une comptabilité. La
  commission porte sur le hors-TVA — la TVA du devis est due à l'État, et en
  prélever une part reviendrait à se servir dans une taxe.

  **Soixante-douze heures, puis le service valide à sa place.** Sans ce délai, un
  demandeur qui ne rouvre jamais l'application retiendrait indéfiniment l'argent
  d'un travail fait, et c'est le prestataire qui paierait le silence. Au-delà de
  cinq cents euros, la libération attend un second regard.

  **Deux dépendances silencieuses révélées en cours de route.** `COMPLETED` a
  cessé d'être terminal : le chiffrage d'un devis s'appuyait sur `est_terminal`
  et aurait rouvert le devis d'une intervention faite ; et la transition vers
  `VALIDATED` devenait atteignable par la route de statut du prestataire, qui
  aurait pu signer la réception de son propre travail. Les deux sont fermées, et
  testées.

  Non livré : la capture 3DS2, le virement, le gel en cas de litige (FR-034), le
  courriel récapitulatif du balayage (Story 9.2) et la console ops qui lèvera les
  libérations `PENDING_OPS` (Epic 8).

- **4.9** — **Statut de Mission en temps réel** :
  `POST /api/v1/realtime/ticket`, `GET /api/v1/missions/{id}/events`.

  **Le canal est PostgreSQL, pas la mémoire du service.** Un canal en mémoire ne
  relie que les clients connectés au même exemplaire ; dès qu'il y en a deux, la
  moitié des utilisateurs cesse de recevoir quoi que ce soit, et rien ne le
  signale — l'écran ne bouge simplement plus. `LISTEN`/`NOTIFY` relie tous les
  exemplaires par le seul point qu'ils partagent déjà.

  **L'avis part dans la même transaction que le changement.** PostgreSQL ne le
  délivre qu'au `COMMIT` : une écriture abandonnée n'annonce rien, une écriture
  commise annonce toujours. Deux tests d'intégration le vérifient en ouvrant une
  vraie écoute pendant qu'une vraie transaction s'écrit, y compris le cas d'une
  transition perdue à la course.

  **L'événement ne porte aucun détail** : un identifiant de Mission, un genre,
  un instant. La charge d'un `NOTIFY` traverse la base et ses journaux et part
  vers tous les exemplaires. Le client relit par les routes qui vérifient déjà
  ses droits — c'est aussi ce qui permet de diffuser le même événement au
  demandeur et au prestataire, qui ne voient pas la même chose.

  **Un billet, pas un jeton dans l'URL.** Un navigateur ne peut pas poser
  d'en-tête `Authorization` sur une WebSocket, et une URL de socket finit dans
  les journaux du serveur, du proxy et l'historique du navigateur. Le billet
  vaut trente secondes et une seule fois ; seul son condensé est conservé. Un
  billet présenté est dépensé même quand l'accès est refusé, sinon le même
  servirait à essayer des identifiants de Mission jusqu'à en trouver un.

  **Le sondage reste, ralenti de cinq à trente secondes.** Une socket coupée par
  un proxy ne se signale pas ; un écran qui cesse de bouger sans le dire est
  pire qu'un écran lent. Le temps réel accélère, il ne remplace pas.

  Écart au plan : `actix-ws` plutôt que `actix-web-actors`, l'API que le projet
  actix recommande désormais. Limite assumée : les billets vivent en mémoire,
  donc par exemplaire du service.

- **4.1** — **Envoi d'un devis** (FR-016) : `POST /api/v1/missions/{id}/quote`,
  et le devis rendu dans les deux suivis.

  **Le prix vient du prestataire, et c'est un test qui le dit.** Le formulaire
  n'a aucune valeur par défaut, aucun montant conseillé, aucun rappel du dernier
  prix pratiqué : une suggestion serait une fixation de prix douce, et c'est ce
  que regarde la loi belge du 26 avril 2024 sur le travail de plateforme
  (invariant §10.2). `security_le_montant_rendu_est_exactement_celui_propose`
  parcourt toute l'échelle admissible et vérifie que rien n'a bougé, du clavier
  jusqu'à la ligne écrite.

  **La TVA est calculée une fois et conservée.** 180 € HTVA à 21 % font
  217,80 €, et ce total est écrit tel quel : recalculer à la lecture après un
  changement de taux réécrirait un document contractuel. Les trois taux belges
  applicables sont admis, pas un de plus, et tout taux réduit exige une preuve.

  **Deux comptages, tenus par la base et pas au-dessus.** Un seul devis en
  attente par Mission — index unique partiel — et trois devis au maximum — un
  `WHERE` sur l'insertion. Lire puis décider laisserait deux envois simultanés
  poser deux prix, et le demandeur ne saurait pas lequel l'engage. Un devis
  vivant prime sur le plafond : annuler la Mission alors qu'une offre attend une
  réponse détruirait ce que le demandeur est en train de lire.

  Le devis expire au bout d'une heure ; le balayage de `klaar-expirer` l'éteint
  et prévient son émetteur. La notification ne porte **pas le montant** : elle
  s'affiche sur un écran verrouillé, et ce que coûte la réparation d'une
  chaudière est une information sur la situation de quelqu'un.

  Non livré, et écrit comme tel : la pré-autorisation du séquestre (Stripe, avec
  la Story 4.2), l'acceptation ou le refus depuis l'écran (FR-017), et la remise
  en diffusion après un second devis expiré — défaire une attribution suppose
  une décision sur l'argent engagé que FR-017 n'a pas tranchée.

- **3.9** — **File d'attente hors ligne** (hors plan initial) : une Demande
  écrite sans réseau part au retour de la connexion.

  **La file existait sans producteur, et son rejeu partait sans jeton.** Le
  module est là depuis la Story 0.2, mais aucun formulaire ne l'alimentait et
  `flushQueue` rejouait sans autorisation — une écriture authentifiée aurait
  reçu un 401 et fini dans les refusées. Pendant ce temps, la pastille annonçait
  « vos saisies sont conservées ».

  Le rejeu **reprend la session** depuis le cookie de rafraîchissement, le jeton
  d'accès ne survivant pas au rechargement. Il ne le fait que s'il y a quelque
  chose à rejouer : sinon la file ferait une rotation de refresh toutes les
  trente secondes, et une rotation trop fréquente finit par ressembler au rejeu
  d'un jeton volé.

  Limite : le service ne lit pas encore `Idempotency-Key`. Ce qui protège d'une
  double soumission est la fenêtre de doublon de cinq minutes.

## Parcours filmés — documentation vivante

`scripts/parcours-filmes.sh` monte un service **réel** — PostgreSQL, migrations,
jeu de démonstration, `klaar-api` — puis déroule chaque parcours dans un
navigateur et en garde la vidéo. `npm run vitrine` assemble ensuite la page
publiée, vidéos incluses ; la CI la déploie sur GitHub Pages.

**Une suite à part, et pas un interrupteur sur l'existante.** Les tests de
`tests/e2e` vérifient : ils vont vite, simulent l'API et n'ont pas à être
regardés. Ceux de `tests/demo` **montrent** : ils tournent contre le service
réel, à vitesse humaine, et leur produit est une vidéo. Une seconde au moins
sépare chaque geste, la saisie se fait caractère par caractère, et un bandeau
incrusté dit ce qui se passe et qui agit.

**Le parcours à deux acteurs est le seul qui prouve la valeur** : ce qui compte
n'est pas ce que chacun fait, mais ce que chacun voit *pendant* que l'autre
agit. Deux contextes de navigateur, deux vidéos, publiées côte à côte.

Site et API sont servis sur la **même origine** par un petit serveur sans
dépendance, comme derrière le proxy inverse d'un déploiement réel. Pointer le
front ailleurs aurait demandé du CORS — relâcher une garantie de production pour
une démonstration.

**Deux défauts réels trouvés en filmant**, qu'aucun test simulé ne pouvait
voir : deux îlots Svelte se déconnectaient mutuellement en rafraîchissant la
session en parallèle (la rotation du refresh y voyait un rejeu), et l'indicateur
de connexion affirmait « En ligne » avant d'avoir rien vérifié.

## CI, premier run réel

Le premier run CI a échoué deux fois avant de passer, corrections gardées ici pour mémoire :
1. `cargo-deny-action` a un input `manifest-path` dédié ; le passer aussi via `arguments` duplique le flag
2. Le job contrat API compilait `klaar-api` à la volée avant de le lancer en arrière-plan puis d'attendre 20 s max : en CI à froid la compilation seule dépasse ce délai. Corrigé en compilant d'abord (`cargo build`), puis en laissant `schemathesis --wait-for-schema=30` gérer l'attente de démarrage du binaire déjà prêt

## Ce qui manque avant que le Sprint 0 soit réellement terminé

Stories 0.7a/0.7b/0.7c (Terraform, salt-ssh, GitOps) et 0.11 (tile-server OSM + Valhalla) : elles nécessitent un compte OVH provisionné, donc restent bloquées tant qu'il n'y a pas de client payant. Ce n'est pas un manque d'effort, c'est un prérequis qui n'existe pas ici.

La Story 0.12 (push) l'était aussi, pour des comptes développeur payants ; **ADR-010 l'a débloquée** en remplaçant le PoC Tauri par Web Push VAPID, qui se vérifie intégralement en local. Les Stories 0.8 et 0.10 restent partielles (cf. ci-dessus).
