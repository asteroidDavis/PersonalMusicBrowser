use clap::{Parser, Subcommand};
use regex::Regex;
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};

use music_browser::db::models::*;
use music_browser::db::queries;

// ===========================================================================
// CLI definition
// ===========================================================================

#[derive(Parser)]
#[command(
    name = "bulk-import",
    about = "Bulk import data into the music browser"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Import cover song setups from a markdown file with pipe-delimited tables.
    /// Expects sections headed by "**Guitar Songs**", "**Bass Songs**",
    /// "**Piano Songs**" (or similar) followed by markdown tables.
    Markdown {
        /// Path to the markdown file
        #[arg(short, long)]
        file: PathBuf,

        /// Dry-run: parse and print what would be imported without writing
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Import data from another SQLite database by attaching it.
    /// Copies rows from the source tables into the target, skipping duplicates
    /// by title/name.
    Sqlite {
        /// Path to the source .db / .sqlite3 file
        #[arg(short, long)]
        file: PathBuf,

        /// Dry-run: show counts without writing
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

// ===========================================================================
// Markdown parsing
// ===========================================================================

/// A single row parsed from the cover-song markdown table.
#[derive(Debug, Clone)]
struct MdSongRow {
    instrument_section: String, // "guitar", "bass", "piano"
    title: String,
    ultrawave: String,
    plethora: String,
    pog2: String,
    other_changes: String,
    description: String,
    score_url: String,
}

/// Parse a markdown file into `MdSongRow`s grouped by instrument section.
fn parse_markdown_tables(content: &str) -> Vec<MdSongRow> {
    let mut rows = Vec::new();
    let mut current_section = String::new();

    // Detect instrument section headers like **Guitar Songs**, **Bass Songs**
    let section_re = Regex::new(r"(?i)\*\*(\w+)\s+song").unwrap();

    for line in content.lines() {
        // Check for section header
        if let Some(cap) = section_re.captures(line) {
            current_section = cap[1].to_lowercase();
            continue;
        }

        // Skip non-table lines and separator lines
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || trimmed.contains("---") {
            continue;
        }

        // Split pipe-delimited columns, keeping interior empty cells so
        // column indices stay correct. Only strip leading/trailing empties
        // produced by the outer pipes.
        let all_cols: Vec<&str> = trimmed.split('|').map(|c| c.trim()).collect();
        let cols: Vec<&str> = if all_cols.first() == Some(&"") && all_cols.last() == Some(&"") {
            all_cols[1..all_cols.len() - 1].to_vec()
        } else if all_cols.first() == Some(&"") {
            all_cols[1..].to_vec()
        } else if all_cols.last() == Some(&"") {
            all_cols[..all_cols.len() - 1].to_vec()
        } else {
            all_cols
        };

        if cols.is_empty() {
            continue;
        }

        // Skip header rows (contain "Song" literally as first meaningful column)
        let first_col_lower = cols[0].to_lowercase();
        if first_col_lower == "song" || first_col_lower == "id" {
            continue;
        }

        // Determine column layout based on section.
        // Guitar: Song | Ultrawave | Plethora | POG2 | Other | Description | Score
        // Bass:   id | Song | Ultrawave | Plethora | POG2 | Other | Score
        // Piano:  Song | Ultrawave | Plethora | POG2 | Other | Bass | Score | Production | Mastered

        let row = if current_section == "bass" {
            // Bass table has an id column first
            let has_id_col = cols[0].parse::<i32>().is_ok() || cols[0].is_empty();
            let offset = if has_id_col { 1 } else { 0 };
            MdSongRow {
                instrument_section: current_section.clone(),
                title: cols.get(offset).unwrap_or(&"").to_string(),
                ultrawave: cols.get(offset + 1).unwrap_or(&"").to_string(),
                plethora: cols.get(offset + 2).unwrap_or(&"").to_string(),
                pog2: cols.get(offset + 3).unwrap_or(&"").to_string(),
                other_changes: cols.get(offset + 4).unwrap_or(&"").to_string(),
                description: String::new(),
                score_url: extract_md_link(cols.get(offset + 5).unwrap_or(&"")),
            }
        } else {
            // Guitar / Piano / default
            MdSongRow {
                instrument_section: current_section.clone(),
                title: cols.first().unwrap_or(&"").to_string(),
                ultrawave: cols.get(1).unwrap_or(&"").to_string(),
                plethora: cols.get(2).unwrap_or(&"").to_string(),
                pog2: cols.get(3).unwrap_or(&"").to_string(),
                other_changes: cols.get(4).unwrap_or(&"").to_string(),
                description: cols.get(5).unwrap_or(&"").to_string(),
                score_url: extract_md_link(cols.get(6).unwrap_or(&"")),
            }
        };

        if !row.title.is_empty() {
            rows.push(row);
        }
    }
    rows
}

/// Extract a URL from a markdown link `[text](url)`, or return the raw string.
fn extract_md_link(s: &str) -> String {
    let link_re = Regex::new(r"\[.*?\]\((.*?)\)").unwrap();
    if let Some(cap) = link_re.captures(s) {
        cap[1].to_string()
    } else {
        s.to_string()
    }
}

/// Ensure an instrument exists (by name + type) and return its id.
async fn ensure_instrument(
    pool: &SqlitePool,
    name: &str,
    instrument_type: &str,
) -> Result<i64, sqlx::Error> {
    let existing = sqlx::query("SELECT id FROM instruments WHERE name = ? AND instrument_type = ?")
        .bind(name)
        .bind(instrument_type)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = existing {
        Ok(row.get("id"))
    } else {
        let input = CreateInstrument {
            name: name.to_string(),
            instrument_type: instrument_type.to_string(),
        };
        queries::create_instrument(pool, &input).await
    }
}

/// Ensure a device exists by name and return its id.
async fn ensure_device(
    pool: &SqlitePool,
    name: &str,
    device_type: &str,
) -> Result<i64, sqlx::Error> {
    let existing = sqlx::query("SELECT id FROM devices WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?;

    if let Some(row) = existing {
        Ok(row.get("id"))
    } else {
        let input = CreateDevice {
            name: name.to_string(),
            device_type: device_type.to_string(),
            manual_path: String::new(),
            notes: String::new(),
        };
        queries::create_device(pool, &input).await
    }
}

/// Ensure a device preset exists and return its id.
async fn ensure_device_preset(
    pool: &SqlitePool,
    device_id: i64,
    preset_code: &str,
) -> Result<i64, sqlx::Error> {
    let existing =
        sqlx::query("SELECT id FROM device_presets WHERE device_id = ? AND preset_code = ?")
            .bind(device_id)
            .bind(preset_code)
            .fetch_optional(pool)
            .await?;

    if let Some(row) = existing {
        Ok(row.get("id"))
    } else {
        let input = CreateDevicePreset {
            device_id,
            name: preset_code.to_string(),
            preset_code: preset_code.to_string(),
            description: String::new(),
        };
        queries::create_device_preset(pool, &input).await
    }
}

/// Import parsed markdown rows into the database.
async fn import_markdown_rows(
    pool: &SqlitePool,
    rows: &[MdSongRow],
    dry_run: bool,
) -> Result<ImportStats, Box<dyn std::error::Error>> {
    let mut stats = ImportStats::default();

    for row in rows {
        // Map section name to instrument type
        let inst_type = match row.instrument_section.as_str() {
            "guitar" => "guitar",
            "bass" => "bass",
            "piano" => "piano",
            _ => "other",
        };

        if dry_run {
            println!(
                "[dry-run] Song: '{}' ({}) | UW: {} | PL: {} | POG2: {} | Score: {}",
                row.title,
                row.instrument_section,
                row.ultrawave,
                row.plethora,
                row.pog2,
                row.score_url
            );
            stats.songs_parsed += 1;
            continue;
        }

        // Check if song already exists by title
        let existing = sqlx::query("SELECT id FROM songs WHERE title = ?")
            .bind(&row.title)
            .fetch_optional(pool)
            .await?;

        let song_id = if let Some(existing_row) = existing {
            stats.songs_skipped += 1;
            existing_row.get("id")
        } else {
            let input = CreateSong {
                title: row.title.clone(),
                album_id: None,
                sheet_music: String::new(),
                lyrics: String::new(),
                song_type: SongType::Cover,
                key: String::new(),
                bpm_lower: None,
                bpm_upper: None,
                original_artist: String::new(),
                score_url: row.score_url.clone(),
                description: row.description.clone(),
                workflow_state: WorkflowState::Discovered,
                scores_folder: String::new(),
                export_folder: String::new(),
                musicxml_path: String::new(),
                artist_ids: vec![],
            };
            let id = queries::create_song(pool, &input).await?;
            stats.songs_created += 1;
            id
        };

        // Ensure the instrument exists
        let instrument_id = ensure_instrument(pool, &capitalize(inst_type), inst_type).await?;

        // Collect device preset ids for the song_instrument
        let mut preset_ids: Vec<i64> = Vec::new();

        // Ultrawave presets
        if !row.ultrawave.is_empty() && row.ultrawave.to_lowercase() != "none" {
            let dev_id = ensure_device(pool, "Ultrawave", "pedal").await?;
            let preset_id = ensure_device_preset(pool, dev_id, &row.ultrawave).await?;
            preset_ids.push(preset_id);
            stats.presets_created += 1;
        }

        // Plethora presets
        if !row.plethora.is_empty() && row.plethora.to_lowercase() != "none" {
            let dev_id = ensure_device(pool, "Plethora X5", "pedal").await?;
            let preset_id = ensure_device_preset(pool, dev_id, &row.plethora).await?;
            preset_ids.push(preset_id);
            stats.presets_created += 1;
        }

        // POG2 presets
        if !row.pog2.is_empty() && row.pog2.to_lowercase() != "none" {
            let dev_id = ensure_device(pool, "POG2", "pedal").await?;
            let preset_id = ensure_device_preset(pool, dev_id, &row.pog2).await?;
            preset_ids.push(preset_id);
            stats.presets_created += 1;
        }

        // Check if song_instrument already exists
        let si_exists =
            sqlx::query("SELECT id FROM song_instruments WHERE song_id = ? AND instrument_id = ?")
                .bind(song_id)
                .bind(instrument_id)
                .fetch_optional(pool)
                .await?;

        if si_exists.is_none() {
            let si_input = CreateSongInstrument {
                song_id,
                instrument_id,
                description: row.other_changes.clone(),
                score_url: row.score_url.clone(),
                production_path: String::new(),
                mastering_path: String::new(),
                preset_ids,
            };
            queries::create_song_instrument(pool, &si_input).await?;
            stats.song_instruments_created += 1;
        }
    }

    Ok(stats)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

// ===========================================================================
// SQLite import
// ===========================================================================

async fn import_sqlite(
    pool: &SqlitePool,
    source_path: &Path,
    dry_run: bool,
) -> Result<ImportStats, Box<dyn std::error::Error>> {
    let mut stats = ImportStats::default();

    let source_str = source_path.to_str().ok_or("Invalid source path")?;

    // Attach the source database
    sqlx::query(&format!("ATTACH DATABASE '{}' AS src", source_str))
        .execute(pool)
        .await?;

    // Map of source table -> target table for simple name-based tables
    let simple_tables = vec![
        ("instruments", "name"),
        ("bands", "name"),
        ("artists", "name"),
        ("albums", "title"),
    ];

    for (table, unique_col) in &simple_tables {
        let count_row = sqlx::query(&format!(
            "SELECT COUNT(*) as c FROM src.{table} WHERE {unique_col} NOT IN (SELECT {unique_col} FROM main.{table})"
        ))
        .fetch_one(pool)
        .await?;
        let count: i32 = count_row.get("c");

        if dry_run {
            println!("[dry-run] {table}: {count} new rows to import");
            stats.songs_parsed += count as usize;
        } else if count > 0 {
            // Get column names from source
            let col_rows = sqlx::query(&format!("PRAGMA src.table_info('{table}')"))
                .fetch_all(pool)
                .await?;
            let cols: Vec<String> = col_rows
                .iter()
                .map(|r| r.get::<String, _>("name"))
                .filter(|c| c != "id")
                .collect();
            let col_list = cols.join(", ");

            sqlx::query(&format!(
                "INSERT INTO main.{table} ({col_list}) \
                 SELECT {col_list} FROM src.{table} \
                 WHERE {unique_col} NOT IN (SELECT {unique_col} FROM main.{table})"
            ))
            .execute(pool)
            .await?;

            println!("{table}: imported {count} rows");
            stats.songs_created += count as usize;
        } else {
            println!("{table}: nothing new to import");
        }
    }

    // Songs: match by title to avoid duplicates
    let song_count_row = sqlx::query(
        "SELECT COUNT(*) as c FROM src.songs WHERE title NOT IN (SELECT title FROM main.songs)",
    )
    .fetch_one(pool)
    .await?;
    let song_count: i32 = song_count_row.get("c");

    if dry_run {
        println!("[dry-run] songs: {song_count} new rows to import");
    } else if song_count > 0 {
        // Get the columns that exist in both source and target
        let src_col_rows = sqlx::query("PRAGMA src.table_info('songs')")
            .fetch_all(pool)
            .await?;
        let src_cols: Vec<String> = src_col_rows
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .filter(|c| c != "id")
            .collect();

        let tgt_col_rows = sqlx::query("PRAGMA main.table_info('songs')")
            .fetch_all(pool)
            .await?;
        let tgt_cols: Vec<String> = tgt_col_rows
            .iter()
            .map(|r| r.get::<String, _>("name"))
            .filter(|c| c != "id")
            .collect();

        // Only copy columns that exist in both
        let common_cols: Vec<&String> = src_cols.iter().filter(|c| tgt_cols.contains(c)).collect();
        let col_list = common_cols
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        sqlx::query(&format!(
            "INSERT INTO main.songs ({col_list}) \
             SELECT {col_list} FROM src.songs \
             WHERE title NOT IN (SELECT title FROM main.songs)"
        ))
        .execute(pool)
        .await?;

        println!("songs: imported {song_count} rows");
        stats.songs_created += song_count as usize;
    } else {
        println!("songs: nothing new to import");
    }

    // Detach source
    sqlx::query("DETACH DATABASE src").execute(pool).await?;

    Ok(stats)
}

// ===========================================================================
// Stats
// ===========================================================================

#[derive(Debug, Default)]
struct ImportStats {
    songs_parsed: usize,
    songs_created: usize,
    songs_skipped: usize,
    song_instruments_created: usize,
    presets_created: usize,
}

impl std::fmt::Display for ImportStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Import complete:\n  Songs parsed: {}\n  Songs created: {}\n  Songs skipped (existing): {}\n  Song instruments created: {}\n  Presets referenced: {}",
            self.songs_parsed,
            self.songs_created,
            self.songs_skipped,
            self.song_instruments_created,
            self.presets_created,
        )
    }
}

// ===========================================================================
// Main
// ===========================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let cli = Cli::parse();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:music_browser.db".into());

    let pool = music_browser::db::pool::init_pool(&database_url)
        .await
        .expect("Failed to initialise database");

    match cli.command {
        Commands::Markdown { file, dry_run } => {
            let content = std::fs::read_to_string(&file)?;
            let rows = parse_markdown_tables(&content);
            println!("Parsed {} rows from {}", rows.len(), file.display());

            let stats = import_markdown_rows(&pool, &rows, dry_run).await?;
            println!("{stats}");
        }
        Commands::Sqlite { file, dry_run } => {
            println!("Importing from SQLite: {}", file.display());
            let stats = import_sqlite(&pool, &file, dry_run).await?;
            println!("{stats}");
        }
    }

    Ok(())
}

