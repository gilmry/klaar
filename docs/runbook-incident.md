# Runbook — Signalement d'incident de sécurité (NIS2)

*Story 0.10. Applicable dès que Klaar traite des données de production réelles — à ce stade (Sprint 0, projet vitrine sans déploiement), ce document prépare l'obligation plutôt qu'il ne répond à un incident réel.*

## Cadre légal

- **Directive NIS2 (UE 2022/2555)** : notification à l'autorité compétente dans les **24 heures** suivant la connaissance d'un incident significatif, rapport intermédiaire à 72h, rapport final sous 1 mois (docs/bmad-livrables/03-Architecture.md §6.3).
- **Autorité belge** : Centre for Cybersecurity Belgium (CCB), via [safeonweb.be](https://safeonweb.be) / le portail de notification CCB.
- **Seuil "entité essentielle"** : applicable si Klaar dépasse 50 salariés ou 10 M€ CA avec une plateforme numérique critique — pas le cas à ce stade (1 intervenant, pas de CA), mais la procédure est préparée en amont plutôt qu'improvisée sous pression.

## Qui déclenche ce runbook

Toute personne (dev, superviseur, alerte automatisée) qui constate :
- un accès non autorisé confirmé ou suspecté à des données (utilisateurs, paiements, secrets) ;
- une vulnérabilité activement exploitée (pas juste une CVE `cargo audit` détectée — voir `klaar/README.md` pour la gestion de routine des CVE connues) ;
- une compromission de l'infrastructure (VPS, CI/CD, dépôt git) ;
- une fuite de secret confirmée (au-delà de ce que `gitleaks` bloque déjà en CI).

## Séquence (H0 = découverte)

| Délai | Action |
|---|---|
| **H0** | Confiner : révoquer les accès/clés compromis, isoler le service touché (arrêt du conteneur/VPS si nécessaire). Ne pas effacer de preuves. |
| **H0–H1** | Consigner les faits observés (horodatage, portée estimée, action de confinement) — point de départ du rapport CCB, pas une reconstitution a posteriori. |
| **≤ H24** | Notification initiale au CCB via [safeonweb.be](https://safeonweb.be) : nature de l'incident, portée estimée, mesures de confinement déjà prises. Une notification incomplète mais dans les temps vaut mieux qu'une notification complète en retard. |
| **≤ H72** | Rapport intermédiaire : analyse technique de la cause, impact affiné, actions correctives en cours. |
| **≤ 1 mois** | Rapport final : cause racine, chronologie complète, mesures définitives, actions de prévention. |

## Ce qui alimente l'enquête

- `audit_logs` (append-only, PostgreSQL — Architecture §3.1, quand implémenté) : trace immuable des actions métier.
- Logs CI/CD (`gh run view <id> --log`) : historique des déploiements et de leurs vérifications (quality/security gates).
- `cargo audit` / `cargo deny` / SBOM signés (ce job CI) : état des dépendances au moment du build concerné — utile pour dater l'exposition à une CVE donnée.
- Historique git (commits signés le cas échéant, `git log`) : qui a changé quoi, quand.

## Contacts

*À compléter avant toute mise en production réelle — laissé volontairement vide ici, un contact factice serait plus dangereux qu'une case à remplir :*
- Responsable technique / DPO : `[à compléter]`
- CCB (signalement) : https://safeonweb.be
- Hébergeur (OVHcloud, une fois provisionné — Stories 0.7a/b/c) : `[à compléter]`

## Statut de ce runbook

**Non testé en jeu de rôle** (DoD Story 0.10 complet demande un exercice avec une équipe ops — inapplicable à ce stade, intervenant unique, pas de déploiement réel). À exécuter en simulation dès qu'une première mise en production a lieu, avant d'en avoir besoin pour de vrai.
