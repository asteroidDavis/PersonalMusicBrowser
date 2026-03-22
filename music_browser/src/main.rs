use actix_web::{web, App, HttpResponse, HttpServer};
use askama::Template;
use serde::Deserialize;
use sqlx::SqlitePool;

use music_browser::db::models::*;
use music_browser::db::queries;

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
    form: web::Form<SongFormData>,
) -> actix_web::Result<HttpResponse> {
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
    form: web::Form<SongFormData>,
) -> actix_web::Result<HttpResponse> {
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
    form: web::Form<AlbumFormData>,
) -> actix_web::Result<HttpResponse> {
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
    form: web::Form<ArtistFormData>,
) -> actix_web::Result<HttpResponse> {
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
    form: web::Form<InstrumentFormData>,
) -> actix_web::Result<HttpResponse> {
    let it = if form.instrument_type.is_empty() {
        "other".to_string()
    } else {
        form.instrument_type.clone()
    };
    let input = CreateInstrument {
        name: form.name.clone(),
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
    form: web::Form<BandFormData>,
) -> actix_web::Result<HttpResponse> {
    let input = CreateBand {
        name: form.name.clone(),
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
    })
    .bind(&bind)?
    .run()
    .await
}
