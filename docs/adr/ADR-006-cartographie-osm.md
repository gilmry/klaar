# ADR-006 — Cartographie : OpenStreetMap + Valhalla (vs Mapbox)

- **Statut** : Accepté
- **Date** : 2026-07-18
- **Décideur** : Architecte (validé superviseur via Validateur 100/100)
- **Superviseur valideur** : ✅ 2026-07-18

## Contexte

PRD §11 + §8.2 : l'app mobile Tauri doit afficher :
- Carte interactive RBC avec position User et Provider
- Géocoding (adresse → coordonnées)
- Routing (calcul temps de trajet Provider → User, FR-010, FR-018)

Le marché propose :
- **Mapbox** : service SaaS US, riche, APIs mûres, mais payant (~0.50 $/1 000 tiles, ~2.50 $/1 000 directions), exposition RGPD (Cloud Act)
- **OpenStreetMap + Valhalla** : OSS auto-hébergé sur OVH BE/EU, souveraineté totale, coût 0 par requête, mais opérateur (maintenance, MAJ données OSM)
- **Google Maps** : écarté d'emblée (cher, lock-in, US)

## Décision

**OpenStreetMap + Valhalla auto-hébergé sur OVHcloud BE/EU**.

- **Tiles** : `openstreetmap-tile-server` Docker auto-hébergé, données Belgique extract
- **Géocoding** : `Pelias` ou `Photon` (basé sur OSM)
- **Routing** : `Valhalla` (engine OSS, supporté par Mapbox historiquement mais indépendant)
- **Frontend** : `maplibre-gl` (fork OSS de Mapbox GL JS, compatible tiles OSM)

## Alternatives écartées

### Mapbox (SaaS)
Écartée car :
- **Coût** : à 300 k MAU/an 3 avec ~5 maps/user/mois = 18 M tiles/an ≈ 9 000 €/an (correctif Phase 2 Estimateur)
- **RGPD Cloud Act** : données User susceptibles de transmission US (Schrems II)
- **Dépendance fournisseur** : changement tarifaire unilatéral possible (mitigation H-6 Stricte)

### Google Maps
Écartée car : coûteux, lock-in, US-centric.

### TomTom / HERE
Écartées car : payants, lock-in entreprise, peu flexibles.

## Conséquences

### Positives
- **Coût** : 0 €/requête (auto-hébergé), coût fixe = infra OVH (~50 €/mois pour tile-server)
- **Souveraineté totale** : données OSM BE téléchargées, hébergées OVH BE/EU, aucune exposition US
- **Aligné Manifeste §2 (sumak kawsay)** : sobriété, OSS, commun
- **Pas de dépendance réseau externe** : tiles servies en local → latence faible
- **Pas de rate-limit** : scaling horizontal maîtrisé

### Négatives / risques à tracer
- **Maintenance** : MAJ des données OSM BE à automatiser (job hebdo, ~10 Go)
- **Story habilitante Sprint 0** : déployer `openstreetmap-tile-server` + Valhalla sur OVH (Story 0.11 à ajouter)
- **Performance routing** : Valhalla légèrement moins rapide que Mapbox, mais suffisant pour RBC
- **Geocoding Belgie** : qualité OSM BE bonne mais inégale sur rues récentes — compléter avec `service-public.fr/belgium` ou BOSA

## Sagesse racine (manifeste)

- **Sumak kawsay** — OSS sobre, pas de SaaS payant pour des données communautaires
- **Écologie des savoirs** — OSM = commun mondial, contribution possible (Klaar peut contribuer en retour)
- **Arbitrage hybride (foyer)** : souveraineté maximale (résidence BE/EU, capacité interne, réversibilité totale)
- **Répondre-de** : aucune exposition Cloud Act, données UserBE jamais transférées

## Point irréversible

- Choix cartographie : **réversible** (swap possible mais coûteux si intégration profonde)
- **Validation humaine** : ✅ Superviseur

## Suivi

- Sprint 0 : Story 0.11 (nouvelle) — déployer tile-server + Valhalla sur OVH (M, 3 tours)
- Monitoring : latence tiles P99 < 200 ms (à valider)
- Si scale > 1 M users : ajouter CDN ( Bunny.net ou Cloudflare R2 souverain )
