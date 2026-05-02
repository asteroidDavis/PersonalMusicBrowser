use sqlx::{Row, SqlitePool};

use super::models::*;

// ============================================================================
// Instruments
// ============================================================================

pub async fn list_instruments(pool: &SqlitePool) -> Result<Vec<Instrument>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, name, instrument_type FROM instruments ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|r| Instrument {
            id: r.get("id"),
            name: r.get("name"),
            instrument_type: r.get("instrument_type"),
        })
        .collect())
}

pub async fn create_instrument(
    pool: &SqlitePool,
    input: &CreateInstrument,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query("INSERT INTO instruments (name, instrument_type) VALUES (?, ?)")
        .bind(&input.name)
        .bind(&input.instrument_type)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_instrument(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM instruments WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Bands
// ============================================================================

pub async fn list_bands(pool: &SqlitePool) -> Result<Vec<Band>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, name FROM bands ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|r| Band {
            id: r.get("id"),
            name: r.get("name"),
        })
        .collect())
}

pub async fn create_band(pool: &SqlitePool, input: &CreateBand) -> Result<i64, sqlx::Error> {
    let result = sqlx::query("INSERT INTO bands (name) VALUES (?)")
        .bind(&input.name)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_band(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM bands WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Artists
// ============================================================================

async fn fetch_bands_for(pool: &SqlitePool, artist_id: i64) -> Result<Vec<Band>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT b.id, b.name FROM bands b \
         INNER JOIN artist_bands ab ON ab.band_id = b.id \
         WHERE ab.artist_id = ?",
    )
    .bind(artist_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| Band {
            id: r.get("id"),
            name: r.get("name"),
        })
        .collect())
}

pub async fn list_artists(pool: &SqlitePool) -> Result<Vec<Artist>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, name FROM artists ORDER BY name")
        .fetch_all(pool)
        .await?;

    let mut artists = Vec::new();
    for row in &rows {
        let id: i64 = row.get("id");
        let bands = fetch_bands_for(pool, id).await?;
        artists.push(Artist {
            id,
            name: row.get("name"),
            bands,
        });
    }
    Ok(artists)
}

pub async fn get_artist(pool: &SqlitePool, id: i64) -> Result<Option<Artist>, sqlx::Error> {
    let row = sqlx::query("SELECT id, name FROM artists WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(row) => {
            let aid: i64 = row.get("id");
            let bands = fetch_bands_for(pool, aid).await?;
            Ok(Some(Artist {
                id: aid,
                name: row.get("name"),
                bands,
            }))
        }
        None => Ok(None),
    }
}

pub async fn create_artist(pool: &SqlitePool, input: &CreateArtist) -> Result<i64, sqlx::Error> {
    let result = sqlx::query("INSERT INTO artists (name) VALUES (?)")
        .bind(&input.name)
        .execute(pool)
        .await?;
    let artist_id = result.last_insert_rowid();

    for band_id in &input.band_ids {
        sqlx::query("INSERT INTO artist_bands (artist_id, band_id) VALUES (?, ?)")
            .bind(artist_id)
            .bind(band_id)
            .execute(pool)
            .await?;
    }

    Ok(artist_id)
}

pub async fn delete_artist(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM artist_bands WHERE artist_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM artists WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Albums
// ============================================================================

pub async fn list_albums(pool: &SqlitePool) -> Result<Vec<Album>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, title, released, url FROM albums ORDER BY title")
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|r| Album {
            id: r.get("id"),
            title: r.get("title"),
            released: r.get("released"),
            url: r.get::<Option<String>, _>("url").unwrap_or_default(),
        })
        .collect())
}

pub async fn get_album(pool: &SqlitePool, id: i64) -> Result<Option<Album>, sqlx::Error> {
    let row = sqlx::query("SELECT id, title, released, url FROM albums WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| Album {
        id: r.get("id"),
        title: r.get("title"),
        released: r.get("released"),
        url: r.get::<Option<String>, _>("url").unwrap_or_default(),
    }))
}

pub async fn create_album(pool: &SqlitePool, input: &CreateAlbum) -> Result<i64, sqlx::Error> {
    let result = sqlx::query("INSERT INTO albums (title, released, url) VALUES (?, ?, ?)")
        .bind(&input.title)
        .bind(input.released)
        .bind(&input.url)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_album(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM albums WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Songs
// ============================================================================

async fn fetch_song_artists(pool: &SqlitePool, song_id: i64) -> Result<Vec<Artist>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT ar.id, ar.name FROM artists ar \
         INNER JOIN song_artists sa ON sa.artist_id = ar.id \
         WHERE sa.song_id = ?",
    )
    .bind(song_id)
    .fetch_all(pool)
    .await?;

    let mut artists = Vec::new();
    for row in &rows {
        let aid: i64 = row.get("id");
        let bands = fetch_bands_for(pool, aid).await?;
        artists.push(Artist {
            id: aid,
            name: row.get("name"),
            bands,
        });
    }
    Ok(artists)
}

struct SongFields {
    sheet_music: String,
    lyrics: String,
    key: String,
    album_title: String,
    bpm_lower: Option<i32>,
    bpm_upper: Option<i32>,
    original_artist: String,
    score_url: String,
    description: String,
    workflow_state: WorkflowState,
    scores_folder: String,
    export_folder: String,
    musicxml_path: String,
    practice_project_path: String,
    time_signature: String,
    practice_priority: i32,
}

fn row_to_song_fields(row: &sqlx::sqlite::SqliteRow) -> SongFields {
    let wf_str: String = row
        .get::<Option<String>, _>("workflow_state")
        .unwrap_or_else(|| "discovered".to_string());
    SongFields {
        sheet_music: row
            .get::<Option<String>, _>("sheet_music")
            .unwrap_or_default(),
        lyrics: row.get::<Option<String>, _>("lyrics").unwrap_or_default(),
        key: row.get::<Option<String>, _>("key").unwrap_or_default(),
        album_title: row
            .get::<Option<String>, _>("album_title")
            .unwrap_or_default(),
        bpm_lower: row.get("bpm_lower"),
        bpm_upper: row.get("bpm_upper"),
        original_artist: row
            .get::<Option<String>, _>("original_artist")
            .unwrap_or_default(),
        score_url: row
            .get::<Option<String>, _>("score_url")
            .unwrap_or_default(),
        description: row
            .get::<Option<String>, _>("description")
            .unwrap_or_default(),
        workflow_state: WorkflowState::parse(&wf_str).unwrap_or(WorkflowState::Discovered),
        scores_folder: row
            .get::<Option<String>, _>("scores_folder")
            .unwrap_or_default(),
        export_folder: row
            .get::<Option<String>, _>("export_folder")
            .unwrap_or_default(),
        musicxml_path: row
            .get::<Option<String>, _>("musicxml_path")
            .unwrap_or_default(),
        practice_project_path: row
            .get::<Option<String>, _>("practice_project_path")
            .unwrap_or_default(),
        time_signature: row
            .get::<Option<String>, _>("time_signature")
            .unwrap_or_else(|| "4/4".to_string()),
        practice_priority: row.get::<Option<i32>, _>("practice_priority").unwrap_or(0),
    }
}

