use actix_web::dev::Payload;
use actix_web::{web, App, FromRequest, HttpRequest, HttpResponse, HttpServer};
use askama::Template;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sqlx::SqlitePool;

use music_browser::db::models::*;
use music_browser::db::queries;
use music_browser::jobs::{
    check_hydration, JobQueue, JobRecord, JobStatus, JobStore, Operation, TargetType, WorkflowJob,
};

/// Extract the `Referer` header from a request, falling back to `default`.
fn redirect_back(req: &HttpRequest, default: &str) -> HttpResponse {
    let loc = req
        .headers()
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(default)
        .to_string();
    HttpResponse::SeeOther()
        .insert_header(("Location", loc))
        .finish()
}

// ---------------------------------------------------------------------------
// QsForm: a form extractor that uses serde_qs instead of serde_urlencoded.
// This correctly handles repeated keys (checkbox arrays) and treats empty
// strings as None for Option<T> fields.
// ---------------------------------------------------------------------------

pub struct QsForm<T>(pub T);

impl<T: DeserializeOwned + 'static> FromRequest for QsForm<T> {
    type Error = actix_web::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let fut = web::Bytes::from_request(req, payload);
        Box::pin(async move {
            let bytes = fut.await?;
            let qs_config = serde_qs::Config::new(5, false);
            let value: T = qs_config
                .deserialize_bytes(&bytes)
                .map_err(|e| actix_web::error::ErrorBadRequest(format!("Parse error: {e}")))?;
            Ok(QsForm(value))
        })
    }
}

// ---------------------------------------------------------------------------
// Template structs
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "songs.html")]
struct SongsTemplate {
    songs: Vec<SongView>,
}

struct SongView {
    id: i64,
    title: String,
    album_title: String,
    song_type: String,
    sheet_music: String,
    artist_names: String,
}

#[derive(Template)]
#[template(path = "song_form.html")]
struct SongFormTemplate {
    editing: bool,
    title: String,
    album_id: i64,
    song_type: String,
    sheet_music: String,
    lyrics: String,
    key: String,
    bpm_lower: Option<i32>,
    bpm_upper: Option<i32>,
    original_artist: String,
    score_url: String,
    description: String,
    albums: Vec<Album>,
    artists: Vec<ArtistRow>,
    return_to: String,
}

struct ArtistRow {
    id: i64,
    name: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "albums.html")]
struct AlbumsTemplate {
    albums: Vec<Album>,
}

#[derive(Template)]
#[template(path = "album_form.html")]
struct AlbumFormTemplate {
    title: String,
    released: bool,
    url: String,
}

#[derive(Template)]
#[template(path = "artists.html")]
struct ArtistsTemplate {
    artists: Vec<ArtistView>,
}

struct ArtistView {
    id: i64,
    name: String,
    band_names: String,
}

#[derive(Template)]
#[template(path = "artist_form.html")]
struct ArtistFormTemplate {
    name: String,
    bands: Vec<BandRow>,
}

struct BandRow {
    id: i64,
    name: String,
    selected: bool,
}

#[derive(Template)]
#[template(path = "instruments.html")]
struct InstrumentsTemplate {
    instruments: Vec<Instrument>,
}

#[derive(Template)]
#[template(path = "instrument_form.html")]
struct InstrumentFormTemplate {
    name: String,
    instrument_type: String,
}

#[derive(Template)]
#[template(path = "recordings.html")]
struct RecordingsTemplate {
    recordings: Vec<RecordingView>,
}

struct RecordingView {
    id: i64,
    recording_type: String,
    song_id: i64,
    path: String,
    instrument_names: String,
}

#[derive(Template)]
#[template(path = "bands.html")]
struct BandsTemplate {
    bands: Vec<Band>,
}

#[derive(Template)]
#[template(path = "band_form.html")]
struct BandFormTemplate {
    name: String,
}

// ---------------------------------------------------------------------------
// Jobs view structs
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "jobs.html")]
struct JobsTemplate {
    jobs: Vec<JobRowView>,
}

struct JobRowView {
    id: u64,
    status: String,
    status_label: String,
    operation: String,
    target_type: String,
    target: String,
    log_count: usize,
    resolved_paths: Vec<String>,
    output_dir: String,
}

#[derive(Template)]
#[template(path = "job_detail.html")]
struct JobDetailTemplate {
    row: JobRowView,
    log_lines: Vec<String>,
    /// Pre-filled JSON for the re-run form.
    prefill_json: String,
}

fn job_to_row(r: &JobRecord) -> JobRowView {
    let (status, status_label) = match r.status {
        JobStatus::Queued => ("queued", "⏳ Queued"),
        JobStatus::Running => ("running", "⚙️ Running"),
        JobStatus::Done => ("done", "✅ Done"),
        JobStatus::Failed => ("failed", "❌ Failed"),
    };
    JobRowView {
        id: r.job.id,
        status: status.to_string(),
        status_label: status_label.to_string(),
        operation: r.job.operation.as_str().to_string(),
        target_type: format!("{:?}", r.job.target_type).to_lowercase(),
        target: r.job.target_id_or_path.clone(),
        log_count: r.log_lines.len(),
        resolved_paths: r.job.resolved_paths.clone(),
        output_dir: r.job.output_dir.clone().unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// Production & Practice view structs
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "production.html")]
struct ProductionTemplate {
    songs: Vec<ProductionSongView>,
}

struct ProductionSongView {
    song_id: i64,
    title: String,
    song_type: String,
    original_artist: String,
    key: String,
    bpm: String,
    album_title: String,
    stages: Vec<StageView>,
    files: Vec<FileView>,
}

struct StageView {
    id: i64,
    name: String,
    status_str: String,
    steps: Vec<StepView>,
}

struct StepView {
    id: i64,
    name: String,
    status_str: String,
    instrument_name: String,
    notes: String,
}

struct FileView {
    id: i64,
    file_type: String,
    path: String,
    instrument_name: String,
    description: String,
}

#[derive(Template)]
#[template(path = "practice.html")]
struct PracticeTemplate {
    songs: Vec<PracticeSongView>,
}

struct PracticeSongView {
    song_id: i64,
    title: String,
    song_type: String,
    original_artist: String,
    key: String,
    bpm: String,
    time_signature: String,
    practice_project_path: String,
    practice_priority: i32,
    workflow_state: String,
    score_url: String,
    instruments: Vec<SongInstrumentView>,
    files: Vec<FileView>,
}

struct SongInstrumentView {
    instrument_name: String,
    description: String,
    score_url: String,
    production_path: String,
    mastering_path: String,
    presets: Vec<PresetView>,
}

