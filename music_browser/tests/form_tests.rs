use actix_web::dev::Payload;
use actix_web::{test, web, App, FromRequest, HttpRequest};
use serde::de::DeserializeOwned;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use tempfile::NamedTempFile;

use music_browser::db::models::*;
use music_browser::db::queries;

// ---------------------------------------------------------------------------
// QsForm extractor (mirrors the one in main.rs since we can't import from
// the binary crate). Uses serde_qs to properly handle repeated form keys
// and empty string → None conversions.
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
// Form structs and handlers for testing (mirrors main.rs)
// ---------------------------------------------------------------------------

mod app {
    use super::QsForm;
    use actix_web::{web, HttpResponse};
    use serde::Deserialize;
    use sqlx::SqlitePool;

    use music_browser::db::models::*;
    use music_browser::db::queries;

    #[derive(Deserialize)]
    pub struct ArtistFormData {
        name: String,
        #[serde(default)]
        band_ids: Vec<i64>,
    }

    #[derive(Deserialize)]
    pub struct SongFormData {
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

    pub async fn artist_create(
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

    pub async fn song_create(
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

    pub fn configure(cfg: &mut web::ServiceConfig) {
        cfg.route("/artists/new", web::post().to(artist_create))
            .route("/songs/new", web::post().to(song_create));
    }
}

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

// ===========================================================================
// Bug 1: Creating an artist in a band → "expected a sequence"
//
// Gherkin:
//   Given I have any band
//   When I create an artist in the band
//   Then I can find the band in that artist's page
// ===========================================================================

#[actix_web::test]
async fn test_create_artist_with_single_band_does_not_error() {
    let (pool, _tmp) = setup_pool().await;

    // Given: I have any band
    let band_id = sqlx::query("INSERT INTO bands (name) VALUES ('The Midnight')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    let pool_data = web::Data::new(pool.clone());
    let app = test::init_service(App::new().app_data(pool_data).configure(app::configure)).await;

    // When: I create an artist in the band (single checkbox → single value, not an array)
    let req = test::TestRequest::post()
        .uri("/artists/new")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload(format!("name=Tim+McEwan&band_ids[0]={band_id}"))
        .to_request();

    let resp = test::call_service(&app, req).await;

    // Then: the response is a redirect (303), not an error
    assert_eq!(
        resp.status().as_u16(),
        303,
        "Expected 303 redirect after creating artist with one band, got {}. \
         This indicates the serde 'expected a sequence' bug is back.",
        resp.status()
    );

    // And: I can find the band in that artist's data
    let artists = queries::list_artists(&pool).await.unwrap();
    let artist = artists
        .iter()
        .find(|a| a.name == "Tim McEwan")
        .expect("Artist 'Tim McEwan' should exist after form submission");

    assert!(
        artist.bands.iter().any(|b| b.id == band_id),
        "Artist should belong to band id={band_id}, but bands are: {:?}",
        artist.bands
    );
}

#[actix_web::test]
async fn test_create_artist_with_multiple_bands() {
    let (pool, _tmp) = setup_pool().await;

    // Given: two bands exist
    let b1 = sqlx::query("INSERT INTO bands (name) VALUES ('Band A')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
    let b2 = sqlx::query("INSERT INTO bands (name) VALUES ('Band B')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    let pool_data = web::Data::new(pool.clone());
    let app = test::init_service(App::new().app_data(pool_data).configure(app::configure)).await;

    // When: I create an artist selecting both bands (multiple values)
    let req = test::TestRequest::post()
        .uri("/artists/new")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload(format!(
            "name=Multi+Artist&band_ids[0]={b1}&band_ids[1]={b2}"
        ))
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status().as_u16(),
        303,
        "Expected 303 redirect, got {}",
        resp.status()
    );

    let artists = queries::list_artists(&pool).await.unwrap();
    let artist = artists
        .iter()
        .find(|a| a.name == "Multi Artist")
        .expect("Artist 'Multi Artist' should exist");
    assert_eq!(
        artist.bands.len(),
        2,
        "Artist should belong to 2 bands, got {:?}",
        artist.bands
    );
}

#[actix_web::test]
async fn test_create_artist_without_band() {
    let (pool, _tmp) = setup_pool().await;

    let pool_data = web::Data::new(pool.clone());
    let app = test::init_service(App::new().app_data(pool_data).configure(app::configure)).await;

    // When: I create an artist with no band selected (no band_ids field at all)
    let req = test::TestRequest::post()
        .uri("/artists/new")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload("name=Solo+Artist")
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status().as_u16(),
        303,
        "Expected 303 redirect, got {}",
        resp.status()
    );

    let artists = queries::list_artists(&pool).await.unwrap();
    let artist = artists
        .iter()
        .find(|a| a.name == "Solo Artist")
        .expect("Artist 'Solo Artist' should exist");
    assert!(
        artist.bands.is_empty(),
        "Artist should have no bands, got {:?}",
        artist.bands
    );
}

// ===========================================================================
// Bug 2: Song form with empty optional fields → "cannot parse integer from
// empty string" / "expected a sequence"
//
// Gherkin:
//   Given I have an artist
//   When I create a song with that artist and leave bpm, album_id empty
//   Then the song is created without errors
//   And the song has null bpm and null album_id
//   And the song is linked to the artist
// ===========================================================================

#[actix_web::test]
async fn test_create_song_with_empty_optional_fields() {
    let (pool, _tmp) = setup_pool().await;

    let pool_data = web::Data::new(pool.clone());
    let app = test::init_service(App::new().app_data(pool_data).configure(app::configure)).await;

    // When: I submit a song with only required fields, leaving bpm and album_id empty
    // (This is exactly what the browser sends: album_id= and bpm_lower= and bpm_upper=)
    let req = test::TestRequest::post()
        .uri("/songs/new")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload(
            "title=Empty+Fields+Song&album_id=&song_type=cover&sheet_music=\
             &lyrics=&key=&bpm_lower=&bpm_upper=&original_artist=&score_url=\
             &description=",
        )
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status().as_u16(),
        303,
        "Expected 303 redirect when all optional fields are empty, got {}. \
         This indicates the 'cannot parse integer from empty string' bug is back.",
        resp.status()
    );

    let songs = queries::list_songs(&pool).await.unwrap();
    let song = songs
        .iter()
        .find(|s| s.title == "Empty Fields Song")
        .expect("Song 'Empty Fields Song' should exist after form submission");

    assert_eq!(
        song.album_id, None,
        "album_id should be None when submitted empty, got {:?}",
        song.album_id
    );
    assert_eq!(
        song.bpm_lower, None,
        "bpm_lower should be None when submitted empty, got {:?}",
        song.bpm_lower
    );
    assert_eq!(
        song.bpm_upper, None,
        "bpm_upper should be None when submitted empty, got {:?}",
        song.bpm_upper
    );
}

#[actix_web::test]
async fn test_create_song_with_single_artist_does_not_error() {
    let (pool, _tmp) = setup_pool().await;

    // Given: an artist exists
    let artist_id = sqlx::query("INSERT INTO artists (name) VALUES ('Test Singer')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    let pool_data = web::Data::new(pool.clone());
    let app = test::init_service(App::new().app_data(pool_data).configure(app::configure)).await;

    // When: I create a song selecting one artist (single checkbox value)
    let req = test::TestRequest::post()
        .uri("/songs/new")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload(format!(
            "title=Artist+Song&album_id=&song_type=cover&sheet_music=\
             &lyrics=&key=Am&bpm_lower=120&bpm_upper=130&original_artist=Original\
             &score_url=&description=test+desc&artist_ids[0]={artist_id}"
        ))
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status().as_u16(),
        303,
        "Expected 303 redirect when creating song with one artist, got {}. \
         This indicates the 'expected a sequence' bug for artist_ids is back.",
        resp.status()
    );

    let song = queries::get_song(&pool, 1)
        .await
        .unwrap()
        .expect("Song should exist");
    assert_eq!(song.title, "Artist Song");
    assert_eq!(song.bpm_lower, Some(120), "bpm_lower should be 120");
    assert_eq!(song.bpm_upper, Some(130), "bpm_upper should be 130");
    assert!(
        song.artists.iter().any(|a| a.id == artist_id),
        "Song should be linked to artist id={artist_id}, but artists are: {:?}",
        song.artists
    );
}

#[actix_web::test]
async fn test_create_song_with_all_fields_filled() {
    let (pool, _tmp) = setup_pool().await;

    // Given: an album and two artists
    let album_id = sqlx::query(
        "INSERT INTO albums (title, released, url) VALUES ('Test Album', 1, 'https://example.com')",
    )
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();

    let a1 = sqlx::query("INSERT INTO artists (name) VALUES ('Artist A')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
    let a2 = sqlx::query("INSERT INTO artists (name) VALUES ('Artist B')")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

    let pool_data = web::Data::new(pool.clone());
    let app = test::init_service(App::new().app_data(pool_data).configure(app::configure)).await;

    // When: all fields including album, bpm, and multiple artists
    let req = test::TestRequest::post()
        .uri("/songs/new")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload(format!(
            "title=Full+Song&album_id={album_id}&song_type=original\
             &sheet_music=path.pdf&lyrics=lyrics.txt&key=C+major&bpm_lower=90\
             &bpm_upper=100&original_artist=Me&score_url=https://score.com\
             &description=Full+desc&artist_ids[0]={a1}&artist_ids[1]={a2}"
        ))
        .to_request();

    let resp = test::call_service(&app, req).await;

    assert_eq!(
        resp.status().as_u16(),
        303,
        "Expected 303 redirect, got {}",
        resp.status()
    );

    let songs = queries::list_songs(&pool).await.unwrap();
    let song = songs
        .iter()
        .find(|s| s.title == "Full Song")
        .expect("Song 'Full Song' should exist");

    assert_eq!(song.album_id, Some(album_id), "album_id mismatch");
    assert_eq!(song.bpm_lower, Some(90), "bpm_lower mismatch");
    assert_eq!(song.bpm_upper, Some(100), "bpm_upper mismatch");
    assert_eq!(
        song.artists.len(),
        2,
        "Song should have 2 artists, got {:?}",
        song.artists
    );
}

// ===========================================================================
// Issue #19: Inline actions on Production and Practice pages must return
// 204 No Content (not a redirect) so the browser JS can handle them without
// scrolling the page back to the top.
//
// Gherkin:
//   Given a song with a production stage and step exists
//   When I POST to update the stage status / step status / practice priority
//   Then the response is 204 No Content (no redirect)
//   And when I POST to delete a stage or auto-add stages/steps
//   Then the response is 204 No Content (JS reloads at saved scroll position)
// ===========================================================================

mod inline_actions {
    use super::*;
    use actix_web::{web, HttpResponse};
    use serde::Deserialize;
    use sqlx::SqlitePool;

    #[derive(Deserialize)]
    struct StatusForm {
        status: String,
    }

    #[derive(Deserialize)]
    struct PriorityForm {
        priority: i32,
    }

    async fn stage_update_status(
        pool: web::Data<SqlitePool>,
        path: web::Path<i64>,
        form: QsForm<StatusForm>,
    ) -> actix_web::Result<HttpResponse> {
        let status =
            ProductionStatus::parse(&form.0.status).unwrap_or(ProductionStatus::NotStarted);
        queries::update_production_stage_status(&pool, path.into_inner(), &status)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;
        Ok(HttpResponse::NoContent().finish())
    }

    async fn step_update_status(
        pool: web::Data<SqlitePool>,
        path: web::Path<i64>,
        form: QsForm<StatusForm>,
    ) -> actix_web::Result<HttpResponse> {
        let status =
            ProductionStatus::parse(&form.0.status).unwrap_or(ProductionStatus::NotStarted);
        queries::update_production_step_status(&pool, path.into_inner(), &status)
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

    async fn auto_add_stages(
        pool: web::Data<SqlitePool>,
        path: web::Path<i64>,
    ) -> actix_web::Result<HttpResponse> {
        queries::auto_add_stages(&pool, path.into_inner())
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;
        Ok(HttpResponse::NoContent().finish())
    }

    async fn practice_priority_update(
        pool: web::Data<SqlitePool>,
        path: web::Path<i64>,
        form: QsForm<PriorityForm>,
    ) -> actix_web::Result<HttpResponse> {
        let priority = form.0.priority.clamp(0, 5);
        queries::update_practice_priority(&pool, path.into_inner(), priority)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;
        Ok(HttpResponse::NoContent().finish())
    }

    pub fn configure(cfg: &mut web::ServiceConfig) {
        cfg.route(
            "/production/stages/{id}/status",
            web::post().to(stage_update_status),
        )
        .route(
            "/production/steps/{id}/status",
            web::post().to(step_update_status),
        )
        .route(
            "/production/stages/{id}/delete",
            web::post().to(stage_delete),
        )
        .route(
            "/production/songs/{id}/stages/auto",
            web::post().to(auto_add_stages),
        )
        .route(
            "/practice/songs/{id}/priority",
            web::post().to(practice_priority_update),
        );
    }

    async fn make_song_with_stage(pool: &SqlitePool) -> (i64, i64) {
        let song_id =
            sqlx::query("INSERT INTO songs (title, song_type) VALUES ('Test Song', 'song')")
                .execute(pool)
                .await
                .unwrap()
                .last_insert_rowid();
        let stage_id = sqlx::query(
            "INSERT INTO production_stages (song_id, stage, status) VALUES (?, 'Mixing', 'not_started')",
        )
        .bind(song_id)
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();
        (song_id, stage_id)
    }

    #[actix_web::test]
    async fn test_stage_status_update_returns_204_not_redirect() {
        let (pool, _tmp) = setup_pool().await;
        let (_song_id, stage_id) = make_song_with_stage(&pool).await;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .configure(configure),
        )
        .await;

        let req = test::TestRequest::post()
            .uri(&format!("/production/stages/{stage_id}/status"))
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload("status=in_progress")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            204,
            "Stage status update must return 204 (not a redirect) to avoid scroll-to-top; got {}",
            resp.status()
        );
    }

    #[actix_web::test]
    async fn test_step_status_update_returns_204_not_redirect() {
        let (pool, _tmp) = setup_pool().await;
        let (_song_id, stage_id) = make_song_with_stage(&pool).await;
        let step_id =
            sqlx::query("INSERT INTO production_steps (stage_id, name, status, sort_order) VALUES (?, 'Mix stems', 'not_started', 1)")
                .bind(stage_id)
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_rowid();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .configure(configure),
        )
        .await;

        let req = test::TestRequest::post()
            .uri(&format!("/production/steps/{step_id}/status"))
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload("status=complete")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            204,
            "Step status update must return 204 (not a redirect) to avoid scroll-to-top; got {}",
            resp.status()
        );
    }

