.PHONY: help dev up down logs run build test clean db-shell format check

help: ## Show available commands
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "  Pragma Monitoring - Commands"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-12s %s\n", $$1, $$2}'

# === Development ===

dev: up migrate ## Full dev setup (start services + run migrations)
	@echo "✅ Development environment ready!"
	@echo "   Run 'make run' to start the monitoring service"

up: ## Start Docker services (PostgreSQL + OTEL)
	@echo "📦 Starting services..."
	@docker-compose up -d
	@echo "⏳ Waiting for PostgreSQL..."
	@until docker exec pragma-postgres pg_isready -U postgres > /dev/null 2>&1; do sleep 1; done
	@echo "✅ Services ready"
	@echo "   PostgreSQL: localhost:5432"
	@echo "   Grafana:    http://localhost:3000 (admin/admin)"
	@echo "   OTLP:       localhost:4317"

down: ## Stop Docker services
	@docker-compose down

logs: ## Show Docker logs
	@docker-compose logs -f

migrate: ## Run database migrations
	@echo "🗄️  Running migrations..."
	@docker exec -i pragma-postgres psql -U postgres -d pragma_monitoring < migrations/001_init.sql 2>/dev/null || true
	@echo "✅ Migrations complete"

db-shell: ## Open PostgreSQL shell
	@docker exec -it pragma-postgres psql -U postgres -d pragma_monitoring

# === Build & Run ===

run: ## Run the monitoring service
	@cargo run

build: ## Build release binary
	@cargo build --release

test: ## Run tests
	@cargo test

# === Code Quality ===

format: ## Format and lint code
	cargo fmt
	cargo clippy --all -- -D warnings
	cargo clippy --tests --no-deps -- -D warnings

check: ## Check code without making changes
	@cargo fmt --check
	@cargo clippy --all -- -D warnings

# === Cleanup ===

clean: ## Clean build artifacts and Docker volumes
	@cargo clean
	@docker-compose down -v 2>/dev/null || true
	@echo "✅ Clean complete"
