# ADR-007 — Notifications push : APNs + FCM (avec plan B UnifiedPush)

- **Statut** : Accepté
- **Date** : 2026-07-18
- **Décideur** : Architecte (validé superviseur via Validateur 100/100)
- **Superviseur valideur** : ✅ 2026-07-18

## Contexte

PRD §11 : notifications push requis pour :
- FR-003 (validation KYC ops)
- FR-008 (notification matching Provider)
- FR-019 (transitions Mission User)
- FR-022 (annulation)
- FR-030 (messages conversation)

**Risque Brief H-2** : maturité des plugins Tauri 2.0 Mobile pour push natif iOS (APNs) et Android (FCM). Le Validateur a exigé une décision avant Sprint 0.

## Décision

**APNs (iOS) + FCM (Android) via plugin Tauri 2.0 standard**.

- Plugin : `tauri-plugin-notification` + `tauri-plugin-push` (v2, stable depuis Q4 2024)
- APNs : HTTP/2 + JWT (clé p8 Apple Developer)
- FCM : API v1 (clé service account Google Cloud)
- Backend : `klaar-push-adapter` (crate Rust) implémente `PushNotifier` (trait Application)

**Plan B** : si plugin Tauri push s'avère instable en production (H-2), bascule sur :
- iOS : `capacitorjs` push plugin dans webview Tauri (compatibilité)
- Android : **UnifiedPush** (OSS, auto-hébergé, fallback souverain)

## Alternatives écartées

### UnifiedPush seul (OSS)
Écarté car :
- Pas de support iOS nat (Apple impose APNs)
- Maturité Android encore en adoption (communauté OSS)

### OneSignal / Airship (SaaS)
Écarté car :
- Dépendance SaaS US
- Coût récurrent
- Pas de souveraineté données

### Pas de push (pull only)
Écarté car : UX dégradée, matching < 5 min impossible sans push

## Conséquences

### Positives
- **Push natif éprouvé** : APNs + FCM = standards Apple/Google, documentation riche
- **Latence faible** : < 5 s Apple/Google
- **Story habilitante validée** : PoC plugin Tauri 2.0 push en Sprint 0 (Story 0.12 nouvelle)
- **Plan B UnifiedPush** : mitigation H-2 documentée

### Négatives / risques à tracer
- **Dépendance Apple Developer + Google Cloud** : comptes requis (99 €/an Apple, gratuit Google)
- **Clés à rotation** : p8 Apple rotée annuellement, service account GCP
- **Latence Apple ~5 s en moyenne** : acceptable pour Klaar (Brief fill rate > 60 %)
- **Plan B complexe** : si bascule, réécriture partie frontend (Tauri plugin ↔ capacitor)

## Sagesse racine (manifeste)

- **Écologie des savoirs** : APNs/FCM = standards, compétences répandues
- **Mottainai** : pas de SaaS payant pour pousser des notifications
- **Répondre-de** : plan B traçé, dépendance assumée (Apple + Google obligatoires pour stores)

## Point irréversible

- Choix push provider : **réversible** (plan B UnifiedPush documenté)
- **Validation humaine** : ✅ Superviseur

## Suivi

- Sprint 0 : Story 0.12 (nouvelle) — PoC push Tauri 2.0 + Provider recevoir 1 notif + Provider click → ouvre Mission (L, 5 tours)
- Si PoC échoue : déclencher plan B UnifiedPush en Sprint 1 (story 1.11 nouvelle)
- Monitoring : taux de livraison push ≥ 95 % (Prometheus)
