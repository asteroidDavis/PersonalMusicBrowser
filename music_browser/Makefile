# PersonalMusicBrowser Makefile

# Configuration
BACKUP_DIR ?= $(HOME)
BACKUP_SMB_PATH ?= smb://BeanNAS._smb._tcp.local/NatesMusic/ApplicationBackups/
DATABASE_URL ?= sqlite:music_browser.db
DB_FILE ?= music_browser.db

# Default target
.PHONY: help
help: ## Show this help message
	@echo "PersonalMusicBrowser - Rust Music Management Application"
	@echo ""
	@echo "Available targets:"
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  %-15s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# Development targets
.PHONY: build
build: ## Build the application
	cargo build

.PHONY: run
run: ## Run the web application
	cargo run --bin music-browser

.PHONY: run-import
run-import: ## Run the bulk import CLI
	cargo run --bin bulk-import

.PHONY: test
test: ## Run all tests
	cargo test

.PHONY: check
check: ## Run formatting, clippy, and tests
	cargo fmt --check
	cargo clippy -- -D warnings
	cargo test

# Database targets
.PHONY: migrate
migrate: ## Run database migrations
	sqlx migrate run --database-url $(DATABASE_URL)

.PHONY: backup-db
backup-db: ## Backup database to configured location
	@TIMESTAMP=$$(date +"%Y%m%d_%H%M%S"); \
	BACKUP_FILE="music_browser_backup_$$TIMESTAMP.db"; \
	echo "Creating backup: $$BACKUP_FILE"; \
	cp $(DB_FILE) "$$BACKUP_FILE"; \
	if [ -n "$$BACKUP_SMB_PATH" ] && [ "$$BACKUP_SMB_PATH" != "$(HOME)" ]; then \
		echo "Copying to SMB: $$BACKUP_SMB_PATH"; \
		smbclient "$$BACKUP_SMB_PATH" -c "put $$BACKUP_FILE" || echo "SMB copy failed, using local backup"; \
	fi; \
	mv "$$BACKUP_FILE" "$(BACKUP_DIR)/"; \
	echo "Backup completed: $(BACKUP_DIR)/$$BACKUP_FILE"

.PHONY: backup
backup: ## Run standalone backup service
	cargo run --manifest-path Cargo_backup.toml --bin music-browser-backup

.PHONY: restore-db
restore-db: ## Restore database from backup (usage: make restore-db BACKUP_FILE=path/to/backup.db)
	@if [ -z "$(BACKUP_FILE)" ]; then \
		echo "Usage: make restore-db BACKUP_FILE=path/to/backup.db"; \
		exit 1; \
	fi
	@echo "Restoring from: $(BACKUP_FILE)"
	cp "$(BACKUP_FILE)" music_browser.db
	echo "Database restored"

# Deployment targets
.PHONY: docker-build
docker-build: ## Build Docker image
	docker build -t personal-music-browser .

.PHONY: docker-build-backup
docker-build-backup: ## Build backup Docker image
	docker build -f Dockerfile.backup -t music-browser-backup .

.PHONY: docker-run
docker-run: ## Run Docker container
	docker run -p 3000:3000 --env-file .env personal-music-browser

.PHONY: docker-compose-up
docker-compose-up: ## Start application with backup sidecar
	docker-compose up -d

.PHONY: docker-compose-down
docker-compose-down: ## Stop application and backup sidecar
	docker-compose down

.PHONY: clean
clean: ## Clean build artifacts
	cargo clean

# Environment
.PHONY: setup
setup: ## Set up development environment
	cp .env.example .env
	@echo "Created .env file from example"
	@echo "Edit .env to configure your settings"
