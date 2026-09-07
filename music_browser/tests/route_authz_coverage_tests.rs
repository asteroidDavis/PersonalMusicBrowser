//! Route-coverage regression guard for mutating (POST/PUT/DELETE) endpoints.
//!
//! Two layers:
//!
//! 1. `all_mutating_routes_are_classified` (always runs, no PocketBase needed)
//!    scans `configure_app` in `src/app.rs` for `web::post/put/delete/patch`
//!    registrations and asserts the set matches `MUTATING_ROUTES` exactly.
//!    Every new mutating route must be classified here — that is the
//!    hard-to-forget construct that keeps authorization coverage complete.
//!
//! 2. The service-level scenarios run a real `actix_test` server with the same
//!    middleware stack as `main.rs` (`CsrfMiddleware` outermost, then
//!    `JwtMiddleware`, a real PocketBase via `POCKETBASE_TEST_URL`) and
//!    exercise each owner-protected route as "user B" against a resource
//!    owned by "user A".
//!
//!    Routes whose authorization gap is not yet fixed carry
//!    `open_until: Some("PR N")`; for those the test asserts the request is
//!    currently *allowed* and the mutation really happens, so it fails loudly
//!    once the fix lands and the flag must be flipped to `None`.
//!
//! Skip behaviour follows the existing convention: with `POCKETBASE_TEST_URL`
//! unset the service-level tests no-op; run them via
//! `scripts/run-pocketbase-integration-tests.sh`.

use std::collections::BTreeSet;
use std::str::FromStr;

use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig};
use actix_web::http::header;
use actix_web::http::{Method, StatusCode};
use actix_web::{middleware, web, App};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tempfile::NamedTempFile;
use uuid::Uuid;

use music_browser::acl::{AccessLevel, CreateGroup, CreateShare, ResourceType};
use music_browser::app;
use music_browser::auth::{AuthConfig, AuthenticatedUser, JwtMiddleware};
use music_browser::jobs::{JobQueue, JobStore};
use music_browser::permissions;
use music_browser::pocketbase_client::PocketBaseClient;

const FORM: &str = "application/x-www-form-urlencoded";
const JSON: &str = "application/json";

// ---------------------------------------------------------------------------
// Route manifest — every mutating route registered in `configure_app`.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct MutatingRoute {
    method: &'static str,
    /// Route path template exactly as registered (with `{id}` placeholders).
    path: &'static str,
    kind: RouteKind,
}

#[derive(Clone, Copy)]
enum RouteKind {
    /// Publicly reachable without authentication (login, signup, logout).
    Public,
    /// Creates a caller-owned resource; any authenticated user may call it.
    Creates {
        content_type: &'static str,
        body: &'static str,
    },
    /// Mutates global single-tenant state (journal/schedule/profile). These
    /// have no per-resource ownership today; an authenticated caller is
    /// allowed — per-user scoping lands when profiles exist.
    LoginOnly {
        seed: Seed,
        content_type: &'static str,
        body: &'static str,
        mutated: Mutation,
    },
    /// Mutates a resource carrying share-based ownership. An authenticated
    /// non-owner must be denied. `open_until: Some(pr)` marks the not-yet-
    /// landed fix so the still-open gap is asserted (and must be flipped to
    /// `None` once `pr` lands).
    OwnerProtected {
        seed: Seed,
        content_type: &'static str,
        body: &'static str,
        mutated: Mutation,
        open_until: Option<&'static str>,
    },
}

/// What the route's `{id}` path parameter (or `{id}`/`{other}` body
/// placeholders) refer to, and how to build the fixture. Shareable resources
/// are seeded with a creator `admin` share for user A.
#[derive(Clone, Copy)]
enum Seed {
    /// No fixture row needed.
    None,
    /// A share-owned row of the given resource type. `{id}` = row id.
    Owned(ResourceType),
    /// An owned set plus an extra song. `{id}` = set id, `{other}` = song id.
    OwnedSetWithSong,
    /// A `live_set_songs` join row under an owned set. `{id}` = join row id.
    JoinUnderOwnedSet,
    /// A `production_stages` row under an owned song. `{id}` = stage id.
    StageUnderOwnedSong,
    /// A `production_steps` row under a stage of an owned song. `{id}` = step id.
    StepUnderOwnedSong,
    /// A `song_files` row under an owned song. `{id}` = file id.
    FileUnderOwnedSong,
    /// A PocketBase group owned by user A. `{id}` = group record id.
    OwnedGroup,
    /// A PocketBase group owned by user A plus an owned song.
    /// `{id}` = group record id, `{other}` = song id.
    OwnedGroupAndSong,
    /// A global `journal_entries` row. `{id}` = entry id.
    JournalEntry,
    /// A global `schedule_items` row. `{id}` = item id.
    ScheduleItem,
    /// A global `schedule_events` row. `{id}` = event id.
    ScheduleEvent,
}

