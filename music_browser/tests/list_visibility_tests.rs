//! List-view authorization tests (Phase 3, PR 3).
//!
//! Every `*_list` handler resolves the resources visible to the request's
//! `AuthenticatedUser` (direct `shares` ∪ `group_shares` of groups the
//! user belongs to) via `permissions::list_visibility` and retains only
//! those rows. In single-tenant mode (`AUTH_REQUIRE_LOGIN=false`) or
//! without PocketBase configured, lists stay unfiltered.
//!
//! Gated on `POCKETBASE_TEST_URL`; skipped otherwise.

use std::str::FromStr;

use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig};
use actix_web::http::StatusCode;
use actix_web::{middleware, test::TestRequest, web, App};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tempfile::NamedTempFile;
use uuid::Uuid;

use music_browser::acl::{
    AccessLevel, CreateGroup, CreateGroupMember, CreateGroupShare, CreateShare, GroupRole,
    ResourceType,
};
use music_browser::app;
use music_browser::auth::{AuthConfig, AuthenticatedUser, JwtMiddleware, TokenVerifyCache};
use music_browser::jobs::{JobQueue, JobStore};
use music_browser::permissions;
use music_browser::pocketbase_client::PocketBaseClient;

const TEST_USER_A_EMAIL: &str = "acl-test-user-1@example.com";
const TEST_USER_A_PASSWORD: &str = "AclTestUser1Password!";
const TEST_USER_B_EMAIL: &str = "acl-test-user-2@example.com";
const TEST_USER_B_PASSWORD: &str = "AclTestUser2Password!";

fn pb_url() -> Option<String> {
    std::env::var("POCKETBASE_TEST_URL")
        .ok()
        .filter(|u| !u.is_empty())
}

struct TestUser {
    id: String,
    token: String,
}

async fn authenticate(base_url: &str, email: &str, password: &str) -> TestUser {
    let response = reqwest::Client::new()
        .post(format!(
            "{base_url}/api/collections/users/auth-with-password"
        ))
        .json(&serde_json::json!({ "identity": email, "password": password }))
        .send()
        .await
        .expect("failed to reach PocketBase to authenticate seeded test user");
    assert!(
        response.status().is_success(),
        "seeded test user auth failed with status {}",
        response.status()
    );
    let body: Value = response.json().await.expect("valid auth response JSON");
    TestUser {
        id: body["record"]["id"].as_str().expect("user id").to_string(),
        token: body["token"].as_str().expect("token").to_string(),
    }
}

async fn test_pool() -> (SqlitePool, NamedTempFile) {
    let tmp = NamedTempFile::new().expect("temp file");
    let url = format!("sqlite:{}", tmp.path().to_str().unwrap());
    let opts = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .expect("pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");
    (pool, tmp)
}

struct Harness {
    server: actix_test::TestServer,
    pool: SqlitePool,
    pb: PocketBaseClient,
    user_a: TestUser,
    user_b: TestUser,
}

