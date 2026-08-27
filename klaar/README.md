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

## CI, premier run réel

Le premier run CI a échoué deux fois avant de passer, corrections gardées ici pour mémoire :
1. `cargo-deny-action` a un input `manifest-path` dédié ; le passer aussi via `arguments` duplique le flag
2. Le job contrat API compilait `klaar-api` à la volée avant de le lancer en arrière-plan puis d'attendre 20 s max : en CI à froid la compilation seule dépasse ce délai. Corrigé en compilant d'abord (`cargo build`), puis en laissant `schemathesis --wait-for-schema=30` gérer l'attente de démarrage du binaire déjà prêt

## Ce qui manque avant que le Sprint 0 soit réellement terminé

Stories 0.7a/0.7b/0.7c (Terraform, salt-ssh, GitOps) et 0.11 (tile-server OSM + Valhalla) : elles nécessitent un compte OVH provisionné, donc restent bloquées tant qu'il n'y a pas de client payant. Ce n'est pas un manque d'effort, c'est un prérequis qui n'existe pas ici.

La Story 0.12 (push) l'était aussi, pour des comptes développeur payants ; **ADR-010 l'a débloquée** en remplaçant le PoC Tauri par Web Push VAPID, qui se vérifie intégralement en local. Les Stories 0.8 et 0.10 restent partielles (cf. ci-dessus).
