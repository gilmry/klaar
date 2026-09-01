#!/usr/bin/env bash
# Exécute une commande contre une base neuve, jetée à la fin.
#
#   scripts/base-jetable.sh cargo test --workspace
#
# **Pourquoi une base par exécution.** La base de développement n'était jamais
# purgée, et trois défauts distincts en sont sortis le même jour, tous
# invisibles en intégration continue où la base est neuve à chaque fois :
#
#   - le tirage d'un numéro d'entreprise entrait en collision avec ceux des
#     exécutions précédentes, de plus en plus souvent à mesure que la table
#     grossissait — une exécution complète sur deux échouait à onze mille
#     lignes ;
#   - le balayage de validation lisait un lot borné à deux cents Missions
#     échues, ordonné par identifiant : quatre cent trente et une Missions
#     accumulées et intraitables suffisaient à ce que le lot n'en contienne
#     plus d'autres, et un cas ne trouvait plus jamais la sienne ;
#   - un cas de purge insérait un jeton pour un compte qu'un cas voisin venait
#     d'effacer.
#
# Les deux premiers ont été corrigés dans le code — c'étaient de vrais défauts,
# la base ne faisait que les révéler. Mais ils ont coûté une demi-journée de
# diagnostic chacun, et ils reviendront sous une autre forme tant que les tests
# écriront dans une base qui garde tout.
#
# **Un gabarit, et non trente-huit migrations à chaque fois.** La première
# exécution construit une base modèle et y applique les migrations ; les
# suivantes la clonent, ce que PostgreSQL fait par copie de fichiers. Le nom du
# gabarit porte l'empreinte du répertoire `migrations/` : changer une migration
# en construit un neuf, sans qu'on ait à penser à l'invalider.
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "usage: scripts/base-jetable.sh <commande...>" >&2
  exit 2
fi

RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RACINE"

# **Comment on lance cargo**, pour construire le gabarit. `cargo` sur l'hôte par
# défaut ; `KLAAR_CARGO=scripts/cargo-conteneur.sh` le fait passer par le
# conteneur, sur une machine dont le disque ne tient pas un `target/` de plus.
CARGO="${KLAAR_CARGO:-cargo}"
PORT="${KLAAR_PG_PORT:-5433}"
HOTE="${KLAAR_PG_HOTE:-localhost}"
UTILISATEUR="${KLAAR_PG_UTILISATEUR:-klaar}"
MOT_DE_PASSE="${KLAAR_PG_MOT_DE_PASSE:-klaar_dev_only}"

# `psql` s'il est là, sinon celui du conteneur. Le second cas est le courant :
# la base de développement tourne dans docker et rien n'oblige à installer un
# client PostgreSQL sur l'hôte pour autant.
CONTENEUR="${KLAAR_PG_CONTENEUR:-klaar-postgres-1}"
if command -v psql >/dev/null 2>&1; then
  sql() { PGPASSWORD="$MOT_DE_PASSE" psql -h "$HOTE" -p "$PORT" -U "$UTILISATEUR" -d postgres -tAc "$1"; }
else
  DOCKER=(docker)
  docker info >/dev/null 2>&1 || DOCKER=(sudo -n docker)
  sql() { "${DOCKER[@]}" exec "$CONTENEUR" psql -U "$UTILISATEUR" -d postgres -tAc "$1"; }
fi

sql "SELECT 1" >/dev/null 2>&1 || {
  echo "base de développement injoignable : \`make db-up\` d'abord" >&2
  exit 1
}

# L'empreinte porte sur le contenu des migrations, pas sur leurs noms : une
# migration corrigée sans être renommée doit reconstruire le gabarit.
EMPREINTE="$(cat "$RACINE"/migrations/* | sha256sum | cut -c1-12)"
GABARIT="klaar_gabarit_$EMPREINTE"
JETABLE="klaar_test_$$_$(date +%s)"

nettoyer() {
  sql "DROP DATABASE IF EXISTS \"$JETABLE\" WITH (FORCE)" >/dev/null 2>&1 || true
}
trap nettoyer EXIT

if [ "$(sql "SELECT count(*) FROM pg_database WHERE datname = '$GABARIT'")" != "1" ]; then
  echo "→ construction du gabarit $GABARIT (une seule fois par jeu de migrations)" >&2
  sql "DROP DATABASE IF EXISTS \"$GABARIT\" WITH (FORCE)" >/dev/null
  sql "CREATE DATABASE \"$GABARIT\"" >/dev/null
  DATABASE_URL="postgres://$UTILISATEUR:$MOT_DE_PASSE@$HOTE:$PORT/$GABARIT" \
    $CARGO run -q -p klaar-api --bin klaar-migrate >&2
  # Marqué modèle **après** les migrations : un gabarit à moitié construit qui
  # survivrait à une interruption serait cloné tel quel par toutes les
  # exécutions suivantes, et l'erreur serait alors partout sauf à sa source.
  sql "UPDATE pg_database SET datistemplate = true WHERE datname = '$GABARIT'" >/dev/null
fi

sql "CREATE DATABASE \"$JETABLE\" TEMPLATE \"$GABARIT\"" >/dev/null

export DATABASE_URL="postgres://$UTILISATEUR:$MOT_DE_PASSE@$HOTE:$PORT/$JETABLE"
echo "→ base jetable $JETABLE" >&2
"$@"
