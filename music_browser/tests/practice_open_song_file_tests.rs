//! Browser-driven test for issue #65: opening a song's lead sheet link from
//! the Practice page's "Project Files & Backing Tracks" view.
//!
//! This exercises the *actual* link-opening behavior (a real browser
//! following `target="_blank"`), not just that the server renders the right
//! `href`, so it needs a real WebDriver-controlled browser rather than the
//! in-process `actix_web::test` helpers used elsewhere in this test suite.
//!
//! Supports two browsers, selected via `WEBDRIVER_BROWSER`:
//!   - `firefox` (default): via `geckodriver`. Runs headless unless
//!     `WEBDRIVER_HEADLESS=0`. Firefox/geckodriver ship preinstalled on
//!     GitHub Actions' `ubuntu-latest` and `windows-latest` runners
//!     (`$GECKOWEBDRIVER`); locally, `brew install --cask firefox
//!     geckodriver` (macOS) or your distro's packages.
//!   - `safari`: via `safaridriver` (macOS-only, ships with the OS). Only
//!     runs headed (Safari has no headless mode). One-time setup:
//!       1. Safari > Settings > Advanced > "Show features for web developers".
//!       2. Safari > Develop menu > "Allow Remote Automation".
//!       3. `safaridriver --enable` (needs an admin password once).
//!
//! Run via `music_browser/scripts/run-browser-integration-tests.sh`, which
//! starts the right driver on a free port and exports `WEBDRIVER_URL`
//! (`SAFARIDRIVER_URL` also works, for back-compat with earlier
//! Safari-only runs). If neither is set (e.g. a plain `cargo test` run),
//! this test is skipped with a note rather than failing, so it doesn't
//! block unrelated local development.
//!
//! This currently covers only the most basic case from issue #40 (opening a
//! text-note-style lead sheet link in a new tab); other file/link behaviors
//! from #40 should extend this test module rather than duplicating its
//! server/browser setup.

use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig};
use actix_web::{web, App, HttpServer};
use music_browser::app;
use music_browser::auth::AuthConfig;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;
use tempfile::NamedTempFile;
use thirtyfour::common::capabilities::firefox::FirefoxPreferences;
use thirtyfour::error::WebDriverError;
use thirtyfour::{By, Capabilities, DesiredCapabilities, WebDriver, WindowHandle};

/// The lead sheet resource linked from issue #65.
const LEAD_SHEET_URL: &str =
    "https://1drv.ms/t/c/03c89073c048f55d/IQBd9UjAc5DIIIAD0vEEAAAAASvmkMoh7-qXYpJqjYr8RBw?e=c82Miw";

/// A unique token — the shared folder's id, from `LEAD_SHEET_URL`'s path —
/// that OneDrive preserves (case-insensitively, under varying parameter
/// names/shapes depending on the browser) through its `1drv.ms` ->
/// `onedrive.live.com` redirect, so we can recognize we landed on the right
/// resource even if the new tab finishes following that redirect before we
/// inspect its URL.
const LEAD_SHEET_RESOURCE_TOKEN: &str = "03c89073c048f55d";

/// Which browser/driver to exercise, resolved from `WEBDRIVER_BROWSER`
/// (`firefox`, the default, or `safari`).
enum Browser {
    Firefox,
    Safari,
}

impl Browser {
    fn from_env() -> Self {
        match std::env::var("WEBDRIVER_BROWSER") {
            Ok(v) if v.eq_ignore_ascii_case("safari") => Browser::Safari,
            _ => Browser::Firefox,
        }
    }

    /// Build this browser's WebDriver capabilities. Firefox runs headless
    /// unless `WEBDRIVER_HEADLESS=0`, and forces `target="_blank"` links to
    /// open in a new tab (rather than a new OS window) so tab-counting
    /// assertions behave the same across platforms/CI images. Safari has no
    /// headless mode and always opens `target="_blank"` links in a new tab.
    fn capabilities(&self) -> Capabilities {
        match self {
            Browser::Firefox => {
                let mut caps = DesiredCapabilities::firefox();
                let headless = std::env::var("WEBDRIVER_HEADLESS").map_or(true, |v| v != "0");
                if headless {
                    caps.set_headless().expect("infallible");
                }
                let mut prefs = FirefoxPreferences::new();
                prefs
                    .set("browser.link.open_newwindow", 3)
                    .expect("infallible");
                caps.set_preferences(prefs).expect("infallible");
                caps.into()
            }
            Browser::Safari => DesiredCapabilities::safari().into(),
        }
    }
}

/// Returns the WebDriver server URL to test against, or `None` if the caller
/// should skip (no WebDriver instance was set up for this run).
fn webdriver_url() -> Option<String> {
    // `SAFARIDRIVER_URL` is kept as a back-compat alias from when this test
    // only supported Safari.
    std::env::var("WEBDRIVER_URL")
        .ok()
        .or_else(|| std::env::var("SAFARIDRIVER_URL").ok())
}

/// Skips the current test (with an explanatory message) unless a WebDriver
/// instance is available.
macro_rules! require_browser_or_skip {
    () => {
        match webdriver_url() {
            Some(url) => url,
            None => {
                eprintln!(
                    "skipping {}: WEBDRIVER_URL is not set. Run via \
                     `music_browser/scripts/run-browser-integration-tests.sh` \
                     to exercise the real-browser link-opening test.",
                    module_path!()
                );
                return;
            }
        }
    };
}

