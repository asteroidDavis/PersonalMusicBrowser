use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use tempfile::NamedTempFile;

// We reference the crate's db module via the binary's mod structure.
// Since main.rs uses `mod db;`, tests need to duplicate the module path.
// Instead we inline the migration and test the SQL queries directly.

async fn setup_pool() -> (SqlitePool, NamedTempFile) {
    let tmp = NamedTempFile::new().expect("Failed to create temp file");
    let db_path = tmp.path().to_str().unwrap().to_string();
    let url = format!("sqlite:{db_path}");

    let opts = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .expect("Failed to create pool");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Migration failed");

    (pool, tmp)
}

// ---------------------------------------------------------------------------
// Instrument tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_and_list_instruments() {
    let (pool, _tmp) = setup_pool().await;

    // Initially empty
    let rows = sqlx::query("SELECT id, name FROM instruments ORDER BY name")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        rows.is_empty(),
        "Expected no instruments, got {}",
        rows.len()
    );

    // Insert
    let res = sqlx::query("INSERT INTO instruments (name) VALUES (?)")
        .bind("Guitar")
        .execute(&pool)
        .await
        .unwrap();
    let guitar_id = res.last_insert_rowid();
    assert!(guitar_id > 0, "Expected positive id, got {guitar_id}");

    sqlx::query("INSERT INTO instruments (name) VALUES (?)")
        .bind("Piano")
        .execute(&pool)
        .await
        .unwrap();

    // List
    let rows = sqlx::query("SELECT id, name FROM instruments ORDER BY name")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "Expected 2 instruments, got {}", rows.len());
    let first_name: String = rows[0].get("name");
    assert_eq!(
        first_name, "Guitar",
        "Expected 'Guitar', got '{first_name}'"
    );

    // Delete
    sqlx::query("DELETE FROM instruments WHERE id = ?")
        .bind(guitar_id)
        .execute(&pool)
        .await
        .unwrap();
    let rows = sqlx::query("SELECT id FROM instruments")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "Expected 1 instrument after delete, got {}",
        rows.len()
    );
}

// ---------------------------------------------------------------------------
// Band tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_and_list_bands() {
    let (pool, _tmp) = setup_pool().await;

    sqlx::query("INSERT INTO bands (name) VALUES (?)")
        .bind("The Rust Band")
        .execute(&pool)
        .await
        .unwrap();

    let rows = sqlx::query("SELECT id, name FROM bands ORDER BY name")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "Expected 1 band, got {}", rows.len());
    let name: String = rows[0].get("name");
    assert_eq!(
        name, "The Rust Band",
        "Expected 'The Rust Band', got '{name}'"
    );
}

// ---------------------------------------------------------------------------
// Artist tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_artist_with_bands() {
    let (pool, _tmp) = setup_pool().await;

    // Create a band
    let band_res = sqlx::query("INSERT INTO bands (name) VALUES (?)")
        .bind("Jazz Quartet")
        .execute(&pool)
        .await
        .unwrap();
    let band_id = band_res.last_insert_rowid();

    // Create an artist
    let artist_res = sqlx::query("INSERT INTO artists (name) VALUES (?)")
        .bind("Miles")
        .execute(&pool)
        .await
        .unwrap();
    let artist_id = artist_res.last_insert_rowid();

    // Link artist to band
    sqlx::query("INSERT INTO artist_bands (artist_id, band_id) VALUES (?, ?)")
        .bind(artist_id)
        .bind(band_id)
        .execute(&pool)
        .await
        .unwrap();

    // Verify join
    let bands = sqlx::query(
        "SELECT b.id, b.name FROM bands b \
         INNER JOIN artist_bands ab ON ab.band_id = b.id \
         WHERE ab.artist_id = ?",
    )
    .bind(artist_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        bands.len(),
        1,
        "Expected 1 band for artist, got {}",
        bands.len()
    );
    let band_name: String = bands[0].get("name");
    assert_eq!(
        band_name, "Jazz Quartet",
        "Expected 'Jazz Quartet', got '{band_name}'"
    );
}