    #[actix_web::test]
    async fn test_stage_delete_returns_204_not_redirect() {
        let (pool, _tmp) = setup_pool().await;
        let (_song_id, stage_id) = make_song_with_stage(&pool).await;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .configure(configure),
        )
        .await;

        let req = test::TestRequest::post()
            .uri(&format!("/production/stages/{stage_id}/delete"))
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload("")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            204,
            "Stage delete must return 204 (not a redirect) to avoid scroll-to-top; got {}",
            resp.status()
        );
    }

    #[actix_web::test]
    async fn test_auto_add_stages_returns_204_not_redirect() {
        let (pool, _tmp) = setup_pool().await;
        let song_id =
            sqlx::query("INSERT INTO songs (title, song_type) VALUES ('Auto Song', 'song')")
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_rowid();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .configure(configure),
        )
        .await;

        let req = test::TestRequest::post()
            .uri(&format!("/production/songs/{song_id}/stages/auto"))
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload("")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            204,
            "Auto-add stages must return 204 (not a redirect) to avoid scroll-to-top; got {}",
            resp.status()
        );
    }

    #[actix_web::test]
    async fn test_practice_priority_update_returns_204_not_redirect() {
        let (pool, _tmp) = setup_pool().await;
        let song_id =
            sqlx::query("INSERT INTO songs (title, song_type) VALUES ('Priority Song', 'song')")
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_rowid();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .configure(configure),
        )
        .await;

        let req = test::TestRequest::post()
            .uri(&format!("/practice/songs/{song_id}/priority"))
            .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
            .set_payload("priority=2")
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            204,
            "Practice priority update must return 204 (not a redirect) to avoid scroll-to-top; got {}",
            resp.status()
        );
    }
}