async fn start_harness(pb_url: &str, require_login: bool) -> (Harness, NamedTempFile) {
    let (pool, tmp) = test_pool().await;
    let config = AuthConfig {
        pocketbase_url: pb_url.to_string(),
        cookie_secure: false,
        require_login,
        pocketbase_ca_cert: None,
        public_paths: vec!["/login".into(), "/signup".into(), "/logout".into()],
        workflow_allowed_roots: vec![],
    };
    let pb = PocketBaseClient::new(pb_url.to_string(), reqwest::Client::new());
    let (queue, _rx) = JobQueue::new(16);
    let store = JobStore::new();
    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"list-visibility-test-csrf-secret!");

    let pool_data = web::Data::new(pool.clone());
    let auth_data = web::Data::new(config.clone());
    let pb_data = web::Data::new(pb.clone());
    let queue_data = web::Data::new(queue);
    let store_data = web::Data::new(store);
    let csrf_data = csrf_config.clone();
    let token_cache = web::Data::new(TokenVerifyCache::default());
    let factory_config = config;

    let server = actix_test::start_with(actix_test::config().disable_redirects(), move || {
        App::new()
            .app_data(pool_data.clone())
            .app_data(auth_data.clone())
            .app_data(pb_data.clone())
            .app_data(queue_data.clone())
            .app_data(store_data.clone())
            .app_data(csrf_data.clone())
            .app_data(token_cache.clone())
            .wrap(middleware::Condition::new(
                factory_config.require_login,
                JwtMiddleware::new(factory_config.clone(), token_cache.clone()),
            ))
            .wrap(CsrfMiddleware::new(csrf_data.clone()))
            .configure(app::configure_app)
    });

    let user_a = authenticate(pb_url, TEST_USER_A_EMAIL, TEST_USER_A_PASSWORD).await;
    let user_b = authenticate(pb_url, TEST_USER_B_EMAIL, TEST_USER_B_PASSWORD).await;

    (
        Harness {
            server,
            pool,
            pb,
            user_a,
            user_b,
        },
        tmp,
    )
}

async fn get(h: &Harness, path: &str, bearer: Option<&str>) -> (StatusCode, String) {
    let mut req = h.server.get(path);
    if let Some(token) = bearer {
        req = req.insert_header(("Authorization", format!("Bearer {token}")));
    }
    let mut resp = req.send().await.expect("response");
    let status = resp.status();
    let body = String::from_utf8(resp.body().await.expect("body").to_vec()).expect("utf8 body");
    (status, body)
}

async fn insert_row(pool: &SqlitePool, sql: &str, binds: &[i64]) -> i64 {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool)
        .await
        .expect("seed insert")
        .last_insert_rowid()
}

async fn grant(h: &Harness, user: &TestUser, rt: ResourceType, id: i64, level: AccessLevel) {
    h.pb.create_share(
        &user.token,
        &CreateShare {
            user_id: user.id.clone(),
            resource_type: rt.as_str().to_string(),
            resource_id: id.to_string(),
            access_level: level,
            created_by: user.id.clone(),
        },
    )
    .await
    .expect("seed share");
}

/// Every list endpoint redirects unauthenticated callers to /login.
#[actix_web::test]
async fn list_views_redirect_unauthenticated_callers() {
    let Some(pb_url) = pb_url() else {
        eprintln!("skipping: POCKETBASE_TEST_URL not set");
        return;
    };
    let (h, _tmp) = start_harness(&pb_url, true).await;
    for path in [
        "/",
        "/albums",
        "/artists",
        "/instruments",
        "/recordings",
        "/bands",
        "/production",
        "/practice",
        "/workflow",
        "/exercises",
        "/goals",
        "/sets",
    ] {
        let (status, _) = get(&h, path, None).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{path} should redirect");
    }
}

