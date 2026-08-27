# ADR-010 — Stack client : PWA Astro + Svelte uniquement, abandon de Tauri

- **Statut** : Accepté — **remplace [ADR-008](ADR-008-stack-mobile-tauri-pwa.md)**
- **Date** : 2026-08-27
- **Décideur** : Superviseur (humain)
- **Superviseur valideur** : [à signer]
- **Livrables impactés** : Epics v2.1 (Stories 0.2, 0.12, Epic 11), Architecture §2, PRD module E2' (FR-051 à FR-055)
- **ADR liés** : amende [ADR-001](ADR-001-coherence-rust.md) (le volet « Tauri embarqué » tombe), amende [ADR-007](ADR-007-push-apns-fcm.md) (APNs/FCM directs → Web Push VAPID)

## Contexte

ADR-008 avait retenu **Tauri 2.0 + PWA**, en écartant React Native et Flutter. Sa
justification était économique et tenait sur une prémisse : il y avait un client, un
devis, des jalons payés à ne pas jeter.

Cette prémisse est tombée avec le devis, décliné le 27/07/2026. Klaar est depuis un
projet vitrine sans budget de provisioning. Or Tauri Mobile en a besoin :

1. **macOS + Xcode** pour la cible iOS. Indisponible ici, et non contournable :
   la chaîne de build iOS ne s'exécute pas ailleurs.
2. **Comptes développeur Apple (99 $/an) et Google (25 $)** pour signer, distribuer
   et surtout **tester le push** — un push APNs ne se vérifie pas sans certificat.

Conséquence factuelle, constatée au Sprint 0 du 2026-08-24 : les Stories **0.2**
(bootstrap Tauri) et **0.12** (PoC push mobile) n'étaient pas « en retard », elles
étaient **structurellement bloquées**. Deux des quatre stories non livrées du Sprint 0
l'étaient pour cette seule raison.

S'y ajoute le risque déjà écrit dans ADR-008 lui-même :

> ⚠️ **Le repli natif n'existe plus.** Tauri devient un point de passage obligé sans
> plan B de stack. Cette décision **aggrave le concern C-2** du Validateur.

Un point de passage obligé, non testable dans l'environnement de développement réel du
projet, et dont l'échec n'a pas de rattrapage : c'est la définition d'un risque qu'on
ne devrait pas garder quand on peut le supprimer.

## Décision

**Le client de Klaar est une PWA, et rien d'autre. Tauri est retiré de la roadmap.**

Stack retenue, alignée sur celle d'**Elevia** (déjà en production, déjà MIT, donc
réutilisable dans les deux sens sans friction de licence) :

| Couche | Choix |
|---|---|
| Rendu | **Astro** (îles, HTML statique par défaut) |
| Interactivité | **Svelte 5** dans les îles |
| Installabilité | Web App Manifest + icônes maskables |
| Hors-ligne | Service worker + **queue IndexedDB** (`idb`) rejouée à la reconnexion |
| Push | **Web Push (VAPID, RFC 8291/8292)** — un seul protocole pour tous les navigateurs |
| Types API | `@klaar/client`, généré depuis l'OpenAPI (Story 0.6, inchangé) |

Il n'y a **pas** de `tauri-app/` ; il y a `web/`, servant à la fois les surfaces User,
Provider et Admin par des routes distinctes, sans triple codebase.

## Alternatives écartées

- **Maintenir Tauri** — écarté : sa justification (préserver des jalons payés) n'a plus
  d'objet, et il bloque deux stories du Sprint 0 sans moyen de les débloquer ici.
- **React Native / Flutter** — restent écartés, pour les raisons d'ADR-008 (double
  codebase, double compétence, bus factor de 1), inchangées.
- **PWA maintenant, Tauri plus tard en surcouche** — *non écarté, simplement non
  planifié*. C'est l'option que cette décision garde ouverte, voir §Point irréversible.

## Conséquences

### Positives

