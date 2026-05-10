use clap::{Parser, Subcommand};
use sqlx::{sqlite::SqlitePool, Connection, SqliteConnection};
use std::fs;
use std::path::Path;

#[derive(Parser)]
#[command(name = "migrate")]
#[command(about = "Database migration CLI with dry-run and backup support", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run pending migrations
    Run {
        /// Database URL (default: from DATABASE_URL env var)
        #[arg(long)]
        database_url: Option<String>,
        /// Show SQL without executing (dry run)
        #[arg(long)]
        dry_run: bool,
        /// Backup database before running migrations
        #[arg(long)]
        backup: bool,
        /// Backup destination directory
        #[arg(long)]
        backup_dir: Option<String>,
    },
    /// Rollback last migration
    Rollback {
        /// Database URL (default: from DATABASE_URL env var)
        #[arg(long)]
        database_url: Option<String>,
        /// Show SQL without executing (dry run)
        #[arg(long)]
        dry_run: bool,
        /// Backup database before rollback
        #[arg(long)]
        backup: bool,
        /// Backup destination directory
        #[arg(long)]
        backup_dir: Option<String>,
    },
    /// List migration status
    Status {
        /// Database URL (default: from DATABASE_URL env var)
        #[arg(long)]
        database_url: Option<String>,
    },
    /// Create a new migration
    Create {
        /// Migration description (e.g., "add_user_table")
        description: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            database_url,
            dry_run,
            backup,
            backup_dir,
        } => {
            let db_url = database_url.unwrap_or_else(|| {
                std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "sqlite:music_browser.db".to_string())
            });

            if backup {
                backup_database(&db_url, backup_dir.as_deref())?;
            }

            if dry_run {
                println!("=== DRY RUN MODE ===");
                println!("Would run migrations against: {}", db_url);
                list_pending_migrations(&db_url).await?;
            } else {
                run_migrations(&db_url).await?;
            }
        }
        Commands::Rollback {
            database_url,
            dry_run,
            backup,
            backup_dir,
        } => {
            let db_url = database_url.unwrap_or_else(|| {
                std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "sqlite:music_browser.db".to_string())
            });

            if backup {
                backup_database(&db_url, backup_dir.as_deref())?;
            }

            if dry_run {
                println!("=== DRY RUN MODE ===");
                println!("Would rollback last migration from: {}", db_url);
                show_rollback_sql().await?;
            } else {
                rollback_migration(&db_url).await?;
            }
        }
        Commands::Status { database_url } => {
            let db_url = database_url.unwrap_or_else(|| {
                std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "sqlite:music_browser.db".to_string())
            });
            show_migration_status(&db_url).await?;
        }
        Commands::Create { description } => {
            create_migration(&description)?;
        }
    }

    Ok(())
}

fn backup_database(
    db_url: &str,
    backup_dir: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Extract database file path from URL
    let db_path = if let Some(stripped) = db_url.strip_prefix("sqlite:") {
        stripped
    } else {
        db_url
    };

    if !Path::new(db_path).exists() {
        println!(
            "Warning: Database file {} does not exist, skipping backup",
            db_path
        );
        return Ok(());
    }

    let backup_dest = if let Some(dir) = backup_dir {
        fs::create_dir_all(dir)?;
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        format!("{}/music_browser_{}.db", dir, timestamp)
    } else {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        format!("{}.backup.{}", db_path, timestamp)
    };

    fs::copy(db_path, &backup_dest)?;
    println!("Database backed up to: {}", backup_dest);
    Ok(())
}

