# Klaar

Marketplace de dépannage et de services à la demande, conçue pour la Région de
Bruxelles-Capitale. Backend Rust en architecture hexagonale, PWA Astro + Svelte,
PostgreSQL/PostGIS.

*Klaar* : « prêt, terminé » en néerlandais, employé tel quel dans le français bruxellois.
C'est l'état qu'on veut à la fin d'une intervention.

> **Statut : socle en place, métier à écrire.** Le backend sert un seul endpoint
> (`/api/v1/health`) et les 7 bounded contexts sont des crates vides. La PWA est une
> coquille installable et fonctionnelle hors ligne, sans parcours métier. Ce dépôt vaut
> aujourd'hui pour sa **conception** et son **socle de délivrabilité**, pas pour ses
> fonctionnalités.

## Ce que c'est vraiment

Une vitrine de la **Méthode Foyer** ([`gilmry/foyer`](https://github.com/gilmry/foyer)) :
un cadre de développement logiciel sobre, durable et transmissible. Le dépôt montre à quoi
ressemble un projet mené selon cette méthode, depuis les exigences jusqu'au code, avec sa
traçabilité et ses réserves assumées.

L'intérêt n'est pas le produit. C'est la chaîne : 68 exigences fonctionnelles toutes
déclinées en Gherkin sur quatre classes (`@happy @negative @edge @security`), une
architecture hexagonale qui isole les traitements réglementés, et des ADR qui tracent
chaque décision structurante, y compris celles qui ont été renversées.

## Structure

```
klaar/
├── web/                PWA Astro + Svelte (ADR-010) — manifeste, service worker,
│                       queue d'écritures hors-ligne IndexedDB
├── crates/             workspace Cargo (19 crates)
│   ├── klaar-shared-kernel/     value objects (Email, Geo, Money, VatRate, Locale…)
│   ├── klaar-{identity,catalog,matching,intervention,payment,messaging,trust}/
│   │                            les 7 bounded contexts cœur (Domain) — stubs
│   ├── klaar-application/       ports + use cases
│   ├── klaar-*-adapter/         adapters Infrastructure — stubs
│   └── klaar-api/               API HTTP actix-web + OpenAPI utoipa
├── migrations/         refinery, embarquées dans le binaire
├── observability/      Prometheus + Grafana provisionnés
└── packages/klaar-client/       client TypeScript généré depuis l'OpenAPI

docs/
├── adr/                10 décisions d'architecture
└── bmad-livrables/     PRD, Architecture, Epics & Stories
```

## Démarrer

```sh
cd klaar
make bootstrap   # build backend + tests + PWA, idempotent
make db-up       # PostgreSQL + PostGIS local
make migrate     # migrations refinery, idempotentes
```

Détail dans [`klaar/README.md`](klaar/README.md), y compris l'observabilité locale et le
codegen du client TypeScript.

## Conformité

**Lisez [`klaar/COMPLIANCE.md`](klaar/COMPLIANCE.md) avant tout déploiement.**

Ce code met en œuvre des flux relevant du RGPD (géolocalisation), de DSP2 (séquestre de
paiement), de l'AI Act (matching algorithmique) et de la loi belge du 26 avril 2024 sur le
travail de plateforme. L'analyse d'impact RGPD n'est pas fournie et elle est légalement
obligatoire *avant* tout traitement de géolocalisation. La clause *AS IS* de MIT couvre la
garantie logicielle, pas la conformité de votre déploiement.

## Livrables non publiés

Trois des huit livrables de conception restent privés, ainsi que le Product Brief : ils
contiennent le chiffrage commercial et la situation de l'intervenant. Les documents publiés
y font encore quelques renvois (`00-Capability-Breakdown-Estimation.md`,
`01-Product-Brief.md`, `05-Validation.md`, `06-Chef-de-projet.md`, `07-Estimateur.md`) qui
resteront sans cible. C'est délibéré.

Une chose vaut d'être mentionnée puisqu'elle n'est pas vérifiable ici : le rapport de
validation de ce projet a été refait après qu'une première passe se soit auto-attribué
100/100. Un relecteur chargé de réfuter qui se décerne un sans-faute signale de la
complaisance, pas de la qualité. La seconde passe a rendu un PASS conditionnel avec sept
réserves ouvertes.

## Décisions notables

- **[ADR-010](docs/adr/ADR-010-stack-pwa-only.md)** retire Tauri : le client est une PWA,
  et rien d'autre. Ça supprime le seul point de passage obligé sans plan B du projet, au
  prix de la géolocalisation en arrière-plan, qui devient une capacité absente et non un
  chantier reporté.
- **[ADR-009](docs/adr/ADR-009-license-mit.md)** fait passer le code en MIT, en
  documentant que ce n'est pas la licence que la doctrine désigne.

## Licence

MIT — voir [`klaar/LICENSE.md`](klaar/LICENSE.md).

Le choix est tracé dans [`docs/adr/ADR-009-license-mit.md`](docs/adr/ADR-009-license-mit.md),
qui renverse ADR-005 (propriétaire) et documente honnêtement ce que MIT coûte : c'est une
déviation de la réciprocité *share-alike* que porte le Manifeste Maury, et contrairement à
la décision qu'il remplace, **elle n'est pas réversible**.

---

*Dérivé du Manifeste Maury ([`gilmry/manifest`](https://github.com/gilmry/manifest),
CC BY-SA 4.0). Méthode Foyer.*
