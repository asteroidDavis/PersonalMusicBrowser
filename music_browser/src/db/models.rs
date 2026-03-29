use serde::{Deserialize, Serialize};

// ============================================================================
// Core entities
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Instrument {
    pub id: i64,
    pub name: String,
    pub instrument_type: String,
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

// ============================================================================
// Song types & Song
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SongType {
    Song,
    Cover,
    Composition,
    Original,
    Practice,
}

impl SongType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SongType::Song => "song",
            SongType::Cover => "cover",
            SongType::Composition => "composition",
            SongType::Original => "original",
            SongType::Practice => "practice",
        }
    }

    pub fn parse(s: &str) -> Option<SongType> {
        match s {
            "song" => Some(SongType::Song),
            "cover" => Some(SongType::Cover),
            "composition" => Some(SongType::Composition),
            "original" => Some(SongType::Original),
            "practice" => Some(SongType::Practice),
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
    pub album_id: Option<i64>,
    pub album_title: String,
    pub sheet_music: String,
    pub lyrics: String,
    pub song_type: SongType,
    pub key: String,
    pub bpm_lower: Option<i32>,
    pub bpm_upper: Option<i32>,
    pub original_artist: String,
    pub score_url: String,
    pub description: String,
    pub workflow_state: WorkflowState,
    pub scores_folder: String,
    pub export_folder: String,
    pub musicxml_path: String,
    pub artists: Vec<Artist>,
}

// ============================================================================
// Cover & Composition details (retained from v1)
// ============================================================================

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

// ============================================================================
// Recordings
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordingType {
    Audacity,
    Mix,
    Master,
    LoopCoreList,
    Wav,
    DawProject,
    Practice,
}

impl RecordingType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecordingType::Audacity => "audacity",
            RecordingType::Mix => "mix",
            RecordingType::Master => "master",
            RecordingType::LoopCoreList => "loop-core-list",
            RecordingType::Wav => "wav",
            RecordingType::DawProject => "daw-project",
            RecordingType::Practice => "practice",
        }
    }

    pub fn parse(s: &str) -> Option<RecordingType> {
        match s {
            "audacity" => Some(RecordingType::Audacity),
            "mix" => Some(RecordingType::Mix),
            "master" => Some(RecordingType::Master),
            "loop-core-list" => Some(RecordingType::LoopCoreList),
            "wav" => Some(RecordingType::Wav),
            "daw-project" => Some(RecordingType::DawProject),
            "practice" => Some(RecordingType::Practice),
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
            RecordingType::DawProject,
            RecordingType::Practice,
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

// ============================================================================
// Devices & presets
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Device {
    pub id: i64,
    pub name: String,
    pub device_type: String,
    pub manual_path: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DevicePreset {
    pub id: i64,
    pub device_id: i64,
    pub name: String,
    pub preset_code: String,
    pub description: String,
}

// ============================================================================
// Song instruments — normalized live config
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SongInstrument {
    pub id: i64,
    pub song_id: i64,
    pub instrument_id: i64,
    pub instrument_name: String,
    pub description: String,
    pub score_url: String,
    pub production_path: String,
    pub mastering_path: String,
    pub presets: Vec<DevicePreset>,
}

// ============================================================================
// Production stages & steps
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProductionStatus {
    NotStarted,
    Skipped,
    InProgress,
    NearingCompletion,
    Borked,
    Complete,
    Exceptional,
}

impl ProductionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProductionStatus::NotStarted => "not_started",
            ProductionStatus::Skipped => "skipped",
            ProductionStatus::InProgress => "in_progress",
            ProductionStatus::NearingCompletion => "nearing_completion",
            ProductionStatus::Borked => "borked",
            ProductionStatus::Complete => "complete",
            ProductionStatus::Exceptional => "exceptional",
        }
    }

    pub fn parse(s: &str) -> Option<ProductionStatus> {
        match s {
            "not_started" => Some(ProductionStatus::NotStarted),
            "skipped" => Some(ProductionStatus::Skipped),
            "in_progress" => Some(ProductionStatus::InProgress),
            "nearing_completion" => Some(ProductionStatus::NearingCompletion),
            "borked" => Some(ProductionStatus::Borked),
            "complete" => Some(ProductionStatus::Complete),
            "exceptional" => Some(ProductionStatus::Exceptional),
            _ => None,
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            ProductionStatus::NotStarted => "∅",
            ProductionStatus::Skipped => "🚫",
            ProductionStatus::InProgress => "🔄",
            ProductionStatus::NearingCompletion => "🏁",
            ProductionStatus::Borked => "❤️‍🩹",
            ProductionStatus::Complete => "✅",
            ProductionStatus::Exceptional => "📈",
        }
    }
}

