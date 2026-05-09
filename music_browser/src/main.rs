use actix_web::{middleware, web, App, HttpServer};

use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig};
use music_browser::app;
use music_browser::auth::{self, AuthConfig};
use music_browser::db::pool;
use music_browser::jobs::{run_worker, JobQueue};

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
    let auth_config = AuthConfig::from_env();
    let auth_data = web::Data::new(auth_config.clone());

    // CSRF middleware configuration (can be disabled via CSRF_ENABLED=false for testing)
    let csrf_enabled = std::env::var("CSRF_ENABLED")
        .unwrap_or_else(|_| "true".to_string())
        .parse::<bool>()
        .unwrap_or(true);
    let csrf_config = CsrfMiddlewareConfig::double_submit_cookie(
        std::env::var("CSRF_SECRET")
            .unwrap_or_else(|_| "change-me-to-a-secure-random-32-byte-secret".to_string())
            .as_bytes(),
    )
    .with_skip_for(vec![
        "/login".to_string(),
        "/signup".to_string(),
        "/logout".to_string(),
    ]);

    let (job_queue, job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    tokio::spawn(run_worker(job_receiver, job_store.clone()));
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    log::info!("Listening on http://{bind}");

    HttpServer::new(move || {
        let request_auth = auth_data.require_login;

        App::new()
            .wrap(middleware::Condition::new(
                csrf_enabled,
                CsrfMiddleware::new(csrf_config.clone()),
            ))
            .wrap(middleware::Condition::new(
                request_auth,
                auth::JwtMiddleware::new(auth_data.get_ref().clone()),
            ))
            .app_data(pool_data.clone())
            .app_data(auth_data.clone())
            .app_data(queue_data.clone())
            .app_data(store_data.clone())
            .configure(app::configure_app)
    })
    .bind(&bind)?
    .run()
    .await
}