fn song_from_row(row: &sqlx::sqlite::SqliteRow, f: SongFields, artists: Vec<Artist>) -> Song {
    let song_type_str: String = row.get("song_type");
    Song {
        id: row.get("id"),
        title: row.get("title"),
        album_id: row.get("album_id"),
        album_title: f.album_title,
        sheet_music: f.sheet_music,
        lyrics: f.lyrics,
        song_type: SongType::parse(&song_type_str).unwrap_or(SongType::Song),
        key: f.key,
        bpm_lower: f.bpm_lower,
        bpm_upper: f.bpm_upper,
        original_artist: f.original_artist,
        score_url: f.score_url,
        description: f.description,
        workflow_state: f.workflow_state,
        scores_folder: f.scores_folder,
        export_folder: f.export_folder,
        musicxml_path: f.musicxml_path,
        practice_project_path: f.practice_project_path,
        time_signature: f.time_signature,
        practice_priority: f.practice_priority,
        artists,
    }
}

const SONG_SELECT_COLS: &str = "s.id, s.title, s.album_id, COALESCE(a.title, '') as album_title, \
     s.sheet_music, s.lyrics, s.song_type, s.key, s.bpm_lower, s.bpm_upper, \
     s.original_artist, s.score_url, s.description, \
     s.workflow_state, s.scores_folder, s.export_folder, s.musicxml_path, \
     s.practice_project_path, s.time_signature, s.practice_priority";

pub async fn list_songs(pool: &SqlitePool) -> Result<Vec<Song>, sqlx::Error> {
    let sql = format!(
        "SELECT {SONG_SELECT_COLS} FROM songs s \
         LEFT JOIN albums a ON a.id = s.album_id \
         ORDER BY s.title"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await?;

    let mut songs = Vec::new();
    for row in &rows {
        let sid: i64 = row.get("id");
        let artists = fetch_song_artists(pool, sid).await?;
        let f = row_to_song_fields(row);
        songs.push(song_from_row(row, f, artists));
    }
    Ok(songs)
}

pub async fn get_song(pool: &SqlitePool, id: i64) -> Result<Option<Song>, sqlx::Error> {
    let sql = format!(
        "SELECT {SONG_SELECT_COLS} FROM songs s \
         LEFT JOIN albums a ON a.id = s.album_id \
         WHERE s.id = ?"
    );
    let row = sqlx::query(&sql).bind(id).fetch_optional(pool).await?;

    match row {
        Some(row) => {
            let sid: i64 = row.get("id");
            let artists = fetch_song_artists(pool, sid).await?;
            let f = row_to_song_fields(&row);
            Ok(Some(song_from_row(&row, f, artists)))
        }
        None => Ok(None),
    }
}

pub async fn create_song(pool: &SqlitePool, input: &CreateSong) -> Result<i64, sqlx::Error> {
    let song_type_str = input.song_type.as_str();
    let result = sqlx::query(
        "INSERT INTO songs (title, album_id, sheet_music, lyrics, song_type, \
         key, bpm_lower, bpm_upper, original_artist, score_url, description, \
         workflow_state, scores_folder, export_folder, musicxml_path, \
         practice_project_path, time_signature, practice_priority) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&input.title)
    .bind(input.album_id)
    .bind(&input.sheet_music)
    .bind(&input.lyrics)
    .bind(song_type_str)
    .bind(&input.key)
    .bind(input.bpm_lower)
    .bind(input.bpm_upper)
    .bind(&input.original_artist)
    .bind(&input.score_url)
    .bind(&input.description)
    .bind(input.workflow_state.as_str())
    .bind(&input.scores_folder)
    .bind(&input.export_folder)
    .bind(&input.musicxml_path)
    .bind(&input.practice_project_path)
    .bind(&input.time_signature)
    .bind(input.practice_priority)
    .execute(pool)
    .await?;
    let song_id = result.last_insert_rowid();

    for artist_id in &input.artist_ids {
        sqlx::query("INSERT INTO song_artists (song_id, artist_id) VALUES (?, ?)")
            .bind(song_id)
            .bind(artist_id)
            .execute(pool)
            .await?;
    }

    Ok(song_id)
}

pub async fn update_song(pool: &SqlitePool, input: &UpdateSong) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE songs SET title = ?, album_id = ?, song_type = ?, sheet_music = ?, lyrics = ?, \
         key = ?, bpm_lower = ?, bpm_upper = ?, original_artist = ?, \
         score_url = ?, description = ?, \
         scores_folder = ?, export_folder = ?, musicxml_path = ?, \
         practice_project_path = ?, time_signature = ?, practice_priority = ? \
         WHERE id = ?",
    )
    .bind(&input.title)
    .bind(input.album_id)
    .bind(input.song_type.as_str())
    .bind(&input.sheet_music)
    .bind(&input.lyrics)
    .bind(&input.key)
    .bind(input.bpm_lower)
    .bind(input.bpm_upper)
    .bind(&input.original_artist)
    .bind(&input.score_url)
    .bind(&input.description)
    .bind(&input.scores_folder)
    .bind(&input.export_folder)
    .bind(&input.musicxml_path)
    .bind(&input.practice_project_path)
    .bind(&input.time_signature)
    .bind(input.practice_priority)
    .bind(input.id)
    .execute(pool)
    .await?;

    sqlx::query("DELETE FROM song_artists WHERE song_id = ?")
        .bind(input.id)
        .execute(pool)
        .await?;
    for artist_id in &input.artist_ids {
        sqlx::query("INSERT INTO song_artists (song_id, artist_id) VALUES (?, ?)")
            .bind(input.id)
            .bind(artist_id)
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn delete_song(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM song_artists WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    let _ = sqlx::query("DELETE FROM cover_instruments WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM cover_details WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM composition_instruments WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM composition_details WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await;
    let _ = sqlx::query(
        "DELETE FROM recording_instruments WHERE recording_id IN \
         (SELECT id FROM recordings WHERE song_id = ?)",
    )
    .bind(id)
    .execute(pool)
    .await;
    sqlx::query("DELETE FROM recordings WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    let _ = sqlx::query(
        "DELETE FROM song_instrument_presets WHERE song_instrument_id IN \
         (SELECT id FROM song_instruments WHERE song_id = ?)",
    )
    .bind(id)
    .execute(pool)
    .await;
    let _ = sqlx::query("DELETE FROM song_instruments WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await;
    let _ = sqlx::query(
        "DELETE FROM production_steps WHERE stage_id IN \
         (SELECT id FROM production_stages WHERE song_id = ?)",
    )
    .bind(id)
    .execute(pool)
    .await;
    let _ = sqlx::query("DELETE FROM production_stages WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM song_files WHERE song_id = ?")
        .bind(id)
        .execute(pool)
        .await;
    sqlx::query("DELETE FROM songs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Recordings
// ============================================================================

pub async fn list_recordings(pool: &SqlitePool) -> Result<Vec<Recording>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, recording_type, path, song_id, notes_image FROM recordings ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    let mut recordings = Vec::new();
    for row in &rows {
        let rid: i64 = row.get("id");
        let instruments = sqlx::query(
            "SELECT i.id, i.name, i.instrument_type FROM instruments i \
             INNER JOIN recording_instruments ri ON ri.instrument_id = i.id \
             WHERE ri.recording_id = ?",
        )
        .bind(rid)
        .fetch_all(pool)
        .await?;

        let rec_type_str: String = row.get("recording_type");
        let recording_type = RecordingType::parse(&rec_type_str).unwrap_or(RecordingType::Wav);

        recordings.push(Recording {
            id: rid,
            recording_type,
            path: row.get::<Option<String>, _>("path").unwrap_or_default(),
            song_id: row.get("song_id"),
            notes_image: row
                .get::<Option<String>, _>("notes_image")
                .unwrap_or_default(),
            instruments: instruments
                .iter()
                .map(|r| Instrument {
                    id: r.get("id"),
                    name: r.get("name"),
                    instrument_type: r.get("instrument_type"),
                })
                .collect(),
        });
    }
    Ok(recordings)
}

pub async fn create_recording(
    pool: &SqlitePool,
    input: &CreateRecording,
) -> Result<i64, sqlx::Error> {
    let rec_type = input.recording_type.as_str();
    let result = sqlx::query(
        "INSERT INTO recordings (recording_type, path, song_id, notes_image) VALUES (?, ?, ?, ?)",
    )
    .bind(rec_type)
    .bind(&input.path)
    .bind(input.song_id)
    .bind(&input.notes_image)
    .execute(pool)
    .await?;
    let rec_id = result.last_insert_rowid();

    for instrument_id in &input.instrument_ids {
        sqlx::query(
            "INSERT INTO recording_instruments (recording_id, instrument_id) VALUES (?, ?)",
        )
        .bind(rec_id)
        .bind(instrument_id)
        .execute(pool)
        .await?;
    }

    Ok(rec_id)
}

pub async fn delete_recording(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM recording_instruments WHERE recording_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM recordings WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Devices
// ============================================================================

pub async fn list_devices(pool: &SqlitePool) -> Result<Vec<Device>, sqlx::Error> {
    let rows =
        sqlx::query("SELECT id, name, device_type, manual_path, notes FROM devices ORDER BY name")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .iter()
        .map(|r| Device {
            id: r.get("id"),
            name: r.get("name"),
            device_type: r.get("device_type"),
            manual_path: r
                .get::<Option<String>, _>("manual_path")
                .unwrap_or_default(),
            notes: r.get::<Option<String>, _>("notes").unwrap_or_default(),
        })
        .collect())
}

pub async fn create_device(pool: &SqlitePool, input: &CreateDevice) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO devices (name, device_type, manual_path, notes) VALUES (?, ?, ?, ?)",
    )
    .bind(&input.name)
    .bind(&input.device_type)
    .bind(&input.manual_path)
    .bind(&input.notes)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_device(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM device_presets WHERE device_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Device presets
// ============================================================================

pub async fn list_device_presets(pool: &SqlitePool) -> Result<Vec<DevicePreset>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, device_id, name, preset_code, description FROM device_presets ORDER BY name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| DevicePreset {
            id: r.get("id"),
            device_id: r.get("device_id"),
            name: r.get("name"),
            preset_code: r
                .get::<Option<String>, _>("preset_code")
                .unwrap_or_default(),
            description: r
                .get::<Option<String>, _>("description")
                .unwrap_or_default(),
        })
        .collect())
}

pub async fn list_presets_for_device(
    pool: &SqlitePool,
    device_id: i64,
) -> Result<Vec<DevicePreset>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, device_id, name, preset_code, description \
         FROM device_presets WHERE device_id = ? ORDER BY name",
    )
    .bind(device_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| DevicePreset {
            id: r.get("id"),
            device_id: r.get("device_id"),
            name: r.get("name"),
            preset_code: r
                .get::<Option<String>, _>("preset_code")
                .unwrap_or_default(),
            description: r
                .get::<Option<String>, _>("description")
                .unwrap_or_default(),
        })
        .collect())
}