/// How the test observes that the mutation actually happened.
#[derive(Clone, Copy)]
enum Mutation {
    /// No row with `id = {id}` remains in this table.
    RowDeleted(&'static str),
    /// A row matching all `(column, expected)` pairs exists; expected values
    /// support `{id}` / `{other}` placeholder substitution.
    RowMatching {
        table: &'static str,
        conditions: &'static [(&'static str, &'static str)],
    },
    /// User B can see a share on the seeded shareable resource.
    ShareVisibleToB,
    /// User B has a `group_memberships` row in group `{id}`.
    GroupMembershipForB,
    /// Some `group_shares` row exists for group `{id}`.
    GroupShareExists,
    /// A job whose raw `target_id_or_path` is `{id}` reached the `JobStore`.
    JobEnqueued,
}

const MUTATING_ROUTES: &[MutatingRoute] = &[
    // --- Public auth endpoints ---
    MutatingRoute { method: "POST", path: "/login", kind: RouteKind::Public },
    MutatingRoute { method: "POST", path: "/signup", kind: RouteKind::Public },
    MutatingRoute { method: "POST", path: "/logout", kind: RouteKind::Public },
    // --- ACL management API ---
    MutatingRoute {
        method: "POST",
        path: "/api/shares",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: JSON,
            body: r#"{"user_id":"{b}","resource_type":"song","resource_id":"{id}","access_level":"viewer"}"#,
            mutated: Mutation::ShareVisibleToB,
            open_until: Some("PR 4 (share_create ownership check)"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/api/groups",
        kind: RouteKind::Creates {
            content_type: JSON,
            body: r#"{"name":"coverage-group","description":"route coverage"}"#,
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/api/groups/{id}/members",
        kind: RouteKind::OwnerProtected {
            seed: Seed::OwnedGroup,
            content_type: JSON,
            body: r#"{"user_id":"{b}","role":"member"}"#,
            mutated: Mutation::GroupMembershipForB,
            open_until: Some("PR 4 (group owner check)"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/api/groups/{id}/shares",
        kind: RouteKind::OwnerProtected {
            seed: Seed::OwnedGroupAndSong,
            content_type: JSON,
            body: r#"{"resource_type":"song","resource_id":"{other}","access_level":"viewer"}"#,
            mutated: Mutation::GroupShareExists,
            open_until: Some("PR 4 (group owner + resource admin check)"),
        },
    },
    // --- Songs ---
    MutatingRoute {
        method: "POST",
        path: "/songs/new",
        kind: RouteKind::Creates {
            content_type: FORM,
            body: "title=Coverage%20Song&song_type=song",
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/songs/{id}/edit",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: FORM,
            body: "title=owned-update&song_type=song",
            mutated: Mutation::RowMatching {
                table: "songs",
                conditions: &[("id", "{id}"), ("title", "owned-update")],
            },
            open_until: None,
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/songs/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("songs"),
            open_until: None,
        },
    },
    // --- Albums ---
    MutatingRoute {
        method: "POST",
        path: "/albums/new",
        kind: RouteKind::Creates {
            content_type: FORM,
            body: "title=Coverage%20Album",
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/albums/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Album),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("albums"),
            open_until: Some("PR 5"),
        },
    },
    // --- Artists ---
    MutatingRoute {
        method: "POST",
        path: "/artists/new",
        kind: RouteKind::Creates {
            content_type: FORM,
            body: "name=Coverage%20Artist",
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/artists/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Artist),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("artists"),
            open_until: Some("PR 5"),
        },
    },
    // --- Instruments ---
    MutatingRoute {
        method: "POST",
        path: "/instruments/new",
        kind: RouteKind::Creates {
            content_type: FORM,
            body: "name=Coverage%20Instrument&instrument_type=guitar",
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/instruments/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Instrument),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("instruments"),
            open_until: Some("PR 5"),
        },
    },
    // --- Recordings ---
    MutatingRoute {
        method: "POST",
        path: "/recordings/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Recording),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("recordings"),
            open_until: Some("PR 5"),
        },
    },
    // --- Bands ---
    MutatingRoute {
        method: "POST",
        path: "/bands/new",
        kind: RouteKind::Creates {
            content_type: FORM,
            body: "name=Coverage%20Band",
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/bands/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Band),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("bands"),
            open_until: Some("PR 5"),
        },
    },
    // --- Production: child resources cascade to the parent song's ACL ---
    MutatingRoute {
        method: "POST",
        path: "/production/songs/{id}/stages/new",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: FORM,
            body: "stage=writing",
            mutated: Mutation::RowMatching {
                table: "production_stages",
                conditions: &[("song_id", "{id}")],
            },
            open_until: None,
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/production/stages/{id}/status",
        kind: RouteKind::OwnerProtected {
            seed: Seed::StageUnderOwnedSong,
            content_type: FORM,
            body: "status=in_progress",
            mutated: Mutation::RowMatching {
                table: "production_stages",
                conditions: &[("id", "{id}"), ("status", "in_progress")],
            },
            open_until: Some("PR 7"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/production/stages/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::StageUnderOwnedSong,
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("production_stages"),
            open_until: Some("PR 7"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/production/stages/{id}/steps/new",
        kind: RouteKind::OwnerProtected {
            seed: Seed::StageUnderOwnedSong,
            content_type: FORM,
            body: "name=coverage-step",
            mutated: Mutation::RowMatching {
                table: "production_steps",
                conditions: &[("stage_id", "{id}")],
            },
            open_until: Some("PR 7"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/production/steps/{id}/status",
        kind: RouteKind::OwnerProtected {
            seed: Seed::StepUnderOwnedSong,
            content_type: FORM,
            body: "status=in_progress",
            mutated: Mutation::RowMatching {
                table: "production_steps",
                conditions: &[("id", "{id}"), ("status", "in_progress")],
            },
            open_until: Some("PR 7"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/production/songs/{id}/files/new",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: FORM,
            body: "file_type=lyrics&path=/tmp/coverage-lyrics.txt",
            mutated: Mutation::RowMatching {
                table: "song_files",
                conditions: &[("song_id", "{id}")],
            },
            open_until: None,
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/production/files/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::FileUnderOwnedSong,
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("song_files"),
            open_until: Some("PR 7"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/production/songs/{id}/stages/auto",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowMatching {
                table: "production_stages",
                conditions: &[("song_id", "{id}")],
            },
            open_until: None,
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/production/stages/{id}/steps/auto",
        kind: RouteKind::OwnerProtected {
            seed: Seed::StageUnderOwnedSong,
            content_type: FORM,
            body: "",
            mutated: Mutation::RowMatching {
                table: "production_steps",
                conditions: &[("stage_id", "{id}")],
            },
            open_until: Some("PR 7"),
        },
    },
    // --- Kanban workflow state ---
    MutatingRoute {
        method: "POST",
        path: "/workflow/songs/{id}/state",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: FORM,
            body: "workflow_state=learning",
            mutated: Mutation::RowMatching {
                table: "songs",
                conditions: &[("id", "{id}"), ("workflow_state", "learning")],
            },
            open_until: Some("PR 6"),
        },
    },
    MutatingRoute {
        method: "PUT",
        path: "/api/workflow/songs/{id}/state",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: JSON,
            body: r#"{"workflow_state":"learning"}"#,
            mutated: Mutation::RowMatching {
                table: "songs",
                conditions: &[("id", "{id}"), ("workflow_state", "learning")],
            },
            open_until: Some("PR 6"),
        },
    },
    // --- Exercises ---
    MutatingRoute {
        method: "POST",
        path: "/exercises/new",
        kind: RouteKind::Creates {
            content_type: FORM,
            body: "name=Coverage%20Exercise&category=technique",
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/exercises/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::PracticeExercise),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("practice_exercises"),
            open_until: Some("PR 5"),
        },
    },
    // --- Goals ---
    MutatingRoute {
        method: "POST",
        path: "/goals/new",
        kind: RouteKind::Creates {
            content_type: FORM,
            body: "horizon=1_week&category=general&title=Coverage%20Goal&description=&target_date=&sort_order=0",
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/goals/{id}/toggle",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Goal),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowMatching {
                table: "goals",
                conditions: &[("id", "{id}"), ("completed", "1")],
            },
            open_until: Some("PR 6"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/goals/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Goal),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("goals"),
            open_until: Some("PR 6"),
        },
    },
    // --- Profile / journal / schedule: global single-tenant state ---
    MutatingRoute {
        method: "POST",
        path: "/profile",
        kind: RouteKind::LoginOnly {
            seed: Seed::None,
            content_type: FORM,
            body: "display_name=tampered&songs_capacity=3&warmup_minutes=15&drill_minutes=15&song_minutes=30&review_minutes=10&notes=",
            mutated: Mutation::RowMatching {
                table: "user_profile",
                conditions: &[("display_name", "tampered")],
            },
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/journal/{id}/notes",
        kind: RouteKind::LoginOnly {
            seed: Seed::JournalEntry,
            content_type: FORM,
            body: "notes=tampered",
            mutated: Mutation::RowMatching {
                table: "journal_entries",
                conditions: &[("id", "{id}"), ("notes", "tampered")],
            },
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/journal/{id}/delete",
        kind: RouteKind::LoginOnly {
            seed: Seed::JournalEntry,
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("journal_entries"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/schedule/generate",
        kind: RouteKind::LoginOnly {
            seed: Seed::None,
            content_type: FORM,
            body: "start_date=2030-01-06&num_blocks=1",
            mutated: Mutation::RowMatching {
                table: "schedule_events",
                conditions: &[("event_date", "2030-01-06")],
            },
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/schedule/items/{id}/toggle",
        kind: RouteKind::LoginOnly {
            seed: Seed::ScheduleItem,
            content_type: FORM,
            body: "",
            mutated: Mutation::RowMatching {
                table: "schedule_items",
                conditions: &[("id", "{id}"), ("completed", "1")],
            },
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/schedule/events/{id}/delete",
        kind: RouteKind::LoginOnly {
            seed: Seed::ScheduleEvent,
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("schedule_events"),
        },
    },
    // --- Live sets ---
    MutatingRoute {
        method: "POST",
        path: "/sets/new",
        kind: RouteKind::Creates {
            content_type: FORM,
            body: "name=Coverage%20Set",
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/sets/{id}/delete",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::LiveSet),
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("live_sets"),
            open_until: Some("PR 6"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/sets/{id}/songs",
        kind: RouteKind::OwnerProtected {
            seed: Seed::OwnedSetWithSong,
            content_type: FORM,
            body: "song_id={other}&sort_order=0&backing_track_path=&duration_seconds=0&transition_notes=",
            mutated: Mutation::RowMatching {
                table: "live_set_songs",
                conditions: &[("set_id", "{id}"), ("song_id", "{other}")],
            },
            open_until: Some("PR 6"),
        },
    },
    MutatingRoute {
        method: "POST",
        path: "/sets/songs/{id}/remove",
        kind: RouteKind::OwnerProtected {
            seed: Seed::JoinUnderOwnedSet,
            content_type: FORM,
            body: "",
            mutated: Mutation::RowDeleted("live_set_songs"),
            open_until: Some("PR 6"),
        },
    },
    // --- Practice priority (song-scoped mutation) ---
    MutatingRoute {
        method: "POST",
        path: "/practice/songs/{id}/priority",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::Song),
            content_type: FORM,
            body: "priority=3",
            mutated: Mutation::RowMatching {
                table: "songs",
                conditions: &[("id", "{id}"), ("practice_priority", "3")],
            },
            open_until: Some("PR 6"),
        },
    },
    // --- Workflow jobs API ---
    MutatingRoute {
        method: "POST",
        path: "/api/workflows",
        kind: RouteKind::OwnerProtected {
            seed: Seed::Owned(ResourceType::LiveSet),
            content_type: JSON,
            body: r#"{"target_type":"live_set","target_id_or_path":"{id}","operation":"repomix"}"#,
            mutated: Mutation::JobEnqueued,
            open_until: Some("PR 8"),
        },
    },
];

// ---------------------------------------------------------------------------
// Static coverage: every mutating route in configure_app must be classified.
// ---------------------------------------------------------------------------

/// Scan `src/app.rs` for the routes registered by `configure_app` and return
/// the (method, path) pairs that mutate state. Hand-rolled scan of
/// `.route("PATH", web::post()...)` entries plus the `web::resource("PATH")`
/// block whose inner `.route(web::post()...)` calls carry no path literal.
fn registered_mutating_routes() -> BTreeSet<(String, String)> {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app.rs"))
        .expect("src/app.rs readable");
    let start = source
        .find("pub fn configure_app")
        .expect("configure_app exists in src/app.rs");
    let body = &source[start..];

    // Split into segments starting at each `.route(` or `.service(` call.
    let mut split_points: Vec<usize> = Vec::new();
    for needle in [".route(", ".service("] {
        split_points.extend(body.match_indices(needle).map(|(i, _)| i));
    }
    split_points.sort_unstable();

    let mut routes = BTreeSet::new();
    let mut current_resource: Option<String> = None;

    for (idx, &seg_start) in split_points.iter().enumerate() {
        let end = split_points.get(idx + 1).copied().unwrap_or(body.len());
        let segment = &body[seg_start..end];

        if segment.starts_with(".service(") {
            if let Some(marker) = segment.find("web::resource(") {
                current_resource = read_string_literal(&segment[marker + "web::resource(".len()..]);
            }
            continue;
        }

        // `.route(` — the path is the first string literal; when it opens with
        // something else (e.g. `web::post()`) it belongs to the enclosing
        // `web::resource` block.
        let path =
            read_string_literal(&segment[".route(".len()..]).or_else(|| current_resource.clone());

        for (marker, method) in [
            ("web::post()", "POST"),
            ("web::put()", "PUT"),
            ("web::delete()", "DELETE"),
            ("web::patch()", "PATCH"),
        ] {
            if segment.contains(marker) {
                let path = path
                    .clone()
                    .expect("mutating route must have a path literal or enclosing web::resource");
                routes.insert((method.to_string(), path));
            }
        }
    }
    routes
}

/// Read a `"..."` string literal at the start of `s` (after whitespace);
/// returns `None` when the next token is not a string literal.
fn read_string_literal(s: &str) -> Option<String> {
    let s = s.trim_start();
    if !s.starts_with('"') {
        return None;
    }
    let end = s[1..].find('"')? + 1;
    Some(s[1..end].to_string())
}

#[test]
fn all_mutating_routes_are_classified() {
    let registered = registered_mutating_routes();
    let manifest: BTreeSet<(String, String)> = MUTATING_ROUTES
        .iter()
        .map(|r| (r.method.to_string(), r.path.to_string()))
        .collect();
    assert_eq!(
        manifest.len(),
        MUTATING_ROUTES.len(),
        "duplicate manifest entries"
    );
    assert_eq!(
        registered, manifest,
        "mutating routes in configure_app and MUTATING_ROUTES diverged — \
         classify any new mutating route in tests/route_authz_coverage_tests.rs"
    );
}

// ---------------------------------------------------------------------------
// Service-level scenarios (require a real PocketBase instance).
// ---------------------------------------------------------------------------

const TEST_USER_A_EMAIL: &str = "acl-test-user-1@example.com";
const TEST_USER_A_PASSWORD: &str = "AclTestUser1Password!";
const TEST_USER_B_EMAIL: &str = "acl-test-user-2@example.com";
const TEST_USER_B_PASSWORD: &str = "AclTestUser2Password!";

fn pocketbase_test_url() -> Option<String> {
    std::env::var("POCKETBASE_TEST_URL").ok()
}

macro_rules! require_pocketbase_or_skip {
    () => {
        match pocketbase_test_url() {
            Some(url) => url,
            None => {
                eprintln!(
                    "skipping {}: POCKETBASE_TEST_URL is not set. Run via \
                     `music_browser/scripts/run-pocketbase-integration-tests.sh`.",
                    module_path!()
                );
                return;
            }
        }
    };
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

/// The anonymous CSRF session carried by every test request: a `pre-session`
/// cookie, the `CSRF-ANON` cookie, and the matching `X-CSRF-Token` value.
/// Authenticated requests use a `Bearer` header (not the `auth_token`/`id`
/// cookies) so they stay in the anonymous CSRF class.
struct CsrfSession {
    token: String,
    cookie_header: String,
}

struct Harness {
    server: actix_test::TestServer,
    pool: SqlitePool,
    pb: PocketBaseClient,
    pb_url: String,
    store: JobStore,
    user_a: TestUser,
    user_b: TestUser,
    csrf: CsrfSession,
}

async fn start_harness(pb_url: &str) -> (Harness, NamedTempFile) {
    let (pool, tmp) = test_pool().await;
    let pb = PocketBaseClient::new(pb_url.to_string(), reqwest::Client::new());
    let config = auth_config(pb_url);
    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"route-coverage-test-csrf-secret-32B");
    let (queue, _receiver) = JobQueue::new(16);
    let store = queue.store.clone();

    let pool_data = web::Data::new(pool.clone());
    let auth_data = web::Data::new(config.clone());
    let pb_data = web::Data::new(pb.clone());
    let queue_data = web::Data::new(queue);
    let store_data = web::Data::new(store.clone());
    let csrf_data = csrf_config.clone();
    let factory_config = config.clone();

    let server = actix_test::start_with(
        // The authz assertions key off 303 redirect statuses — the client must
        // not follow them.
        actix_test::config().disable_redirects(),
        move || {
            App::new()
                .app_data(pool_data.clone())
                .app_data(auth_data.clone())
                .app_data(pb_data.clone())
                .app_data(queue_data.clone())
                .app_data(store_data.clone())
                .app_data(csrf_data.clone())
                .wrap(middleware::Condition::new(
                    factory_config.require_login,
                    JwtMiddleware::new(factory_config.clone()),
                ))
                .wrap(CsrfMiddleware::new(csrf_data.clone()))
                .configure(app::configure_app)
        },
    );

    let csrf = csrf_session(&server).await;
    let user_a = authenticate(pb_url, TEST_USER_A_EMAIL, TEST_USER_A_PASSWORD).await;
    let user_b = authenticate(pb_url, TEST_USER_B_EMAIL, TEST_USER_B_PASSWORD).await;

    (
        Harness {
            server,
            pool,
            pb,
            pb_url: pb_url.to_string(),
            store,
            user_a,
            user_b,
            csrf,
        },
        tmp,
    )
}

fn auth_config(pocketbase_url: &str) -> AuthConfig {
    AuthConfig {
        pocketbase_url: pocketbase_url.to_string(),
        cookie_secure: false,
        require_login: true,
        pocketbase_ca_cert: None,
        public_paths: vec!["/login".into(), "/signup".into(), "/logout".into()],
    }
}

/// GET /login (a public path) once to establish the anonymous CSRF session.
async fn csrf_session(server: &actix_test::TestServer) -> CsrfSession {
    let mut resp = server
        .get("/login")
        .send()
        .await
        .expect("GET /login response");
    let mut csrf_anon = String::new();
    let mut pre_session = String::new();
    for value in resp.headers().get_all(header::SET_COOKIE) {
        let value = value.to_str().expect("set-cookie is ascii");
        let pair = value.split(';').next().unwrap_or("");
        if let Some((name, val)) = pair.split_once('=') {
            match name {
                "CSRF-ANON" => csrf_anon = val.to_string(),
                "pre-session" => pre_session = val.to_string(),
                _ => {}
            }
        }
    }
    let body = resp.body().await.expect("login body");
    let body = String::from_utf8(body.to_vec()).expect("utf8 login body");
    let token = body
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("csrf_token field in login page")
        .to_string();
    CsrfSession {
        token,
        cookie_header: format!("pre-session={pre_session}; CSRF-ANON={csrf_anon}"),
    }
}

/// Send a mutating request through the real middleware stack. Returns the
/// response status and `Location` header.
async fn send(
    h: &Harness,
    method: &'static str,
    path: &str,
    bearer: Option<&str>,
    content_type: &'static str,
    body: &str,
) -> (StatusCode, Option<String>) {
    let req = match method {
        "PUT" => h.server.put(path),
        "DELETE" => h.server.request(Method::DELETE, path),
        _ => h.server.post(path),
    };
    let mut req = req
        .insert_header(("X-CSRF-Token", h.csrf.token.clone()))
        .insert_header(("Cookie", h.csrf.cookie_header.clone()));
    if let Some(token) = bearer {
        req = req.insert_header(("Authorization", format!("Bearer {token}")));
    }
    let mut resp = req
        .insert_header(("Content-Type", content_type))
        .send_body(body.to_string())
        .await
        .expect("request sent");
    let status = resp.status();
    let location = resp
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    // Drain the body so the connection is reusable.
    let _ = resp.body().await;
    (status, location)
}

fn interpolate(template: &str, fixture: &Fixture, b_id: &str) -> String {
    template
        .replace("{id}", &fixture.id)
        .replace("{other}", &fixture.other)
        .replace("{b}", b_id)
}

// ---------------------------------------------------------------------------
// Seeding & mutation observation
// ---------------------------------------------------------------------------

/// The ids a seed produced. `id` substitutes `{id}` in paths/bodies, `other`
/// substitutes `{other}`, and `shareable` records the `(type, id)` used by
/// `Mutation::ShareVisibleToB`.
#[derive(Clone)]
struct Fixture {
    id: String,
    other: String,
    shareable: Option<(ResourceType, i64)>,
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

async fn grant_admin(h: &Harness, rt: ResourceType, id: i64) {
    h.pb.create_share(
        &h.user_a.token,
        &CreateShare {
            user_id: h.user_a.id.clone(),
            resource_type: rt.as_str().to_string(),
            resource_id: id.to_string(),
            access_level: AccessLevel::Admin,
            created_by: h.user_a.id.clone(),
        },
    )
    .await
    .expect("seed admin share for owner");
}

async fn seed_owned_song(h: &Harness) -> i64 {
    let id = insert_row(
        &h.pool,
        "INSERT INTO songs (title) VALUES ('owned-song')",
        &[],
    )
    .await;
    grant_admin(h, ResourceType::Song, id).await;
    id
}

async fn run_seed(seed: Seed, h: &Harness) -> Fixture {
    match seed {
        Seed::None => Fixture {
            id: "1".into(),
            other: "1".into(),
            shareable: None,
        },
        Seed::Owned(rt) => {
            let id = match rt {
                ResourceType::Song => seed_owned_song(h).await,
                ResourceType::Album => {
                    insert_row(
                        &h.pool,
                        "INSERT INTO albums (title, released, url) VALUES ('a', 0, '')",
                        &[],
                    )
                    .await
                }
                ResourceType::Artist => {
                    insert_row(&h.pool, "INSERT INTO artists (name) VALUES ('a')", &[]).await
                }
                ResourceType::Instrument => {
                    insert_row(
                        &h.pool,
                        "INSERT INTO instruments (name, instrument_type) VALUES ('a', 'guitar')",
                        &[],
                    )
                    .await
                }
                ResourceType::Recording => {
                    let song = insert_row(
                        &h.pool,
                        "INSERT INTO songs (title) VALUES ('carrier')",
                        &[],
                    )
                    .await;
                    insert_row(
                        &h.pool,
                        "INSERT INTO recordings (recording_type, path, song_id) VALUES ('wav', '/tmp/x', ?)",
                        &[song],
                    )
                    .await
                }
                ResourceType::Band => {
                    insert_row(&h.pool, "INSERT INTO bands (name) VALUES ('b')", &[]).await
                }
                ResourceType::PracticeExercise => {
                    insert_row(
                        &h.pool,
                        "INSERT INTO practice_exercises (name, category) VALUES ('x', 'technique')",
                        &[],
                    )
                    .await
                }
                ResourceType::Goal => {
                    insert_row(
                        &h.pool,
                        "INSERT INTO goals (horizon, category, title) VALUES ('1_week', 'general', 'g')",
                        &[],
                    )
                    .await
                }
                ResourceType::LiveSet => {
                    insert_row(&h.pool, "INSERT INTO live_sets (name) VALUES ('s')", &[]).await
                }
                other => panic!("no seed recipe for ResourceType::{other:?}"),
            };
            if rt != ResourceType::Song {
                grant_admin(h, rt, id).await;
            }
            Fixture {
                id: id.to_string(),
                other: String::new(),
                shareable: Some((rt, id)),
            }
        }
        Seed::OwnedSetWithSong => {
            let set = insert_row(&h.pool, "INSERT INTO live_sets (name) VALUES ('s')", &[]).await;
            grant_admin(h, ResourceType::LiveSet, set).await;
            let song = insert_row(
                &h.pool,
                "INSERT INTO songs (title) VALUES ('set-candidate')",
                &[],
            )
            .await;
            Fixture {
                id: set.to_string(),
                other: song.to_string(),
                shareable: Some((ResourceType::LiveSet, set)),
            }
        }
        Seed::JoinUnderOwnedSet => {
            let set = insert_row(&h.pool, "INSERT INTO live_sets (name) VALUES ('s')", &[]).await;
            grant_admin(h, ResourceType::LiveSet, set).await;
            let song = insert_row(
                &h.pool,
                "INSERT INTO songs (title) VALUES ('set-member')",
                &[],
            )
            .await;
            let join = insert_row(
                &h.pool,
                "INSERT INTO live_set_songs (set_id, song_id) VALUES (?, ?)",
                &[set, song],
            )
            .await;
            Fixture {
                id: join.to_string(),
                other: String::new(),
                shareable: Some((ResourceType::LiveSet, set)),
            }
        }
        Seed::StageUnderOwnedSong => {
            let song = seed_owned_song(h).await;
            let stage = insert_row(
                &h.pool,
                "INSERT INTO production_stages (song_id, stage, status) VALUES (?, 'writing', 'not_started')",
                &[song],
            )
            .await;
            Fixture {
                id: stage.to_string(),
                other: String::new(),
                shareable: Some((ResourceType::Song, song)),
            }
        }
        Seed::StepUnderOwnedSong => {
            let song = seed_owned_song(h).await;
            let stage = insert_row(
                &h.pool,
                "INSERT INTO production_stages (song_id, stage, status) VALUES (?, 'writing', 'not_started')",
                &[song],
            )
            .await;
            let step = insert_row(
                &h.pool,
                "INSERT INTO production_steps (stage_id, name) VALUES (?, 'x')",
                &[stage],
            )
            .await;
            Fixture {
                id: step.to_string(),
                other: String::new(),
                shareable: Some((ResourceType::Song, song)),
            }
        }
        Seed::FileUnderOwnedSong => {
            let song = seed_owned_song(h).await;
            let file = insert_row(
                &h.pool,
                "INSERT INTO song_files (song_id, file_type, path) VALUES (?, 'lyrics', '/tmp/x')",
                &[song],
            )
            .await;
            Fixture {
                id: file.to_string(),
                other: String::new(),
                shareable: Some((ResourceType::Song, song)),
            }
        }
        Seed::OwnedGroup => {
            let group = create_group(h).await;
            Fixture {
                id: group,
                other: String::new(),
                shareable: None,
            }
        }
        Seed::OwnedGroupAndSong => {
            let group = create_group(h).await;
            let song = seed_owned_song(h).await;
            Fixture {
                id: group,
                other: song.to_string(),
                shareable: Some((ResourceType::Song, song)),
            }
        }
        Seed::JournalEntry => {
            let goal = insert_row(
                &h.pool,
                "INSERT INTO goals (horizon, category, title) VALUES ('1_week', 'general', 'carrier')",
                &[],
            )
            .await;
            let entry = insert_row(
                &h.pool,
                "INSERT INTO journal_entries (entry_date, entry_type, goal_id) VALUES ('2030-01-01', 'goal', ?)",
                &[goal],
            )
            .await;
            Fixture {
                id: entry.to_string(),
                other: String::new(),
                shareable: None,
            }
        }
        Seed::ScheduleItem => {
            let event = insert_row(
                &h.pool,
                "INSERT INTO schedule_events (event_date, title) VALUES ('2030-01-06', 'e')",
                &[],
            )
            .await;
            let item = insert_row(
                &h.pool,
                "INSERT INTO schedule_items (event_id, item_type, title) VALUES (?, 'warmup', 'w')",
                &[event],
            )
            .await;
            Fixture {
                id: item.to_string(),
                other: String::new(),
                shareable: None,
            }
        }
        Seed::ScheduleEvent => {
            let event = insert_row(
                &h.pool,
                "INSERT INTO schedule_events (event_date, title) VALUES ('2030-01-06', 'e')",
                &[],
            )
            .await;
            Fixture {
                id: event.to_string(),
                other: String::new(),
                shareable: None,
            }
        }
    }
}

async fn create_group(h: &Harness) -> String {
    let created =
        h.pb.create_group(
            &h.user_a.token,
            &CreateGroup {
                name: format!("coverage-{}", Uuid::new_v4()),
                description: "route coverage".to_string(),
                owner_id: h.user_a.id.clone(),
            },
        )
        .await
        .expect("seed group");
    created["id"].as_str().expect("group id").to_string()
}

/// Whether the mutation `m` has happened for the fixture.
async fn mutation_applied(m: Mutation, fixture: &Fixture, h: &Harness) -> bool {
    match m {
        Mutation::RowDeleted(table) => {
            let count: i64 = sqlx::query_scalar(&format!(
                "SELECT COUNT(*) FROM {table} WHERE CAST(id AS TEXT) = ?"
            ))
            .bind(&fixture.id)
            .fetch_one(&h.pool)
            .await
            .expect("row count");
            count == 0
        }
        Mutation::RowMatching { table, conditions } => {
            let mut sql = format!("SELECT COUNT(*) FROM {table} WHERE 1=1");
            let mut values: Vec<String> = Vec::new();
            for (col, expected) in conditions.iter() {
                sql.push_str(&format!(" AND CAST({col} AS TEXT) = ?"));
                values.push(
                    expected
                        .replace("{id}", &fixture.id)
                        .replace("{other}", &fixture.other),
                );
            }
            let mut q = sqlx::query_scalar::<_, i64>(&sql);
            for v in values {
                q = q.bind(v);
            }
            q.fetch_one(&h.pool).await.expect("row matching") > 0
        }
        Mutation::ShareVisibleToB => {
            let (rt, id) = fixture.shareable.expect("shareable fixture");
            let b = AuthenticatedUser {
                id: h.user_b.id.clone(),
                token: h.user_b.token.clone(),
            };
            permissions::check_user_access(&h.pb, &b, rt, id)
                .await
                .expect("share check")
                .is_some()
        }
        Mutation::GroupMembershipForB => {
            h.pb.get_group_members(&h.user_b.token, &fixture.id)
                .await
                .expect("group members list")
                .iter()
                .any(|m| m.user_id == h.user_b.id)
        }
        Mutation::GroupShareExists => {
            let filter = urlencoding::encode(&format!("group_id = '{}'", fixture.id)).into_owned();
            let resp = reqwest::Client::new()
                .get(format!(
                    "{}/api/collections/group_shares/records?filter={filter}",
                    h.pb_url
                ))
                .bearer_auth(&h.user_b.token)
                .send()
                .await
                .expect("group_shares list")
                .json::<Value>()
                .await
                .expect("group_shares json");
            resp["items"]
                .as_array()
                .map(|items| !items.is_empty())
                .unwrap_or(false)
        }
        Mutation::JobEnqueued => h
            .store
            .list()
            .iter()
            .any(|r| r.job.target_id_or_path == fixture.id),
    }
}

fn is_denied(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    )
}

fn is_login_redirect(status: StatusCode, location: Option<&str>) -> bool {
    status == StatusCode::SEE_OTHER && location == Some("/login")
}

// ---------------------------------------------------------------------------
// Scenario 1: no credentials → the JWT middleware redirects to /login.
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn mutating_routes_redirect_unauthenticated_callers() {
    let pb_url = require_pocketbase_or_skip!();
    let (h, _tmp) = start_harness(&pb_url).await;

    let mut failures = Vec::new();
    for route in MUTATING_ROUTES {
        if matches!(route.kind, RouteKind::Public) {
            continue;
        }
        let path = route.path.replace("{id}", "1");
        let (status, location) = send(&h, route.method, &path, None, FORM, "").await;
        if !is_login_redirect(status, location.as_deref()) {
            failures.push(format!(
                "{} {}: unauthenticated request got {} (location {:?}), expected 303 /login",
                route.method, route.path, status, location
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------
// Scenario 2: owner-protected routes deny "user B" on user A's resources.
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn owner_protected_routes_enforce_ownership() {
    let pb_url = require_pocketbase_or_skip!();
    let (h, _tmp) = start_harness(&pb_url).await;

    let mut failures = Vec::new();
    for route in MUTATING_ROUTES {
        let RouteKind::OwnerProtected {
            seed,
            content_type,
            body,
            mutated,
            open_until,
        } = route.kind
        else {
            continue;
        };

        let fixture = run_seed(seed, &h).await;
        let path = interpolate(route.path, &fixture, &h.user_b.id);
        let request_body = interpolate(body, &fixture, &h.user_b.id);
        let label = format!("{} {}", route.method, route.path);

        // Unauthenticated requests are rejected by the middleware.
        let (status, location) =
            send(&h, route.method, &path, None, content_type, &request_body).await;
        if !is_login_redirect(status, location.as_deref()) {
            failures.push(format!(
                "{label}: unauthenticated request got {status} (location {location:?}), expected 303 /login"
            ));
        }

        // User B (authenticated, no access to A's resource).
        let (status, _) = send(
            &h,
            route.method,
            &path,
            Some(&h.user_b.token),
            content_type,
            &request_body,
        )
        .await;
        let applied = mutation_applied(mutated, &fixture, &h).await;

        match open_until {
            Some(pr) => {
                // Known open gap: the request is currently allowed. Once `pr`
                // lands, flip `open_until` to None.
                if is_denied(status) || !applied {
                    failures.push(format!(
                        "{label}: expected the known-open gap ({pr}) to still allow user B — \
                         got {status}, mutation applied = {applied}. If {pr} just landed, \
                         flip open_until to None."
                    ));
                }
            }
            None => {
                if !is_denied(status) || applied {
                    failures.push(format!(
                        "{label}: user B must be denied (401/403/404) and the resource \
                         unchanged — got {status}, mutation applied = {applied}"
                    ));
                }
                // The owner (user A) must still be able to perform the action.
                let (status, _) = send(
                    &h,
                    route.method,
                    &path,
                    Some(&h.user_a.token),
                    content_type,
                    &request_body,
                )
                .await;
                let applied = mutation_applied(mutated, &fixture, &h).await;
                if is_denied(status) || !applied {
                    failures.push(format!(
                        "{label}: owner (user A) must be allowed — got {status}, \
                         mutation applied = {applied}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------
// Scenario 3: create/global routes allow any authenticated caller.
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn create_and_global_routes_allow_authenticated_users() {
    let pb_url = require_pocketbase_or_skip!();
    let (h, _tmp) = start_harness(&pb_url).await;

    let mut failures = Vec::new();
    for route in MUTATING_ROUTES {
        match route.kind {
            RouteKind::Creates { content_type, body } => {
                let label = format!("{} {}", route.method, route.path);
                let (status, _) = send(
                    &h,
                    route.method,
                    route.path,
                    Some(&h.user_b.token),
                    content_type,
                    body,
                )
                .await;
                if is_denied(status) {
                    failures.push(format!(
                        "{label}: create route must accept authenticated callers — got {status}"
                    ));
                }
            }
            RouteKind::LoginOnly {
                seed,
                content_type,
                body,
                mutated,
            } => {
                let fixture = run_seed(seed, &h).await;
                let path = interpolate(route.path, &fixture, &h.user_b.id);
                let request_body = interpolate(body, &fixture, &h.user_b.id);
                let label = format!("{} {}", route.method, route.path);

                let (status, _) = send(
                    &h,
                    route.method,
                    &path,
                    Some(&h.user_b.token),
                    content_type,
                    &request_body,
                )
                .await;
                let applied = mutation_applied(mutated, &fixture, &h).await;
                if is_denied(status) || !applied {
                    failures.push(format!(
                        "{label}: global-state route must accept authenticated callers and \
                         apply the mutation — got {status}, mutation applied = {applied}"
                    ));
                }
            }
            _ => continue,
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
