#!/usr/bin/env bash
# Lance cargo dans un conteneur, cible et registre sur des volumes docker.
#
# **Pourquoi pas simplement `cargo` sur l'hôte.** Le `target/` de ce workspace
# atteint 28 Go. La racine de la machine de développement n'en fait que 48 :
# une compilation complète la remplit, et « No space left on device » n'arrête
# pas que la compilation. Les volumes docker vivent sur le second disque.
#
# Pointer `CARGO_TARGET_DIR` directement vers ce second disque depuis l'hôte ne
# marche pas ici : il est monté sous le répertoire de données de docker, que
# l'utilisateur ordinaire ne peut pas traverser. Le rendre traversable
# affaiblirait l'isolation des données de docker pour un gain de confort ;
# passer par un conteneur ne coûte rien de tel.
#
# `--network host` : la base de développement écoute sur `localhost:5433` de
# l'hôte, et les tests d'intégration en ont besoin.
#
# `--user` : sans lui, les fichiers écrits dans le dépôt monté appartiendraient
# à root, et la commande suivante échouerait sur ses propres artefacts.
#
# Usage : scripts/cargo-conteneur.sh test --workspace
set -euo pipefail

BASE="${KLAAR_IMAGE_RUST:-rust:1.88-bookworm}"
# L'image officielle n'embarque ni clippy ni rustfmt. Les ajouter à chaque
# lancement les retéléchargerait à chaque fois, le conteneur étant jetable :
# on construit une fois une image locale qui les porte.
IMAGE="klaar-cargo:1.88"
RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

DOCKER=(docker)
docker info >/dev/null 2>&1 || DOCKER=(sudo -n docker)

if ! "${DOCKER[@]}" image inspect "$IMAGE" >/dev/null 2>&1; then
  printf 'construction de %s (une seule fois)\n' "$IMAGE" >&2
  printf 'FROM %s\nRUN rustup component add clippy rustfmt\n' "$BASE" \
    | "${DOCKER[@]}" build -t "$IMAGE" -
fi

# Les volumes appartiennent à root à leur création : un premier passage les
# donne à l'utilisateur courant, sinon cargo ne peut rien y écrire.
"${DOCKER[@]}" run --rm \
  -v klaar-cargo-cible:/cible -v klaar-cargo-registre:/registre \
  "$IMAGE" chown -R "$(id -u):$(id -g)" /cible /registre

exec "${DOCKER[@]}" run --rm --network host \
  --user "$(id -u):$(id -g)" \
  -v "$RACINE":/travail -w /travail \
  -v klaar-cargo-cible:/cible \
  -v klaar-cargo-registre:/registre \
  -e CARGO_TARGET_DIR=/cible \
  -e CARGO_HOME=/registre \
  -e DATABASE_URL="${DATABASE_URL:-postgres://klaar:klaar_dev_only@localhost:5433/klaar}" \
  "$IMAGE" cargo "$@"
