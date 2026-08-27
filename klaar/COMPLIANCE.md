# Conformité — à lire avant tout déploiement

Ce dépôt est publié sous licence MIT (voir `LICENSE.md` et `docs/adr/ADR-009-license-mit.md`).
La clause *AS IS* de MIT couvre la garantie logicielle. **Elle ne couvre pas la conformité
réglementaire de votre déploiement**, qui vous incombe entièrement.

Ce code met en œuvre des flux relevant de plusieurs régimes contraignants en Belgique et
dans l'Union. Le déployer tel quel, avec de vrais utilisateurs, sans le travail juridique
correspondant, vous met en infraction — pas l'auteur.

## Ce qui n'est pas fourni

| Obligation | Régime | État dans ce dépôt |
|---|---|---|
| **DPIA (analyse d'impact)** avant tout traitement de géolocalisation | RGPD art. 35 | **Absente.** Obligatoire *avant* le traitement, pas après. |
| Analyse de classification des travailleurs | Loi BE du 26/04/2024 + directive UE 2024/2831 (Platform Work) | Absente. Les invariants de non-fixation des prix sont décrits en conception, pas audités. |
| Agrément / passeport établissement de paiement, SCA | DSP2 | Absent. Le séquestre s'appuie sur Stripe Connect ; l'agrément reste celui de votre entité. |
| Documentation et audit de biais d'un matching algorithmique | AI Act art. 10-15 | Décrit en conception (FR-012, FR-056), non implémenté. |
| Mesures CyFun Basic (MFA ops, chiffrement at-rest, journal WORM) | NIS2 | Non implémentées. Les stories correspondantes sont bloquées faute de provisioning. |
| Régime TVA, taux applicables, facturation | TVA BE (21 % / 6 % / 12 %) | Décrit en conception, non implémenté. |

## Ce qui est fourni

Une **architecture** qui prend ces contraintes au sérieux dès la conception : séparation
hexagonale permettant d'isoler les traitements réglementés, journal d'audit prévu comme
immuable, minimisation des données pensée au niveau du modèle, traçabilité des exigences
vers les documents de conception (`docs/bmad-livrables/`).

C'est un point de départ défendable. Ce n'est pas une conformité.

## Un point connu, non corrigé

Le span racine de `tracing-actix-web` journalise par défaut `http.client_ip` et
`http.user_agent`. Une adresse IP est une donnée personnelle. Sans conséquence tant que
`/api/v1/health` est le seul endpoint exposé, à corriger avant tout endpoint réel — voir
le commentaire dans `crates/klaar-api/src/main.rs`.

## Vulnérabilité transitive acceptée

`cargo audit` et `cargo deny` ignorent **RUSTSEC-2026-0258** (h2 < 0.4.16, déni de service
par frames DATA vides), transitive via `actix-http` — toute la branche h2 0.3.x d'actix-web
v4 en hérite et aucun correctif amont n'existe à ce jour. Acceptable tant que le service
n'est pas exposé publiquement. **À réévaluer à chaque mise à jour de dépendances**
(`cargo tree -i h2`).

## Publication de ce dépôt

Si vous forkez ou republiez, notez que la version d'origine a été extraite d'un dépôt privé
contenant des documents commerciaux et les besoins d'un prospect. Ces documents ne font pas
partie de la publication et ne doivent pas y être réintroduits, historique git compris.
