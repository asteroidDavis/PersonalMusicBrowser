use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use tempfile::NamedTempFile;

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

// Helper: create an instrument and return its id.
async fn insert_instrument(pool: &SqlitePool, name: &str, itype: &str) -> i64 {
    sqlx::query("INSERT INTO instruments (name, instrument_type) VALUES (?, ?)")
        .bind(name)
        .bind(itype)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
}

// Helper: create a song (no album) and return its id.
async fn insert_song(pool: &SqlitePool, title: &str, song_type: &str) -> i64 {
    sqlx::query("INSERT INTO songs (title, song_type) VALUES (?, ?)")
        .bind(title)
        .bind(song_type)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
}

// Helper: create a song with album and return its id.
async fn insert_song_with_album(
    pool: &SqlitePool,
    title: &str,
    album_id: i64,
    song_type: &str,
) -> i64 {
    sqlx::query("INSERT INTO songs (title, album_id, song_type) VALUES (?, ?, ?)")
        .bind(title)
        .bind(album_id)
        .bind(song_type)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
}

// Helper: create an album and return its id.
async fn insert_album(pool: &SqlitePool, title: &str) -> i64 {
    sqlx::query("INSERT INTO albums (title, released) VALUES (?, ?)")
        .bind(title)
        .bind(false)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid()
}

// ===========================================================================
// Migration & schema tests
// ===========================================================================

#[tokio::test]
async fn test_migration_creates_all_tables() {
    let (pool, _tmp) = setup_pool().await;

    let tables = sqlx::query(
        "SELECT name FROM sqlite_master \
         WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations' \
         ORDER BY name",
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
        "device_presets",
        "devices",
        "instruments",
        "production_stages",
        "production_steps",
        "recording_instruments",
        "recordings",
        "sample_instruments",
        "samples",
        "song_artists",
        "song_files",
        "song_instrument_presets",
        "song_instruments",
        "songs",
    ];

    for exp in &expected {
        assert!(
            table_names.contains(&exp.to_string()),
            "Expected table '{exp}' to exist, found tables: {table_names:?}"
        );
    }
}

// ===========================================================================
// Instrument tests (with instrument_type)
// ===========================================================================

#[tokio::test]
async fn test_instrument_crud_with_type() {
    let (pool, _tmp) = setup_pool().await;

    let rows = sqlx::query("SELECT id FROM instruments")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        rows.is_empty(),
        "Expected no instruments, got {}",
        rows.len()
    );

    let guitar_id = insert_instrument(&pool, "Guitar", "guitar").await;
    assert!(guitar_id > 0, "Expected positive id, got {guitar_id}");

    insert_instrument(&pool, "Piano", "piano").await;

    let rows = sqlx::query("SELECT id, name, instrument_type FROM instruments ORDER BY name")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2, "Expected 2 instruments, got {}", rows.len());
    let itype: String = rows[0].get("instrument_type");
    assert_eq!(itype, "guitar", "Expected 'guitar', got '{itype}'");

    sqlx::query("DELETE FROM instruments WHERE id = ?")
        .bind(guitar_id)
        .execute(&pool)
        .await
        .unwrap();
    let remaining = sqlx::query("SELECT id FROM instruments")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        remaining.len(),
        1,
        "Expected 1 instrument after delete, got {}",
        remaining.len()
    );
}

#[tokio::test]
async fn test_instrument_type_default() {
    let (pool, _tmp) = setup_pool().await;

    // instrument_type defaults to 'other' when not provided
    sqlx::query("INSERT INTO instruments (name) VALUES (?)")
        .bind("Kazoo")
        .execute(&pool)
        .await
        .unwrap();

    let row = sqlx::query("SELECT instrument_type FROM instruments WHERE name = 'Kazoo'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let itype: String = row.get("instrument_type");
    assert_eq!(itype, "other", "Expected default 'other', got '{itype}'");
}

#[tokio::test]
async fn test_instrument_type_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let bad = sqlx::query("INSERT INTO instruments (name, instrument_type) VALUES (?, ?)")
        .bind("Bad")
        .bind("kazoo")
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject invalid instrument_type 'kazoo'"
    );
}

// ===========================================================================
// Band tests
// ===========================================================================