pub async fn create_device_preset(
    pool: &SqlitePool,
    input: &CreateDevicePreset,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO device_presets (device_id, name, preset_code, description) VALUES (?, ?, ?, ?)",
    )
    .bind(input.device_id)
    .bind(&input.name)
    .bind(&input.preset_code)
    .bind(&input.description)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_device_preset(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM song_instrument_presets WHERE device_preset_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM device_presets WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Song instruments (live config)
// ============================================================================

pub async fn list_song_instruments(
    pool: &SqlitePool,
    song_id: i64,
) -> Result<Vec<SongInstrument>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT si.id, si.song_id, si.instrument_id, i.name as instrument_name, \
         si.description, si.score_url, si.production_path, si.mastering_path \
         FROM song_instruments si \
         INNER JOIN instruments i ON i.id = si.instrument_id \
         WHERE si.song_id = ? ORDER BY i.name",
    )
    .bind(song_id)
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for row in &rows {
        let si_id: i64 = row.get("id");
        let preset_rows = sqlx::query(
            "SELECT dp.id, dp.device_id, dp.name, dp.preset_code, dp.description \
             FROM device_presets dp \
             INNER JOIN song_instrument_presets sip ON sip.device_preset_id = dp.id \
             WHERE sip.song_instrument_id = ?",
        )
        .bind(si_id)
        .fetch_all(pool)
        .await?;

        let presets: Vec<DevicePreset> = preset_rows
            .iter()
            .map(|r| DevicePreset {
                id: r.get("id"),
                device_id: r.get("device_id"),
                name: r.get("name"),
                preset_code: r
                    .get::<Option<String>, _>("preset_code")
                    .unwrap_or_default(),
                description: r
                    .get::<Option<String>, _>("description")
                    .unwrap_or_default(),
            })
            .collect();

        result.push(SongInstrument {
            id: si_id,
            song_id: row.get("song_id"),
            instrument_id: row.get("instrument_id"),
            instrument_name: row.get("instrument_name"),
            description: row
                .get::<Option<String>, _>("description")
                .unwrap_or_default(),
            score_url: row
                .get::<Option<String>, _>("score_url")
                .unwrap_or_default(),
            production_path: row
                .get::<Option<String>, _>("production_path")
                .unwrap_or_default(),
            mastering_path: row
                .get::<Option<String>, _>("mastering_path")
                .unwrap_or_default(),
            presets,
        });
    }
    Ok(result)
}

