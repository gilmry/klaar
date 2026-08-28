.PHONY: bootstrap test test-sobre build db-up db-down migrate hooks codegen frontend frontend-test msrv audit-biais

# Story 0.1 — DoD : idempotent, tous les crates compilent, cargo test vert.
bootstrap: build test frontend

build:
	cargo build --workspace

test:
	cargo test --workspace

# La compilation incrémentale fait grossir `target/` d'un facteur six (4 Go
# contre 25 Go observés sur ce dépôt) : elle conserve un état par version de
# chaque unité de compilation, et rien ne le purge. Utile en itération courte,
# coûteuse sur une machine partagée — d'où cette cible pour les longues séances.
test-sobre:
	CARGO_INCREMENTAL=0 cargo test --workspace

# Story 3.8 — rapport d'audit de la trace de matching (AI Act art. 12).
#
# À lancer tous les six mois, et à archiver : c'est le document à produire si
# l'APD le demande. Sortie 2 quand la chaîne de signatures est rompue, ce qu'un
# ordonnanceur remarque là où une ligne de plus dans un rapport passe inaperçue.
audit-biais:
	cargo run --bin klaar-audit-biais > audit-biais-$$(date +%Y-%m-%d).json

# Story 0.1 — la MSRV déclarée dans Cargo.toml doit être vraie, pas décorative.
msrv:
	rustup toolchain install $$(grep '^rust-version' Cargo.toml | cut -d'"' -f2) --profile minimal
	cargo +$$(grep '^rust-version' Cargo.toml | cut -d'"' -f2) check --workspace

# Story 0.2 — PWA Astro + Svelte (ADR-010). Remplace le bootstrap Tauri.
frontend:
	npm --prefix web ci --no-audit --no-fund || npm --prefix web install --no-audit --no-fund
	npm --prefix web run build

frontend-test:
	npm --prefix web run test

# Story 0.3 — PostgreSQL + PostGIS local (dev uniquement, pas de secret réel).
db-up:
	docker compose up -d
	@echo "Attends que 'docker compose ps' affiche healthy avant 'make migrate'."

db-down:
	docker compose down

migrate:
	cargo run -p klaar-api --bin klaar-migrate

# Parcours filmés — documentation vivante.
#
# **Délègue au script, et n'en refait pas la moitié.** `parcours-filmes.sh`
# monte le service avec les réglages sans lesquels les parcours échouent — pas
# de méthode de paiement exigée (Stripe est hors périmètre), quotas d'écritures
# et de Demandes relevés, dont l'un est compté en base et survit au redémarrage
# — puis repeuple le jeu de démonstration, que les parcours mutent en le
# jouant. Lancer `npm run demo` seul contre une API démarrée à la main échoue
# pour ces raisons-là, et pour aucune qui tienne au code.
#
# Une cible ici plutôt qu'une ligne à retenir : la procédure existait déjà dans
# le script, mais rien ne menait à lui depuis le point d'entrée habituel.
demo:
	scripts/parcours-filmes.sh

# Story 0.9 — hooks Git locaux DRY avec la CI (Story 0.4).
# git rev-parse --git-path (pas ".git/hooks" en dur) : correct aussi
# quand ce repo est un submodule ou un worktree (.git est alors un fichier).
hooks:
	install -m 755 scripts/hooks/pre-commit "$$(git rev-parse --git-path hooks/pre-commit)"
	install -m 755 scripts/hooks/pre-push "$$(git rev-parse --git-path hooks/pre-push)"
	@echo "hooks installés dans $$(git rev-parse --git-path hooks)/"

# Story 0.6 — codegen TS client partagé (@klaar/client) depuis l'OpenAPI de klaar-api.
codegen:
	bash scripts/codegen.sh
