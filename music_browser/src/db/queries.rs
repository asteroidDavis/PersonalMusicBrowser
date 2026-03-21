use sqlx::{Row, SqlitePool};

use super::models::*;

// --- Instruments ---

pub async fn list_instruments(pool: &SqlitePool) -> Result<Vec<Instrument>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, name FROM instruments ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .iter()
        .map(|r| Instrument {
            id: r.get("id"),
            name: r.get("name"),
        })
        .collect())
}

pub async fn create_instrument(
    pool: &SqlitePool,
    input: &CreateInstrument,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query("INSERT INTO instruments (name) VALUES (?)")
        .bind(&input.name)
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

// --- Bands ---

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

// --- Artists ---

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

#[allow(dead_code)]
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

// --- Albums ---

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

#[allow(dead_code)]
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

// --- Songs ---

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

pub async fn list_songs(pool: &SqlitePool) -> Result<Vec<Song>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT s.id, s.title, s.album_id, a.title as album_title, \
         s.sheet_music, s.lyrics, s.song_type \
         FROM songs s \
         INNER JOIN albums a ON a.id = s.album_id \
         ORDER BY s.title",
    )
    .fetch_all(pool)
    .await?;

    let mut songs = Vec::new();
    for row in &rows {
        let sid: i64 = row.get("id");
        let song_type_str: String = row.get("song_type");
        let artists = fetch_song_artists(pool, sid).await?;
        songs.push(Song {
            id: sid,
            title: row.get("title"),
            album_id: row.get("album_id"),
            album_title: row.get("album_title"),
            sheet_music: row
                .get::<Option<String>, _>("sheet_music")
                .unwrap_or_default(),
            lyrics: row.get::<Option<String>, _>("lyrics").unwrap_or_default(),
            song_type: SongType::from_str(&song_type_str).unwrap_or(SongType::Song),
            artists,
        });
    }
    Ok(songs)
}

pub async fn get_song(pool: &SqlitePool, id: i64) -> Result<Option<Song>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT s.id, s.title, s.album_id, a.title as album_title, \
         s.sheet_music, s.lyrics, s.song_type \
         FROM songs s \
         INNER JOIN albums a ON a.id = s.album_id \
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
            Ok(Some(Song {
                id: sid,
                title: row.get("title"),
                album_id: row.get("album_id"),
                album_title: row.get("album_title"),
                sheet_music: row
                    .get::<Option<String>, _>("sheet_music")
                    .unwrap_or_default(),
                lyrics: row.get::<Option<String>, _>("lyrics").unwrap_or_default(),
                song_type: SongType::from_str(&song_type_str).unwrap_or(SongType::Song),
                artists,
            }))
        }
        None => Ok(None),
    }
}

pub async fn create_song(pool: &SqlitePool, input: &CreateSong) -> Result<i64, sqlx::Error> {
    let song_type_str = input.song_type.as_str();
    let result = sqlx::query(
        "INSERT INTO songs (title, album_id, sheet_music, lyrics, song_type) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&input.title)
    .bind(input.album_id)
    .bind(&input.sheet_music)
    .bind(&input.lyrics)
    .bind(song_type_str)
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
        "UPDATE songs SET title = ?, album_id = ?, sheet_music = ?, lyrics = ? WHERE id = ?",
    )
    .bind(&input.title)
    .bind(input.album_id)
    .bind(&input.sheet_music)
    .bind(&input.lyrics)
    .bind(input.id)
    .execute(pool)
    .await?;

    // Replace artist associations
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
    // Clean up associations and detail tables first
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
    // Delete recordings referencing this song
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
    sqlx::query("DELETE FROM songs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// --- Recordings ---

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
            "SELECT i.id, i.name FROM instruments i \
             INNER JOIN recording_instruments ri ON ri.instrument_id = i.id \
             WHERE ri.recording_id = ?",
        )
        .bind(rid)
        .fetch_all(pool)
        .await?;

        let rec_type_str: String = row.get("recording_type");
        let recording_type = RecordingType::from_str(&rec_type_str).unwrap_or(RecordingType::Wav);

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
                })
                .collect(),
        });
    }
    Ok(recordings)
}

#[allow(dead_code)]
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
