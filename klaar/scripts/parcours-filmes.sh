#!/usr/bin/env bash
# Enregistre les parcours filmés — documentation vivante (Story 4.11).
#
#   scripts/parcours-filmes.sh
#
# Monte un service **réel** — PostgreSQL, migrations, jeu de démonstration,
# klaar-api — puis déroule les parcours dans un navigateur et en garde la
# vidéo. Rien n'est simulé : c'est la différence avec `npm run test:e2e`, qui
# vérifie vite en interceptant les appels réseau.
#
# Le service est **relancé à chaque fois**. La limitation de débit vit en
# mémoire du processus, et une exécution précédente aurait épuisé le quota de
# connexions ; le relancer part d'un compteur neuf, sans qu'aucun réglage
# n'ait à être désactivé.
set -euo pipefail

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RACINE"

DATABASE_URL="${DATABASE_URL:-postgres://klaar:klaar_dev_only@localhost:5433/klaar}"
export DATABASE_URL

# Secrets de démonstration, jetés avec le processus. Ils n'ont pas à être
# solides : rien de réel ne transite ici, et les nommer ainsi évite qu'on les
# recopie ailleurs par inadvertance.
export KLAAR_JWT_SECRET="${KLAAR_JWT_SECRET:-secret-de-parcours-filmes-jamais-en-production-48}"
export KLAAR_TRACE_HMAC_KEY="${KLAAR_TRACE_HMAC_KEY:-cle-de-parcours-filmes-jamais-en-production}"
# Stripe est hors périmètre : exiger une carte bloquerait tout parcours.
export KLAAR_EXIGER_METHODE_PAIEMENT=0
# Le navigateur parle en clair à 127.0.0.1 ; le cookie `Secure` serait refusé.
export KLAAR_COOKIE_SECURE=0
# Plusieurs parcours se connectent depuis la même adresse en quelques minutes.
export KLAAR_QUOTA_ECRITURE_SENSIBLE=200
# Et le même compte de démonstration soumet plusieurs Demandes d'une exécution
# à l'autre. Le quota est compté en base, donc il survit au redémarrage : sans
# ce relèvement, un parcours rejoué dans l'heure buterait sur un refus qui n'a
# rien à voir avec ce qu'il démontre.
export KLAAR_MAX_DEMANDES_PAR_HEURE=200
export KLAAR_PRESTATAIRES_DEMO=1
export CARGO_INCREMENTAL=0

echo "→ migrations"
cargo run -q --bin klaar-migrate

echo "→ jeu de démonstration (prestataires et demandeurs, état remis à zéro)"
cargo run -q --bin klaar-prestataires-demo

echo "→ construction du binaire de service"
cargo build -q --bin klaar-api

echo "→ démarrage du service"
./target/debug/klaar-api > /tmp/klaar-parcours.log 2>&1 &
API_PID=$!
# Arrêté quoi qu'il arrive : un service laissé derrière tiendrait le port et
# ferait échouer l'exécution suivante sans dire pourquoi.
trap 'kill "$API_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 30); do
  if curl -sf -m 2 http://127.0.0.1:8080/api/v1/health > /dev/null; then break; fi
  sleep 1
done
curl -sf -m 2 http://127.0.0.1:8080/api/v1/health > /dev/null || {
  echo "le service n'a pas démarré ; voir /tmp/klaar-parcours.log" >&2
  exit 1
}

cd web
echo "→ construction du site"
npm run build

echo "→ enregistrement des parcours"
npm run demo

echo "→ vidéos dans web/demo-resultats, rapport dans web/playwright-report-demo"
