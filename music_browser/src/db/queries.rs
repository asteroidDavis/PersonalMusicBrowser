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

fn row_to_song_fields(
    row: &sqlx::sqlite::SqliteRow,
) -> (
    String,
    String,
    String,
    String,
    Option<i32>,
    Option<i32>,
    String,
    String,
    String,
) {
    let sheet_music = row
        .get::<Option<String>, _>("sheet_music")
        .unwrap_or_default();
    let lyrics = row.get::<Option<String>, _>("lyrics").unwrap_or_default();
    let key = row.get::<Option<String>, _>("key").unwrap_or_default();
    let album_title = row
        .get::<Option<String>, _>("album_title")
        .unwrap_or_default();
    let bpm_lower: Option<i32> = row.get("bpm_lower");
    let bpm_upper: Option<i32> = row.get("bpm_upper");
    let original_artist = row
        .get::<Option<String>, _>("original_artist")
        .unwrap_or_default();
    let score_url = row
        .get::<Option<String>, _>("score_url")
        .unwrap_or_default();
    let description = row
        .get::<Option<String>, _>("description")
        .unwrap_or_default();
    (
        sheet_music,
        lyrics,
        key,
        album_title,
        bpm_lower,
        bpm_upper,
        original_artist,
        score_url,
        description,
    )
}

pub async fn list_songs(pool: &SqlitePool) -> Result<Vec<Song>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT s.id, s.title, s.album_id, COALESCE(a.title, '') as album_title, \
         s.sheet_music, s.lyrics, s.song_type, s.key, s.bpm_lower, s.bpm_upper, \
         s.original_artist, s.score_url, s.description \
         FROM songs s \
         LEFT JOIN albums a ON a.id = s.album_id \
         ORDER BY s.title",
    )
    .fetch_all(pool)
    .await?;

    let mut songs = Vec::new();
    for row in &rows {
        let sid: i64 = row.get("id");
        let song_type_str: String = row.get("song_type");
        let artists = fetch_song_artists(pool, sid).await?;
        let (
            sheet_music,
            lyrics,
            key,
            album_title,
            bpm_lower,
            bpm_upper,
            original_artist,
            score_url,
            description,
        ) = row_to_song_fields(row);
        songs.push(Song {
            id: sid,
            title: row.get("title"),
            album_id: row.get("album_id"),
            album_title,
            sheet_music,
            lyrics,
            song_type: SongType::parse(&song_type_str).unwrap_or(SongType::Song),
            key,
            bpm_lower,
            bpm_upper,
            original_artist,
            score_url,
            description,
            artists,
        });
    }
    Ok(songs)
}

pub async fn get_song(pool: &SqlitePool, id: i64) -> Result<Option<Song>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT s.id, s.title, s.album_id, COALESCE(a.title, '') as album_title, \
         s.sheet_music, s.lyrics, s.song_type, s.key, s.bpm_lower, s.bpm_upper, \
         s.original_artist, s.score_url, s.description \
         FROM songs s \
         LEFT JOIN albums a ON a.id = s.album_id \
         WHERE s.id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let sid: i64 = row.get("id");
            let song_type_str: String = row.get("song_type");
            let artists = fetch_song_artists(pool, sid).await?;
            let (
                sheet_music,
                lyrics,
                key,
                album_title,
                bpm_lower,
                bpm_upper,
                original_artist,
                score_url,
                description,
            ) = row_to_song_fields(&row);
            Ok(Some(Song {
                id: sid,
                title: row.get("title"),
                album_id: row.get("album_id"),
                album_title,
                sheet_music,
                lyrics,
                song_type: SongType::parse(&song_type_str).unwrap_or(SongType::Song),
                key,
                bpm_lower,
                bpm_upper,
                original_artist,
                score_url,
                description,
                artists,
            }))
        }
        None => Ok(None),
    }
}

pub async fn create_song(pool: &SqlitePool, input: &CreateSong) -> Result<i64, sqlx::Error> {
    let song_type_str = input.song_type.as_str();
    let result = sqlx::query(
        "INSERT INTO songs (title, album_id, sheet_music, lyrics, song_type, \
         key, bpm_lower, bpm_upper, original_artist, score_url, description) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        "UPDATE songs SET title = ?, album_id = ?, sheet_music = ?, lyrics = ?, \
         key = ?, bpm_lower = ?, bpm_upper = ?, original_artist = ?, \
         score_url = ?, description = ? WHERE id = ?",
    )
    .bind(&input.title)
    .bind(input.album_id)
    .bind(&input.sheet_music)
    .bind(&input.lyrics)
    .bind(&input.key)
    .bind(input.bpm_lower)
    .bind(input.bpm_upper)
    .bind(&input.original_artist)
    .bind(&input.score_url)
    .bind(&input.description)
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