struct PresetView {
    name: String,
    preset_code: String,
    description: String,
}

#[derive(Template)]
#[template(path = "stage_form.html")]
struct StageFormTemplate {
    song_title: String,
}

#[derive(Template)]
#[template(path = "step_form.html")]
struct StepFormTemplate {
    stage_name: String,
    instruments: Vec<Instrument>,
}

#[derive(Template)]
#[template(path = "song_file_form.html")]
struct SongFileFormTemplate {
    song_title: String,
    instruments: Vec<Instrument>,
    return_to: String,
}

// ---------------------------------------------------------------------------
// Form deserialization structs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SongFormData {
    title: String,
    #[serde(default)]
    album_id: Option<i64>,
    song_type: String,
    #[serde(default)]
    sheet_music: String,
    #[serde(default)]
    lyrics: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    bpm_lower: Option<i32>,
    #[serde(default)]
    bpm_upper: Option<i32>,
    #[serde(default)]
    original_artist: String,
    #[serde(default)]
    score_url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    artist_ids: Vec<i64>,
    #[serde(default)]
    return_to: String,
}

#[derive(Deserialize)]
struct AlbumFormData {
    title: String,
    #[serde(default)]
    released: Option<String>,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct ArtistFormData {
    name: String,
    #[serde(default)]
    band_ids: Vec<i64>,
}

#[derive(Deserialize)]
struct InstrumentFormData {
    name: String,
    #[serde(default)]
    instrument_type: String,
}

#[derive(Deserialize)]
struct BandFormData {
    name: String,
}

#[derive(Deserialize)]
struct StageFormData {
    stage: String,
    #[serde(default)]
    status: String,
}

#[derive(Deserialize)]
struct StepFormData {
    name: String,
    #[serde(default)]
    instrument_id: Option<i64>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    sort_order: Option<i32>,
    #[serde(default)]
    notes: String,
}

#[derive(Deserialize)]
struct StatusFormData {
    status: String,
}

#[derive(Deserialize)]
struct SongFileFormData {
    file_type: String,
    path: String,
    #[serde(default)]
    instrument_id: Option<i64>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    return_to: String,
}

// ---------------------------------------------------------------------------
// Utility Functions
// ---------------------------------------------------------------------------

/// Validates that a redirect URL is safe (local path only, not external)
fn is_safe_redirect(url: &str) -> bool {
    if url.is_empty() {
        return true;
    }
    // Only allow relative paths or paths starting with /
    if url.starts_with('/') && !url.starts_with("//") {
        // Ensure no protocol injection (e.g., /\\evil.com or /\evil.com)
        !url.contains("://") && !url.contains("\\")
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Handlers — Songs
// ---------------------------------------------------------------------------

async fn song_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let songs = queries::list_songs(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let views: Vec<SongView> = songs
        .into_iter()
        .map(|s| SongView {
            id: s.id,
            title: s.title,
            album_title: s.album_title,
            song_type: s.song_type.to_string(),
            sheet_music: s.sheet_music,
            artist_names: s
                .artists
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();

    let body = SongsTemplate { songs: views }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn song_new(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let albums = queries::list_albums(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let artists = queries::list_artists(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let body = SongFormTemplate {
        editing: false,
        title: String::new(),
        album_id: 0,
        song_type: "song".into(),
        sheet_music: String::new(),
        lyrics: String::new(),
        key: String::new(),
        bpm_lower: None,
        bpm_upper: None,
        original_artist: String::new(),
        score_url: String::new(),
        description: String::new(),
        albums,
        artists: artists
            .into_iter()
            .map(|a| ArtistRow {
                id: a.id,
                name: a.name,
                selected: false,
            })
            .collect(),
        return_to: String::new(),
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn song_create(
    pool: web::Data<SqlitePool>,
    form: QsForm<SongFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let st = SongType::parse(&form.song_type).unwrap_or(SongType::Song);
    let input = CreateSong {
        title: form.title.clone(),
        album_id: form.album_id,
        sheet_music: form.sheet_music.clone(),
        lyrics: form.lyrics.clone(),
        song_type: st,
        key: form.key.clone(),
        bpm_lower: form.bpm_lower,
        bpm_upper: form.bpm_upper,
        original_artist: form.original_artist.clone(),
        score_url: form.score_url.clone(),
        description: form.description.clone(),
        workflow_state: WorkflowState::Discovered,
        scores_folder: String::new(),
        export_folder: String::new(),
        musicxml_path: String::new(),
        practice_project_path: String::new(),
        time_signature: "4/4".to_string(),
        practice_priority: 0,
        artist_ids: form.artist_ids.clone(),
    };
    queries::create_song(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/"))
        .finish())
}

async fn song_edit(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> actix_web::Result<HttpResponse> {
    let id = path.into_inner();
    let return_to = query.get("return_to").cloned().unwrap_or_default();
    let song = queries::get_song(&pool, id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Song not found"))?;

    let albums = queries::list_albums(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let artists = queries::list_artists(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let selected_artist_ids: Vec<i64> = song.artists.iter().map(|a| a.id).collect();

    let body = SongFormTemplate {
        editing: true,
        title: song.title,
        album_id: song.album_id.unwrap_or(0),
        song_type: song.song_type.to_string(),
        sheet_music: song.sheet_music,
        lyrics: song.lyrics,
        key: song.key,
        bpm_lower: song.bpm_lower,
        bpm_upper: song.bpm_upper,
        original_artist: song.original_artist,
        score_url: song.score_url,
        description: song.description,
        albums,
        artists: artists
            .into_iter()
            .map(|a| {
                let sel = selected_artist_ids.contains(&a.id);
                ArtistRow {
                    id: a.id,
                    name: a.name,
                    selected: sel,
                }
            })
            .collect(),
        return_to,
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn song_update(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<SongFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let song_id = path.into_inner();
    let existing = queries::get_song(&pool, song_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Song not found"))?;
    let st = SongType::parse(&form.song_type).unwrap_or(existing.song_type);
    let input = UpdateSong {
        id: song_id,
        title: form.title.clone(),
        album_id: form.album_id,
        song_type: st,
        sheet_music: form.sheet_music.clone(),
        lyrics: form.lyrics.clone(),
        key: form.key.clone(),
        bpm_lower: form.bpm_lower,
        bpm_upper: form.bpm_upper,
        original_artist: form.original_artist.clone(),
        score_url: form.score_url.clone(),
        description: form.description.clone(),
        scores_folder: existing.scores_folder,
        export_folder: existing.export_folder,
        musicxml_path: existing.musicxml_path,
        practice_project_path: existing.practice_project_path,
        time_signature: existing.time_signature,
        practice_priority: existing.practice_priority,
        artist_ids: form.artist_ids.clone(),
    };
    queries::update_song(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let loc = if form.return_to.is_empty() || !is_safe_redirect(&form.return_to) {
        "/".to_string()
    } else {
        form.return_to.clone()
    };
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", loc))
        .finish())
}

async fn song_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_song(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/"))
        .finish())
}

// ---------------------------------------------------------------------------
// Handlers — Albums
// ---------------------------------------------------------------------------

async fn album_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let albums = queries::list_albums(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = AlbumsTemplate { albums }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn album_new() -> actix_web::Result<HttpResponse> {
    let body = AlbumFormTemplate {
        title: String::new(),
        released: false,
        url: String::new(),
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn album_create(
    pool: web::Data<SqlitePool>,
    form: QsForm<AlbumFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let input = CreateAlbum {
        title: form.title.clone(),
        released: form.released.as_deref() == Some("true"),
        url: form.url.clone(),
    };
    queries::create_album(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/albums"))
        .finish())
}

async fn album_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_album(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/albums"))
        .finish())
}

// ---------------------------------------------------------------------------
// Handlers — Artists
// ---------------------------------------------------------------------------

async fn artist_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let artists = queries::list_artists(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let views: Vec<ArtistView> = artists
        .into_iter()
        .map(|a| ArtistView {
            id: a.id,
            name: a.name,
            band_names: a
                .bands
                .iter()
                .map(|b| b.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();
    let body = ArtistsTemplate { artists: views }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn artist_new(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let bands = queries::list_bands(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = ArtistFormTemplate {
        name: String::new(),
        bands: bands
            .into_iter()
            .map(|b| BandRow {
                id: b.id,
                name: b.name,
                selected: false,
            })
            .collect(),
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn artist_create(
    pool: web::Data<SqlitePool>,
    form: QsForm<ArtistFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let input = CreateArtist {
        name: form.name.clone(),
        band_ids: form.band_ids.clone(),
    };
    queries::create_artist(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/artists"))
        .finish())
}

async fn artist_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_artist(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/artists"))
        .finish())
}

// ---------------------------------------------------------------------------
// Handlers — Instruments
// ---------------------------------------------------------------------------

async fn instrument_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let instruments = queries::list_instruments(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = InstrumentsTemplate { instruments }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn instrument_new() -> actix_web::Result<HttpResponse> {
    let body = InstrumentFormTemplate {
        name: String::new(),
        instrument_type: "other".into(),
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn instrument_create(
    pool: web::Data<SqlitePool>,
    form: QsForm<InstrumentFormData>,
) -> actix_web::Result<HttpResponse> {
    let it = if form.0.instrument_type.is_empty() {
        "other".to_string()
    } else {
        form.0.instrument_type.clone()
    };
    let input = CreateInstrument {
        name: form.0.name.clone(),
        instrument_type: it,
    };
    queries::create_instrument(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/instruments"))
        .finish())
}

async fn instrument_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_instrument(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/instruments"))
        .finish())
}

// ---------------------------------------------------------------------------
// Handlers — Recordings
// ---------------------------------------------------------------------------

async fn recording_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let recordings = queries::list_recordings(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let views: Vec<RecordingView> = recordings
        .into_iter()
        .map(|r| RecordingView {
            id: r.id,
            recording_type: r.recording_type.to_string(),
            song_id: r.song_id,
            path: r.path,
            instrument_names: r
                .instruments
                .iter()
                .map(|i| i.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
        })
        .collect();
    let body = RecordingsTemplate { recordings: views }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn recording_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_recording(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/recordings"))
        .finish())
}

// ---------------------------------------------------------------------------
// Handlers — Bands
// ---------------------------------------------------------------------------

async fn band_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let bands = queries::list_bands(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = BandsTemplate { bands }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn band_new() -> actix_web::Result<HttpResponse> {
    let body = BandFormTemplate {
        name: String::new(),
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn band_create(
    pool: web::Data<SqlitePool>,
    form: QsForm<BandFormData>,
) -> actix_web::Result<HttpResponse> {
    let input = CreateBand {
        name: form.0.name.clone(),
    };
    queries::create_band(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/bands"))
        .finish())
}

async fn band_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_band(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/bands"))
        .finish())
}

// ---------------------------------------------------------------------------
// Handlers — Production
// ---------------------------------------------------------------------------

fn format_bpm(lower: Option<i32>, upper: Option<i32>) -> String {
    match (lower, upper) {
        (Some(l), Some(u)) if l == u => format!("{l}"),
        (Some(l), Some(u)) => format!("{l}–{u}"),
        (Some(l), None) => format!("{l}"),
        (None, Some(u)) => format!("{u}"),
        _ => String::new(),
    }
}

fn song_to_file_views(files: &[SongFile]) -> Vec<FileView> {
    files
        .iter()
        .map(|f| FileView {
            id: f.id,
            file_type: f.file_type.clone(),
            path: f.path.clone(),
            instrument_name: f.instrument_name.clone(),
            description: f.description.clone(),
        })
        .collect()
}

async fn production_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let data = queries::list_all_production_stages(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let mut songs = Vec::new();
    for (song, stages) in data {
        let files = queries::list_song_files(&pool, song.id)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;

        songs.push(ProductionSongView {
            song_id: song.id,
            title: song.title,
            song_type: song.song_type.to_string(),
            original_artist: song.original_artist,
            key: song.key,
            bpm: format_bpm(song.bpm_lower, song.bpm_upper),
            album_title: song.album_title,
            stages: stages
                .iter()
                .map(|st| StageView {
                    id: st.id,
                    name: st.stage.clone(),
                    status_str: st.status.as_str().to_string(),
                    steps: st
                        .steps
                        .iter()
                        .map(|sp| StepView {
                            id: sp.id,
                            name: sp.name.clone(),
                            status_str: sp.status.as_str().to_string(),
                            instrument_name: sp.instrument_name.clone(),
                            notes: sp.notes.clone(),
                        })
                        .collect(),
                })
                .collect(),
            files: song_to_file_views(&files),
        });
    }

    let body = ProductionTemplate { songs }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn stage_new(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    let song_id = path.into_inner();
    let song = queries::get_song(&pool, song_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Song not found"))?;

    let body = StageFormTemplate {
        song_title: song.title,
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn stage_create(
    req: HttpRequest,
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<StageFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let status = ProductionStatus::parse(&form.status).unwrap_or(ProductionStatus::NotStarted);
    let input = CreateProductionStage {
        song_id: path.into_inner(),
        stage: form.stage,
        status,
    };
    queries::create_production_stage(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(redirect_back(&req, "/production"))
}

async fn stage_update_status(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<StatusFormData>,
) -> actix_web::Result<HttpResponse> {
    let status = ProductionStatus::parse(&form.0.status).unwrap_or(ProductionStatus::NotStarted);
    queries::update_production_stage_status(&pool, path.into_inner(), &status)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

async fn stage_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_production_stage(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

async fn step_new(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    let stage_id = path.into_inner();
    let instruments = queries::list_instruments(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    // Fetch stage name for display
    let stages_row = sqlx::query("SELECT stage FROM production_stages WHERE id = ?")
        .bind(stage_id)
        .fetch_optional(pool.get_ref())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let stage_name = stages_row
        .map(|r| sqlx::Row::get::<String, _>(&r, "stage"))
        .unwrap_or_default();

    let body = StepFormTemplate {
        stage_name,
        instruments,
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn step_create(
    req: HttpRequest,
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<StepFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let status = ProductionStatus::parse(&form.status).unwrap_or(ProductionStatus::NotStarted);
    let input = CreateProductionStep {
        stage_id: path.into_inner(),
        instrument_id: form.instrument_id,
        name: form.name,
        status,
        sort_order: form.sort_order.unwrap_or(0),
        notes: form.notes,
    };
    queries::create_production_step(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(redirect_back(&req, "/production"))
}

async fn step_update_status(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<StatusFormData>,
) -> actix_web::Result<HttpResponse> {
    let status = ProductionStatus::parse(&form.0.status).unwrap_or(ProductionStatus::NotStarted);
    queries::update_production_step_status(&pool, path.into_inner(), &status)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

async fn song_file_new(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> actix_web::Result<HttpResponse> {
    let song_id = path.into_inner();
    let return_to = query.get("return_to").cloned().unwrap_or_default();
    let song = queries::get_song(&pool, song_id)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?
        .ok_or_else(|| actix_web::error::ErrorNotFound("Song not found"))?;
    let instruments = queries::list_instruments(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let body = SongFileFormTemplate {
        song_title: song.title,
        instruments,
        return_to,
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn song_file_create(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<SongFileFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let input = CreateSongFile {
        song_id: path.into_inner(),
        file_type: form.file_type,
        path: form.path,
        instrument_id: form.instrument_id,
        description: form.description,
    };
    queries::create_song_file(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let loc = if form.return_to.is_empty() || !is_safe_redirect(&form.return_to) {
        "/production".to_string()
    } else {
        form.return_to.clone()
    };
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", loc))
        .finish())
}

async fn song_file_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_song_file(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

// ---------------------------------------------------------------------------
// Handlers — Auto-populate stages & steps
// ---------------------------------------------------------------------------

async fn auto_add_stages(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::auto_add_stages(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

async fn auto_add_steps(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    let stage_id = path.into_inner();
    // Determine if the song is a cover to pick the right composition steps
    let is_cover: bool = sqlx::query_scalar(
        "SELECT CASE WHEN s.song_type = 'cover' THEN 1 ELSE 0 END \
         FROM production_stages ps JOIN songs s ON s.id = ps.song_id \
         WHERE ps.id = ?",
    )
    .bind(stage_id)
    .fetch_one(pool.get_ref())
    .await
    .map(|v: i32| v == 1)
    .unwrap_or(false);

    queries::auto_add_steps(&pool, stage_id, is_cover)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

// ---------------------------------------------------------------------------
// Handlers — Practice
// ---------------------------------------------------------------------------

async fn practice_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let all_songs = queries::list_songs(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let mut songs = Vec::new();
    for song in all_songs {
        let instruments = queries::list_song_instruments(&pool, song.id)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;
        let files = queries::list_song_files(&pool, song.id)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;

        songs.push(PracticeSongView {
            song_id: song.id,
            title: song.title,
            song_type: song.song_type.to_string(),
            original_artist: song.original_artist,
            key: song.key.clone(),
            bpm: format_bpm(song.bpm_lower, song.bpm_upper),
            time_signature: song.time_signature,
            practice_project_path: song.practice_project_path,
            practice_priority: song.practice_priority,
            workflow_state: song.workflow_state.label().to_string(),
            score_url: song.score_url,
            instruments: instruments
                .iter()
                .map(|si| SongInstrumentView {
                    instrument_name: si.instrument_name.clone(),
                    description: si.description.clone(),
                    score_url: si.score_url.clone(),
                    production_path: si.production_path.clone(),
                    mastering_path: si.mastering_path.clone(),
                    presets: si
                        .presets
                        .iter()
                        .map(|p| PresetView {
                            name: p.name.clone(),
                            preset_code: p.preset_code.clone(),
                            description: p.description.clone(),
                        })
                        .collect(),
                })
                .collect(),
            files: song_to_file_views(&files),
        });
    }

    let body = PracticeTemplate { songs }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

// ---------------------------------------------------------------------------
// Kanban workflow board
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct KanbanColumn {
    state: String,
    label: String,
    emoji: String,
    songs: Vec<KanbanSong>,
}

#[derive(Debug, Clone)]
struct KanbanSong {
    id: i64,
    title: String,
    song_type: String,
    original_artist: String,
    key: String,
    bpm: String,
    scores_folder: String,
    export_folder: String,
    musicxml_path: String,
}

#[derive(Template)]
#[template(path = "kanban.html")]
struct KanbanTemplate {
    columns: Vec<KanbanColumn>,
    all_states: Vec<(String, String)>,
}

async fn kanban_board(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let mut columns = Vec::new();
    for state in WorkflowState::all() {
        let songs = queries::list_songs_by_workflow_state(&pool, state)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;
        columns.push(KanbanColumn {
            state: state.as_str().to_string(),
            label: state.label().to_string(),
            emoji: state.emoji().to_string(),
            songs: songs
                .iter()
                .map(|s| KanbanSong {
                    id: s.id,
                    title: s.title.clone(),
                    song_type: s.song_type.to_string(),
                    original_artist: s.original_artist.clone(),
                    key: s.key.clone(),
                    bpm: format_bpm(s.bpm_lower, s.bpm_upper),
                    scores_folder: s.scores_folder.clone(),
                    export_folder: s.export_folder.clone(),
                    musicxml_path: s.musicxml_path.clone(),
                })
                .collect(),
        });
    }

    let all_states: Vec<(String, String)> = WorkflowState::all()
        .iter()
        .map(|s| (s.as_str().to_string(), s.label().to_string()))
        .collect();

    let body = KanbanTemplate {
        columns,
        all_states,
    }
    .render()
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

#[derive(Deserialize)]
struct WorkflowUpdateForm {
    workflow_state: String,
}

async fn workflow_update(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<WorkflowUpdateForm>,
) -> actix_web::Result<HttpResponse> {
    let song_id = path.into_inner();
    let state = WorkflowState::parse(&form.0.workflow_state)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid workflow state"))?;
    queries::update_workflow_state(&pool, song_id, &state)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/workflow"))
        .finish())
}

// JSON endpoint for drag-and-drop
async fn workflow_update_json(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    body: web::Json<WorkflowUpdateForm>,
) -> actix_web::Result<HttpResponse> {
    let song_id = path.into_inner();
    let state = WorkflowState::parse(&body.workflow_state)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid workflow state"))?;
    queries::update_workflow_state(&pool, song_id, &state)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().json(serde_json::json!({"ok": true})))
}

// ---------------------------------------------------------------------------
// Practice exercises
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "exercises.html")]
struct ExercisesTemplate {
    exercises: Vec<PracticeExercise>,
}

#[derive(Template)]
#[template(path = "exercise_form.html")]
struct ExerciseFormTemplate {}

#[derive(Deserialize)]
struct ExerciseFormData {
    exercise_list: String,
    category: String,
    description: String,
    source: String,
}

async fn exercise_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let exercises = queries::list_exercises(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = ExercisesTemplate { exercises }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn exercise_new(_pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let body = ExerciseFormTemplate {}
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn exercise_create(
    pool: web::Data<SqlitePool>,
    form: QsForm<ExerciseFormData>,
) -> actix_web::Result<HttpResponse> {
    let f = form.0;

    // Parse line-separated exercise names
    let exercise_names: Vec<&str> = f
        .exercise_list
        .lines()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    // Create each exercise with auto-incrementing sort_order
    for (i, name) in exercise_names.iter().enumerate() {
        queries::create_exercise(
            &pool,
            &CreatePracticeExercise {
                instrument_id: None, // No instrument-specific selection
                name: name.to_string(),
                category: f.category.clone(),
                description: f.description.clone(),
                source: f.source.clone(),
                sort_order: i as i32,
            },
        )
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    }

    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/exercises"))
        .finish())
}

async fn exercise_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_exercise(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/exercises"))
        .finish())
}

// ---------------------------------------------------------------------------
// Goals
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "goals.html")]
struct GoalsTemplate {
    goals: Vec<Goal>,
}

#[derive(Template)]
#[template(path = "goal_form.html")]
struct GoalFormTemplate {}

#[derive(Deserialize)]
struct GoalFormData {
    horizon: String,
    category: String,
    title: String,
    description: String,
    target_date: String,
    sort_order: i32,
}

async fn goal_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let goals = queries::list_goals(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = GoalsTemplate { goals }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn goal_new() -> actix_web::Result<HttpResponse> {
    let body = GoalFormTemplate {}
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn goal_create(
    pool: web::Data<SqlitePool>,
    form: QsForm<GoalFormData>,
) -> actix_web::Result<HttpResponse> {
    let f = form.0;
    queries::create_goal(
        &pool,
        &CreateGoal {
            horizon: f.horizon,
            category: f.category,
            title: f.title,
            description: f.description,
            target_date: f.target_date,
            sort_order: f.sort_order,
        },
    )
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/goals"))
        .finish())
}

async fn goal_toggle(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::toggle_goal(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/goals"))
        .finish())
}

async fn goal_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_goal(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/goals"))
        .finish())
}

// ---------------------------------------------------------------------------
// User profile
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "profile.html")]
struct ProfileTemplate {
    profile: UserProfile,
}

#[derive(Deserialize)]
struct ProfileFormData {
    display_name: String,
    songs_capacity: i32,
    warmup_minutes: i32,
    drill_minutes: i32,
    song_minutes: i32,
    review_minutes: i32,
    notes: String,
}

async fn profile_view(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let profile = queries::get_profile(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = ProfileTemplate { profile }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn profile_update(
    pool: web::Data<SqlitePool>,
    form: QsForm<ProfileFormData>,
) -> actix_web::Result<HttpResponse> {
    let f = form.0;
    queries::update_profile(
        &pool,
        &UpdateUserProfile {
            display_name: f.display_name,
            songs_capacity: f.songs_capacity,
            warmup_minutes: f.warmup_minutes,
            drill_minutes: f.drill_minutes,
            song_minutes: f.song_minutes,
            review_minutes: f.review_minutes,
            notes: f.notes,
        },
    )
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/profile"))
        .finish())
}

// ---------------------------------------------------------------------------
// Schedule
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "schedule.html")]
struct ScheduleTemplate {
    events: Vec<ScheduleEvent>,
}

#[derive(Deserialize)]
struct GenerateScheduleForm {
    start_date: String,
    num_blocks: i32,
}

async fn schedule_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let events = queries::list_schedule_events(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = ScheduleTemplate { events }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn schedule_generate(
    pool: web::Data<SqlitePool>,
    form: QsForm<GenerateScheduleForm>,
) -> actix_web::Result<HttpResponse> {
    let f = form.0;
    queries::generate_schedule(&pool, &f.start_date, f.num_blocks)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/schedule"))
        .finish())
}

async fn schedule_item_toggle(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::toggle_schedule_item(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/schedule"))
        .finish())
}

async fn schedule_event_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_schedule_event(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/schedule"))
        .finish())
}

// ---------------------------------------------------------------------------
// ICS export
// ---------------------------------------------------------------------------

async fn schedule_ics_export(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let events = queries::list_schedule_events(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let mut ics = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//PersonalMusicBrowser//EN\r\nCALSCALE:GREGORIAN\r\n");

    for event in &events {
        let date_clean = event.event_date.replace('-', "");
        let total_minutes: i32 = event.items.iter().map(|i| i.duration_minutes).sum();
        let hours = total_minutes / 60;
        let mins = total_minutes % 60;

        ics.push_str("BEGIN:VEVENT\r\n");
        ics.push_str(&format!("UID:event-{}@personalmusicbrowser\r\n", event.id));
        ics.push_str(&format!("DTSTART;VALUE=DATE:{date_clean}\r\n"));
        ics.push_str(&format!("DTEND;VALUE=DATE:{date_clean}\r\n"));
        ics.push_str(&format!("SUMMARY:{}\r\n", event.title.replace(',', "\\,")));

        let mut desc = format!("Duration: {hours}h {mins}m\\n\\n");
        for item in &event.items {
            let check = if item.completed { "✅" } else { "⬜" };
            desc.push_str(&format!(
                "{check} {} ({}min)\\n",
                item.title, item.duration_minutes
            ));
        }
        ics.push_str(&format!("DESCRIPTION:{desc}\r\n"));
        ics.push_str("END:VEVENT\r\n");
    }

    ics.push_str("END:VCALENDAR\r\n");

    Ok(HttpResponse::Ok()
        .content_type("text/calendar; charset=utf-8")
        .insert_header((
            "Content-Disposition",
            "attachment; filename=\"practice-schedule.ics\"",
        ))
        .body(ics))
}

// ---------------------------------------------------------------------------
// Handlers — Live Sets
// ---------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "sets.html")]
struct SetsTemplate {
    sets: Vec<LiveSet>,
    songs: Vec<Song>,
}

#[derive(Template)]
#[template(path = "set_form.html")]
struct SetFormTemplate {}

#[derive(Deserialize)]
struct SetFormData {
    name: String,
    #[serde(default)]
    set_type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    target_duration_seconds: i32,
}

#[derive(Deserialize)]
struct SetSongFormData {
    song_id: i64,
    #[serde(default)]
    sort_order: i32,
    #[serde(default)]
    backing_track_path: String,
    #[serde(default)]
    duration_seconds: i32,
    #[serde(default)]
    transition_notes: String,
}

#[derive(Deserialize)]
struct PriorityFormData {
    priority: i32,
}

async fn set_list(pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let sets = queries::list_live_sets(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let songs = queries::list_songs(&pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let body = SetsTemplate { sets, songs }
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn set_new(_pool: web::Data<SqlitePool>) -> actix_web::Result<HttpResponse> {
    let body = SetFormTemplate {}
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn set_create(
    pool: web::Data<SqlitePool>,
    form: QsForm<SetFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let input = CreateLiveSet {
        name: form.name,
        set_type: if form.set_type.is_empty() {
            "live".to_string()
        } else {
            form.set_type
        },
        description: form.description,
        target_duration_seconds: form.target_duration_seconds,
    };
    queries::create_live_set(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/sets"))
        .finish())
}

async fn set_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_live_set(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/sets"))
        .finish())
}

async fn set_add_song(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<SetSongFormData>,
) -> actix_web::Result<HttpResponse> {
    let form = form.0;
    let input = CreateLiveSetSong {
        set_id: path.into_inner(),
        song_id: form.song_id,
        sort_order: form.sort_order,
        backing_track_path: form.backing_track_path,
        duration_seconds: form.duration_seconds,
        transition_notes: form.transition_notes,
    };
    queries::add_song_to_set(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/sets"))
        .finish())
}

async fn set_remove_song(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::remove_song_from_set(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/sets"))
        .finish())
}

async fn practice_priority_update(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
    form: QsForm<PriorityFormData>,
) -> actix_web::Result<HttpResponse> {
    let priority = form.0.priority.clamp(0, 5);
    queries::update_practice_priority(&pool, path.into_inner(), priority)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::NoContent().finish())
}

// ---------------------------------------------------------------------------
// Handlers — Job monitor
// ---------------------------------------------------------------------------

async fn jobs_list(store: web::Data<JobStore>) -> actix_web::Result<HttpResponse> {
    let jobs: Vec<JobRowView> = store.list().iter().map(job_to_row).collect();
    let tmpl = JobsTemplate { jobs };
    let body = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn job_detail(
    store: web::Data<JobStore>,
    path: web::Path<u64>,
) -> actix_web::Result<HttpResponse> {
    let id = path.into_inner();
    let record = store
        .get(id)
        .ok_or_else(|| actix_web::error::ErrorNotFound("job not found"))?;
    let prefill_json = serde_json::to_string_pretty(&serde_json::json!({
        "target_type": format!("{:?}", record.job.target_type).to_lowercase(),
        "target_id_or_path": record.job.target_id_or_path,
        "operation": record.job.operation.as_str(),
    }))
    .unwrap_or_default();
    let tmpl = JobDetailTemplate {
        log_lines: record.log_lines.clone(),
        prefill_json,
        row: job_to_row(&record),
    };
    let body = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

// ---------------------------------------------------------------------------
// Handlers — Workflow jobs API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct WorkflowRequest {
    target_type: String,
    target_id_or_path: String,
    operation: String,
}

async fn workflows_enqueue(
    pool: web::Data<SqlitePool>,
    queue: web::Data<JobQueue>,
    body: web::Json<WorkflowRequest>,
) -> actix_web::Result<HttpResponse> {
    let target_type = match body.target_type.as_str() {
        "song" => TargetType::Song,
        "live_set" => TargetType::LiveSet,
        "file" => TargetType::File,
        "directory" => TargetType::Directory,
        other => {
            return Err(actix_web::error::ErrorBadRequest(format!(
                "unknown target_type: {other}"
            )))
        }
    };

    let operation = Operation::parse(&body.operation).ok_or_else(|| {
        actix_web::error::ErrorBadRequest(format!("unknown operation: {}", body.operation))
    })?;

    let (resolved_paths, output_dir) =
        resolve_paths(&pool, &target_type, &operation, &body.target_id_or_path).await?;

    let job = WorkflowJob {
        id: 0,
        target_type,
        target_id_or_path: body.target_id_or_path.clone(),
        operation,
        resolved_paths,
        output_dir,
    };

    let job_id = queue
        .enqueue(job)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Accepted().json(serde_json::json!({
        "ok": true,
        "job_id": job_id
    })))
}

/// Resolve a target to a list of subprocess input paths and an optional
/// `--output-dir` for the `music-operations` CLI.
///
/// The semantics are operation-aware:
///
/// | Target           | Operation            | Inputs                                      | Output dir           |
/// |------------------|----------------------|---------------------------------------------|----------------------|
/// | `Song`           | GenerateSheetMusic   | every `.wav` found under `export_folder`    | `scores_folder`      |
/// | `Song`           | Repomix              | `practice_project_path` (fallback: export)  | `export_folder`      |
/// | `Song`           | Hitpoints            | `export_folder`                             | `export_folder`      |
/// | `LiveSet`        | *                    | live-set name (opaque, handled by CLI)      | `None`               |
/// | `File`/`Directory` | *                  | the path itself (after hydration check)     | `None` (uses parent) |
async fn resolve_paths(
    pool: &SqlitePool,
    target_type: &TargetType,
    operation: &Operation,
    target_id_or_path: &str,
) -> actix_web::Result<(Vec<String>, Option<String>)> {
    match target_type {
        TargetType::Song => {
            let id: i64 = target_id_or_path
                .parse()
                .map_err(|_| actix_web::error::ErrorBadRequest("song id must be numeric"))?;
            let song = queries::get_song(pool, id)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?
                .ok_or_else(|| actix_web::error::ErrorNotFound("song not found"))?;

            resolve_song_paths(&song, operation)
        }
        TargetType::LiveSet => {
            let id: i64 = target_id_or_path
                .parse()
                .map_err(|_| actix_web::error::ErrorBadRequest("live_set id must be numeric"))?;
            let set = queries::get_live_set(pool, id)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?
                .ok_or_else(|| actix_web::error::ErrorNotFound("live_set not found"))?;
            Ok((vec![set.name.clone()], None))
        }
        TargetType::File | TargetType::Directory => {
            let path = std::path::Path::new(target_id_or_path);
            match check_hydration(path) {
                music_browser::jobs::HydrationStatus::NotFound => {
                    Err(actix_web::error::ErrorUnprocessableEntity(format!(
                        "path not found on disk: {target_id_or_path}"
                    )))
                }
                music_browser::jobs::HydrationStatus::Placeholder => {
                    Err(actix_web::error::ErrorUnprocessableEntity(format!(
                        "file is a cloud placeholder (not hydrated): {target_id_or_path}"
                    )))
                }
                music_browser::jobs::HydrationStatus::Hydrated => {
                    Ok((vec![target_id_or_path.to_string()], None))
                }
            }
        }
    }
}

/// Operation-aware resolution for `Song` targets.
fn resolve_song_paths(
    song: &music_browser::db::models::Song,
    operation: &Operation,
) -> actix_web::Result<(Vec<String>, Option<String>)> {
    let non_empty = |s: &str| {
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    };

    match operation {
        Operation::GenerateSheetMusic => {
            // AnthemScore needs a .wav file.  Scan the configured export
            // folder for audio; if it is itself a file, use it directly.
            let export = non_empty(&song.export_folder).ok_or_else(|| {
                actix_web::error::ErrorUnprocessableEntity(
                    "song has no export_folder configured — required for generate_sheet_music",
                )
            })?;
            let inputs = find_wav_inputs(&export)?;
            if inputs.is_empty() {
                return Err(actix_web::error::ErrorUnprocessableEntity(format!(
                    "no .wav files found at or under {export}"
                )));
            }
            let output = non_empty(&song.scores_folder).unwrap_or(export);
            Ok((inputs, Some(output)))
        }
        Operation::Repomix => {
            let input = non_empty(&song.practice_project_path)
                .or_else(|| non_empty(&song.export_folder))
                .ok_or_else(|| {
                    actix_web::error::ErrorUnprocessableEntity(
                        "song has no practice_project_path or export_folder to pack",
                    )
                })?;
            let output = non_empty(&song.export_folder).unwrap_or_else(|| input.clone());
            Ok((vec![input], Some(output)))
        }
        Operation::Hitpoints => {
            let input = non_empty(&song.export_folder).ok_or_else(|| {
                actix_web::error::ErrorUnprocessableEntity(
                    "song has no export_folder configured — required for hitpoints",
                )
            })?;
            let output = input.clone();
            Ok((vec![input], Some(output)))
        }
    }
}

/// Return `[path]` when `root` is an existing `.wav` file, or all `.wav`
/// files directly inside `root` when it is a directory.  Does not recurse.
fn find_wav_inputs(root: &str) -> actix_web::Result<Vec<String>> {
    let p = std::path::Path::new(root);
    let meta = std::fs::metadata(p).map_err(|e| {
        actix_web::error::ErrorUnprocessableEntity(format!("cannot stat {root}: {e}"))
    })?;
    if meta.is_file() {
        return Ok(vec![root.to_string()]);
    }
    let read = std::fs::read_dir(p).map_err(|e| {
        actix_web::error::ErrorUnprocessableEntity(format!("cannot read {root}: {e}"))
    })?;
    let mut out: Vec<String> = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        let is_wav = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("wav"))
            .unwrap_or(false);
        if is_wav && path.is_file() {
            if let Some(s) = path.to_str() {
                out.push(s.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn configure_app(cfg: &mut web::ServiceConfig) {
    cfg
        // Songs
        .route("/", web::get().to(song_list))
        .route("/songs/new", web::get().to(song_new))
        .route("/songs/new", web::post().to(song_create))
        .route("/songs/{id}/edit", web::get().to(song_edit))
        .route("/songs/{id}/edit", web::post().to(song_update))
        .route("/songs/{id}/delete", web::post().to(song_delete))
        // Albums
        .route("/albums", web::get().to(album_list))
        .route("/albums/new", web::get().to(album_new))
        .route("/albums/new", web::post().to(album_create))
        .route("/albums/{id}/delete", web::post().to(album_delete))
        // Artists
        .route("/artists", web::get().to(artist_list))
        .route("/artists/new", web::get().to(artist_new))
        .route("/artists/new", web::post().to(artist_create))
        .route("/artists/{id}/delete", web::post().to(artist_delete))
        // Instruments
        .route("/instruments", web::get().to(instrument_list))
        .route("/instruments/new", web::get().to(instrument_new))
        .route("/instruments/new", web::post().to(instrument_create))
        .route(
            "/instruments/{id}/delete",
            web::post().to(instrument_delete),
        )
        // Recordings
        .route("/recordings", web::get().to(recording_list))
        .route("/recordings/{id}/delete", web::post().to(recording_delete))
        // Bands
        .route("/bands", web::get().to(band_list))
        .route("/bands/new", web::get().to(band_new))
        .route("/bands/new", web::post().to(band_create))
        .route("/bands/{id}/delete", web::post().to(band_delete))
        // Production
        .route("/production", web::get().to(production_list))
        .route(
            "/production/songs/{id}/stages/new",
            web::get().to(stage_new),
        )
        .route(
            "/production/songs/{id}/stages/new",
            web::post().to(stage_create),
        )
        .route(
            "/production/stages/{id}/status",
            web::post().to(stage_update_status),
        )
        .route(
            "/production/stages/{id}/delete",
            web::post().to(stage_delete),
        )
        .route("/production/stages/{id}/steps/new", web::get().to(step_new))
        .route(
            "/production/stages/{id}/steps/new",
            web::post().to(step_create),
        )
        .route(
            "/production/steps/{id}/status",
            web::post().to(step_update_status),
        )
        .route(
            "/production/songs/{id}/files/new",
            web::get().to(song_file_new),
        )
        .route(
            "/production/songs/{id}/files/new",
            web::post().to(song_file_create),
        )
        .route(
            "/production/files/{id}/delete",
            web::post().to(song_file_delete),
        )
        // Auto-populate stages & steps
        .route(
            "/production/songs/{id}/stages/auto",
            web::post().to(auto_add_stages),
        )
        .route(
            "/production/stages/{id}/steps/auto",
            web::post().to(auto_add_steps),
        )
        // Practice
        .route("/practice", web::get().to(practice_list))
        // Kanban workflow board
        .route("/workflow", web::get().to(kanban_board))
        .route(
            "/workflow/songs/{id}/state",
            web::post().to(workflow_update),
        )
        .route(
            "/api/workflow/songs/{id}/state",
            web::put().to(workflow_update_json),
        )
        // Exercises
        .route("/exercises", web::get().to(exercise_list))
        .route("/exercises/new", web::get().to(exercise_new))
        .route("/exercises/new", web::post().to(exercise_create))
        .route("/exercises/{id}/delete", web::post().to(exercise_delete))
        // Goals
        .route("/goals", web::get().to(goal_list))
        .route("/goals/new", web::get().to(goal_new))
        .route("/goals/new", web::post().to(goal_create))
        .route("/goals/{id}/toggle", web::post().to(goal_toggle))
        .route("/goals/{id}/delete", web::post().to(goal_delete))
        // Profile
        .route("/profile", web::get().to(profile_view))
        .route("/profile", web::post().to(profile_update))
        // Schedule
        .route("/schedule", web::get().to(schedule_list))
        .route("/schedule/generate", web::post().to(schedule_generate))
        .route(
            "/schedule/items/{id}/toggle",
            web::post().to(schedule_item_toggle),
        )
        .route(
            "/schedule/events/{id}/delete",
            web::post().to(schedule_event_delete),
        )
        .route("/schedule/export.ics", web::get().to(schedule_ics_export))
        // Live Sets
        .route("/sets", web::get().to(set_list))
        .route("/sets/new", web::get().to(set_new))
        .route("/sets/new", web::post().to(set_create))
        .route("/sets/{id}/delete", web::post().to(set_delete))
        .route("/sets/{id}/songs", web::post().to(set_add_song))
        .route("/sets/songs/{id}/remove", web::post().to(set_remove_song))
        // Practice priority
        .route(
            "/practice/songs/{id}/priority",
            web::post().to(practice_priority_update),
        )
        // Workflow jobs API
        .service(
            web::resource("/api/workflows")
                .route(
                    web::post()
                        .guard(actix_web::guard::fn_guard(|req| {
                            req.head()
                                .headers
                                .get("content-type")
                                .and_then(|h| h.to_str().ok())
                                .map(|s| s.starts_with("multipart/form-data"))
                                .unwrap_or(false)
                        }))
                        .to(workflows_enqueue_upload),
                )
                .route(web::post().to(workflows_enqueue)),
        )
        // Job monitor UI
        .route("/jobs", web::get().to(jobs_list))
        .route("/jobs/{id}", web::get().to(job_detail));
}

// ---------------------------------------------------------------------------
// App bootstrap
// ---------------------------------------------------------------------------

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:music_browser.db".into());

    let pool = music_browser::db::pool::init_pool(&database_url)
        .await
        .expect("Failed to initialise database");

    let pool_data = web::Data::new(pool);
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());

    let (job_queue, job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    tokio::spawn(music_browser::jobs::run_worker(
        job_receiver,
        job_store.clone(),
    ));
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    log::info!("Listening on http://{bind}");

    HttpServer::new(move || {
        App::new()
            .app_data(pool_data.clone())
            .app_data(queue_data.clone())
            .app_data(store_data.clone())
            .configure(configure_app)
    })
    .bind(&bind)?
    .run()
    .await
}

use actix_multipart::form::MultipartForm;

#[derive(actix_multipart::form::MultipartForm)]
struct WorkflowUploadForm {
    #[multipart(rename = "target_type")]
    target_type: actix_multipart::form::text::Text<String>,
    #[multipart(rename = "target_id_or_path")]
    target_id_or_path: actix_multipart::form::text::Text<String>,
    #[multipart(rename = "operation")]
    operation: actix_multipart::form::text::Text<String>,
    #[multipart(rename = "audio_file")]
    audio_file: Option<actix_multipart::form::tempfile::TempFile>,
}

async fn workflows_enqueue_upload(
    pool: web::Data<sqlx::SqlitePool>,
    queue: web::Data<JobQueue>,
    MultipartForm(form): MultipartForm<WorkflowUploadForm>,
) -> actix_web::Result<HttpResponse> {
    let target_type_str = form.target_type.into_inner();
    let target_type = match target_type_str.as_str() {
        "song" => TargetType::Song,
        "live_set" => TargetType::LiveSet,
        "file" => TargetType::File,
        "directory" => TargetType::Directory,
        other => {
            return Err(actix_web::error::ErrorBadRequest(format!(
                "unknown target_type: {other}"
            )))
        }
    };

    let op_str = form.operation.into_inner();
    let operation = Operation::parse(&op_str).ok_or_else(|| {
        actix_web::error::ErrorBadRequest(format!("unknown operation: {}", op_str))
    })?;

    let mut target_id_or_path = form.target_id_or_path.into_inner();

    if let Some(file) = form.audio_file {
        let temp_dir = std::env::temp_dir().join("pmb_uploads");
        std::fs::create_dir_all(&temp_dir)?;
        let file_name = file.file_name.unwrap_or_else(|| "upload.wav".to_string());
        let dest = temp_dir.join(format!("{}_{}", uuid::Uuid::new_v4(), file_name));
        file.file
            .persist(&dest)
            .map_err(actix_web::error::ErrorInternalServerError)?;
        target_id_or_path = dest.to_string_lossy().to_string();
    }

    let (resolved_paths, output_dir) =
        resolve_paths(&pool, &target_type, &operation, &target_id_or_path).await?;

    let job = WorkflowJob {
        id: 0,
        target_type,
        target_id_or_path,
        operation,
        resolved_paths,
        output_dir,
    };

    let job_id = queue
        .enqueue(job)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Accepted().json(serde_json::json!({
        "ok": true,
        "job_id": job_id
    })))
}
