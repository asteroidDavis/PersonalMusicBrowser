# Personal Music Browser

A lightweight music production planning app built with **Rust**, **Actix-web**, **Askama** templates, and **SQLite** (via SQLx).

## Model Diagram

```
┌─────────────┐       ┌──────────────┐       ┌─────────────┐
│  Instrument  │       │     Band     │       │    Album    │
├─────────────┤       ├──────────────┤       ├─────────────┤
│ id    (PK)  │       │ id    (PK)   │       │ id    (PK)  │
│ name        │       │ name         │       │ title       │
└──────┬──────┘       └──────┬───────┘       │ released    │
       │                     │               │ url         │
       │                     │               └──────┬──────┘
       │              ┌──────┴───────┐              │
       │              │ artist_bands │              │
       │              │  (M2M join)  │              │
       │              └──────┬───────┘              │
       │                     │                      │
       │              ┌──────┴───────┐              │
       │              │    Artist    │              │
       │              ├──────────────┤              │
       │              │ id    (PK)   │              │
       │              │ name         │              │
       │              └──────┬───────┘              │
       │                     │                      │
       │              ┌──────┴───────┐              │
       │              │ song_artists │              │
       │              │  (M2M join)  │              │
       │              └──────┬───────┘              │
       │                     │                      │
       │              ┌──────┴───────┐     FK       │
       │              │     Song     │◄─────────────┘
       │              ├──────────────┤
       │              │ id     (PK)  │
       │              │ title        │
       │              │ album_id(FK) │
       │              │ sheet_music  │
       │              │ lyrics       │
       │              │ song_type    │──┐
       │              └──────┬───────┘  │
       │                     │          │
       │    ┌────────────────┼──────────┼──────────────┐
       │    │                │          │              │
       │    ▼                ▼          ▼              ▼
       │ ┌─────────┐  ┌───────────┐ ┌──────────────┐
       │ │Recording│  │CoverDetail│ │ Composition  │
       │ ├─────────┤  ├───────────┤ │    Detail     │
       │ │id  (PK) │  │song_id(FK)│ ├──────────────┤
       │ │rec_type │  │notes_image│ │ song_id (FK) │
       │ │path     │  │notes_done │ │ bpm_upper    │
       │ │song_id  │  └───────────┘ │ bpm_lower    │
       │ │notes_img│                └──────────────┘
       │ └────┬────┘
       │      │
       ├──────┘  (recording_instruments, cover_instruments,
       │          composition_instruments — M2M joins)
       │
  Instrument is linked via M2M to Recording, Cover, and Composition
```

### Entity Summary

| Entity | Description |
|---|---|
| **Instrument** | A musical instrument (e.g. Guitar, Piano) |
| **Band** | A named group of artists |
| **Artist** | A musician; belongs to zero or more Bands |
| **Album** | A collection of songs; has released status and URL |
| **Song** | A track on an album; type is `song`, `cover`, or `composition` |
| **CoverDetail** | Extra fields for cover songs (notes image, completion status, instruments) |
| **CompositionDetail** | Extra fields for compositions (BPM range, instruments) |
| **Recording** | A recorded file for a song (type: audacity, mix, master, loop-core-list, wav) |

## Prerequisites

- **Rust** (stable toolchain): https://rustup.rs
- **SQLx CLI** (for migrations):
  ```bash
  cargo install sqlx-cli --no-default-features --features sqlite
  ```

## Quick Start

```bash
cd music_browser

# Create the database and run migrations
cp .env.example .env          # or create: echo 'DATABASE_URL=sqlite:music_browser.db' > .env
sqlx database create
sqlx migrate run --source ./migrations

# Build and run
cargo run
# App is at http://127.0.0.1:3000
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `sqlite:music_browser.db` | SQLite connection string |
| `BIND_ADDR` | `127.0.0.1:3000` | Address to bind the server |
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |

## Database

### Log in / Inspect the Database

```bash
# Using the sqlite3 CLI (ships with macOS):
sqlite3 music_browser/music_browser.db

# Useful commands inside sqlite3:
.tables              -- list all tables
.schema songs        -- show CREATE TABLE for songs
SELECT * FROM songs; -- query data
.quit                -- exit
```

### Apply Migrations

Migrations live in `music_browser/migrations/`. To apply:

```bash
cd music_browser
sqlx migrate run --source ./migrations
```

To add a new migration:

```bash
sqlx migrate add -r <description> --source ./migrations
# Edit the generated .sql file, then run:
sqlx migrate run --source ./migrations
```

## Testing

### Run All Tests (terminal)

```bash
cd music_browser
cargo test
```

### Run a Single Test (terminal)

```bash
cargo test test_song_crud           # by name substring
cargo test test_song_crud -- --exact # exact match
```

### Run Tests in JetBrains (CLion / IntelliJ + Rust plugin)

1. Open the `music_browser` directory as a project (or the parent repo).
2. In `tests/db_tests.rs`, click the green ▶ gutter icon next to any `#[tokio::test]` function.
3. Or right-click a test function → **Run 'test_name'**.
4. To run all tests: open the terminal tab and run `cargo test`.

### Test Coverage

The test suite (`tests/db_tests.rs`) covers:
- CRUD for instruments, bands, artists, albums, songs, recordings
- Many-to-many relationships (artist↔band, song↔artist, recording↔instrument)
- Cover and Composition detail tables
- Song type and recording type CHECK constraints
- FK RESTRICT (album can't be deleted while songs reference it)
- Migration idempotency (all expected tables exist)

## Pre-commit Hooks

### Setup

```bash
# From the repo root:
bash music_browser/scripts/install-hooks.sh
```

This installs a Git pre-commit hook that runs:
1. `cargo fmt --check` — formatting
2. `cargo clippy -- -D warnings` — linting
3. `cargo test` — all tests

### Alternative: Python pre-commit

If you prefer [pre-commit](https://pre-commit.com/):

```bash
pip install pre-commit
cd music_browser
pre-commit install
```

Config is in `music_browser/.pre-commit-config.yaml`.

## Project Structure

```
music_browser/
├── Cargo.toml                 # Dependencies and build config
├── .env                       # Environment variables (gitignored)
├── migrations/
│   └── 0001_initial.sql       # Database schema
├── scripts/
│   └── install-hooks.sh       # Pre-commit hook installer
├── src/
│   ├── main.rs                # Actix-web server, routes, handlers
│   └── db/
│       ├── mod.rs             # Module declarations
│       ├── models.rs          # Rust structs and enums
│       ├── pool.rs            # SQLite pool init and migrations
│       └── queries.rs         # SQL query functions
├── templates/                 # Askama HTML templates
│   ├── base.html              # Layout with nav
│   ├── songs.html             # Song list
│   ├── song_form.html         # Create/edit song
│   ├── albums.html            # Album list
│   ├── album_form.html        # Create album
│   ├── artists.html           # Artist list
│   ├── artist_form.html       # Create artist
│   ├── instruments.html       # Instrument list
│   ├── instrument_form.html   # Create instrument
│   ├── bands.html             # Band list
│   ├── band_form.html         # Create band
│   └── recordings.html        # Recording list
└── tests/
    └── db_tests.rs            # Database integration tests
```

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust (stable) |
| Web framework | Actix-web 4 |
| Templates | Askama 0.12 (Jinja2-like) |
| Database | SQLite via SQLx 0.8 |
| Migrations | SQLx migrate |