#[tokio::test]
async fn test_delete_artist_cascades_band_link() {
    let (pool, _tmp) = setup_pool().await;

    let band_res = sqlx::query("INSERT INTO bands (name) VALUES (?)")
        .bind("Band1")
        .execute(&pool)
        .await
        .unwrap();
    let band_id = band_res.last_insert_rowid();

    let artist_res = sqlx::query("INSERT INTO artists (name) VALUES (?)")
        .bind("Artist1")
        .execute(&pool)
        .await
        .unwrap();
    let artist_id = artist_res.last_insert_rowid();

    sqlx::query("INSERT INTO artist_bands (artist_id, band_id) VALUES (?, ?)")
        .bind(artist_id)
        .bind(band_id)
        .execute(&pool)
        .await
        .unwrap();

    // Delete artist_bands then artist (mirroring queries::delete_artist)
    sqlx::query("DELETE FROM artist_bands WHERE artist_id = ?")
        .bind(artist_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM artists WHERE id = ?")
        .bind(artist_id)
        .execute(&pool)
        .await
        .unwrap();

    let links = sqlx::query("SELECT * FROM artist_bands WHERE artist_id = ?")
        .bind(artist_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        links.is_empty(),
        "Expected no artist_bands rows after delete, got {}",
        links.len()
    );

    // Band still exists
    let band_row = sqlx::query("SELECT id FROM bands WHERE id = ?")
        .bind(band_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(
        band_row.is_some(),
        "Band should still exist after artist deletion"
    );
}

// ---------------------------------------------------------------------------
// Album tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_and_list_albums() {
    let (pool, _tmp) = setup_pool().await;

    sqlx::query("INSERT INTO albums (title, released, url) VALUES (?, ?, ?)")
        .bind("My Album")
        .bind(true)
        .bind("https://example.com")
        .execute(&pool)
        .await
        .unwrap();

    let rows = sqlx::query("SELECT id, title, released, url FROM albums ORDER BY title")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "Expected 1 album, got {}", rows.len());
    let title: String = rows[0].get("title");
    assert_eq!(title, "My Album", "Expected 'My Album', got '{title}'");
    let released: bool = rows[0].get("released");
    assert!(released, "Expected album to be released");
}

