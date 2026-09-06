use actix_web::cookie::SameSite;
use actix_web::{middleware, web, App, HttpServer};
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};

use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig};

// Import everything from the library crate to avoid shadowing conflicts
use music_browser::{app, auth};

use music_browser::db::pool;
use music_browser::jobs::{run_worker, JobQueue};
use music_browser::pocketbase_client::PocketBaseClient;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:music_browser.db".into());

    let pool = pool::init_pool(&database_url)
        .await
        .expect("Failed to initialise database");

    let pool_data = web::Data::new(pool);
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let auth_config = auth::AuthConfig::from_env();
    let auth_data = web::Data::new(auth_config.clone());
    let pocketbase_client = PocketBaseClient::from_auth_config(&auth_config)
        .expect("Failed to initialise PocketBase client");
    let pocketbase_data = web::Data::new(pocketbase_client);

    // CSRF middleware configuration
    let csrf_secret = std::env::var("CSRF_SECRET")
        .unwrap_or_else(|_| "change-me-to-a-secure-random-32-byte-secret".to_string());

    // HTTPS configuration
    let https_enabled = std::env::var("HTTPS_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    // CSRF cookie secure flag - must be true when using HTTPS
    let cookie_secure = std::env::var("CSRF_COOKIE_SECURE")
        .unwrap_or_else(|_| if https_enabled { "true" } else { "false" }.to_string())
        .parse::<bool>()
        .unwrap_or(https_enabled);

    // Use Strict for HTTPS, Lax for HTTP
    let same_site = if cookie_secure {
        SameSite::Strict
    } else {
        SameSite::Lax
    };

    let csrf_config = CsrfMiddlewareConfig::double_submit_cookie(csrf_secret.as_bytes())
        .with_token_cookie_config(actix_csrf_middleware::CsrfDoubleSubmitCookie {
            http_only: false,
            same_site,
        })
        .with_secure(cookie_secure);

    if csrf_secret == "change-me-to-a-secure-random-32-byte-secret" {
        log::warn!(
            "⚠️  SECURITY WARNING: Using default CSRF_SECRET. Generate a secure 32+ byte secret for production (e.g., `openssl rand -base64 32`)."
        );
    }

    let (job_queue, job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    tokio::spawn(run_worker(job_receiver, job_store.clone()));
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    log::info!("JobStore registered as app_data");

    let protocol = if https_enabled { "https" } else { "http" };
    log::info!("Listening on {}://{bind}", protocol);

    let server = HttpServer::new(move || {
        let request_auth = auth_data.require_login;

        App::new()
            .app_data(pool_data.clone())
            .app_data(auth_data.clone())
            .app_data(pocketbase_data.clone())
            .app_data(queue_data.clone())
            .app_data(store_data.clone())
            .app_data(csrf_config.clone())
            .wrap(middleware::Condition::new(
                request_auth,
                auth::JwtMiddleware::new(auth_config.clone()),
            ))
            .wrap(CsrfMiddleware::new(csrf_config.clone()))
            .configure(app::configure_app)
    });

    let server = if https_enabled {
        let cert_path =
            std::env::var("SSL_CERT_PATH").unwrap_or_else(|_| "./certs/server.crt".to_string());
        let key_path =
            std::env::var("SSL_KEY_PATH").unwrap_or_else(|_| "./certs/server.key".to_string());

        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        builder
            .set_certificate_file(&cert_path, SslFiletype::PEM)
            .expect("Failed to load SSL certificate");
        builder
            .set_private_key_file(&key_path, SslFiletype::PEM)
            .expect("Failed to load SSL private key");

        server.bind_openssl(&bind, builder)?
    } else {
        server.bind(&bind)?
    };

    server.run().await
}