// ===========================================================================
// POST /api/workflows integration tests
// ===========================================================================

mod workflow_api_tests {
    use super::setup_pool;
    use actix_web::{test, web, App};
    use music_browser::jobs::JobQueue;
    use serde_json::Value;
    use std::io::Write;

    fn configure_workflow(cfg: &mut web::ServiceConfig) {
        use actix_web::web;
        use music_browser::db::queries;
        use music_browser::jobs::{check_hydration, JobQueue, Operation, TargetType, WorkflowJob};
        use serde::Deserialize;
        use sqlx::SqlitePool;

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
        ) -> actix_web::Result<actix_web::HttpResponse> {
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
            let resolved_paths =
                resolve_paths(&pool, &target_type, &body.target_id_or_path).await?;
            let job = WorkflowJob {
                id: 0,
                target_type,
                target_id_or_path: body.target_id_or_path.clone(),
                operation,
                resolved_paths,
            };
            let job_id = queue
                .enqueue(job)
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;
            Ok(actix_web::HttpResponse::Accepted()
                .json(serde_json::json!({ "ok": true, "job_id": job_id })))
        }

        async fn resolve_paths(
            pool: &SqlitePool,
            target_type: &TargetType,
            target_id_or_path: &str,
        ) -> actix_web::Result<Vec<String>> {
            match target_type {
                TargetType::Song => {
                    let id: i64 = target_id_or_path.parse().map_err(|_| {
                        actix_web::error::ErrorBadRequest("song id must be numeric")
                    })?;
                    let song = queries::get_song(pool, id)
                        .await
                        .map_err(actix_web::error::ErrorInternalServerError)?
                        .ok_or_else(|| actix_web::error::ErrorNotFound("song not found"))?;
                    let mut paths = Vec::new();
                    for p in [
                        &song.scores_folder,
                        &song.practice_project_path,
                        &song.export_folder,
                    ] {
                        if !p.is_empty() {
                            paths.push(p.clone());
                        }
                    }
                    Ok(paths)
                }
                TargetType::LiveSet => {
                    let id: i64 = target_id_or_path.parse().map_err(|_| {
                        actix_web::error::ErrorBadRequest("live_set id must be numeric")
                    })?;
                    let set = queries::get_live_set(pool, id)
                        .await
                        .map_err(actix_web::error::ErrorInternalServerError)?
                        .ok_or_else(|| actix_web::error::ErrorNotFound("live_set not found"))?;
                    Ok(vec![set.name.clone()])
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
                            Ok(vec![target_id_or_path.to_string()])
                        }
                    }
                }
            }
        }

        cfg.route("/api/workflows", web::post().to(workflows_enqueue));
    }

    fn make_queue() -> (
        JobQueue,
        tokio::sync::mpsc::Receiver<music_browser::jobs::WorkflowJob>,
    ) {
        JobQueue::new(64)
    }

    // -----------------------------------------------------------------------
    // Given a song with a scores_folder set, POST /api/workflows with target
    // song ID returns 202 and a job_id.
    // -----------------------------------------------------------------------

    #[actix_web::test]
    async fn test_post_workflow_song_target_returns_202() {
        let (pool, _tmp) = setup_pool().await;

        let song_id = sqlx::query(
            "INSERT INTO songs (title, song_type, scores_folder) VALUES ('Test', 'song', '/scores/test')",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let (queue, _rx) = make_queue();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(queue))
                .configure(configure_workflow),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/workflows")
            .insert_header(("Content-Type", "application/json"))
            .set_json(serde_json::json!({
                "target_type": "song",
                "target_id_or_path": song_id.to_string(),
                "operation": "generate_sheet_music"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            202,
            "expected 202 Accepted for valid song target, got {}",
            resp.status()
        );

        let body: Value = test::read_body_json(resp).await;
        assert_eq!(
            body["ok"], true,
            "expected ok:true in response body, got {body:?}"
        );
        assert!(
            body["job_id"].as_u64().is_some(),
            "expected numeric job_id in response body, got {body:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Given a missing song ID, POST /api/workflows returns 404.
    // -----------------------------------------------------------------------

    #[actix_web::test]
    async fn test_post_workflow_missing_song_returns_404() {
        let (pool, _tmp) = setup_pool().await;
        let (queue, _rx) = make_queue();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(queue))
                .configure(configure_workflow),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/workflows")
            .insert_header(("Content-Type", "application/json"))
            .set_json(serde_json::json!({
                "target_type": "song",
                "target_id_or_path": "99999",
                "operation": "repomix"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            404,
            "expected 404 for non-existent song id, got {}",
            resp.status()
        );
    }

    // -----------------------------------------------------------------------
    // Given an unknown operation, POST /api/workflows returns 400.
    // -----------------------------------------------------------------------

    #[actix_web::test]
    async fn test_post_workflow_unknown_operation_returns_400() {
        let (pool, _tmp) = setup_pool().await;
        let (queue, _rx) = make_queue();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(queue))
                .configure(configure_workflow),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/workflows")
            .insert_header(("Content-Type", "application/json"))
            .set_json(serde_json::json!({
                "target_type": "file",
                "target_id_or_path": "/tmp/some.wav",
                "operation": "not_a_real_op"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            400,
            "expected 400 for unknown operation, got {}",
            resp.status()
        );
    }

    // -----------------------------------------------------------------------
    // Given an unknown target_type, POST /api/workflows returns 400.
    // -----------------------------------------------------------------------

    #[actix_web::test]
    async fn test_post_workflow_unknown_target_type_returns_400() {
        let (pool, _tmp) = setup_pool().await;
        let (queue, _rx) = make_queue();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(queue))
                .configure(configure_workflow),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/workflows")
            .insert_header(("Content-Type", "application/json"))
            .set_json(serde_json::json!({
                "target_type": "banana",
                "target_id_or_path": "/tmp/x",
                "operation": "repomix"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            400,
            "expected 400 for unknown target_type, got {}",
            resp.status()
        );
    }

    // -----------------------------------------------------------------------
    // Given a file target with a path not on disk, POST /api/workflows
    // returns 422.
    // -----------------------------------------------------------------------

    #[actix_web::test]
    async fn test_post_workflow_file_not_found_returns_422() {
        let (pool, _tmp) = setup_pool().await;
        let (queue, _rx) = make_queue();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(queue))
                .configure(configure_workflow),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/workflows")
            .insert_header(("Content-Type", "application/json"))
            .set_json(serde_json::json!({
                "target_type": "file",
                "target_id_or_path": "/tmp/__nonexistent_music_browser_test_xyz__.wav",
                "operation": "hitpoints"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            422,
            "expected 422 for missing file path, got {}",
            resp.status()
        );
    }

    // -----------------------------------------------------------------------
    // Given a file target with a hydrated (real content) file,
    // POST /api/workflows returns 202.
    // -----------------------------------------------------------------------

    #[actix_web::test]
    async fn test_post_workflow_hydrated_file_returns_202() {
        let (pool, _tmp) = setup_pool().await;

        let mut tmp_file = tempfile::NamedTempFile::new().expect("tempfile");
        tmp_file.write_all(b"RIFF fake wav data").expect("write");
        let path = tmp_file.path().to_string_lossy().to_string();

        let (queue, _rx) = make_queue();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(queue))
                .configure(configure_workflow),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/api/workflows")
            .insert_header(("Content-Type", "application/json"))
            .set_json(serde_json::json!({
                "target_type": "file",
                "target_id_or_path": path,
                "operation": "hitpoints"
            }))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            202,
            "expected 202 Accepted for hydrated file, got {}",
            resp.status()
        );
    }

    // -----------------------------------------------------------------------
    // Multiple sequential enqueues produce ascending job IDs.
    // -----------------------------------------------------------------------

    #[actix_web::test]
    async fn test_post_workflow_sequential_jobs_have_ascending_ids() {
        let (pool, _tmp) = setup_pool().await;

        let song_id = sqlx::query("INSERT INTO songs (title, song_type) VALUES ('Multi', 'song')")
            .execute(&pool)
            .await
            .unwrap()
            .last_insert_rowid();

        let (queue, _rx) = JobQueue::new(64);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool))
                .app_data(web::Data::new(queue))
                .configure(configure_workflow),
        )
        .await;

        let make_req = |id: i64| {
            test::TestRequest::post()
                .uri("/api/workflows")
                .insert_header(("Content-Type", "application/json"))
                .set_json(serde_json::json!({
                    "target_type": "song",
                    "target_id_or_path": id.to_string(),
                    "operation": "repomix"
                }))
                .to_request()
        };

        let resp1 = test::call_service(&app, make_req(song_id)).await;
        let body1: Value = test::read_body_json(resp1).await;
        let id1 = body1["job_id"]
            .as_u64()
            .expect("job_id must be numeric for first call");

        let resp2 = test::call_service(&app, make_req(song_id)).await;
        let body2: Value = test::read_body_json(resp2).await;
        let id2 = body2["job_id"]
            .as_u64()
            .expect("job_id must be numeric for second call");

        assert!(
            id1 < id2,
            "expected ascending job IDs, got id1={id1} id2={id2}"
        );
    }
}
