use actix_web::dev::Payload;
use actix_web::{test, web, App, FromRequest, HttpRequest};
use serde::de::DeserializeOwned;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use tempfile::NamedTempFile;

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
