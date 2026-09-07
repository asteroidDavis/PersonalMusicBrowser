//! Integration tests for the two authorization-side caches:
//!
//! 1. `TokenVerifyCache` — `JwtMiddleware` verifies each request's token
//!    against PocketBase's auth-refresh endpoint. With the cache, N
//!    requests presenting the same token issue a single PocketBase call
//!    within the TTL; `logout` invalidates the entry so the next request
//!    re-verifies.
//! 2. `RequestAclCache` — `permissions::*_cached` helpers memoize share
//!    lookups in request extensions, so repeated checks of the same
//!    resource within one request issue a single PocketBase call.
//!
//! Both are verified against a counting fake PocketBase: an
//! `actix_test` server that records how often each endpoint was hit.
//! These tests need no real PocketBase and always run.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig, CsrfToken};
use actix_web::http::StatusCode;
use actix_web::{middleware, web, App, HttpRequest, HttpResponse};
use serde_json::json;

use music_browser::acl::ResourceType;
use music_browser::auth::{self, AuthConfig, JwtMiddleware, TokenVerifyCache};
use music_browser::permissions;
use music_browser::pocketbase_client::PocketBaseClient;

const TEST_USER_ID: &str = "fake-user-1";

/// Counts how often each fake PocketBase endpoint is hit.
#[derive(Default)]
struct FakePocketBase {
    refresh_calls: AtomicUsize,
    share_list_calls: AtomicUsize,
}

async fn auth_refresh(counter: web::Data<FakePocketBase>) -> HttpResponse {
    counter.refresh_calls.fetch_add(1, Ordering::SeqCst);
    HttpResponse::Ok().json(json!({
        "record": {"id": TEST_USER_ID},
        "token": "rotated-token",
    }))
}

async fn list_shares(counter: web::Data<FakePocketBase>) -> HttpResponse {
    counter.share_list_calls.fetch_add(1, Ordering::SeqCst);
    HttpResponse::Ok().json(json!({
        "items": [{
            "id": "share-1",
            "user_id": TEST_USER_ID,
            "resource_type": "song",
            "resource_id": "7",
            "access_level": "admin",
            "created_by": TEST_USER_ID,
        }]
    }))
}

fn fake_pocketbase() -> (actix_test::TestServer, web::Data<FakePocketBase>) {
    let counter = web::Data::new(FakePocketBase::default());
    let counter_in_app = counter.clone();
    let server = actix_test::start_with(actix_test::config().disable_redirects(), move || {
        App::new()
            .app_data(counter_in_app.clone())
            .route(
                "/api/collections/users/auth-refresh",
                web::post().to(auth_refresh),
            )
            .route(
                "/api/collections/shares/records",
                web::get().to(list_shares),
            )
    });
    (server, counter)
}

async fn protected() -> HttpResponse {
    HttpResponse::Ok().finish()
}

/// Performs four logical share lookups (two resource-scoped edit checks
/// plus two per-user share lists). All four must collapse into two
/// PocketBase calls — one `list_resource_shares` and one
/// `list_user_shares` — thanks to the request-scoped memo.
async fn cached_checks(
    req: HttpRequest,
    pocketbase: Option<web::Data<PocketBaseClient>>,
) -> actix_web::Result<HttpResponse> {
    permissions::require_edit_access_or_401(&req, pocketbase.as_ref(), ResourceType::Song, 7)
        .await?;
    permissions::require_edit_access_or_401(&req, pocketbase.as_ref(), ResourceType::Song, 7)
        .await?;
    let user = permissions::authenticated_user(&req).expect("authenticated user");
    let pb = pocketbase.as_ref().expect("pocketbase client");
    permissions::user_shares_cached(&req, pb.get_ref(), &user).await?;
    permissions::user_shares_cached(&req, pb.get_ref(), &user).await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Minimal CSRF-token endpoint so tests can run the anonymous-class
/// double-submit-cookie dance for POST /logout.
async fn csrf_form(csrf: CsrfToken) -> HttpResponse {
    HttpResponse::Ok().body(csrf.0)
}

fn app_server(pb_url: &str, token_cache: web::Data<TokenVerifyCache>) -> actix_test::TestServer {
    let config = AuthConfig {
        pocketbase_url: pb_url.to_string(),
        cookie_secure: false,
        require_login: true,
        pocketbase_ca_cert: None,
        public_paths: vec!["/login".into(), "/logout".into(), "/csrf-form".into()],
    };
    let pb_client = web::Data::new(PocketBaseClient::new(
        pb_url.to_string(),
        reqwest::Client::new(),
    ));
    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"auth-cache-test-csrf-secret-32B!");
    let csrf_mw_config = csrf_config.clone();

    actix_test::start_with(actix_test::config().disable_redirects(), move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
            .app_data(pb_client.clone())
            .app_data(token_cache.clone())
            .app_data(csrf_config.clone())
            .wrap(middleware::Condition::new(
                true,
                JwtMiddleware::new(config.clone(), token_cache.clone()),
            ))
            .wrap(CsrfMiddleware::new(csrf_mw_config.clone()))
            .route("/protected", web::get().to(protected))
            .route("/cached-checks", web::get().to(cached_checks))
            .route("/csrf-form", web::get().to(csrf_form))
            .route("/logout", web::post().to(auth::logout))
    })
}