#[tokio::test]
async fn test_band_crud() {
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

// ===========================================================================
// Artist tests
// ===========================================================================

#[tokio::test]
async fn test_artist_with_bands() {
    let (pool, _tmp) = setup_pool().await;

    let band_id = sqlx::query("INSERT INTO bands (name) VALUES (?)")
        .bind("Jazz Quartet")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    let artist_id = sqlx::query("INSERT INTO artists (name) VALUES (?)")
        .bind("Miles")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    sqlx::query("INSERT INTO artist_bands (artist_id, band_id) VALUES (?, ?)")
        .bind(artist_id)
        .bind(band_id)
        .execute(&pool)
        .await
        .unwrap();

    let bands = sqlx::query(
        "SELECT b.name FROM bands b \
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
async fn test_delete_artist_removes_band_link() {
    let (pool, _tmp) = setup_pool().await;

    let band_id = sqlx::query("INSERT INTO bands (name) VALUES ('B1')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    let artist_id = sqlx::query("INSERT INTO artists (name) VALUES ('A1')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    sqlx::query("INSERT INTO artist_bands (artist_id, band_id) VALUES (?, ?)")
        .bind(artist_id)
        .bind(band_id)
        .execute(&pool)
        .await
        .unwrap();

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
        "Expected no artist_bands rows, got {}",
        links.len()
    );

    let band_exists = sqlx::query("SELECT id FROM bands WHERE id = ?")
        .bind(band_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(
        band_exists.is_some(),
        "Band should still exist after artist deletion"
    );
}

// ===========================================================================
// Album tests
// ===========================================================================

#[tokio::test]
async fn test_album_crud() {
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

// ===========================================================================
// Song tests (expanded with new fields)
// ===========================================================================

#[tokio::test]
async fn test_song_without_album() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "No Album Song", "song").await;

    let row = sqlx::query(
        "SELECT s.id, s.title, s.album_id, COALESCE(a.title, '') as album_title \
         FROM songs s LEFT JOIN albums a ON a.id = s.album_id WHERE s.id = ?",
    )
    .bind(song_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let album_id: Option<i64> = row.get("album_id");
    assert!(
        album_id.is_none(),
        "Expected NULL album_id, got {album_id:?}"
    );
    let album_title: String = row.get("album_title");
    assert_eq!(
        album_title, "",
        "Expected empty album_title, got '{album_title}'"
    );
}

#[tokio::test]
async fn test_song_with_all_new_fields() {
    let (pool, _tmp) = setup_pool().await;

    let album_id = insert_album(&pool, "Test Album").await;

    sqlx::query(
        "INSERT INTO songs (title, album_id, song_type, key, bpm_lower, bpm_upper, \
         original_artist, score_url, description) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("Full Song")
    .bind(album_id)
    .bind("cover")
    .bind("Am")
    .bind(90)
    .bind(120)
    .bind("Led Zeppelin")
    .bind("https://scores.example.com/full")
    .bind("A detailed description")
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(
        "SELECT key, bpm_lower, bpm_upper, original_artist, score_url, description \
         FROM songs WHERE title = 'Full Song'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let key: String = row.get("key");
    assert_eq!(key, "Am", "Expected key 'Am', got '{key}'");
    let bpm_lo: Option<i32> = row.get("bpm_lower");
    assert_eq!(bpm_lo, Some(90), "Expected bpm_lower 90, got {bpm_lo:?}");
    let bpm_hi: Option<i32> = row.get("bpm_upper");
    assert_eq!(bpm_hi, Some(120), "Expected bpm_upper 120, got {bpm_hi:?}");
    let orig: String = row.get("original_artist");
    assert_eq!(
        orig, "Led Zeppelin",
        "Expected 'Led Zeppelin', got '{orig}'"
    );
}

#[tokio::test]
async fn test_song_type_constraint_expanded() {
    let (pool, _tmp) = setup_pool().await;

    for song_type in &["song", "cover", "composition", "original", "practice"] {
        let res = sqlx::query("INSERT INTO songs (title, song_type) VALUES (?, ?)")
            .bind(format!("Type {song_type}"))
            .bind(*song_type)
            .execute(&pool)
            .await;
        assert!(
            res.is_ok(),
            "Expected song_type '{song_type}' to be valid, but got error: {:?}",
            res.err()
        );
    }

    let bad = sqlx::query("INSERT INTO songs (title, song_type) VALUES (?, ?)")
        .bind("Bad")
        .bind("invalid_type")
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject 'invalid_type'"
    );
}

#[tokio::test]
async fn test_song_crud_full() {
    let (pool, _tmp) = setup_pool().await;

    let album_id = insert_album(&pool, "CRUD Album").await;
    let artist_id = sqlx::query("INSERT INTO artists (name) VALUES ('TestArtist')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    let song_id = insert_song_with_album(&pool, "Test Song", album_id, "song").await;

    sqlx::query("INSERT INTO song_artists (song_id, artist_id) VALUES (?, ?)")
        .bind(song_id)
        .bind(artist_id)
        .execute(&pool)
        .await
        .unwrap();

    let row = sqlx::query(
        "SELECT s.title, a.title as album_title FROM songs s \
         INNER JOIN albums a ON a.id = s.album_id WHERE s.id = ?",
    )
    .bind(song_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let title: String = row.get("title");
    assert_eq!(title, "Test Song", "Expected 'Test Song', got '{title}'");

    sqlx::query("UPDATE songs SET title = 'Updated' WHERE id = ?")
        .bind(song_id)
        .execute(&pool)
        .await
        .unwrap();
    let updated: String = sqlx::query("SELECT title FROM songs WHERE id = ?")
        .bind(song_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("title");
    assert_eq!(updated, "Updated", "Expected 'Updated', got '{updated}'");

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

// ===========================================================================
// Recording tests (expanded types)
// ===========================================================================

#[tokio::test]
async fn test_recording_crud() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "Rec Song", "song").await;
    let inst_id = insert_instrument(&pool, "Drums", "drums").await;

    let rec_id = sqlx::query(
        "INSERT INTO recordings (recording_type, path, song_id, notes_image) VALUES (?, ?, ?, ?)",
    )
    .bind("mix")
    .bind("/path/to/mix.wav")
    .bind(song_id)
    .bind("")
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    sqlx::query("INSERT INTO recording_instruments (recording_id, instrument_id) VALUES (?, ?)")
        .bind(rec_id)
        .bind(inst_id)
        .execute(&pool)
        .await
        .unwrap();

    let rec_type: String = sqlx::query("SELECT recording_type FROM recordings WHERE id = ?")
        .bind(rec_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("recording_type");
    assert_eq!(rec_type, "mix", "Expected 'mix', got '{rec_type}'");

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

#[tokio::test]
async fn test_recording_type_constraint_expanded() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "S", "song").await;

    for rt in &[
        "audacity",
        "mix",
        "master",
        "loop-core-list",
        "wav",
        "daw-project",
        "practice",
    ] {
        let res = sqlx::query("INSERT INTO recordings (recording_type, song_id) VALUES (?, ?)")
            .bind(*rt)
            .bind(song_id)
            .execute(&pool)
            .await;
        assert!(
            res.is_ok(),
            "Expected recording_type '{rt}' to be valid, got error: {:?}",
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

// ===========================================================================
// Cover & Composition detail tests
// ===========================================================================

#[tokio::test]
async fn test_cover_details() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "My Cover", "cover").await;
    let inst_id = insert_instrument(&pool, "Violin", "strings").await;

    sqlx::query(
        "INSERT INTO cover_details (song_id, notes_image, notes_completed) VALUES (?, ?, ?)",
    )
    .bind(song_id)
    .bind("/path/to/notes.png")
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO cover_instruments (song_id, instrument_id) VALUES (?, ?)")
        .bind(song_id)
        .bind(inst_id)
        .execute(&pool)
        .await
        .unwrap();

    let completed: bool =
        sqlx::query("SELECT notes_completed FROM cover_details WHERE song_id = ?")
            .bind(song_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("notes_completed");
    assert!(completed, "Expected notes_completed to be true");
}

#[tokio::test]
async fn test_composition_details() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "My Composition", "composition").await;

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

// ===========================================================================
// Device & preset tests
// ===========================================================================

#[tokio::test]
async fn test_device_crud() {
    let (pool, _tmp) = setup_pool().await;

    let dev_id = sqlx::query(
        "INSERT INTO devices (name, device_type, manual_path, notes) VALUES (?, ?, ?, ?)",
    )
    .bind("Plethora X5")
    .bind("pedal")
    .bind("/manuals/plethora.pdf")
    .bind("Multi-effects pedal")
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let row = sqlx::query("SELECT name, device_type FROM devices WHERE id = ?")
        .bind(dev_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let name: String = row.get("name");
    assert_eq!(name, "Plethora X5", "Expected 'Plethora X5', got '{name}'");
    let dtype: String = row.get("device_type");
    assert_eq!(dtype, "pedal", "Expected 'pedal', got '{dtype}'");

    sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(dev_id)
        .execute(&pool)
        .await
        .unwrap();
    let gone = sqlx::query("SELECT id FROM devices WHERE id = ?")
        .bind(dev_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(gone.is_none(), "Device should be deleted");
}

#[tokio::test]
async fn test_device_type_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let bad = sqlx::query("INSERT INTO devices (name, device_type) VALUES (?, ?)")
        .bind("Bad")
        .bind("banana")
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject invalid device_type 'banana'"
    );
}

#[tokio::test]
async fn test_device_preset_crud_and_cascade() {
    let (pool, _tmp) = setup_pool().await;

    let dev_id = sqlx::query("INSERT INTO devices (name, device_type) VALUES (?, ?)")
        .bind("Ultrawave")
        .bind("pedal")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    let preset_id = sqlx::query(
        "INSERT INTO device_presets (device_id, name, preset_code, description) VALUES (?, ?, ?, ?)",
    )
    .bind(dev_id)
    .bind("Clean Crunch")
    .bind("PC:001")
    .bind("Light overdrive")
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let pname: String = sqlx::query("SELECT name FROM device_presets WHERE id = ?")
        .bind(preset_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("name");
    assert_eq!(
        pname, "Clean Crunch",
        "Expected 'Clean Crunch', got '{pname}'"
    );

    // Cascade: deleting device should delete its presets
    sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(dev_id)
        .execute(&pool)
        .await
        .unwrap();
    let preset_gone = sqlx::query("SELECT id FROM device_presets WHERE id = ?")
        .bind(preset_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(
        preset_gone.is_none(),
        "Preset should be cascade-deleted with device"
    );
}

// ===========================================================================
// Song instruments (live config) tests
// ===========================================================================

#[tokio::test]
async fn test_song_instrument_with_presets() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "Live Song", "cover").await;
    let inst_id = insert_instrument(&pool, "Guitar", "guitar").await;

    let dev_id =
        sqlx::query("INSERT INTO devices (name, device_type) VALUES ('PedalBoard', 'pedal')")
            .execute(&pool)
            .await
            .unwrap()
            .last_insert_rowid();

    let preset_id = sqlx::query(
        "INSERT INTO device_presets (device_id, name, preset_code) VALUES (?, 'OD1', 'PC:42')",
    )
    .bind(dev_id)
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let si_id = sqlx::query(
        "INSERT INTO song_instruments (song_id, instrument_id, description, score_url) VALUES (?, ?, ?, ?)",
    )
    .bind(song_id)
    .bind(inst_id)
    .bind("Lead guitar part")
    .bind("https://scores.example.com/lead")
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    sqlx::query(
        "INSERT INTO song_instrument_presets (song_instrument_id, device_preset_id) VALUES (?, ?)",
    )
    .bind(si_id)
    .bind(preset_id)
    .execute(&pool)
    .await
    .unwrap();

    let preset_rows = sqlx::query(
        "SELECT dp.name FROM device_presets dp \
         INNER JOIN song_instrument_presets sip ON sip.device_preset_id = dp.id \
         WHERE sip.song_instrument_id = ?",
    )
    .bind(si_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        preset_rows.len(),
        1,
        "Expected 1 preset, got {}",
        preset_rows.len()
    );
    let pname: String = preset_rows[0].get("name");
    assert_eq!(pname, "OD1", "Expected preset 'OD1', got '{pname}'");

    // Cascade: delete song_instrument should delete preset links
    sqlx::query("DELETE FROM song_instrument_presets WHERE song_instrument_id = ?")
        .bind(si_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM song_instruments WHERE id = ?")
        .bind(si_id)
        .execute(&pool)
        .await
        .unwrap();
    let gone = sqlx::query("SELECT id FROM song_instruments WHERE id = ?")
        .bind(si_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(gone.is_none(), "Song instrument should be deleted");
}

// ===========================================================================
// Production stages & steps tests
// ===========================================================================

#[tokio::test]
async fn test_production_stages_and_steps() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "Prod Song", "original").await;
    let inst_id = insert_instrument(&pool, "Bass", "bass").await;

    let stage_id =
        sqlx::query("INSERT INTO production_stages (song_id, stage, status) VALUES (?, ?, ?)")
            .bind(song_id)
            .bind("tracking")
            .bind("in_progress")
            .execute(&pool)
            .await
            .unwrap()
            .last_insert_rowid();

    let step_id = sqlx::query(
        "INSERT INTO production_steps (stage_id, instrument_id, name, status, sort_order, notes) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(stage_id)
    .bind(inst_id)
    .bind("Record bass DI")
    .bind("not_started")
    .bind(1)
    .bind("Use new strings")
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let stage_status: String = sqlx::query("SELECT status FROM production_stages WHERE id = ?")
        .bind(stage_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("status");
    assert_eq!(
        stage_status, "in_progress",
        "Expected 'in_progress', got '{stage_status}'"
    );

    let step_name: String = sqlx::query("SELECT name FROM production_steps WHERE id = ?")
        .bind(step_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("name");
    assert_eq!(
        step_name, "Record bass DI",
        "Expected 'Record bass DI', got '{step_name}'"
    );

    // Update status
    sqlx::query("UPDATE production_stages SET status = 'complete' WHERE id = ?")
        .bind(stage_id)
        .execute(&pool)
        .await
        .unwrap();
    let updated: String = sqlx::query("SELECT status FROM production_stages WHERE id = ?")
        .bind(stage_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("status");
    assert_eq!(updated, "complete", "Expected 'complete', got '{updated}'");

    // Cascade: delete stage should delete steps
    sqlx::query("DELETE FROM production_steps WHERE stage_id = ?")
        .bind(stage_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM production_stages WHERE id = ?")
        .bind(stage_id)
        .execute(&pool)
        .await
        .unwrap();
    let step_gone = sqlx::query("SELECT id FROM production_steps WHERE id = ?")
        .bind(step_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(
        step_gone.is_none(),
        "Production step should be deleted with stage"
    );
}

#[tokio::test]
async fn test_production_stage_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "CS", "song").await;

    // Migration 0003 relaxed the stage CHECK to length <= 128, so custom names are allowed.
    // Test that a stage name exceeding the length limit is rejected.
    let long_stage = "x".repeat(129);
    let bad_stage =
        sqlx::query("INSERT INTO production_stages (song_id, stage, status) VALUES (?, ?, ?)")
            .bind(song_id)
            .bind(&long_stage)
            .bind("not_started")
            .execute(&pool)
            .await;
    assert!(
        bad_stage.is_err(),
        "Expected CHECK constraint to reject stage name longer than 128 chars"
    );

    // Status CHECK constraint is still enforced.
    let bad_status =
        sqlx::query("INSERT INTO production_stages (song_id, stage, status) VALUES (?, ?, ?)")
            .bind(song_id)
            .bind("tracking")
            .bind("invalid_status")
            .execute(&pool)
            .await;
    assert!(
        bad_status.is_err(),
        "Expected CHECK constraint to reject invalid status"
    );
}

#[tokio::test]
async fn test_production_stage_unique_per_song() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "Unique Stage", "song").await;

    sqlx::query("INSERT INTO production_stages (song_id, stage, status) VALUES (?, 'tracking', 'not_started')")
        .bind(song_id)
        .execute(&pool)
        .await
        .unwrap();

    let dup = sqlx::query("INSERT INTO production_stages (song_id, stage, status) VALUES (?, 'tracking', 'in_progress')")
        .bind(song_id)
        .execute(&pool)
        .await;
    assert!(
        dup.is_err(),
        "Expected UNIQUE constraint on (song_id, stage)"
    );
}

// ===========================================================================
// Song files tests
// ===========================================================================

#[tokio::test]
async fn test_song_files_crud() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "File Song", "song").await;
    let inst_id = insert_instrument(&pool, "Piano", "piano").await;

    let file_id = sqlx::query(
        "INSERT INTO song_files (song_id, file_type, path, instrument_id, description) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(song_id)
    .bind("daw_project")
    .bind("/projects/file_song.als")
    .bind(inst_id)
    .bind("Ableton project")
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let row = sqlx::query("SELECT file_type, path FROM song_files WHERE id = ?")
        .bind(file_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let ftype: String = row.get("file_type");
    assert_eq!(
        ftype, "daw_project",
        "Expected 'daw_project', got '{ftype}'"
    );

    sqlx::query("DELETE FROM song_files WHERE id = ?")
        .bind(file_id)
        .execute(&pool)
        .await
        .unwrap();
    let gone = sqlx::query("SELECT id FROM song_files WHERE id = ?")
        .bind(file_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(gone.is_none(), "Song file should be deleted");
}

#[tokio::test]
async fn test_song_file_type_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let song_id = insert_song(&pool, "FT", "song").await;

    let bad = sqlx::query("INSERT INTO song_files (song_id, file_type, path) VALUES (?, ?, ?)")
        .bind(song_id)
        .bind("invalid_type")
        .bind("/x")
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject invalid file_type"
    );
}

// ===========================================================================
// Sample tests
// ===========================================================================

#[tokio::test]
async fn test_sample_crud() {
    let (pool, _tmp) = setup_pool().await;

    let inst_id = insert_instrument(&pool, "Synth Pad", "synth").await;

    let sample_id = sqlx::query(
        "INSERT INTO samples (name, path, bpm, key, description) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("Warm Pad")
    .bind("/samples/warm_pad.wav")
    .bind(128)
    .bind("Cm")
    .bind("A warm analog pad")
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    sqlx::query("INSERT INTO sample_instruments (sample_id, instrument_id) VALUES (?, ?)")
        .bind(sample_id)
        .bind(inst_id)
        .execute(&pool)
        .await
        .unwrap();

    let row = sqlx::query("SELECT name, bpm, key FROM samples WHERE id = ?")
        .bind(sample_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let name: String = row.get("name");
    assert_eq!(name, "Warm Pad", "Expected 'Warm Pad', got '{name}'");
    let bpm: Option<i32> = row.get("bpm");
    assert_eq!(bpm, Some(128), "Expected bpm 128, got {bpm:?}");

    let inst_count =
        sqlx::query("SELECT COUNT(*) as c FROM sample_instruments WHERE sample_id = ?")
            .bind(sample_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let count: i32 = inst_count.get("c");
    assert_eq!(count, 1, "Expected 1 sample_instrument link, got {count}");

    // Delete sample (clean up junction)
    sqlx::query("DELETE FROM sample_instruments WHERE sample_id = ?")
        .bind(sample_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM samples WHERE id = ?")
        .bind(sample_id)
        .execute(&pool)
        .await
        .unwrap();
    let gone = sqlx::query("SELECT id FROM samples WHERE id = ?")
        .bind(sample_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(gone.is_none(), "Sample should be deleted");
}

// ===========================================================================
// FK constraint: album delete SET NULL (new behavior)
// ===========================================================================

#[tokio::test]
async fn test_album_delete_sets_song_album_null() {
    let (pool, _tmp) = setup_pool().await;

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    let album_id = insert_album(&pool, "Deletable Album").await;
    let song_id = insert_song_with_album(&pool, "Orphan Song", album_id, "song").await;

    sqlx::query("DELETE FROM albums WHERE id = ?")
        .bind(album_id)
        .execute(&pool)
        .await
        .unwrap();

    let album_ref: Option<i64> = sqlx::query("SELECT album_id FROM songs WHERE id = ?")
        .bind(song_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("album_id");
    assert!(
        album_ref.is_none(),
        "Expected album_id to be NULL after album deletion, got {album_ref:?}"
    );
}

// ===========================================================================
// FK constraint: recording RESTRICT on song delete
// ===========================================================================

#[tokio::test]
async fn test_song_delete_blocked_by_recording_fk() {
    let (pool, _tmp) = setup_pool().await;

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    let song_id = insert_song(&pool, "Protected Song", "song").await;

    sqlx::query("INSERT INTO recordings (recording_type, song_id) VALUES ('wav', ?)")
        .bind(song_id)
        .execute(&pool)
        .await
        .unwrap();

    let delete_result = sqlx::query("DELETE FROM songs WHERE id = ?")
        .bind(song_id)
        .execute(&pool)
        .await;
    assert!(
        delete_result.is_err(),
        "Expected FK RESTRICT to prevent song deletion while recordings reference it"
    );
}

// ===========================================================================
// Song-instrument cascade on song delete
// ===========================================================================

#[tokio::test]
async fn test_song_instruments_cascade_on_song_delete() {
    let (pool, _tmp) = setup_pool().await;

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();

    let song_id = insert_song(&pool, "Cascade Song", "song").await;
    let inst_id = insert_instrument(&pool, "G", "guitar").await;

    sqlx::query("INSERT INTO song_instruments (song_id, instrument_id) VALUES (?, ?)")
        .bind(song_id)
        .bind(inst_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("DELETE FROM songs WHERE id = ?")
        .bind(song_id)
        .execute(&pool)
        .await
        .unwrap();

    let si_rows = sqlx::query("SELECT id FROM song_instruments WHERE song_id = ?")
        .bind(song_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        si_rows.is_empty(),
        "Expected song_instruments to be cascade-deleted, got {}",
        si_rows.len()
    );
}

// ============================================================================
// Scheduling feature tests
// ============================================================================

#[tokio::test]
async fn test_workflow_state_transitions() {
    let (pool, _tmp) = setup_pool().await;
    let song_id = insert_song(&pool, "WF Song", "song").await;

    // Default should be 'discovered'
    let row = sqlx::query("SELECT workflow_state FROM songs WHERE id = ?")
        .bind(song_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let state: String = row.get("workflow_state");
    assert_eq!(
        state, "discovered",
        "Default workflow_state should be 'discovered', got '{state}'"
    );

    // Transition to learning
    use music_browser::db::models::WorkflowState;
    use music_browser::db::queries;

    queries::update_workflow_state(&pool, song_id, &WorkflowState::Learning)
        .await
        .unwrap();
    let songs = queries::list_songs_by_workflow_state(&pool, &WorkflowState::Learning)
        .await
        .unwrap();
    assert_eq!(
        songs.len(),
        1,
        "Expected 1 song in Learning state, got {}",
        songs.len()
    );
    assert_eq!(songs[0].title, "WF Song");

    // Transition to producing
    queries::update_workflow_state(&pool, song_id, &WorkflowState::Producing)
        .await
        .unwrap();
    let empty = queries::list_songs_by_workflow_state(&pool, &WorkflowState::Learning)
        .await
        .unwrap();
    assert!(
        empty.is_empty(),
        "Expected 0 songs in Learning after transition, got {}",
        empty.len()
    );
    let producing = queries::list_songs_by_workflow_state(&pool, &WorkflowState::Producing)
        .await
        .unwrap();
    assert_eq!(
        producing.len(),
        1,
        "Expected 1 song in Producing state, got {}",
        producing.len()
    );
}

#[tokio::test]
async fn test_workflow_state_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let bad = sqlx::query("INSERT INTO songs (title, song_type, workflow_state) VALUES (?, ?, ?)")
        .bind("Bad")
        .bind("song")
        .bind("nonexistent_state")
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject invalid workflow_state"
    );
}

#[tokio::test]
async fn test_practice_exercise_crud() {
    let (pool, _tmp) = setup_pool().await;
    use music_browser::db::models::CreatePracticeExercise;
    use music_browser::db::queries;

    let inst_id = insert_instrument(&pool, "Bass", "bass").await;

    let ex_id = queries::create_exercise(
        &pool,
        &CreatePracticeExercise {
            instrument_id: Some(inst_id),
            name: "Chromatic Warmup".to_string(),
            category: "technique".to_string(),
            description: "4-fret chromatic pattern".to_string(),
            source: "Bass Method p.12".to_string(),
            sort_order: 1,
        },
    )
    .await
    .unwrap();
    assert!(ex_id > 0, "Expected positive exercise id, got {ex_id}");

    let exercises = queries::list_exercises(&pool).await.unwrap();
    assert_eq!(
        exercises.len(),
        1,
        "Expected 1 exercise, got {}",
        exercises.len()
    );
    assert_eq!(exercises[0].name, "Chromatic Warmup");
    assert_eq!(exercises[0].instrument_name, "Bass");
    assert_eq!(exercises[0].category, "technique");

    queries::delete_exercise(&pool, ex_id).await.unwrap();
    let after = queries::list_exercises(&pool).await.unwrap();
    assert!(
        after.is_empty(),
        "Expected 0 exercises after delete, got {}",
        after.len()
    );
}

#[tokio::test]
async fn test_exercise_category_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let bad = sqlx::query("INSERT INTO practice_exercises (name, category) VALUES (?, ?)")
        .bind("Bad Exercise")
        .bind("invalid_category")
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject invalid exercise category"
    );
}

#[tokio::test]
async fn test_song_exercise_link() {
    let (pool, _tmp) = setup_pool().await;
    use music_browser::db::models::{CreatePracticeExercise, CreateSongExercise};
    use music_browser::db::queries;

    let song_id = insert_song(&pool, "Link Song", "cover").await;
    let ex_id = queries::create_exercise(
        &pool,
        &CreatePracticeExercise {
            instrument_id: None,
            name: "Scale Run".to_string(),
            category: "scales".to_string(),
            description: String::new(),
            source: String::new(),
            sort_order: 0,
        },
    )
    .await
    .unwrap();

    let se_id = queries::create_song_exercise(
        &pool,
        &CreateSongExercise {
            song_id,
            exercise_id: ex_id,
            notes: "Focus on timing".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(se_id > 0, "Expected positive song_exercise id, got {se_id}");

    let linked = queries::list_song_exercises(&pool, song_id).await.unwrap();
    assert_eq!(
        linked.len(),
        1,
        "Expected 1 linked exercise, got {}",
        linked.len()
    );
    assert_eq!(linked[0].exercise_name, "Scale Run");
    assert_eq!(linked[0].notes, "Focus on timing");

    // Uniqueness: inserting same link again should be ignored (INSERT OR IGNORE)
    queries::create_song_exercise(
        &pool,
        &CreateSongExercise {
            song_id,
            exercise_id: ex_id,
            notes: "duplicate".to_string(),
        },
    )
    .await
    .unwrap();
    let still_one = queries::list_song_exercises(&pool, song_id).await.unwrap();
    assert_eq!(
        still_one.len(),
        1,
        "Expected still 1 linked exercise after duplicate, got {}",
        still_one.len()
    );

    queries::delete_song_exercise(&pool, se_id).await.unwrap();
    let after = queries::list_song_exercises(&pool, song_id).await.unwrap();
    assert!(
        after.is_empty(),
        "Expected 0 linked exercises after delete, got {}",
        after.len()
    );
}

#[tokio::test]
async fn test_user_profile_crud() {
    let (pool, _tmp) = setup_pool().await;
    use music_browser::db::models::UpdateUserProfile;
    use music_browser::db::queries;

    // Default profile should exist from migration
    let profile = queries::get_profile(&pool).await.unwrap();
    assert_eq!(profile.id, 1);
    assert_eq!(profile.display_name, "Musician");
    assert_eq!(
        profile.songs_capacity, 3,
        "Default capacity should be 3, got {}",
        profile.songs_capacity
    );

    // Update profile
    queries::update_profile(
        &pool,
        &UpdateUserProfile {
            display_name: "Nate".to_string(),
            songs_capacity: 5,
            warmup_minutes: 10,
            drill_minutes: 10,
            song_minutes: 40,
            review_minutes: 5,
            notes: "Updated".to_string(),
        },
    )
    .await
    .unwrap();

    let updated = queries::get_profile(&pool).await.unwrap();
    assert_eq!(updated.display_name, "Nate");
    assert_eq!(
        updated.songs_capacity, 5,
        "Updated capacity should be 5, got {}",
        updated.songs_capacity
    );
    assert_eq!(updated.notes, "Updated");
}

#[tokio::test]
async fn test_goals_crud() {
    let (pool, _tmp) = setup_pool().await;
    use music_browser::db::models::CreateGoal;
    use music_browser::db::queries;

    let id = queries::create_goal(
        &pool,
        &CreateGoal {
            horizon: "1_week".to_string(),
            category: "practice".to_string(),
            title: "Learn bass line".to_string(),
            description: "For the new cover".to_string(),
            target_date: "2026-04-05".to_string(),
            sort_order: 0,
        },
    )
    .await
    .unwrap();
    assert!(id > 0, "Expected positive goal id, got {id}");

    let goals = queries::list_goals(&pool).await.unwrap();
    assert_eq!(goals.len(), 1, "Expected 1 goal, got {}", goals.len());
    assert_eq!(goals[0].title, "Learn bass line");
    assert!(!goals[0].completed, "New goal should not be completed");

    // Toggle
    queries::toggle_goal(&pool, id).await.unwrap();
    let toggled = queries::list_goals(&pool).await.unwrap();
    assert!(
        toggled[0].completed,
        "Goal should be completed after toggle"
    );

    queries::toggle_goal(&pool, id).await.unwrap();
    let untoggled = queries::list_goals(&pool).await.unwrap();
    assert!(
        !untoggled[0].completed,
        "Goal should be uncompleted after second toggle"
    );

    queries::delete_goal(&pool, id).await.unwrap();
    let after = queries::list_goals(&pool).await.unwrap();
    assert!(
        after.is_empty(),
        "Expected 0 goals after delete, got {}",
        after.len()
    );
}

#[tokio::test]
async fn test_goal_horizon_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let bad = sqlx::query("INSERT INTO goals (horizon, category, title) VALUES (?, ?, ?)")
        .bind("10_year")
        .bind("general")
        .bind("Bad")
        .execute(&pool)
        .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject invalid goal horizon '10_year'"
    );
}

#[tokio::test]
async fn test_schedule_event_crud() {
    let (pool, _tmp) = setup_pool().await;
    use music_browser::db::models::{CreateScheduleEvent, CreateScheduleItem};
    use music_browser::db::queries;

    let eid = queries::create_schedule_event(
        &pool,
        &CreateScheduleEvent {
            event_date: "2026-04-01".to_string(),
            title: "Test Session".to_string(),
            event_type: "practice".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(eid > 0, "Expected positive event id, got {eid}");

    let iid = queries::create_schedule_item(
        &pool,
        &CreateScheduleItem {
            event_id: eid,
            item_type: "warmup".to_string(),
            song_id: None,
            exercise_id: None,
            stage_id: None,
            instrument_id: None,
            title: "General warmup".to_string(),
            duration_minutes: 10,
            sort_order: 0,
            notes: String::new(),
        },
    )
    .await
    .unwrap();
    assert!(iid > 0, "Expected positive item id, got {iid}");

    let events = queries::list_schedule_events(&pool).await.unwrap();
    assert_eq!(events.len(), 1, "Expected 1 event, got {}", events.len());
    assert_eq!(
        events[0].items.len(),
        1,
        "Expected 1 item in event, got {}",
        events[0].items.len()
    );
    assert_eq!(events[0].items[0].title, "General warmup");
    assert!(
        !events[0].items[0].completed,
        "New item should not be completed"
    );

    // Toggle item
    queries::toggle_schedule_item(&pool, iid).await.unwrap();
    let after_toggle = queries::list_schedule_events(&pool).await.unwrap();
    assert!(
        after_toggle[0].items[0].completed,
        "Item should be completed after toggle"
    );

    // Delete event cascades items
    queries::delete_schedule_event(&pool, eid).await.unwrap();
    let after_delete = queries::list_schedule_events(&pool).await.unwrap();
    assert!(
        after_delete.is_empty(),
        "Expected 0 events after delete, got {}",
        after_delete.len()
    );
}

#[tokio::test]
async fn test_schedule_generation() {
    let (pool, _tmp) = setup_pool().await;
    use music_browser::db::models::{CreatePracticeExercise, WorkflowState};
    use music_browser::db::queries;

    // Set up: create active songs and exercises
    let s1 = insert_song(&pool, "Song A", "cover").await;
    let s2 = insert_song(&pool, "Song B", "original").await;
    queries::update_workflow_state(&pool, s1, &WorkflowState::Learning)
        .await
        .unwrap();
    queries::update_workflow_state(&pool, s2, &WorkflowState::Producing)
        .await
        .unwrap();

    queries::create_exercise(
        &pool,
        &CreatePracticeExercise {
            instrument_id: None,
            name: "Warmup A".to_string(),
            category: "technique".to_string(),
            description: String::new(),
            source: String::new(),
            sort_order: 0,
        },
    )
    .await
    .unwrap();

    // Generate schedule for 3 days
    let event_ids = queries::generate_schedule(&pool, "2026-04-01", 3)
        .await
        .unwrap();
    assert_eq!(
        event_ids.len(),
        3,
        "Expected 3 generated events, got {}",
        event_ids.len()
    );

    let events = queries::list_schedule_events(&pool).await.unwrap();
    assert_eq!(
        events.len(),
        3,
        "Expected 3 events in list, got {}",
        events.len()
    );

    // Each event should have at least warmup + review items
    for event in &events {
        assert!(
            event.items.len() >= 2,
            "Event '{}' should have >= 2 items (warmup + review), got {}",
            event.title,
            event.items.len()
        );
        // Check that there's at least one warmup and one review
        let has_warmup = event
            .items
            .iter()
            .any(|i| i.item_type == "warmup" || i.item_type == "drill");
        let has_review = event.items.iter().any(|i| i.item_type == "review");
        assert!(
            has_warmup,
            "Event '{}' should have a warmup/drill item",
            event.title
        );
        assert!(
            has_review,
            "Event '{}' should have a review item",
            event.title
        );
    }

    // Verify date sequencing
    let dates: Vec<&str> = events.iter().map(|e| e.event_date.as_str()).collect();
    // Events are sorted DESC, so reverse for checking order
    let mut sorted_dates = dates.clone();
    sorted_dates.sort();
    assert_eq!(
        sorted_dates,
        vec!["2026-04-01", "2026-04-02", "2026-04-03"],
        "Expected dates 04-01..03, got {sorted_dates:?}"
    );
}

#[tokio::test]
async fn test_song_new_fields_persist() {
    let (pool, _tmp) = setup_pool().await;
    use music_browser::db::models::{CreateSong, SongType, WorkflowState};
    use music_browser::db::queries;

    let id = queries::create_song(
        &pool,
        &CreateSong {
            title: "Full Song".to_string(),
            album_id: None,
            sheet_music: String::new(),
            lyrics: String::new(),
            song_type: SongType::Original,
            key: "Am".to_string(),
            bpm_lower: Some(120),
            bpm_upper: Some(130),
            original_artist: "Me".to_string(),
            score_url: String::new(),
            description: "Test".to_string(),
            workflow_state: WorkflowState::Performing,
            scores_folder: "/scores/full".to_string(),
            export_folder: "/export/full".to_string(),
            musicxml_path: "/scores/full/sheet.xml".to_string(),
            artist_ids: vec![],
        },
    )
    .await
    .unwrap();

    let song = queries::get_song(&pool, id).await.unwrap().unwrap();
    assert_eq!(
        song.workflow_state,
        WorkflowState::Performing,
        "workflow_state mismatch: {:?}",
        song.workflow_state
    );
    assert_eq!(
        song.scores_folder, "/scores/full",
        "scores_folder mismatch: {}",
        song.scores_folder
    );
    assert_eq!(
        song.export_folder, "/export/full",
        "export_folder mismatch: {}",
        song.export_folder
    );
    assert_eq!(
        song.musicxml_path, "/scores/full/sheet.xml",
        "musicxml_path mismatch: {}",
        song.musicxml_path
    );
}

#[tokio::test]
async fn test_add_days_to_date_basic() {
    // Test the date arithmetic indirectly via generate_schedule
    let (pool, _tmp) = setup_pool().await;
    use music_browser::db::queries;

    // Generate 1 day starting at month boundary
    let events = queries::generate_schedule(&pool, "2026-01-31", 2)
        .await
        .unwrap();
    assert_eq!(events.len(), 2, "Expected 2 events, got {}", events.len());

    let all_events = queries::list_schedule_events(&pool).await.unwrap();
    let dates: Vec<String> = all_events.iter().map(|e| e.event_date.clone()).collect();
    assert!(
        dates.contains(&"2026-01-31".to_string()),
        "Expected 2026-01-31 in dates, got {dates:?}"
    );
    assert!(
        dates.contains(&"2026-02-01".to_string()),
        "Expected 2026-02-01 in dates, got {dates:?}"
    );
}

#[tokio::test]
async fn test_musicxml_file_type_allowed() {
    let (pool, _tmp) = setup_pool().await;
    let song_id = insert_song(&pool, "MXL Song", "song").await;

    let result = sqlx::query("INSERT INTO song_files (song_id, file_type, path) VALUES (?, ?, ?)")
        .bind(song_id)
        .bind("musicxml")
        .bind("/scores/test.musicxml")
        .execute(&pool)
        .await;
    assert!(
        result.is_ok(),
        "Expected musicxml file_type to be accepted, got error: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_schedule_event_status_constraint() {
    let (pool, _tmp) = setup_pool().await;

    let bad = sqlx::query(
        "INSERT INTO schedule_events (event_date, title, event_type, status) VALUES (?, ?, ?, ?)",
    )
    .bind("2026-04-01")
    .bind("Test")
    .bind("practice")
    .bind("invalid_status")
    .execute(&pool)
    .await;
    assert!(
        bad.is_err(),
        "Expected CHECK constraint to reject invalid schedule event status"
    );
}
