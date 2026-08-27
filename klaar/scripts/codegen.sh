#!/usr/bin/env bash
# Story 0.6 — génère @klaar/client depuis l'OpenAPI de klaar-api.
# Un seul script (une seule invocation shell) : démarrer un serveur en
# arrière-plan puis le tuer depuis une ligne Makefile séparée ne fonctionne
# pas de façon fiable (chaque ligne de recette Make est un shell distinct).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo build -p klaar-api --bin klaar-api
# Secret jetable : le binaire refuse de démarrer sans (Story 1.3), et la
# génération du contrat n'ouvre aucune session.
KLAAR_JWT_SECRET="${KLAAR_JWT_SECRET:-secret-de-codegen-jamais-utilise-ailleurs}" \
    ./target/debug/klaar-api &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

for i in $(seq 1 20); do
    curl -sf http://127.0.0.1:8080/api/v1/openapi.json -o packages/klaar-client/openapi.json && break
    sleep 1
done

npm --prefix packages/klaar-client install
npm --prefix packages/klaar-client run generate
echo "client généré : packages/klaar-client/src/schema.d.ts"