pub async fn create_song_instrument(
    pool: &SqlitePool,
    input: &CreateSongInstrument,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO song_instruments \
         (song_id, instrument_id, description, score_url, production_path, mastering_path) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(input.song_id)
    .bind(input.instrument_id)
    .bind(&input.description)
    .bind(&input.score_url)
    .bind(&input.production_path)
    .bind(&input.mastering_path)
    .execute(pool)
    .await?;
    let si_id = result.last_insert_rowid();

    for preset_id in &input.preset_ids {
        sqlx::query(
            "INSERT INTO song_instrument_presets (song_instrument_id, device_preset_id) VALUES (?, ?)",
        )
        .bind(si_id)
        .bind(preset_id)
        .execute(pool)
        .await?;
    }

    Ok(si_id)
}

pub async fn delete_song_instrument(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM song_instrument_presets WHERE song_instrument_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM song_instruments WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Production stages & steps
// ============================================================================

pub async fn list_production_stages(
    pool: &SqlitePool,
    song_id: i64,
) -> Result<Vec<ProductionStage>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, song_id, stage, status FROM production_stages \
         WHERE song_id = ? ORDER BY id",
    )
    .bind(song_id)
    .fetch_all(pool)
    .await?;

    let mut stages = Vec::new();
    for row in &rows {
        let stage_id: i64 = row.get("id");
        let status_str: String = row.get("status");
        let step_rows = sqlx::query(
            "SELECT ps.id, ps.stage_id, ps.instrument_id, \
             COALESCE(i.name, '') as instrument_name, \
             ps.name, ps.status, ps.sort_order, ps.notes \
             FROM production_steps ps \
             LEFT JOIN instruments i ON i.id = ps.instrument_id \
             WHERE ps.stage_id = ? ORDER BY ps.sort_order, ps.id",
        )
        .bind(stage_id)
        .fetch_all(pool)
        .await?;

        let steps: Vec<ProductionStep> = step_rows
            .iter()
            .map(|r| {
                let s_str: String = r.get("status");
                ProductionStep {
                    id: r.get("id"),
                    stage_id: r.get("stage_id"),
                    instrument_id: r.get("instrument_id"),
                    instrument_name: r.get("instrument_name"),
                    name: r.get("name"),
                    status: ProductionStatus::parse(&s_str).unwrap_or(ProductionStatus::NotStarted),
                    sort_order: r.get("sort_order"),
                    notes: r.get::<Option<String>, _>("notes").unwrap_or_default(),
                }
            })
            .collect();

        stages.push(ProductionStage {
            id: stage_id,
            song_id: row.get("song_id"),
            stage: row.get("stage"),
            status: ProductionStatus::parse(&status_str).unwrap_or(ProductionStatus::NotStarted),
            steps,
        });
    }
    Ok(stages)
}

pub async fn create_production_stage(
    pool: &SqlitePool,
    input: &CreateProductionStage,
) -> Result<i64, sqlx::Error> {
    let result =
        sqlx::query("INSERT INTO production_stages (song_id, stage, status) VALUES (?, ?, ?)")
            .bind(input.song_id)
            .bind(&input.stage)
            .bind(input.status.as_str())
            .execute(pool)
            .await?;
    Ok(result.last_insert_rowid())
}

