.PHONY: bootstrap test build db-up db-down migrate hooks codegen

# Story 0.1 — DoD : idempotent, tous les crates compilent, cargo test vert.
bootstrap: build test

build:
	cargo build --workspace

test:
	cargo test --workspace

# Story 0.3 — PostgreSQL + PostGIS local (dev uniquement, pas de secret réel).
db-up:
	docker compose up -d
	@echo "Attends que 'docker compose ps' affiche healthy avant 'make migrate'."

db-down:
	docker compose down

migrate:
	cargo run -p klaar-api --bin klaar-migrate

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
