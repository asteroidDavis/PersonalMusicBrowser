-- Initial migration: Create discography tables
-- Ported from Django PersonalMusicBrowser models

CREATE TABLE IF NOT EXISTS instruments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 64)
);

CREATE TABLE IF NOT EXISTS bands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 128)
);

CREATE TABLE IF NOT EXISTS artists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 128)
);

CREATE TABLE IF NOT EXISTS artist_bands (
    artist_id INTEGER NOT NULL REFERENCES artists(id) ON DELETE CASCADE,
    band_id INTEGER NOT NULL REFERENCES bands(id) ON DELETE CASCADE,
    PRIMARY KEY (artist_id, band_id)
);

CREATE TABLE IF NOT EXISTS albums (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL CHECK(length(title) <= 256),
    released BOOLEAN NOT NULL DEFAULT 0,
    url TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS songs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL CHECK(length(title) <= 256),
    album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE RESTRICT,
    sheet_music TEXT DEFAULT '',
    lyrics TEXT DEFAULT '',
    song_type TEXT NOT NULL DEFAULT 'song' CHECK(song_type IN ('song', 'cover', 'composition'))
);

CREATE TABLE IF NOT EXISTS song_artists (
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    artist_id INTEGER NOT NULL REFERENCES artists(id) ON DELETE CASCADE,
    PRIMARY KEY (song_id, artist_id)
);

-- Cover-specific fields (song_type = 'cover')
CREATE TABLE IF NOT EXISTS cover_details (
    song_id INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE,
    notes_image TEXT DEFAULT '',
    notes_completed BOOLEAN NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS cover_instruments (
    song_id INTEGER NOT NULL REFERENCES cover_details(song_id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    PRIMARY KEY (song_id, instrument_id)
);

-- Composition-specific fields (song_type = 'composition')
CREATE TABLE IF NOT EXISTS composition_details (
    song_id INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE,
    beats_per_minute_upper INTEGER NOT NULL DEFAULT 120,
    beats_per_minute_lower INTEGER NOT NULL DEFAULT 120
);

CREATE TABLE IF NOT EXISTS composition_instruments (
    song_id INTEGER NOT NULL REFERENCES composition_details(song_id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    PRIMARY KEY (song_id, instrument_id)
);

CREATE TABLE IF NOT EXISTS recordings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    recording_type TEXT NOT NULL CHECK(recording_type IN ('audacity', 'mix', 'master', 'loop-core-list', 'wav')),
    path TEXT DEFAULT '',
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE RESTRICT,
    notes_image TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS recording_instruments (
    recording_id INTEGER NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    PRIMARY KEY (recording_id, instrument_id)
);

-- Enable WAL mode for concurrent access
PRAGMA journal_mode=WAL;