pub async fn update_production_stage_status(
    pool: &SqlitePool,
    id: i64,
    status: &ProductionStatus,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE production_stages SET status = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create_production_step(
    pool: &SqlitePool,
    input: &CreateProductionStep,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO production_steps \
         (stage_id, instrument_id, name, status, sort_order, notes) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(input.stage_id)
    .bind(input.instrument_id)
    .bind(&input.name)
    .bind(input.status.as_str())
    .bind(input.sort_order)
    .bind(&input.notes)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn update_production_step_status(
    pool: &SqlitePool,
    id: i64,
    status: &ProductionStatus,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE production_steps SET status = ? WHERE id = ?")
        .bind(status.as_str())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_production_stage(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM production_steps WHERE stage_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM production_stages WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Production overview — all songs with their stages (for dashboard)
// ============================================================================

pub async fn list_all_production_stages(
    pool: &SqlitePool,
) -> Result<Vec<(Song, Vec<ProductionStage>)>, sqlx::Error> {
    let songs = list_songs(pool).await?;
    let mut result = Vec::new();
    for song in songs {
        let stages = list_production_stages(pool, song.id).await?;
        result.push((song, stages));
    }
    Ok(result)
}

// ============================================================================
// Song files
// ============================================================================

pub async fn list_song_files(
    pool: &SqlitePool,
    song_id: i64,
) -> Result<Vec<SongFile>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT sf.id, sf.song_id, sf.file_type, sf.path, sf.instrument_id, \
         COALESCE(i.name, '') as instrument_name, sf.description \
         FROM song_files sf \
         LEFT JOIN instruments i ON i.id = sf.instrument_id \
         WHERE sf.song_id = ? ORDER BY sf.file_type, sf.id",
    )
    .bind(song_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SongFile {
            id: r.get("id"),
            song_id: r.get("song_id"),
            file_type: r.get("file_type"),
            path: r.get("path"),
            instrument_id: r.get("instrument_id"),
            instrument_name: r.get("instrument_name"),
            description: r
                .get::<Option<String>, _>("description")
                .unwrap_or_default(),
        })
        .collect())
}

pub async fn create_song_file(
    pool: &SqlitePool,
    input: &CreateSongFile,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO song_files (song_id, file_type, path, instrument_id, description) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(input.song_id)
    .bind(&input.file_type)
    .bind(&input.path)
    .bind(input.instrument_id)
    .bind(&input.description)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_song_file(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM song_files WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Samples
// ============================================================================

pub async fn list_samples(pool: &SqlitePool) -> Result<Vec<Sample>, sqlx::Error> {
    let rows =
        sqlx::query("SELECT id, name, path, bpm, key, description FROM samples ORDER BY name")
            .fetch_all(pool)
            .await?;

    let mut samples = Vec::new();
    for row in &rows {
        let sid: i64 = row.get("id");
        let inst_rows = sqlx::query(
            "SELECT i.id, i.name, i.instrument_type FROM instruments i \
             INNER JOIN sample_instruments si ON si.instrument_id = i.id \
             WHERE si.sample_id = ?",
        )
        .bind(sid)
        .fetch_all(pool)
        .await?;

        samples.push(Sample {
            id: sid,
            name: row.get("name"),
            path: row.get::<Option<String>, _>("path").unwrap_or_default(),
            bpm: row.get("bpm"),
            key: row.get::<Option<String>, _>("key").unwrap_or_default(),
            description: row
                .get::<Option<String>, _>("description")
                .unwrap_or_default(),
            instruments: inst_rows
                .iter()
                .map(|r| Instrument {
                    id: r.get("id"),
                    name: r.get("name"),
                    instrument_type: r.get("instrument_type"),
                })
                .collect(),
        });
    }
    Ok(samples)
}

pub async fn create_sample(pool: &SqlitePool, input: &CreateSample) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO samples (name, path, bpm, key, description) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&input.name)
    .bind(&input.path)
    .bind(input.bpm)
    .bind(&input.key)
    .bind(&input.description)
    .execute(pool)
    .await?;
    let sample_id = result.last_insert_rowid();

    for instrument_id in &input.instrument_ids {
        sqlx::query("INSERT INTO sample_instruments (sample_id, instrument_id) VALUES (?, ?)")
            .bind(sample_id)
            .bind(instrument_id)
            .execute(pool)
            .await?;
    }

    Ok(sample_id)
}

// ============================================================================
// Auto-populate standard stages and steps
// ============================================================================

const STANDARD_STAGES: &[&str] = &[
    "writing",
    "composition",
    "tracking",
    "mixing",
    "mastering",
    "publishing",
    "performing",
];

/// Create all 7 standard production stages for a song (skips duplicates).
pub async fn auto_add_stages(pool: &SqlitePool, song_id: i64) -> Result<Vec<i64>, sqlx::Error> {
    let mut ids = Vec::new();
    for stage in STANDARD_STAGES {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO production_stages (song_id, stage, status) VALUES (?, ?, 'not_started')",
        )
        .bind(song_id)
        .bind(stage)
        .execute(pool)
        .await?;
        let id = result.last_insert_rowid();
        if id > 0 {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// Return the default step names for a given stage, accounting for song type.
/// `is_cover` should be true for covers, false for originals.
pub fn default_steps_for_stage(
    stage: &str,
    is_cover: bool,
) -> Vec<(&'static str, Option<&'static str>)> {
    // Returns (step_name, optional_instrument_type) pairs
    match stage {
        "writing" => vec![
            ("Track demo / steel thread", None),
            ("Brainstormed words", None),
            ("Cringe tested", None),
        ],
        "composition" if !is_cover => vec![
            ("Track demo / steel thread", None),
            ("Automated note detection", None),
            ("Fix note detection errors", None),
            ("Sight read", None),
            ("Learn part by ear", None),
            ("Guitar composed", Some("guitar")),
            ("Bass composed", Some("bass")),
            ("Vocals composed", Some("vocals")),
            ("Vocal harmony composed", Some("vocals")),
            ("Drums composed", Some("drums")),
            ("Piano composed", Some("piano")),
        ],
        "composition" => vec![
            // Cover composition
            ("Track demo / steel thread", None),
            ("Automated note detection", None),
            ("Fix note detection errors", None),
            ("Sight read", None),
            ("Learn vocals", Some("vocals")),
        ],
        "tracking" => vec![
            ("Tempo tracked", None),
            ("Steel thread tracked", None),
            ("Guitar tracked", Some("guitar")),
            ("Guitar pedal automation tracked", Some("guitar")),
            ("Bass tracked", Some("bass")),
            ("Bass pedal automation tracked", Some("bass")),
            ("Vocals tracked", Some("vocals")),
            ("Vocal FX automation tracked", Some("vocals")),
            ("Drums tracked", Some("drums")),
            ("Drums reviewed", Some("drums")),
            ("Drum pedal automation tracked", Some("drums")),
            ("Piano tracked", Some("piano")),
            ("Piano pedal/FX automation tracked", Some("piano")),
        ],
        "mixing" => vec![
            ("Rough mix balance", None),
            ("EQ pass", None),
            ("Compression pass", None),
            ("Effects / sends", None),
            ("Automation pass", None),
            ("Reference check", None),
            ("Mix bounce", None),
        ],
        "mastering" => vec![
            ("Import final mix", None),
            ("Loudness / LUFS target", None),
            ("EQ / tonal balance", None),
            ("Stereo imaging", None),
            ("Limiting / final ceiling", None),
            ("A/B reference comparison", None),
            ("Format exports (WAV, MP3, FLAC)", None),
        ],
        "publishing" => vec![
            ("Metadata (title, artist, album, ISRC)", None),
            ("Cover art finalized", None),
            ("Distribution upload (DistroKid / CDBaby / etc.)", None),
            ("Streaming platform verification", None),
            ("Social media announcement", None),
            ("Lyrics submission (Genius / Musixmatch)", None),
        ],
        "performing" => vec![
            ("Arrangement finalized for live", None),
            ("Backing track prepared", None),
            ("Click track / in-ear mix", None),
            ("Rehearsed with band", None),
            ("Setlist placement decided", None),
            ("Stage plot / tech rider updated", None),
        ],
        _ => vec![],
    }
}

/// Create default steps for a given stage. Looks up instrument_id by type if provided.
pub async fn auto_add_steps(
    pool: &SqlitePool,
    stage_id: i64,
    is_cover: bool,
) -> Result<Vec<i64>, sqlx::Error> {
    // Look up stage name
    let stage_name: String = sqlx::query_scalar("SELECT stage FROM production_stages WHERE id = ?")
        .bind(stage_id)
        .fetch_one(pool)
        .await?;

    let steps = default_steps_for_stage(&stage_name, is_cover);
    let mut ids = Vec::new();
    for (i, (name, inst_type)) in steps.iter().enumerate() {
        let instrument_id: Option<i64> = if let Some(itype) = inst_type {
            sqlx::query_scalar("SELECT id FROM instruments WHERE instrument_type = ? LIMIT 1")
                .bind(itype)
                .fetch_optional(pool)
                .await?
        } else {
            None
        };

        let result = sqlx::query(
            "INSERT INTO production_steps (stage_id, instrument_id, name, status, sort_order) \
             VALUES (?, ?, ?, 'not_started', ?)",
        )
        .bind(stage_id)
        .bind(instrument_id)
        .bind(name)
        .bind(i as i32)
        .execute(pool)
        .await?;
        ids.push(result.last_insert_rowid());
    }
    Ok(ids)
}

pub async fn delete_sample(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sample_instruments WHERE sample_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM samples WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Workflow state transitions
// ============================================================================

pub async fn update_workflow_state(
    pool: &SqlitePool,
    song_id: i64,
    state: &WorkflowState,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE songs SET workflow_state = ? WHERE id = ?")
        .bind(state.as_str())
        .bind(song_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_songs_by_workflow_state(
    pool: &SqlitePool,
    state: &WorkflowState,
) -> Result<Vec<Song>, sqlx::Error> {
    let sql = format!(
        "SELECT {SONG_SELECT_COLS} FROM songs s \
         LEFT JOIN albums a ON a.id = s.album_id \
         WHERE s.workflow_state = ? \
         ORDER BY s.title"
    );
    let rows = sqlx::query(&sql)
        .bind(state.as_str())
        .fetch_all(pool)
        .await?;

    let mut songs = Vec::new();
    for row in &rows {
        let sid: i64 = row.get("id");
        let artists = fetch_song_artists(pool, sid).await?;
        let f = row_to_song_fields(row);
        songs.push(song_from_row(row, f, artists));
    }
    Ok(songs)
}

pub async fn list_songs_in_live_sets(pool: &SqlitePool) -> Result<Vec<Song>, sqlx::Error> {
    let sql = format!(
        "SELECT {SONG_SELECT_COLS} FROM songs s \
         LEFT JOIN albums a ON a.id = s.album_id \
         INNER JOIN live_set_songs lss ON lss.song_id = s.id \
         ORDER BY s.title"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await?;

    let mut songs = Vec::new();
    for row in &rows {
        let sid: i64 = row.get("id");
        let artists = fetch_song_artists(pool, sid).await?;
        let f = row_to_song_fields(row);
        songs.push(song_from_row(row, f, artists));
    }
    Ok(songs)
}

// ============================================================================
// Practice exercises
// ============================================================================

pub async fn list_exercises(pool: &SqlitePool) -> Result<Vec<PracticeExercise>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT e.id, e.instrument_id, COALESCE(i.name, '') as instrument_name, \
         e.name, e.category, e.description, e.source, e.sort_order \
         FROM practice_exercises e \
         LEFT JOIN instruments i ON i.id = e.instrument_id \
         ORDER BY e.sort_order, e.name",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| PracticeExercise {
            id: r.get("id"),
            instrument_id: r.get("instrument_id"),
            instrument_name: r.get("instrument_name"),
            name: r.get("name"),
            category: r.get("category"),
            description: r
                .get::<Option<String>, _>("description")
                .unwrap_or_default(),
            source: r.get::<Option<String>, _>("source").unwrap_or_default(),
            sort_order: r.get("sort_order"),
        })
        .collect())
}

pub async fn create_exercise(
    pool: &SqlitePool,
    input: &CreatePracticeExercise,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO practice_exercises \
         (instrument_id, name, category, description, source, sort_order) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(input.instrument_id)
    .bind(&input.name)
    .bind(&input.category)
    .bind(&input.description)
    .bind(&input.source)
    .bind(input.sort_order)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_exercise(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM song_exercises WHERE exercise_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM practice_exercises WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_song_exercises(
    pool: &SqlitePool,
    song_id: i64,
) -> Result<Vec<SongExercise>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT se.id, se.song_id, se.exercise_id, \
         e.name as exercise_name, COALESCE(i.name, '') as instrument_name, se.notes \
         FROM song_exercises se \
         INNER JOIN practice_exercises e ON e.id = se.exercise_id \
         LEFT JOIN instruments i ON i.id = e.instrument_id \
         WHERE se.song_id = ? ORDER BY e.sort_order, e.name",
    )
    .bind(song_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SongExercise {
            id: r.get("id"),
            song_id: r.get("song_id"),
            exercise_id: r.get("exercise_id"),
            exercise_name: r.get("exercise_name"),
            instrument_name: r.get("instrument_name"),
            notes: r.get::<Option<String>, _>("notes").unwrap_or_default(),
        })
        .collect())
}

pub async fn create_song_exercise(
    pool: &SqlitePool,
    input: &CreateSongExercise,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO song_exercises (song_id, exercise_id, notes) VALUES (?, ?, ?)",
    )
    .bind(input.song_id)
    .bind(input.exercise_id)
    .bind(&input.notes)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_song_exercise(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM song_exercises WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// User profile
// ============================================================================

pub async fn get_profile(pool: &SqlitePool) -> Result<UserProfile, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, display_name, songs_capacity, warmup_minutes, \
         drill_minutes, song_minutes, review_minutes, notes \
         FROM user_profile WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;

    Ok(UserProfile {
        id: row.get("id"),
        display_name: row.get("display_name"),
        songs_capacity: row.get("songs_capacity"),
        warmup_minutes: row.get("warmup_minutes"),
        drill_minutes: row.get("drill_minutes"),
        song_minutes: row.get("song_minutes"),
        review_minutes: row.get("review_minutes"),
        notes: row.get::<Option<String>, _>("notes").unwrap_or_default(),
    })
}

pub async fn update_profile(
    pool: &SqlitePool,
    input: &UpdateUserProfile,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE user_profile SET display_name = ?, songs_capacity = ?, \
         warmup_minutes = ?, drill_minutes = ?, song_minutes = ?, \
         review_minutes = ?, notes = ? WHERE id = 1",
    )
    .bind(&input.display_name)
    .bind(input.songs_capacity)
    .bind(input.warmup_minutes)
    .bind(input.drill_minutes)
    .bind(input.song_minutes)
    .bind(input.review_minutes)
    .bind(&input.notes)
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Goals
// ============================================================================

pub async fn list_goals(pool: &SqlitePool) -> Result<Vec<Goal>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, horizon, category, title, description, target_date, \
         completed, created_at, sort_order \
         FROM goals ORDER BY \
         CASE horizon \
           WHEN '5_year' THEN 1 WHEN '1_year' THEN 2 \
           WHEN '6_week' THEN 3 WHEN '1_week' THEN 4 END, \
         sort_order, title",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| Goal {
            id: r.get("id"),
            horizon: r.get("horizon"),
            category: r.get("category"),
            title: r.get("title"),
            description: r
                .get::<Option<String>, _>("description")
                .unwrap_or_default(),
            target_date: r
                .get::<Option<String>, _>("target_date")
                .unwrap_or_default(),
            completed: r.get("completed"),
            created_at: r.get("created_at"),
            sort_order: r.get("sort_order"),
        })
        .collect())
}

