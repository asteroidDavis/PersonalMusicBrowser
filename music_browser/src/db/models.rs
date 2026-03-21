use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Instrument {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Band {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Artist {
    pub id: i64,
    pub name: String,
    pub bands: Vec<Band>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub released: bool,
    pub url: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SongType {
    Song,
    Cover,
    Composition,
}

impl SongType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SongType::Song => "song",
            SongType::Cover => "cover",
            SongType::Composition => "composition",
        }
    }

    pub fn from_str(s: &str) -> Option<SongType> {
        match s {
            "song" => Some(SongType::Song),
            "cover" => Some(SongType::Cover),
            "composition" => Some(SongType::Composition),
            _ => None,
        }
    }
}

impl std::fmt::Display for SongType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Song {
    pub id: i64,
    pub title: String,
    pub album_id: i64,
    pub album_title: String,
    pub sheet_music: String,
    pub lyrics: String,
    pub song_type: SongType,
    pub artists: Vec<Artist>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverDetail {
    pub song_id: i64,
    pub notes_image: String,
    pub notes_completed: bool,
    pub covered_instruments: Vec<Instrument>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompositionDetail {
    pub song_id: i64,
    pub beats_per_minute_upper: i32,
    pub beats_per_minute_lower: i32,
    pub written_instruments: Vec<Instrument>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordingType {
    Audacity,
    Mix,
    Master,
    LoopCoreList,
    Wav,
}

impl RecordingType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecordingType::Audacity => "audacity",
            RecordingType::Mix => "mix",
            RecordingType::Master => "master",
            RecordingType::LoopCoreList => "loop-core-list",
            RecordingType::Wav => "wav",
        }
    }

    pub fn from_str(s: &str) -> Option<RecordingType> {
        match s {
            "audacity" => Some(RecordingType::Audacity),
            "mix" => Some(RecordingType::Mix),
            "master" => Some(RecordingType::Master),
            "loop-core-list" => Some(RecordingType::LoopCoreList),
            "wav" => Some(RecordingType::Wav),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn all() -> &'static [RecordingType] {
        &[
            RecordingType::Audacity,
            RecordingType::Mix,
            RecordingType::Master,
            RecordingType::LoopCoreList,
            RecordingType::Wav,
        ]
    }
}

impl std::fmt::Display for RecordingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recording {
    pub id: i64,
    pub recording_type: RecordingType,
    pub path: String,
    pub song_id: i64,
    pub notes_image: String,
    pub instruments: Vec<Instrument>,
}

// --- Form structs for create/update operations ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSong {
    pub title: String,
    pub album_id: i64,
    pub sheet_music: String,
    pub lyrics: String,
    pub song_type: SongType,
    pub artist_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSong {
    pub id: i64,
    pub title: String,
    pub album_id: i64,
    pub sheet_music: String,
    pub lyrics: String,
    pub artist_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAlbum {
    pub title: String,
    pub released: bool,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateArtist {
    pub name: String,
    pub band_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInstrument {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBand {
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecording {
    pub recording_type: RecordingType,
    pub path: String,
    pub song_id: i64,
    pub notes_image: String,
    pub instrument_ids: Vec<i64>,
}