- **Le concern C-2 disparaît, il n'est pas atténué.** Il portait sur la dépendance à la
  maturité des plugins Tauri Mobile sans plan B. Sans Tauri, il n'y a plus de plugin
  dont dépendre. C'est le second concern du rapport de validation résolu par une
  décision de sobriété plutôt que par une mitigation (le premier, C-5, l'a été par
  ADR-009).
- **Stories 0.2 et 0.12 débloquées** : un bootstrap Astro et un service worker Web Push
  se vérifient intégralement en local, sans compte payant ni macOS.
- **Une seule chaîne de build**, une seule cible de test, zéro store à alimenter.
- **Réutilisation directe d'Elevia** : le service worker, la queue offline et la forme
  du manifeste sont un acquis, pas un chantier.
- **Pas de délai de revue de store** entre un correctif et sa mise à disposition.

### Négatives / risques à tracer

- **La géolocalisation en arrière-plan devient un *won't do*, pas un *conditionnel*.**
  ADR-008 la plaçait sous gate de PoC (FR-053) ; aucune API web ne la fournit. Le suivi
  d'intervention est **foreground uniquement**, application ouverte. C'est une
  régression fonctionnelle réelle, et c'est la contrepartie principale de cette
  décision. Elle est acceptable ici parce que le cas d'usage (un utilisateur qui regarde
  arriver son dépanneur) se déroule écran allumé.
- **Push iOS dégradé et conditionnel.** Web Push fonctionne sur iOS ≥ 16.4 **seulement
  si l'utilisateur a ajouté la PWA à son écran d'accueil**. Un utilisateur Safari non
  installé ne reçoit rien. Pas d'actions inline ni de rich media à parité d'APNs
  (FR-051 dégradé).
- **FR-052 (secure storage + biométrie) perd son moyen.** Plus de Stronghold ni de
  `tauri-plugin-biometric`. Le remplacement est **WebAuthn / passkey avec
  authentificateur de plateforme**, qui couvre l'authentification biométrique mais
  **pas** le stockage local chiffré de secrets applicatifs. À re-spécifier.
- **FR-054 (re-submission stores) devient sans objet** : il n'y a plus de store. La
  story est retirée, pas reportée.
- **Plafond de performance et accès matériel** : inchangé par rapport à ADR-008, mais
  désormais sans échappatoire planifiée.

## Sagesse racine (manifeste) — arbitrage

- **Sobriété** : retirer une dépendance entière plutôt que de la mitiger.
- **Répondre-de** : la perte de la géoloc background est nommée ici comme une
  régression, pas présentée comme une simplification.
- **Mottainai** : l'argument de mottainai d'ADR-008 (« ne pas jeter le travail payé »)
  ne s'applique plus, faute de travail payé. En revanche il s'applique à Elevia, dont
  la stack PWA est réutilisée telle quelle.

## Point irréversible

**Aucun.** C'est la différence notable avec ADR-008 et avec ADR-009.

Tauri 2.0 empaquette un frontend web. Construire la PWA d'abord n'est pas une
alternative à Tauri, c'en est le prérequis : le jour où un compte développeur et un
macOS existent, la même base Svelte se réempaquette. Le coût de réouverture est celui de
la coquille et de la signature, pas celui de l'application.

Autrement dit, ADR-008 avait choisi l'option coûteuse à défaire en la croyant prudente ;
celle-ci est à la fois plus sobre et plus réversible.

- **Indicateur de réouverture** : une exigence produit nécessitant un accès matériel
  qu'aucune API web ne couvre — la géoloc background en est une, si elle redevient
  bloquante commercialement.

## Suivi

- [x] Story **0.2** réécrite : bootstrap PWA Astro + Svelte (remplace le bootstrap Tauri)
- [x] Story **0.12** réécrite : Web Push VAPID de bout en bout (remplace le PoC push Tauri)
- [x] **Epic 11** re-spécifié : FR-053 en *won't do*, FR-054 retirée, FR-051/052 dégradés
- [x] Story **4.4** : la mention « background conditionnel » retirée du titre
- [ ] ADR-001 §Tauri à amender au prochain passage sur le document
- [ ] ADR-007 : Web Push VAPID devient le protocole unique ; APNs reste atteint, mais
      indirectement, par le service de push de Safari