/// A directly shared song appears in the owner's list; an unshared song
/// does not — for either user.
#[actix_web::test]
async fn song_list_shows_only_accessible_songs() {
    let Some(pb_url) = pb_url() else {
        eprintln!("skipping: POCKETBASE_TEST_URL not set");
        return;
    };
    let (h, _tmp) = start_harness(&pb_url, true).await;

    // Explicit ids in a high range: PocketBase state is shared across the
    // whole test run, and other tests' share rows reference small integer
    // ids that would otherwise collide with this fresh database's rowids.
    let shared = insert_row(
        &h.pool,
        "INSERT INTO songs (id, title) VALUES (90001, 'shared-song-xyz')",
        &[],
    )
    .await;
    grant(
        &h,
        &h.user_a,
        ResourceType::Song,
        shared,
        AccessLevel::Admin,
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO songs (id, title) VALUES (90002, 'unshared-song-xyz')",
        &[],
    )
    .await;

    let (status, body) = get(&h, "/", Some(&h.user_a.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("shared-song-xyz"), "owner sees shared song");
    assert!(
        !body.contains("unshared-song-xyz"),
        "unshared song hidden even from owner (no ACL row)"
    );

    let (status, body) = get(&h, "/", Some(&h.user_b.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("shared-song-xyz"),
        "B should not see shared song"
    );
    assert!(
        !body.contains("unshared-song-xyz"),
        "B should not see unshared song"
    );
}

/// A group share grants list visibility to members of the group.
#[actix_web::test]
async fn group_share_grants_list_visibility() {
    let Some(pb_url) = pb_url() else {
        eprintln!("skipping: POCKETBASE_TEST_URL not set");
        return;
    };
    let (h, _tmp) = start_harness(&pb_url, true).await;

    let song = insert_row(
        &h.pool,
        "INSERT INTO songs (id, title) VALUES (90011, 'group-shared-xyz')",
        &[],
    )
    .await;
    grant(&h, &h.user_a, ResourceType::Song, song, AccessLevel::Admin).await;

    let group =
        h.pb.create_group(
            &h.user_a.token,
            &CreateGroup {
                name: format!("list-test-{}", Uuid::new_v4()),
                description: String::new(),
                owner_id: h.user_a.id.clone(),
            },
        )
        .await
        .expect("create group");
    let group_id = group["id"].as_str().expect("group id").to_string();
    h.pb.add_user_to_group(
        &h.user_a.token,
        &CreateGroupMember {
            group_id: group_id.clone(),
            user_id: h.user_b.id.clone(),
            role: GroupRole::Member,
        },
    )
    .await
    .expect("add B to group");
    h.pb.share_with_group(
        &h.user_a.token,
        &CreateGroupShare {
            group_id: group_id.clone(),
            resource_type: ResourceType::Song.as_str().to_string(),
            resource_id: song.to_string(),
            access_level: AccessLevel::Viewer,
            created_by: h.user_a.id.clone(),
        },
    )
    .await
    .expect("share song with group");

    let (status, body) = get(&h, "/", Some(&h.user_b.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("group-shared-xyz"),
        "group member sees group-shared song"
    );

    // The resolved id set covers it too.
    let req = TestRequest::default().to_http_request();
    let b = AuthenticatedUser {
        id: h.user_b.id.clone(),
        token: h.user_b.token.clone(),
    };
    let ids = permissions::accessible_resource_ids(&req, &h.pb, &b, ResourceType::Song)
        .await
        .expect("accessible ids");
    assert!(ids.contains(&song));
}

/// Direct shares grant visibility on a non-song list too (albums here).
#[actix_web::test]
async fn album_list_filtered_by_shares() {
    let Some(pb_url) = pb_url() else {
        eprintln!("skipping: POCKETBASE_TEST_URL not set");
        return;
    };
    let (h, _tmp) = start_harness(&pb_url, true).await;

    let album = insert_row(
        &h.pool,
        "INSERT INTO albums (id, title, released, url) VALUES (90021, 'shared-album-xyz', 0, '')",
        &[],
    )
    .await;
    grant(
        &h,
        &h.user_a,
        ResourceType::Album,
        album,
        AccessLevel::Admin,
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO albums (id, title, released, url) VALUES (90022, 'unshared-album-xyz', 0, '')",
        &[],
    )
    .await;

    let (status, body) = get(&h, "/albums", Some(&h.user_a.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("shared-album-xyz"));
    assert!(!body.contains("unshared-album-xyz"));

    let (status, body) = get(&h, "/albums", Some(&h.user_b.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("shared-album-xyz") && !body.contains("unshared-album-xyz"));
}

/// Single-tenant mode keeps lists unfiltered and unauthenticated.
#[actix_web::test]
async fn single_tenant_lists_stay_unfiltered() {
    let Some(pb_url) = pb_url() else {
        eprintln!("skipping: POCKETBASE_TEST_URL not set");
        return;
    };
    let (h, _tmp) = start_harness(&pb_url, false).await;

    insert_row(
        &h.pool,
        "INSERT INTO songs (id, title) VALUES (90031, 'any-song-xyz')",
        &[],
    )
    .await;

    let (status, body) = get(&h, "/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("any-song-xyz"),
        "single-tenant mode shows everything"
    );
}

/// `/schedule` and `/schedule/export.ics` hide items that reference
/// resources the caller cannot access; items with no resource references
/// stay visible (there is no resource data to leak).
#[actix_web::test]
async fn schedule_list_filters_items_by_linked_resource_access() {
    let Some(pb_url) = pb_url() else {
        eprintln!("skipping: POCKETBASE_TEST_URL not set");
        return;
    };
    let (h, _tmp) = start_harness(&pb_url, true).await;

    // One resource of each linkable kind, owned by user A only.
    insert_row(
        &h.pool,
        "INSERT INTO songs (id, title) VALUES (92001, 'sched-hidden-song')",
        &[],
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO practice_exercises (id, name) VALUES (92002, 'sched-hidden-exercise')",
        &[],
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO instruments (id, name) VALUES (92003, 'sched-hidden-instrument')",
        &[],
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO production_stages (id, song_id, stage) VALUES (92004, 92001, 'tracking')",
        &[],
    )
    .await;
    grant(&h, &h.user_a, ResourceType::Song, 92001, AccessLevel::Admin).await;
    grant(
        &h,
        &h.user_a,
        ResourceType::PracticeExercise,
        92002,
        AccessLevel::Admin,
    )
    .await;
    grant(
        &h,
        &h.user_a,
        ResourceType::Instrument,
        92003,
        AccessLevel::Admin,
    )
    .await;

    insert_row(&h.pool, "INSERT INTO schedule_events (id, event_date, title) VALUES (92010, '2030-01-10', 'sched-test-event')", &[]).await;
    insert_row(&h.pool, "INSERT INTO schedule_items (id, event_id, item_type, song_id, title) VALUES (92020, 92010, 'song_practice', 92001, 'item-hidden-song')", &[]).await;
    insert_row(&h.pool, "INSERT INTO schedule_items (id, event_id, item_type, exercise_id, title) VALUES (92021, 92010, 'exercise', 92002, 'item-hidden-exercise')", &[]).await;
    insert_row(&h.pool, "INSERT INTO schedule_items (id, event_id, item_type, instrument_id, title) VALUES (92022, 92010, 'warmup', 92003, 'item-hidden-instrument')", &[]).await;
    insert_row(&h.pool, "INSERT INTO schedule_items (id, event_id, item_type, stage_id, title) VALUES (92023, 92010, 'song_production', 92004, 'item-hidden-stage')", &[]).await;
    insert_row(&h.pool, "INSERT INTO schedule_items (id, event_id, item_type, title) VALUES (92024, 92010, 'warmup', 'item-open-warmup')", &[]).await;

    let (status, body) = get(&h, "/schedule", Some(&h.user_a.token)).await;
    assert_eq!(status, StatusCode::OK);
    for marker in [
        "item-hidden-song",
        "item-hidden-exercise",
        "item-hidden-instrument",
        "item-hidden-stage",
        "item-open-warmup",
    ] {
        assert!(body.contains(marker), "A sees {marker}");
    }

    for path in ["/schedule", "/schedule/export.ics"] {
        let (status, body) = get(&h, path, Some(&h.user_b.token)).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        for marker in [
            "item-hidden-song",
            "item-hidden-exercise",
            "item-hidden-instrument",
            "item-hidden-stage",
        ] {
            assert!(!body.contains(marker), "B must not see {marker} on {path}");
        }
        assert!(
            body.contains("item-open-warmup"),
            "unreferenced items stay visible on {path}"
        );
    }
}

/// `/profile`'s practice journal lists only entries whose linked goal or
/// schedule item resolves to resources the caller can access.
#[actix_web::test]
async fn profile_journal_filters_entries_by_linked_resource_access() {
    let Some(pb_url) = pb_url() else {
        eprintln!("skipping: POCKETBASE_TEST_URL not set");
        return;
    };
    let (h, _tmp) = start_harness(&pb_url, true).await;

    // A-owned song + goal; one journal entry of each type linked to them.
    insert_row(
        &h.pool,
        "INSERT INTO songs (id, title) VALUES (92101, 'journal-hidden-song')",
        &[],
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO goals (id, horizon, title) VALUES (92102, '1_week', 'journal-hidden-goal')",
        &[],
    )
    .await;
    grant(&h, &h.user_a, ResourceType::Song, 92101, AccessLevel::Admin).await;
    grant(&h, &h.user_a, ResourceType::Goal, 92102, AccessLevel::Admin).await;

    insert_row(&h.pool, "INSERT INTO schedule_events (id, event_date, title) VALUES (92110, '2030-01-11', 'journal-test-event')", &[]).await;
    insert_row(&h.pool, "INSERT INTO schedule_items (id, event_id, item_type, song_id, title) VALUES (92120, 92110, 'song_practice', 92101, 'journal-hidden-item')", &[]).await;
    insert_row(&h.pool, "INSERT INTO journal_entries (id, entry_date, entry_type, goal_id) VALUES (92130, '2030-01-11', 'goal', 92102)", &[]).await;
    insert_row(&h.pool, "INSERT INTO journal_entries (id, entry_date, entry_type, schedule_item_id) VALUES (92131, '2030-01-11', 'schedule_item', 92120)", &[]).await;

    let (status, body) = get(&h, "/profile", Some(&h.user_a.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("journal-hidden-goal"), "A sees goal entry");
    assert!(body.contains("journal-hidden-song"), "A sees item entry");

    let (status, body) = get(&h, "/profile", Some(&h.user_b.token)).await;
    assert_eq!(status, StatusCode::OK);
    for marker in [
        "journal-hidden-goal",
        "journal-hidden-song",
        "journal-hidden-item",
    ] {
        assert!(!body.contains(marker), "B must not see {marker}");
    }
}

/// The song edit form lists only artists the caller can access — except
/// artists already linked to the song, which must remain listed so
/// saving the form does not silently drop their association.
#[actix_web::test]
async fn song_edit_form_lists_only_accessible_artists() {
    let Some(pb_url) = pb_url() else {
        eprintln!("skipping: POCKETBASE_TEST_URL not set");
        return;
    };
    let (h, _tmp) = start_harness(&pb_url, true).await;

    insert_row(
        &h.pool,
        "INSERT INTO songs (id, title) VALUES (92201, 'artist-form-song')",
        &[],
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO artists (id, name) VALUES (92202, 'artist-shared-b')",
        &[],
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO artists (id, name) VALUES (92203, 'artist-hidden-b')",
        &[],
    )
    .await;
    insert_row(
        &h.pool,
        "INSERT INTO artists (id, name) VALUES (92204, 'artist-linked-hidden')",
        &[],
    )
    .await;
    grant(
        &h,
        &h.user_b,
        ResourceType::Song,
        92201,
        AccessLevel::Editor,
    )
    .await;
    grant(
        &h,
        &h.user_b,
        ResourceType::Artist,
        92202,
        AccessLevel::Viewer,
    )
    .await;
    grant(
        &h,
        &h.user_a,
        ResourceType::Artist,
        92203,
        AccessLevel::Admin,
    )
    .await;
    grant(
        &h,
        &h.user_a,
        ResourceType::Artist,
        92204,
        AccessLevel::Admin,
    )
    .await;
    // 92204 is already an artist of the song — B has no share on it.
    insert_row(
        &h.pool,
        "INSERT INTO song_artists (song_id, artist_id) VALUES (92201, 92204)",
        &[],
    )
    .await;

    let (status, body) = get(&h, "/songs/92201/edit", Some(&h.user_b.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("artist-shared-b"), "shared artist listed");
    assert!(
        !body.contains("artist-hidden-b"),
        "unshared artist hidden from the form"
    );
    assert!(
        body.contains("artist-linked-hidden"),
        "already-linked artist stays listed to preserve the association"
    );
}