// ---------------------------------------------------------------------------
// Song CRUD tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_song_crud() {
    let (pool, _tmp) = setup_pool().await;

    // Create album first (FK constraint)
    let album_res = sqlx::query("INSERT INTO albums (title, released) VALUES (?, ?)")
        .bind("Test Album")
        .bind(false)
        .execute(&pool)
        .await
        .unwrap();
    let album_id = album_res.last_insert_rowid();

    // Create artist
    let artist_res = sqlx::query("INSERT INTO artists (name) VALUES (?)")
        .bind("TestArtist")
        .execute(&pool)
        .await
        .unwrap();
    let artist_id = artist_res.last_insert_rowid();

    // Create song
    let song_res = sqlx::query(
        "INSERT INTO songs (title, album_id, sheet_music, lyrics, song_type) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("Test Song")
    .bind(album_id)
    .bind("path/to/sheet.pdf")
    .bind("")
    .bind("song")
    .execute(&pool)
    .await
    .unwrap();
    let song_id = song_res.last_insert_rowid();

    // Link song to artist
    sqlx::query("INSERT INTO song_artists (song_id, artist_id) VALUES (?, ?)")
        .bind(song_id)
        .bind(artist_id)
        .execute(&pool)
        .await
        .unwrap();

    // Verify song with join
    let row = sqlx::query(
        "SELECT s.id, s.title, s.album_id, a.title as album_title, \
         s.sheet_music, s.lyrics, s.song_type \
         FROM songs s \
         INNER JOIN albums a ON a.id = s.album_id \
         WHERE s.id = ?",
    )
    .bind(song_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let fetched_title: String = row.get("title");
    assert_eq!(
        fetched_title, "Test Song",
        "Expected 'Test Song', got '{fetched_title}'"
    );
    let fetched_album: String = row.get("album_title");
    assert_eq!(
        fetched_album, "Test Album",
        "Expected 'Test Album', got '{fetched_album}'"
    );
    let fetched_sm: Option<String> = row.get("sheet_music");
    assert_eq!(
        fetched_sm.as_deref(),
        Some("path/to/sheet.pdf"),
        "Expected 'path/to/sheet.pdf', got '{fetched_sm:?}'"
    );

    // Verify artist link
    let artist_rows = sqlx::query(
        "SELECT ar.name FROM artists ar \
         INNER JOIN song_artists sa ON sa.artist_id = ar.id \
         WHERE sa.song_id = ?",
    )
    .bind(song_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        artist_rows.len(),
        1,
        "Expected 1 artist for song, got {}",
        artist_rows.len()
    );

    // Update song
    sqlx::query("UPDATE songs SET title = ? WHERE id = ?")
        .bind("Updated Song")
        .bind(song_id)
        .execute(&pool)
        .await
        .unwrap();

    let updated = sqlx::query("SELECT title FROM songs WHERE id = ?")
        .bind(song_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let updated_title: String = updated.get("title");
    assert_eq!(
        updated_title, "Updated Song",
        "Expected 'Updated Song', got '{updated_title}'"
    );

    // Delete song (clean up associations first)
    sqlx::query("DELETE FROM song_artists WHERE song_id = ?")
        .bind(song_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM songs WHERE id = ?")
        .bind(song_id)
        .execute(&pool)
        .await
        .unwrap();

    let gone = sqlx::query("SELECT id FROM songs WHERE id = ?")
        .bind(song_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(gone.is_none(), "Song should be deleted");
}

// ---------------------------------------------------------------------------
// Song type constraint test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_song_type_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let album_res = sqlx::query("INSERT INTO albums (title, released) VALUES (?, ?)")
        .bind("Album")
        .bind(false)
        .execute(&pool)
        .await
        .unwrap();
    let album_id = album_res.last_insert_rowid();

    // Valid types
    for song_type in &["song", "cover", "composition"] {
        let res = sqlx::query("INSERT INTO songs (title, album_id, song_type) VALUES (?, ?, ?)")
            .bind(format!("Song of type {song_type}"))
            .bind(album_id)
            .bind(*song_type)
            .execute(&pool)
            .await;
        assert!(
            res.is_ok(),
            "Expected song_type '{song_type}' to be valid, but got error: {:?}",
            res.err()
        );
    }

    // Invalid type
    let bad = sqlx::query("INSERT INTO songs (title, album_id, song_type) VALUES (?, ?, ?)")
        .bind("Bad Song")
        .bind(album_id)
        .bind("invalid_type")
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject 'invalid_type'"
    );
}

// ---------------------------------------------------------------------------
// Recording tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_recording_crud() {
    let (pool, _tmp) = setup_pool().await;

    let album_res = sqlx::query("INSERT INTO albums (title, released) VALUES (?, ?)")
        .bind("Rec Album")
        .bind(true)
        .execute(&pool)
        .await
        .unwrap();
    let album_id = album_res.last_insert_rowid();

    let song_res = sqlx::query("INSERT INTO songs (title, album_id, song_type) VALUES (?, ?, ?)")
        .bind("Rec Song")
        .bind(album_id)
        .bind("song")
        .execute(&pool)
        .await
        .unwrap();
    let song_id = song_res.last_insert_rowid();

    let inst_res = sqlx::query("INSERT INTO instruments (name) VALUES (?)")
        .bind("Drums")
        .execute(&pool)
        .await
        .unwrap();
    let inst_id = inst_res.last_insert_rowid();

    // Create recording
    let rec_res = sqlx::query(
        "INSERT INTO recordings (recording_type, path, song_id, notes_image) VALUES (?, ?, ?, ?)",
    )
    .bind("mix")
    .bind("/path/to/mix.wav")
    .bind(song_id)
    .bind("")
    .execute(&pool)
    .await
    .unwrap();
    let rec_id = rec_res.last_insert_rowid();

    // Link instrument
    sqlx::query("INSERT INTO recording_instruments (recording_id, instrument_id) VALUES (?, ?)")
        .bind(rec_id)
        .bind(inst_id)
        .execute(&pool)
        .await
        .unwrap();

    // Verify
    let rec_row = sqlx::query("SELECT recording_type, path, song_id FROM recordings WHERE id = ?")
        .bind(rec_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let rec_type: String = rec_row.get("recording_type");
    assert_eq!(rec_type, "mix", "Expected 'mix', got '{rec_type}'");
    let rec_song_id: i64 = rec_row.get("song_id");
    assert_eq!(
        rec_song_id, song_id,
        "Expected song_id {song_id}, got {rec_song_id}"
    );

    // Verify instrument link
    let inst_rows = sqlx::query(
        "SELECT i.name FROM instruments i \
         INNER JOIN recording_instruments ri ON ri.instrument_id = i.id \
         WHERE ri.recording_id = ?",
    )
    .bind(rec_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        inst_rows.len(),
        1,
        "Expected 1 instrument for recording, got {}",
        inst_rows.len()
    );

    // Delete recording
    sqlx::query("DELETE FROM recording_instruments WHERE recording_id = ?")
        .bind(rec_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM recordings WHERE id = ?")
        .bind(rec_id)
        .execute(&pool)
        .await
        .unwrap();

    let gone = sqlx::query("SELECT id FROM recordings WHERE id = ?")
        .bind(rec_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(gone.is_none(), "Recording should be deleted");
}

// ---------------------------------------------------------------------------
// Recording type constraint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_recording_type_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let album_res = sqlx::query("INSERT INTO albums (title, released) VALUES (?, ?)")
        .bind("A")
        .bind(false)
        .execute(&pool)
        .await
        .unwrap();
    let album_id = album_res.last_insert_rowid();

    let song_res = sqlx::query("INSERT INTO songs (title, album_id, song_type) VALUES (?, ?, ?)")
        .bind("S")
        .bind(album_id)
        .bind("song")
        .execute(&pool)
        .await
        .unwrap();
    let song_id = song_res.last_insert_rowid();

    for rt in &["audacity", "mix", "master", "loop-core-list", "wav"] {
        let res = sqlx::query("INSERT INTO recordings (recording_type, song_id) VALUES (?, ?)")
            .bind(*rt)
            .bind(song_id)
            .execute(&pool)
            .await;
        assert!(
            res.is_ok(),
            "Expected recording_type '{rt}' to be valid, but got error: {:?}",
            res.err()
        );
    }

    let bad = sqlx::query("INSERT INTO recordings (recording_type, song_id) VALUES (?, ?)")
        .bind("invalid")
        .bind(song_id)
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject 'invalid' recording_type"
    );
}

// ---------------------------------------------------------------------------
// Cover and Composition detail tables
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cover_details() {
    let (pool, _tmp) = setup_pool().await;

    let album_res = sqlx::query("INSERT INTO albums (title, released) VALUES (?, ?)")
        .bind("Cover Album")
        .bind(false)
        .execute(&pool)
        .await
        .unwrap();
    let album_id = album_res.last_insert_rowid();

    let song_res = sqlx::query("INSERT INTO songs (title, album_id, song_type) VALUES (?, ?, ?)")
        .bind("My Cover")
        .bind(album_id)
        .bind("cover")
        .execute(&pool)
        .await
        .unwrap();
    let song_id = song_res.last_insert_rowid();

    sqlx::query(
        "INSERT INTO cover_details (song_id, notes_image, notes_completed) VALUES (?, ?, ?)",
    )
    .bind(song_id)
    .bind("/path/to/notes.png")
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();

    let inst_res = sqlx::query("INSERT INTO instruments (name) VALUES (?)")
        .bind("Violin")
        .execute(&pool)
        .await
        .unwrap();
    let inst_id = inst_res.last_insert_rowid();

    sqlx::query("INSERT INTO cover_instruments (song_id, instrument_id) VALUES (?, ?)")
        .bind(song_id)
        .bind(inst_id)
        .execute(&pool)
        .await
        .unwrap();

    let cover = sqlx::query("SELECT notes_completed FROM cover_details WHERE song_id = ?")
        .bind(song_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let completed: bool = cover.get("notes_completed");
    assert!(completed, "Expected notes_completed to be true");
}

#[tokio::test]
async fn test_composition_details() {
    let (pool, _tmp) = setup_pool().await;

    let album_res = sqlx::query("INSERT INTO albums (title, released) VALUES (?, ?)")
        .bind("Comp Album")
        .bind(false)
        .execute(&pool)
        .await
        .unwrap();
    let album_id = album_res.last_insert_rowid();

    let song_res = sqlx::query("INSERT INTO songs (title, album_id, song_type) VALUES (?, ?, ?)")
        .bind("My Composition")
        .bind(album_id)
        .bind("composition")
        .execute(&pool)
        .await
        .unwrap();
    let song_id = song_res.last_insert_rowid();

    sqlx::query(
        "INSERT INTO composition_details (song_id, beats_per_minute_upper, beats_per_minute_lower) VALUES (?, ?, ?)",
    )
    .bind(song_id)
    .bind(140)
    .bind(120)
    .execute(&pool)
    .await
    .unwrap();

    let comp = sqlx::query(
        "SELECT beats_per_minute_upper, beats_per_minute_lower FROM composition_details WHERE song_id = ?",
    )
    .bind(song_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let upper: i32 = comp.get("beats_per_minute_upper");
    let lower: i32 = comp.get("beats_per_minute_lower");
    assert_eq!(upper, 140, "Expected bpm_upper 140, got {upper}");
    assert_eq!(lower, 120, "Expected bpm_lower 120, got {lower}");
}

// ---------------------------------------------------------------------------
// FK constraint: song PROTECT on album delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_album_delete_blocked_by_song_fk() {
    let (pool, _tmp) = setup_pool().await;

    // Enable foreign keys (SQLite needs this)
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    let album_res = sqlx::query("INSERT INTO albums (title, released) VALUES (?, ?)")
        .bind("Protected Album")
        .bind(false)
        .execute(&pool)
        .await
        .unwrap();
    let album_id = album_res.last_insert_rowid();

    sqlx::query("INSERT INTO songs (title, album_id, song_type) VALUES (?, ?, ?)")
        .bind("Blocking Song")
        .bind(album_id)
        .bind("song")
        .execute(&pool)
        .await
        .unwrap();

    // Attempting to delete the album should fail due to RESTRICT
    let delete_result = sqlx::query("DELETE FROM albums WHERE id = ?")
        .bind(album_id)
        .execute(&pool)
        .await;
    assert!(
        delete_result.is_err(),
        "Expected FK RESTRICT to prevent album deletion while songs reference it"
    );
}

// ---------------------------------------------------------------------------
// Migration idempotency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_runs_cleanly() {
    let (pool, _tmp) = setup_pool().await;

    // Verify all expected tables exist
    let tables = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let table_names: Vec<String> = tables.iter().map(|r| r.get("name")).collect();
    let expected = vec![
        "albums",
        "artist_bands",
        "artists",
        "bands",
        "composition_details",
        "composition_instruments",
        "cover_details",
        "cover_instruments",
        "instruments",
        "recording_instruments",
        "recordings",
        "song_artists",
        "songs",
    ];

    for exp in &expected {
        assert!(
            table_names.contains(&exp.to_string()),
            "Expected table '{exp}' to exist, found tables: {table_names:?}"
        );
    }
}
