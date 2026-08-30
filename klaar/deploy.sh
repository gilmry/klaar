#!/usr/bin/env bash
# Déploiement continu de Klaar, tiré depuis git (Story 0.7).
#
# Sur la machine cible, une fois :   ./deploy.sh
#   -> installe docker, git et cron, crée `.env.deploiement` si absent, et
#      programme un cron qui rappelle ce script avec `--run`.
#
# Ensuite, à chaque tick :           ./deploy.sh --run
#   -> si `origin/main` a bougé, tire les images de ce commit et redéploie ;
#      sinon ne fait rien. Les deux modes se relancent sans risque.
#
# **Rien n'est construit ici.** Les images sont bâties par GitHub Actions et
# publiées sur GHCR ; ce workspace fait dépasser trente gigaoctets à `target/`,
# et une machine qui héberge déjà d'autres services ne peut pas absorber cela
# à chaque déploiement — elle le tenterait au pire moment, celui où l'on
# redéploie parce que quelque chose ne va pas.
set -euo pipefail

DEPOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRANCHE="${KLAAR_BRANCHE:-main}"
CRON_HORAIRE="${KLAAR_CRON:-*/5 * * * *}"
CRON_MARQUEUR="klaar-deploiement-auto"
JOURNAL="$DEPOT/deploy.log"
VERROU="$DEPOT/.deploy.lock"
REVISION_DEPLOYEE="$DEPOT/.deployed_rev"
ENV_FICHIER="$DEPOT/.env.deploiement"

# La variante de publication : Traefik partagé par défaut, puisque la machine
# qui héberge Klaar en fait déjà tourner un. `compose.tls.yml` convient à une
# machine dédiée, où Caddy possède les ports 80 et 443.
COMPOSITIONS="${KLAAR_COMPOSITIONS:--f compose.deploiement.yml -f compose.traefik.yml}"