async fn run_migrations(db_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Running migrations against: {}", db_url);
    let pool = SqlitePool::connect(db_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    println!("Migrations completed successfully");
    Ok(())
}

async fn rollback_migration(db_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Rolling back last migration from: {}", db_url);

    // Get current migration version
    let mut conn = SqliteConnection::connect(db_url).await?;
    let version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1")
            .fetch_one(&mut conn)
            .await?;

    let version = match version {
        Some(v) => v,
        None => {
            println!("No migrations to rollback");
            return Ok(());
        }
    };

    let down_file = format!(
        "./migrations/down/{:04}_{}_down.sql",
        version,
        get_migration_name(version).await?
    );

    if !Path::new(&down_file).exists() {
        println!("No down migration file found: {}", down_file);
        return Ok(());
    }

    let sql = fs::read_to_string(&down_file)?;
    println!("Executing down migration: {}", down_file);

    // Execute each statement
    for statement in sql.split(';') {
        let statement = statement.trim();
        if !statement.is_empty() && !statement.starts_with("--") {
            sqlx::query(statement).execute(&mut conn).await?;
        }
    }

    // Remove migration record
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
        .bind(version)
        .execute(&mut conn)
        .await?;

    println!("Rollback completed successfully");
    Ok(())
}

async fn show_rollback_sql() -> Result<(), Box<dyn std::error::Error>> {
    // Get current migration version from database
    let db_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:music_browser.db".to_string());
    let mut conn = SqliteConnection::connect(&db_url).await?;
    let version: Option<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version DESC LIMIT 1")
            .fetch_one(&mut conn)
            .await?;

    let version = match version {
        Some(v) => v,
        None => {
            println!("No migrations to rollback");
            return Ok(());
        }
    };

    let down_file = format!(
        "./migrations/down/{:04}_{}_down.sql",
        version,
        get_migration_name(version).await?
    );

    if !Path::new(&down_file).exists() {
        println!("No down migration file found: {}", down_file);
        return Ok(());
    }

    let sql = fs::read_to_string(&down_file)?;
    println!("=== Would execute down migration: {} ===", down_file);
    println!("{}", sql);
    Ok(())
}

async fn list_pending_migrations(_db_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let migrations_dir = Path::new("./migrations");
    let mut migration_files: Vec<_> = fs::read_dir(migrations_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "sql")
                .unwrap_or(false)
        })
        .filter(|entry| !entry.path().to_string_lossy().contains("down"))
        .collect();

    migration_files.sort_by_key(|a| a.path());

    println!("=== Pending Migrations ===");
    for entry in migration_files {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy();
        let sql = fs::read_to_string(&path)?;
        println!("\n--- {} ---", filename);
        println!("{}", sql);
    }

    Ok(())
}

async fn show_migration_status(db_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let pool = SqlitePool::connect(db_url).await?;

    println!("=== Migration Status ===");
    println!("Database: {}", db_url);

    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT version, description FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await?;

    if rows.is_empty() {
        println!("No migrations have been applied");
    } else {
        println!("Applied migrations:");
        for (version, description) in &rows {
            println!("  {:04}: {}", version, description);
        }
    }

    // List pending migrations
    let migrations_dir = Path::new("./migrations");
    let mut migration_files: Vec<_> = fs::read_dir(migrations_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "sql")
                .unwrap_or(false)
        })
        .filter(|entry| !entry.path().to_string_lossy().contains("down"))
        .collect();

    migration_files.sort_by_key(|a| a.path());

    let applied_versions: std::collections::HashSet<i64> = rows.iter().map(|(v, _)| *v).collect();

    println!("\nPending migrations:");
    let mut has_pending = false;
    for entry in migration_files {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy();
        if let Some(version_str) = filename.split('_').next() {
            if let Ok(version) = version_str.parse::<i64>() {
                if !applied_versions.contains(&version) {
                    println!("  {}", filename);
                    has_pending = true;
                }
            }
        }
    }

    if !has_pending {
        println!("  (none)");
    }

    Ok(())
}

fn create_migration(description: &str) -> Result<(), Box<dyn std::error::Error>> {
    let migrations_dir = Path::new("./migrations");
    let down_dir = migrations_dir.join("down");

    fs::create_dir_all(&down_dir)?;

    // Find the next migration number
    let mut max_version = 0;
    for entry in fs::read_dir(migrations_dir)? {
        let entry = entry?;
        let filename = entry.file_name().to_string_lossy().to_string();
        if let Some(version_str) = filename.split('_').next() {
            if let Ok(version) = version_str.parse::<i64>() {
                if version > max_version {
                    max_version = version;
                }
            }
        }
    }

    let next_version = max_version + 1;
    let sanitized_name = description.to_lowercase().replace([' ', '-'], "_");
    let filename = format!("{:04}_{}.sql", next_version, sanitized_name);
    let down_filename = format!("{:04}_{}_down.sql", next_version, sanitized_name);

    let migration_path = migrations_dir.join(&filename);
    let down_path = down_dir.join(&down_filename);

    fs::write(
        &migration_path,
        format!("-- Migration {}: {}\n\n", next_version, description),
    )?;
    fs::write(
        &down_path,
        format!("-- Down migration {}: {}\n\n", next_version, description),
    )?;

    println!("Created migration: {}", filename);
    println!("Created down migration: down/{}", down_filename);

    Ok(())
}

async fn get_migration_name(version: i64) -> Result<String, Box<dyn std::error::Error>> {
    let migrations_dir = Path::new("./migrations");
    for entry in fs::read_dir(migrations_dir)? {
        let entry = entry?;
        let filename = entry.file_name().to_string_lossy().to_string();
        if let Some(version_str) = filename.split('_').next() {
            if let Ok(v) = version_str.parse::<i64>() {
                if v == version {
                    let name = filename
                        .strip_prefix(&format!("{:04}_", version))
                        .unwrap_or(&filename)
                        .strip_suffix(".sql")
                        .unwrap_or(&filename);
                    return Ok(name.to_string());
                }
            }
        }
    }
    Ok("unknown".to_string())
}