pub async fn create_goal(pool: &SqlitePool, input: &CreateGoal) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO goals (horizon, category, title, description, target_date, sort_order) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&input.horizon)
    .bind(&input.category)
    .bind(&input.title)
    .bind(&input.description)
    .bind(&input.target_date)
    .bind(input.sort_order)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn toggle_goal(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE goals SET completed = NOT completed WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_goal(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM goals WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Schedule events & items
// ============================================================================

pub async fn list_schedule_events(pool: &SqlitePool) -> Result<Vec<ScheduleEvent>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, event_date, title, event_type, status, notes, created_at \
         FROM schedule_events ORDER BY event_date ASC, id ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut events = Vec::new();
    for row in &rows {
        let eid: i64 = row.get("id");
        let items = list_schedule_items(pool, eid).await?;
        events.push(ScheduleEvent {
            id: eid,
            event_date: row.get("event_date"),
            title: row.get("title"),
            event_type: row.get("event_type"),
            status: row.get("status"),
            notes: row.get::<Option<String>, _>("notes").unwrap_or_default(),
            created_at: row.get("created_at"),
            items,
        });
    }
    Ok(events)
}

pub async fn list_schedule_items(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<Vec<ScheduleItem>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT si.id, si.event_id, si.item_type, si.song_id, \
         COALESCE(s.title, '') as song_title, \
         si.exercise_id, COALESCE(e.name, '') as exercise_name, \
         si.stage_id, COALESCE(ps.stage, '') as stage_name, \
         si.instrument_id, COALESCE(i.name, '') as instrument_name, \
         si.title, si.duration_minutes, si.sort_order, si.completed, si.notes \
         FROM schedule_items si \
         LEFT JOIN songs s ON s.id = si.song_id \
         LEFT JOIN practice_exercises e ON e.id = si.exercise_id \
         LEFT JOIN production_stages ps ON ps.id = si.stage_id \
         LEFT JOIN instruments i ON i.id = si.instrument_id \
         WHERE si.event_id = ? ORDER BY si.sort_order, si.id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| ScheduleItem {
            id: r.get("id"),
            event_id: r.get("event_id"),
            item_type: r.get("item_type"),
            song_id: r.get("song_id"),
            song_title: r.get("song_title"),
            exercise_id: r.get("exercise_id"),
            exercise_name: r.get("exercise_name"),
            stage_id: r.get("stage_id"),
            stage_name: r.get("stage_name"),
            instrument_id: r.get("instrument_id"),
            instrument_name: r.get("instrument_name"),
            title: r.get("title"),
            duration_minutes: r.get("duration_minutes"),
            sort_order: r.get("sort_order"),
            completed: r.get("completed"),
            notes: r.get::<Option<String>, _>("notes").unwrap_or_default(),
        })
        .collect())
}