/// GET `path` with a Bearer token; returns the response status.
async fn get_bearer(srv: &actix_test::TestServer, path: &str, token: &str) -> StatusCode {
    srv.get(path)
        .insert_header(("Authorization", format!("Bearer {token}")))
        .send()
        .await
        .expect("bearer request")
        .status()
}

/// Fetches the CSRF session cookies + token for the anonymous class.
async fn csrf_session(srv: &actix_test::TestServer) -> (String, String) {
    let mut resp = srv.get("/csrf-form").send().await.expect("csrf form");
    let cookies = resp
        .headers()
        .get_all(actix_web::http::header::SET_COOKIE)
        .filter_map(|v| v.to_str().ok())
        .filter_map(|v| v.split(';').next())
        .collect::<Vec<_>>()
        .join("; ");
    let token =
        String::from_utf8(resp.body().await.expect("csrf body").to_vec()).expect("utf8 csrf token");
    (cookies, token)
}

#[actix_web::test]
async fn token_verification_is_reused_within_ttl() {
    let (pb, counter) = fake_pocketbase();
    let srv = app_server(&pb.url("/"), web::Data::new(TokenVerifyCache::default()));

    for _ in 0..3 {
        assert_eq!(
            get_bearer(&srv, "/protected", "token-a").await,
            StatusCode::OK
        );
    }
    assert_eq!(
        counter.refresh_calls.load(Ordering::SeqCst),
        1,
        "three requests with one token must verify once"
    );

    // A different token verifies independently.
    assert_eq!(
        get_bearer(&srv, "/protected", "token-b").await,
        StatusCode::OK
    );
    assert_eq!(counter.refresh_calls.load(Ordering::SeqCst), 2);

    // But the rotated token PocketBase returned is cached too, so a
    // cookie-style client presenting it still hits the cache.
    assert_eq!(
        get_bearer(&srv, "/protected", "rotated-token").await,
        StatusCode::OK
    );
    assert_eq!(counter.refresh_calls.load(Ordering::SeqCst), 2);
}

#[actix_web::test]
async fn logout_invalidates_cached_verification() {
    let (pb, counter) = fake_pocketbase();
    let srv = app_server(&pb.url("/"), web::Data::new(TokenVerifyCache::default()));

    assert_eq!(
        get_bearer(&srv, "/protected", "token-a").await,
        StatusCode::OK
    );
    assert_eq!(counter.refresh_calls.load(Ordering::SeqCst), 1);

    let (cookies, csrf) = csrf_session(&srv).await;
    let resp = srv
        .post("/logout")
        .insert_header(("Authorization", "Bearer token-a"))
        .insert_header(("X-CSRF-Token", csrf))
        .insert_header(("Cookie", cookies))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);

    assert_eq!(
        get_bearer(&srv, "/protected", "token-a").await,
        StatusCode::OK
    );
    assert_eq!(
        counter.refresh_calls.load(Ordering::SeqCst),
        2,
        "logout must drop the cached verification so the token re-verifies"
    );
}

#[actix_web::test]
async fn expired_entries_reverify() {
    let (pb, counter) = fake_pocketbase();
    let cache = web::Data::new(TokenVerifyCache::new(Duration::from_millis(25), 64));
    let srv = app_server(&pb.url("/"), cache);

    assert_eq!(
        get_bearer(&srv, "/protected", "token-a").await,
        StatusCode::OK
    );
    assert_eq!(counter.refresh_calls.load(Ordering::SeqCst), 1);

    tokio::time::sleep(Duration::from_millis(60)).await;

    assert_eq!(
        get_bearer(&srv, "/protected", "token-a").await,
        StatusCode::OK
    );
    assert_eq!(counter.refresh_calls.load(Ordering::SeqCst), 2);
}

#[actix_web::test]
async fn repeated_share_checks_in_one_request_hit_pocketbase_once() {
    let (pb, counter) = fake_pocketbase();
    let srv = app_server(&pb.url("/"), web::Data::new(TokenVerifyCache::default()));

    assert_eq!(
        get_bearer(&srv, "/cached-checks", "token-a").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        counter.share_list_calls.load(Ordering::SeqCst),
        2,
        "four logical share lookups in one request must issue two PocketBase \
         calls (one resource-scoped, one user-scoped)"
    );

    // The memo is request-scoped: a second request re-fetches.
    assert_eq!(
        get_bearer(&srv, "/cached-checks", "token-a").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(counter.share_list_calls.load(Ordering::SeqCst), 4);
}