/// Spin up a temp SQLite DB (migrated) seeded with one song, "Be Nothing",
/// that has a `lead_sheet` file pointing at the real OneDrive URL from issue
/// #65's linked resource.
async fn seed_test_db() -> (SqlitePool, NamedTempFile) {
    let tmp = NamedTempFile::new().expect("failed to create temp db file");
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
        .expect("failed to create pool");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migration failed");

    let song_id: i64 = sqlx::query("INSERT INTO songs (title, song_type) VALUES (?, ?)")
        .bind("Be Nothing")
        .bind("original")
        .execute(&pool)
        .await
        .expect("failed to insert song")
        .last_insert_rowid();

    sqlx::query(
        "INSERT INTO song_files (song_id, file_type, path, description) VALUES (?, ?, ?, ?)",
    )
    .bind(song_id)
    .bind("lead_sheet")
    .bind(LEAD_SHEET_URL)
    .bind("Lead sheet")
    .execute(&pool)
    .await
    .expect("failed to insert song file");

    (pool, tmp)
}

/// Start a real music-browser server (a real TCP listener, not the
/// in-process `actix_web::test` harness) on an OS-assigned port, and return
/// its base URL. Mirrors the single-tenant/no-PocketBase configuration used
/// by the in-process endpoint tests (`tests/endpoint_tests.rs`), which keeps
/// `/practice` fully visible with no auth.
async fn start_test_server(pool: SqlitePool) -> String {
    let pool_data = web::Data::new(pool);
    let auth_config = AuthConfig {
        pocketbase_url: "http://127.0.0.1:1".into(),
        pocketbase_ca_cert: None,
        cookie_secure: false,
        require_login: false,
        public_paths: vec!["/login".into(), "/signup".into(), "/logout".into()],
        workflow_allowed_roots: vec![],
    };
    let auth_data = web::Data::new(auth_config);

    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_secure(false);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(pool_data.clone())
            .app_data(auth_data.clone())
            .wrap(CsrfMiddleware::new(csrf_config.clone()))
            .configure(app::configure_app)
    })
    .bind("127.0.0.1:0")
    .expect("failed to bind test server");

    let port = server.addrs()[0].port();
    tokio::spawn(server.run());

    format!("http://127.0.0.1:{port}")
}

#[actix_web::test]
async fn opening_lead_sheet_from_practice_page_opens_new_tab() {
    let driver_url = require_browser_or_skip!();

    let (pool, _tmp_db) = seed_test_db().await;
    let base_url = start_test_server(pool).await;

    let browser = Browser::from_env();
    let driver = WebDriver::new(driver_url.as_str(), browser.capabilities())
        .await
        .expect(
            "failed to start a WebDriver session — for Firefox, is \
             geckodriver reachable at WEBDRIVER_URL? For Safari, is \
             `safaridriver --enable` set up, and 'Allow Remote Automation' \
             checked in Safari's Develop menu?",
        );

    let result = run_test(&driver, &base_url).await;

    // Always try to close the browser session, even if the test failed.
    let _ = driver.quit().await;
    result.unwrap();
}

async fn run_test(driver: &WebDriver, base_url: &str) -> Result<(), WebDriverError> {
    // Given the "Be Nothing" song with a lead sheet OneDrive path (seeded
    // above) — when we open the /practice page...
    driver.goto(format!("{base_url}/practice")).await?;

    let initial_handles = driver.windows().await?;
    assert_eq!(
        initial_handles.len(),
        1,
        "expected a single tab before opening the lead sheet link"
    );

    // ...and open the song's lead sheet in the "Project Files & Backing
    // Tracks" view...
    let files_heading = driver
        .find(By::XPath(
            "//h4[contains(., 'Project Files & Backing Tracks')]".to_string(),
        ))
        .await?;
    assert!(
        files_heading.text().await?.contains("Project Files"),
        "practice page did not render the 'Project Files & Backing Tracks' section"
    );

    let lead_sheet_link = driver
        .find(By::XPath(format!(
            "//div[contains(@class, 'practice-files')]//a[@href='{LEAD_SHEET_URL}']"
        )))
        .await?;
    lead_sheet_link.click().await?;

    // Then the browser opens a new tab with the song's lead sheet url.
    let new_handle = wait_for_new_window(driver, &initial_handles).await?;
    driver.switch_to_window(new_handle).await?;

    // The new tab starts at "about:blank" until navigation completes, so
    // wait for it to leave that before inspecting the URL. OneDrive's
    // `1drv.ms` short links then redirect to `onedrive.live.com`, and the
    // tab may already have followed that redirect (in a browser-dependent
    // URL shape) by the time we check, so accept either the original short
    // link or a resolved OneDrive URL carrying the resource's token.
    let new_tab_url = wait_for_navigation(driver).await?;
    assert!(
        new_tab_url.starts_with(LEAD_SHEET_URL)
            || new_tab_url
                .to_lowercase()
                .contains(LEAD_SHEET_RESOURCE_TOKEN),
        "expected the new tab to navigate to the lead sheet URL, got: {new_tab_url}"
    );

    Ok(())
}

/// Poll the current tab's URL until it navigates away from "about:blank"
/// (i.e. the browser has started following the clicked link), or time out.
async fn wait_for_navigation(driver: &WebDriver) -> Result<String, WebDriverError> {
    for _ in 0..40 {
        let url = driver.current_url().await?.to_string();
        if url != "about:blank" {
            return Ok(url);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("timed out waiting for the new tab to navigate away from about:blank");
}

/// Poll `driver.windows()` until a window handle not present in
/// `existing_handles` shows up (a new tab having opened), or time out.
async fn wait_for_new_window(
    driver: &WebDriver,
    existing_handles: &[WindowHandle],
) -> Result<WindowHandle, WebDriverError> {
    for _ in 0..40 {
        let handles = driver.windows().await?;
        if let Some(new_handle) = handles.into_iter().find(|h| !existing_handles.contains(h)) {
            return Ok(new_handle);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("timed out waiting for a new tab to open after clicking the lead sheet link");
}