pub async fn create_schedule_event(
    pool: &SqlitePool,
    input: &CreateScheduleEvent,
) -> Result<i64, sqlx::Error> {
    let result =
        sqlx::query("INSERT INTO schedule_events (event_date, title, event_type) VALUES (?, ?, ?)")
            .bind(&input.event_date)
            .bind(&input.title)
            .bind(&input.event_type)
            .execute(pool)
            .await?;
    Ok(result.last_insert_rowid())
}

pub async fn create_schedule_item(
    pool: &SqlitePool,
    input: &CreateScheduleItem,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO schedule_items \
         (event_id, item_type, song_id, exercise_id, stage_id, instrument_id, \
          title, duration_minutes, sort_order, notes) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(input.event_id)
    .bind(&input.item_type)
    .bind(input.song_id)
    .bind(input.exercise_id)
    .bind(input.stage_id)
    .bind(input.instrument_id)
    .bind(&input.title)
    .bind(input.duration_minutes)
    .bind(input.sort_order)
    .bind(&input.notes)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn toggle_schedule_item(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE schedule_items SET completed = NOT completed WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_schedule_event_status(
    pool: &SqlitePool,
    id: i64,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE schedule_events SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_schedule_event(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM schedule_items WHERE event_id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM schedule_events WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_schedule_event(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<ScheduleEvent>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, event_date, title, event_type, status, notes, created_at \
         FROM schedule_events WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let eid: i64 = row.get("id");
            let items = list_schedule_items(pool, eid).await?;
            Ok(Some(ScheduleEvent {
                id: eid,
                event_date: row.get("event_date"),
                title: row.get("title"),
                event_type: row.get("event_type"),
                status: row.get("status"),
                notes: row.get::<Option<String>, _>("notes").unwrap_or_default(),
                created_at: row.get("created_at"),
                items,
            }))
        }
        None => Ok(None),
    }
}

// ============================================================================
// Schedule generation — auto-pick songs based on capacity
// ============================================================================

pub async fn generate_schedule(
    pool: &SqlitePool,
    start_date: &str,
    num_blocks: i32,
) -> Result<Vec<i64>, sqlx::Error> {
    let profile = get_profile(pool).await?;
    let capacity = profile.songs_capacity as usize;

    // Get active songs (learning, shaky, performing, producing, cover_recording)
    let active_states = [
        WorkflowState::Learning,
        WorkflowState::Shaky,
        WorkflowState::Performing,
        WorkflowState::Producing,
        WorkflowState::CoverRecording,
    ];
    let mut active_songs = Vec::new();
    for state in &active_states {
        let mut songs = list_songs_by_workflow_state(pool, state).await?;
        active_songs.append(&mut songs);
    }

    // Also get songs from live sets for practice
    let live_set_songs = list_songs_in_live_sets(pool).await?;
    for song in live_set_songs {
        if !active_songs.iter().any(|s| s.id == song.id) {
            active_songs.push(song);
        }
    }

    // Get all exercises for warmups (only add once per block)
    let exercises = list_exercises(pool).await?;

    let mut event_ids = Vec::new();

    // Generate 3-day blocks instead of single days
    for block_offset in 0..num_blocks {
        let block_start_day = block_offset * 3;
        let date_start = add_days_to_date(start_date, block_start_day);
        let date_end = add_days_to_date(start_date, block_start_day + 2);
        let date_range = if date_start == date_end {
            date_start.clone()
        } else {
            format!("{} to {}", date_start, date_end)
        };

        let event_id = create_schedule_event(
            pool,
            &CreateScheduleEvent {
                event_date: date_start.clone(),
                title: format!("Practice Block — {}", date_range),
                event_type: "mixed".to_string(),
            },
        )
        .await?;

        let mut sort = 0;

        // 1. Warmup exercises (pick up to 2) - only once per 3-day block
        for ex in exercises.iter().take(2) {
            create_schedule_item(
                pool,
                &CreateScheduleItem {
                    event_id,
                    item_type: "warmup".to_string(),
                    song_id: None,
                    exercise_id: Some(ex.id),
                    stage_id: None,
                    instrument_id: None, // Remove instrument-specific selection
                    title: format!("Warmup: {}", ex.name),
                    duration_minutes: profile.warmup_minutes * 3 / 2, // Spread across 3 days
                    sort_order: sort,
                    notes: String::new(),
                },
            )
            .await?;
            sort += 1;
        }

        // 2. Drills (pick up to 2 technique exercises) - only once per 3-day block
        let drills: Vec<&PracticeExercise> = exercises
            .iter()
            .filter(|e| e.category == "technique" || e.category == "scales")
            .take(2)
            .collect();
        for ex in &drills {
            create_schedule_item(
                pool,
                &CreateScheduleItem {
                    event_id,
                    item_type: "drill".to_string(),
                    song_id: None,
                    exercise_id: Some(ex.id),
                    stage_id: None,
                    instrument_id: None, // Remove instrument-specific selection
                    title: format!("Drill: {}", ex.name),
                    duration_minutes: profile.drill_minutes * 3 / 2, // Spread across 3 days
                    sort_order: sort,
                    notes: String::new(),
                },
            )
            .await?;
            sort += 1;
        }

        // 3. Song practice/production — priority-weighted shuffle
        //    Priority 1 (highest) gets weight 5, priority 5 gets weight 1, 0 (unranked) gets 2
        let songs_for_block: Vec<&Song> = {
            let mut weighted: Vec<(&Song, u32)> = active_songs
                .iter()
                .map(|s| {
                    let w = match s.practice_priority {
                        1 => 5u32,
                        2 => 4,
                        3 => 3,
                        4 => 2,
                        5 => 1,
                        _ => 2, // unranked gets moderate weight
                    };
                    (s, w)
                })
                .collect();
            // Deterministic-ish shuffle: rotate by block_offset, then stable-sort
            // descending by weight so higher-priority songs appear first
            let len = weighted.len().max(1);
            weighted.rotate_left((block_offset as usize) % len);
            weighted.sort_by_key(|b| std::cmp::Reverse(b.1));
            weighted.iter().map(|(s, _)| *s).take(capacity).collect()
        };
        for song in &songs_for_block {
            let item_type = match song.workflow_state {
                WorkflowState::Producing | WorkflowState::CoverRecording => "song_production",
                _ => "song_practice",
            };

            // Find linked exercises for warmup context
            let song_exs = list_song_exercises(pool, song.id).await?;
            for se in &song_exs {
                create_schedule_item(
                    pool,
                    &CreateScheduleItem {
                        event_id,
                        item_type: "exercise".to_string(),
                        song_id: Some(song.id),
                        exercise_id: Some(se.exercise_id),
                        stage_id: None,
                        instrument_id: None,
                        title: format!("Song warmup: {} — {}", se.exercise_name, song.title),
                        duration_minutes: 1,
                        sort_order: sort,
                        notes: String::new(),
                    },
                )
                .await?;
                sort += 1;
            }

            create_schedule_item(
                pool,
                &CreateScheduleItem {
                    event_id,
                    item_type: item_type.to_string(),
                    song_id: Some(song.id),
                    exercise_id: None,
                    stage_id: None,
                    instrument_id: None,
                    title: format!(
                        "{}: {}",
                        if item_type == "song_production" {
                            "Produce"
                        } else {
                            "Practice"
                        },
                        song.title
                    ),
                    duration_minutes: profile.song_minutes * 3
                        / songs_for_block.len().max(1) as i32,
                    sort_order: sort,
                    notes: String::new(),
                },
            )
            .await?;
            sort += 1;
        }

        // 4. Review time - only once per 3-day block
        create_schedule_item(
            pool,
            &CreateScheduleItem {
                event_id,
                item_type: "review".to_string(),
                song_id: None,
                exercise_id: None,
                stage_id: None,
                instrument_id: None,
                title: "Review: notes & listen to recordings".to_string(),
                duration_minutes: profile.review_minutes * 3,
                sort_order: sort,
                notes: String::new(),
            },
        )
        .await?;

        event_ids.push(event_id);
    }

    Ok(event_ids)
}

fn add_days_to_date(date_str: &str, days: i32) -> String {
    // Parse YYYY-MM-DD and add days
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return date_str.to_string();
    }
    let year: i32 = parts[0].parse().unwrap_or(2026);
    let month: u32 = parts[1].parse().unwrap_or(1);
    let day: u32 = parts[2].parse().unwrap_or(1);

    // Simple Julian day calculation for date arithmetic
    let days_in_month = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;

    let mut total_day = day as i32 + days;
    let mut m = month;
    let mut y = year;
    loop {
        let max_days = if m == 2 && is_leap {
            29
        } else {
            days_in_month[m as usize]
        };
        if total_day <= max_days {
            break;
        }
        total_day -= max_days;
        m += 1;
        if m > 12 {
            m = 1;
            y += 1;
        }
    }
    format!("{y:04}-{m:02}-{:02}", total_day)
}

// ============================================================================
// Practice priority
// ============================================================================

pub async fn update_practice_priority(
    pool: &SqlitePool,
    song_id: i64,
    priority: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE songs SET practice_priority = ? WHERE id = ?")
        .bind(priority)
        .bind(song_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ============================================================================
// Live sets
// ============================================================================

pub async fn list_live_sets(pool: &SqlitePool) -> Result<Vec<LiveSet>, sqlx::Error> {
    let set_rows = sqlx::query(
        "SELECT id, name, set_type, description, target_duration_seconds, created_at \
         FROM live_sets ORDER BY name",
    )
    .fetch_all(pool)
    .await?;

    let mut sets = Vec::new();
    for sr in &set_rows {
        let set_id: i64 = sr.get("id");
        let song_rows = sqlx::query(
            "SELECT ls.id, ls.set_id, ls.song_id, s.title as song_title, \
             ls.sort_order, ls.backing_track_path, ls.duration_seconds, ls.transition_notes \
             FROM live_set_songs ls \
             JOIN songs s ON s.id = ls.song_id \
             WHERE ls.set_id = ? \
             ORDER BY ls.sort_order, s.title",
        )
        .bind(set_id)
        .fetch_all(pool)
        .await?;

        let songs: Vec<LiveSetSong> = song_rows
            .iter()
            .map(|r| LiveSetSong {
                id: r.get("id"),
                set_id: r.get("set_id"),
                song_id: r.get("song_id"),
                song_title: r.get("song_title"),
                sort_order: r.get("sort_order"),
                backing_track_path: r
                    .get::<Option<String>, _>("backing_track_path")
                    .unwrap_or_default(),
                duration_seconds: r.get::<Option<i32>, _>("duration_seconds").unwrap_or(0),
                transition_notes: r
                    .get::<Option<String>, _>("transition_notes")
                    .unwrap_or_default(),
            })
            .collect();

        let actual_duration: i32 = songs.iter().map(|s| s.duration_seconds).sum();

        sets.push(LiveSet {
            id: set_id,
            name: sr.get("name"),
            set_type: sr.get("set_type"),
            description: sr
                .get::<Option<String>, _>("description")
                .unwrap_or_default(),
            target_duration_seconds: sr
                .get::<Option<i32>, _>("target_duration_seconds")
                .unwrap_or(0),
            created_at: sr.get("created_at"),
            actual_duration_seconds: actual_duration,
            songs,
        });
    }
    Ok(sets)
}

pub async fn get_live_set(pool: &SqlitePool, id: i64) -> Result<Option<LiveSet>, sqlx::Error> {
    let sets = list_live_sets(pool).await?;
    Ok(sets.into_iter().find(|s| s.id == id))
}

pub async fn create_live_set(pool: &SqlitePool, input: &CreateLiveSet) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO live_sets (name, set_type, description, target_duration_seconds) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(&input.name)
    .bind(&input.set_type)
    .bind(&input.description)
    .bind(input.target_duration_seconds)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_live_set(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM live_sets WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn add_song_to_set(
    pool: &SqlitePool,
    input: &CreateLiveSetSong,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO live_set_songs \
         (set_id, song_id, sort_order, backing_track_path, duration_seconds, transition_notes) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(input.set_id)
    .bind(input.song_id)
    .bind(input.sort_order)
    .bind(&input.backing_track_path)
    .bind(input.duration_seconds)
    .bind(&input.transition_notes)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn remove_song_from_set(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM live_set_songs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