// ===========================================================================
// Unit tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_md_link_with_url() {
        let input = "[bass score](https://example.com/score.pdf)";
        let result = extract_md_link(input);
        assert_eq!(
            result, "https://example.com/score.pdf",
            "Expected URL extraction, got '{result}'"
        );
    }

    #[test]
    fn test_extract_md_link_plain_text() {
        let input = "In guitar binder";
        let result = extract_md_link(input);
        assert_eq!(
            result, "In guitar binder",
            "Expected plain text passthrough, got '{result}'"
        );
    }

    #[test]
    fn test_extract_md_link_empty() {
        assert_eq!(extract_md_link(""), "", "Expected empty string");
    }

    #[test]
    fn test_parse_markdown_tables_guitar_section() {
        let md = r#"
**Guitar Songs**

| Song                        | Ultrawave program changes   | Plethora program changes | POG2 presets | Other enumerated changes | Description | Score |
| --------------------------- | --------------------------- | ------------------------ | ------------ | ------------------------ | ----------- | ----- |
| Harrisburg by Josh Ritter   | None                        | PC8                      | None         | STRAT_POS3               | reverb test | [score](https://example.com) |
| Memory of a Daydream        | None                        | PC11                     | angel pad    | STRAT_POS1               |             |       |
"#;
        let rows = parse_markdown_tables(md);
        assert_eq!(rows.len(), 2, "Expected 2 rows, got {}", rows.len());
        assert_eq!(
            rows[0].instrument_section, "guitar",
            "Expected 'guitar' section, got '{}'",
            rows[0].instrument_section
        );
        assert_eq!(
            rows[0].title, "Harrisburg by Josh Ritter",
            "Expected 'Harrisburg by Josh Ritter', got '{}'",
            rows[0].title
        );
        assert_eq!(
            rows[0].plethora, "PC8",
            "Expected 'PC8', got '{}'",
            rows[0].plethora
        );
        assert_eq!(
            rows[0].score_url, "https://example.com",
            "Expected 'https://example.com', got '{}'",
            rows[0].score_url
        );
        assert_eq!(
            rows[1].pog2, "angel pad",
            "Expected 'angel pad', got '{}'",
            rows[1].pog2
        );
    }

    #[test]
    fn test_parse_markdown_tables_bass_section() {
        let md = r#"
**Bass Songs**

| id  | Song                            | Ultrawave program changes | Plethora program changes | POG2 presets | Other enumerated changes | Score |
| --- | ------------------------------- | ------------------------- | ------------------------ | ------------ | ----------------------- | ----- |
| 1   | Be Nothing by The beach fossils |                           |                          |              | PBASS_T10               | [bass score](https://example.com/bass) |
| 2   | Can't Help Falling in love      | None                      | None                     | None         | PBASS_T2                | [tab](https://example.com/tab) |
"#;
        let rows = parse_markdown_tables(md);
        assert_eq!(rows.len(), 2, "Expected 2 rows, got {}", rows.len());
        assert_eq!(
            rows[0].instrument_section, "bass",
            "Expected 'bass', got '{}'",
            rows[0].instrument_section
        );
        assert_eq!(
            rows[0].title, "Be Nothing by The beach fossils",
            "Expected 'Be Nothing by The beach fossils', got '{}'",
            rows[0].title
        );
        assert_eq!(
            rows[0].score_url, "https://example.com/bass",
            "Expected 'https://example.com/bass', got '{}'",
            rows[0].score_url
        );
    }

    #[test]
    fn test_parse_markdown_tables_multiple_sections() {
        let md = r#"
**Guitar Songs**

| Song    | Ultrawave | Plethora | POG2 | Other | Description | Score |
| ------- | --------- | -------- | ---- | ----- | ----------- | ----- |
| Song A  | PC1       | PC2      | None | test  | desc        |       |

**Bass Songs**

| id | Song   | Ultrawave | Plethora | POG2 | Other | Score |
| -- | ------ | --------- | -------- | ---- | ----- | ----- |
| 1  | Song B |           |          |      | PICK  |       |

**Piano Songs**

| Song   | Ultrawave | Plethora | POG2   | Other | Bass | Score |
| ------ | --------- | -------- | ------ | ----- | ---- | ----- |
| Song C | PC9       | PC7      | chorus | MODX8 |      |       |
"#;
        let rows = parse_markdown_tables(md);
        assert_eq!(rows.len(), 3, "Expected 3 rows, got {}", rows.len());
        assert_eq!(
            rows[0].instrument_section, "guitar",
            "Expected 'guitar', got '{}'",
            rows[0].instrument_section
        );
        assert_eq!(
            rows[1].instrument_section, "bass",
            "Expected 'bass', got '{}'",
            rows[1].instrument_section
        );
        assert_eq!(
            rows[2].instrument_section, "piano",
            "Expected 'piano', got '{}'",
            rows[2].instrument_section
        );
    }

    #[test]
    fn test_parse_empty_markdown() {
        let rows = parse_markdown_tables("");
        assert!(
            rows.is_empty(),
            "Expected no rows from empty input, got {}",
            rows.len()
        );
    }

    #[test]
    fn test_parse_skips_empty_title_rows() {
        let md = r#"
**Guitar Songs**

| Song | Ultrawave | Plethora | POG2 | Other | Description | Score |
| ---- | --------- | -------- | ---- | ----- | ----------- | ----- |
|      |           |          |      |       |             |       |
"#;
        let rows = parse_markdown_tables(md);
        assert!(
            rows.is_empty(),
            "Expected empty rows to be skipped, got {}",
            rows.len()
        );
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("guitar"), "Guitar");
        assert_eq!(capitalize("bass"), "Bass");
        assert_eq!(capitalize(""), "");
    }
}