impl std::fmt::Display for ProductionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProductionStage {
    pub id: i64,
    pub song_id: i64,
    pub stage: String,
    pub status: ProductionStatus,
    pub steps: Vec<ProductionStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProductionStep {
    pub id: i64,
    pub stage_id: i64,
    pub instrument_id: Option<i64>,
    pub instrument_name: String,
    pub name: String,
    pub status: ProductionStatus,
    pub sort_order: i32,
    pub notes: String,
}

// ============================================================================
// Song files
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SongFile {
    pub id: i64,
    pub song_id: i64,
    pub file_type: String,
    pub path: String,
    pub instrument_id: Option<i64>,
    pub instrument_name: String,
    pub description: String,
}

// ============================================================================
// Samples
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sample {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub bpm: Option<i32>,
    pub key: String,
    pub description: String,
    pub instruments: Vec<Instrument>,
}

// ============================================================================
// Form structs for create/update operations
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSong {
    pub title: String,
    pub album_id: Option<i64>,
    pub sheet_music: String,
    pub lyrics: String,
    pub song_type: SongType,
    pub key: String,
    pub bpm_lower: Option<i32>,
    pub bpm_upper: Option<i32>,
    pub original_artist: String,
    pub score_url: String,
    pub description: String,
    pub workflow_state: WorkflowState,
    pub scores_folder: String,
    pub export_folder: String,
    pub musicxml_path: String,
    pub artist_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSong {
    pub id: i64,
    pub title: String,
    pub album_id: Option<i64>,
    pub sheet_music: String,
    pub lyrics: String,
    pub key: String,
    pub bpm_lower: Option<i32>,
    pub bpm_upper: Option<i32>,
    pub original_artist: String,
    pub score_url: String,
    pub description: String,
    pub scores_folder: String,
    pub export_folder: String,
    pub musicxml_path: String,
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
    pub instrument_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBand {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecording {
    pub recording_type: RecordingType,
    pub path: String,
    pub song_id: i64,
    pub notes_image: String,
    pub instrument_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDevice {
    pub name: String,
    pub device_type: String,
    pub manual_path: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDevicePreset {
    pub device_id: i64,
    pub name: String,
    pub preset_code: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSongInstrument {
    pub song_id: i64,
    pub instrument_id: i64,
    pub description: String,
    pub score_url: String,
    pub production_path: String,
    pub mastering_path: String,
    pub preset_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProductionStage {
    pub song_id: i64,
    pub stage: String,
    pub status: ProductionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProductionStep {
    pub stage_id: i64,
    pub instrument_id: Option<i64>,
    pub name: String,
    pub status: ProductionStatus,
    pub sort_order: i32,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSongFile {
    pub song_id: i64,
    pub file_type: String,
    pub path: String,
    pub instrument_id: Option<i64>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSample {
    pub name: String,
    pub path: String,
    pub bpm: Option<i32>,
    pub key: String,
    pub description: String,
    pub instrument_ids: Vec<i64>,
}

// ============================================================================
// Workflow state machine for songs
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowState {
    Discovered,
    Learning,
    Shaky,
    Performing,
    Producing,
    CoverRecording,
    Complete,
}

impl WorkflowState {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowState::Discovered => "discovered",
            WorkflowState::Learning => "learning",
            WorkflowState::Shaky => "shaky",
            WorkflowState::Performing => "performing",
            WorkflowState::Producing => "producing",
            WorkflowState::CoverRecording => "cover_recording",
            WorkflowState::Complete => "complete",
        }
    }

    pub fn parse(s: &str) -> Option<WorkflowState> {
        match s {
            "discovered" => Some(WorkflowState::Discovered),
            "learning" => Some(WorkflowState::Learning),
            "shaky" => Some(WorkflowState::Shaky),
            "performing" => Some(WorkflowState::Performing),
            "producing" => Some(WorkflowState::Producing),
            "cover_recording" => Some(WorkflowState::CoverRecording),
            "complete" => Some(WorkflowState::Complete),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            WorkflowState::Discovered => "Discovered",
            WorkflowState::Learning => "Learning",
            WorkflowState::Shaky => "Shaky",
            WorkflowState::Performing => "Performing",
            WorkflowState::Producing => "Producing",
            WorkflowState::CoverRecording => "Cover Recording",
            WorkflowState::Complete => "Complete",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            WorkflowState::Discovered => "🔍",
            WorkflowState::Learning => "📖",
            WorkflowState::Shaky => "🫨",
            WorkflowState::Performing => "🎤",
            WorkflowState::Producing => "🎛️",
            WorkflowState::CoverRecording => "🎙️",
            WorkflowState::Complete => "✅",
        }
    }

    pub fn all() -> &'static [WorkflowState] {
        &[
            WorkflowState::Discovered,
            WorkflowState::Learning,
            WorkflowState::Shaky,
            WorkflowState::Performing,
            WorkflowState::Producing,
            WorkflowState::CoverRecording,
            WorkflowState::Complete,
        ]
    }
}

impl std::fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Practice exercises
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PracticeExercise {
    pub id: i64,
    pub instrument_id: Option<i64>,
    pub instrument_name: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub source: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePracticeExercise {
    pub instrument_id: Option<i64>,
    pub name: String,
    pub category: String,
    pub description: String,
    pub source: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SongExercise {
    pub id: i64,
    pub song_id: i64,
    pub exercise_id: i64,
    pub exercise_name: String,
    pub instrument_name: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSongExercise {
    pub song_id: i64,
    pub exercise_id: i64,
    pub notes: String,
}

// ============================================================================
// User profile
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserProfile {
    pub id: i64,
    pub display_name: String,
    pub songs_capacity: i32,
    pub warmup_minutes: i32,
    pub drill_minutes: i32,
    pub song_minutes: i32,
    pub review_minutes: i32,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserProfile {
    pub display_name: String,
    pub songs_capacity: i32,
    pub warmup_minutes: i32,
    pub drill_minutes: i32,
    pub song_minutes: i32,
    pub review_minutes: i32,
    pub notes: String,
}

// ============================================================================
// Goals — hierarchical planning
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Goal {
    pub id: i64,
    pub horizon: String,
    pub category: String,
    pub title: String,
    pub description: String,
    pub target_date: String,
    pub completed: bool,
    pub created_at: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGoal {
    pub horizon: String,
    pub category: String,
    pub title: String,
    pub description: String,
    pub target_date: String,
    pub sort_order: i32,
}

// ============================================================================
// Schedule events & items
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleEvent {
    pub id: i64,
    pub event_date: String,
    pub title: String,
    pub event_type: String,
    pub status: String,
    pub notes: String,
    pub created_at: String,
    pub items: Vec<ScheduleItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleItem {
    pub id: i64,
    pub event_id: i64,
    pub item_type: String,
    pub song_id: Option<i64>,
    pub song_title: String,
    pub exercise_id: Option<i64>,
    pub exercise_name: String,
    pub stage_id: Option<i64>,
    pub stage_name: String,
    pub instrument_id: Option<i64>,
    pub instrument_name: String,
    pub title: String,
    pub duration_minutes: i32,
    pub sort_order: i32,
    pub completed: bool,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateScheduleEvent {
    pub event_date: String,
    pub title: String,
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateScheduleItem {
    pub event_id: i64,
    pub item_type: String,
    pub song_id: Option<i64>,
    pub exercise_id: Option<i64>,
    pub stage_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub title: String,
    pub duration_minutes: i32,
    pub sort_order: i32,
    pub notes: String,
}
