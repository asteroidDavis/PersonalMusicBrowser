use actix_web::dev::Payload;
use actix_web::{web, App, FromRequest, HttpRequest, HttpResponse, HttpServer};
use askama::Template;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sqlx::SqlitePool;

use music_browser::db::models::*;
use music_browser::db::queries;

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
) -> actix_web::Result<HttpResponse> {
    let id = path.into_inner();
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
    let input = UpdateSong {
        id: path.into_inner(),
        title: form.title.clone(),
        album_id: form.album_id,
        sheet_music: form.sheet_music.clone(),
        lyrics: form.lyrics.clone(),
        key: form.key.clone(),
        bpm_lower: form.bpm_lower,
        bpm_upper: form.bpm_upper,
        original_artist: form.original_artist.clone(),
        score_url: form.score_url.clone(),
        description: form.description.clone(),
        artist_ids: form.artist_ids.clone(),
    };
    queries::update_song(&pool, &input)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/"))
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
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/production"))
        .finish())
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
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/production"))
        .finish())
}

async fn stage_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_production_stage(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/production"))
        .finish())
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
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/production"))
        .finish())
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
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/production"))
        .finish())
}

async fn song_file_new(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    let song_id = path.into_inner();
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
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/production"))
        .finish())
}

async fn song_file_delete(
    pool: web::Data<SqlitePool>,
    path: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    queries::delete_song_file(&pool, path.into_inner())
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::SeeOther()
        .insert_header(("Location", "/production"))
        .finish())
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
            key: song.key,
            bpm: format_bpm(song.bpm_lower, song.bpm_upper),
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
// App configuration (shared between main and tests)
// ---------------------------------------------------------------------------

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
        // Practice
        .route("/practice", web::get().to(practice_list));
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

    log::info!("Listening on http://{bind}");

    HttpServer::new(move || {
        App::new()
            .app_data(pool_data.clone())
            .configure(configure_app)
    })
    .bind(&bind)?
    .run()
    .await
}