journal() { echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*" >> "$JOURNAL"; }

compose() {
  # shellcheck disable=SC2086
  docker compose --env-file "$ENV_FICHIER" $COMPOSITIONS "$@"
}

deployer() {
  cd "$DEPOT"

  # **Un seul déploiement à la fois.** Deux ticks qui se chevauchent — le
  # premier lent, le second à l'heure — lanceraient deux `up` concurrents sur
  # les mêmes conteneurs. `-n` sort sans attendre : le tick suivant reprendra.
  exec 9>"$VERROU"
  flock -n 9 || exit 0

  [ -f "$ENV_FICHIER" ] || { journal "$ENV_FICHIER absent, déploiement impossible"; exit 1; }

  git fetch origin "$BRANCHE" --quiet

  revision_locale="$(git rev-parse "$BRANCHE")"
  revision_distante="$(git rev-parse "origin/$BRANCHE")"
  deployee="$(cat "$REVISION_DEPLOYEE" 2>/dev/null || true)"
  vivants="$(compose ps --status running -q 2>/dev/null || true)"

  if [ "$deployee" = "$revision_distante" ] && [ -n "$vivants" ]; then
    exit 0
  fi

  if [ "$revision_locale" = "$revision_distante" ]; then
    journal "révision $revision_distante non déployée, déploiement"
  else
    journal "nouveau commit sur $BRANCHE ($revision_locale -> $revision_distante), déploiement"
  fi

  # **Avance rapide seulement.** Un dépôt de production n'a pas de travail
  # local à préserver ; s'il a divergé, quelqu'un a modifié des fichiers sur la
  # machine, et écraser silencieusement ce travail cacherait la vraie anomalie.
  if ! git checkout "$BRANCHE" --quiet || ! git merge --ff-only "origin/$BRANCHE" --quiet; then
    journal "avance rapide impossible vers origin/$BRANCHE, déploiement annulé"
    exit 1
  fi

  # **La version tirée est l'empreinte du commit, pas `latest`.** Trois raisons :
  # `latest` ne dit pas ce qui tourne, il peut désigner une image plus récente
  # que le commit qu'on vient de vérifier, et si l'image de ce commit n'est pas
  # encore publiée — la CI ayant quelques minutes de retard sur le push — le
  # `pull` échoue franchement plutôt que de déployer autre chose. Le tick
  # suivant réessaie.
  KLAAR_VERSION="sha-$(git rev-parse --short=7 "$revision_distante")"
  export KLAAR_VERSION
  journal "version visée : $KLAAR_VERSION"

  if ! compose pull >> "$JOURNAL" 2>&1; then
    journal "images $KLAAR_VERSION indisponibles sur GHCR — la CI n'a peut-être pas fini ; nouvelle tentative au prochain tick"
    exit 1
  fi

  if compose up -d --remove-orphans >> "$JOURNAL" 2>&1; then
    journal "déploiement réussi ($revision_distante, $KLAAR_VERSION)"
    echo "$revision_distante" > "$REVISION_DEPLOYEE"
    # Les images remplacées ne servent plus, et chacune pèse quelques centaines
    # de mégaoctets. Sans cela, un disque se remplit en quelques semaines de
    # déploiements, ce qui arrête bien plus que le déploiement.
    docker image prune -f >> "$JOURNAL" 2>&1 || true
  else
    journal "échec du déploiement ($revision_distante), voir ci-dessus ; nouvelle tentative au prochain tick"
    exit 1
  fi
}

amorcer() {
  if [ "$(id -u)" -ne 0 ] && ! command -v sudo >/dev/null 2>&1; then
    echo "root ou sudo requis pour installer les paquets système" >&2
    exit 1
  fi
  local su=""
  [ "$(id -u)" -ne 0 ] && su="sudo"

  command -v apt-get >/dev/null 2>&1 || {
    echo "ce script suppose une distribution basée sur apt (Debian/Ubuntu)" >&2
    exit 1
  }

  echo "==> dépendances système"
  $su apt-get update -qq
  $su apt-get install -y -qq ca-certificates curl git cron >/dev/null

  if ! command -v docker >/dev/null 2>&1; then
    echo "==> installation de Docker"
    curl -fsSL https://get.docker.com | $su sh
  fi

  $su docker compose version >/dev/null 2>&1 || {
    echo "le greffon docker compose v2 est introuvable après installation" >&2
    exit 1
  }

  local compte="${SUDO_USER:-$(id -un)}"
  if [ "$compte" != "root" ] && ! id -nG "$compte" | grep -qw docker; then
    echo "==> ajout de $compte au groupe docker"
    $su usermod -aG docker "$compte"
    echo "    reconnecte-toi (ou 'newgrp docker') pour ce shell ; le cron, lui,"
    echo "    ouvre un nouveau processus et en tiendra compte tout seul."
  fi

  if [ ! -f "$ENV_FICHIER" ]; then
    echo "==> création de $ENV_FICHIER depuis .env.example"
    cp "$DEPOT/.env.example" "$ENV_FICHIER"
    echo "    à éditer AVANT le premier déploiement : la composition refuse de"
    echo "    démarrer sans KLAAR_PG_PASSWORD ni KLAAR_JWT_SECRET, et Traefik"
    echo "    sans KLAAR_DOMAINE."
  fi

  local reseau="${KLAAR_TRAEFIK_RESEAU:-ecosolva-web}"
  if ! $su docker network inspect "$reseau" >/dev/null 2>&1; then
    echo "ATTENTION : le réseau externe '$reseau' n'existe pas." >&2
    echo "            Démarrer le Traefik partagé avant le premier déploiement," >&2
    echo "            ou passer par compose.tls.yml sur une machine dédiée." >&2
  fi

  touch "$JOURNAL"

  echo "==> cron de déploiement ($CRON_HORAIRE)"
  local ligne="$CRON_HORAIRE $DEPOT/deploy.sh --run # $CRON_MARQUEUR"
  local actuel nouveau
  actuel="$(crontab -l 2>/dev/null || true)"
  nouveau="$(echo "$actuel" | grep -vF "$CRON_MARQUEUR" || true)"
  { echo "$nouveau"; echo "$ligne"; } | grep -v '^$' | crontab -

  $su systemctl enable --now cron >/dev/null 2>&1 || true

  cat <<FIN

Amorçage terminé.
  cron      : $ligne
  journal   : $JOURNAL
  variantes : $COMPOSITIONS

Avant le premier déploiement :
  1. Éditer $ENV_FICHIER (KLAAR_PG_PASSWORD, KLAAR_JWT_SECRET, KLAAR_DOMAINE,
     KLAAR_URL_PUBLIQUE, et le webhook de courriel si les inscriptions doivent
     partir).
  2. Vérifier que le Traefik partagé tourne et que son réseau existe.
  3. Attendre le prochain tick, ou lancer : $DEPOT/deploy.sh --run

Les mentions légales sont figées dans l'image du site au moment du build : les
renseigner se fait dans les variables du dépôt GitHub (Settings > Secrets and
variables > Actions > Variables), pas ici.
FIN
}

if [ "${1:-}" = "--run" ]; then
  deployer
else
  amorcer
fi
