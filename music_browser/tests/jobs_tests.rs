use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig};
use actix_web::{middleware, test, web, App};
use music_browser::app;
use music_browser::auth::AuthConfig;
use music_browser::db::pool;
use music_browser::jobs::JobQueue;

#[actix_web::test]
async fn test_jobs_endpoint_with_full_production_config() {
    let (job_queue, _job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    let auth_config = music_browser::auth::AuthConfig::from_env();
    let auth_data = web::Data::new(auth_config.clone());

    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_skip_for(vec!["/workflow".to_string()]);

    // Add pool_data like production
    let pool_data = web::Data::new(pool::init_pool("sqlite::memory:").await.unwrap());

    let app = test::init_service(
        App::new()
            .app_data(pool_data.clone())
            .app_data(auth_data.clone())
            .app_data(store_data.clone())
            .app_data(queue_data.clone())
            // Disable JWT for testing - we know the issue happens when JWT is enabled
            // but we can't easily create a valid JWT token in tests
            .wrap(CsrfMiddleware::new(csrf_config))
            .configure(app::configure_app),
    )
    .await;

    // First, get the login page to establish CSRF session
    let login_req = test::TestRequest::get().uri("/login").to_request();
    let login_resp = test::call_service(&app, login_req).await;

    // Extract CSRF cookie
    let csrf_cookie_name = login_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.name().to_string())
        .expect("CSRF cookie not found");
    let csrf_cookie_value = login_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.value().to_string())
        .expect("CSRF cookie value not found");

    // Extract pre-session cookie
    let pre_session_cookie_value = login_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "pre-session")
        .map(|cookie| cookie.value().to_string());

    let login_body =
        String::from_utf8(test::read_body(login_resp).await.to_vec()).expect("utf8 body");
    let csrf_token = login_body
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split("\"").next())
        .expect("CSRF token not found in response");

    // Reconstruct the cookie with proper attributes
    let mut csrf_cookie = actix_web::cookie::Cookie::new(csrf_cookie_name, csrf_cookie_value);
    csrf_cookie.set_same_site(actix_web::cookie::SameSite::Lax);
    csrf_cookie.set_http_only(false);

    // Build the request with CSRF cookie
    let req = test::TestRequest::get().uri("/jobs").cookie(csrf_cookie);

    // Include pre-session cookie if present
    let req = if let Some(pre_session_value) = pre_session_cookie_value {
        let mut pre_session_cookie =
            actix_web::cookie::Cookie::new("pre-session", pre_session_value);
        pre_session_cookie.set_http_only(true);
        pre_session_cookie.set_secure(true);
        pre_session_cookie.set_same_site(actix_web::cookie::SameSite::Strict);
        req.cookie(pre_session_cookie)
    } else {
        req
    };

    let req = req.to_request();
    let resp = test::call_service(&app, req).await;

    println!(
        "Full production config (no JWT) - Response status: {}",
        resp.status()
    );
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    println!(
        "Full production config (no JWT) - Response body length: {}",
        body.len()
    );

    // This test should pass (no JWT), but production fails with JWT enabled
    // This confirms the issue is specifically with JWT + CSRF middleware combination
}

#[actix_web::test]
async fn test_jobs_endpoint_with_pool_data() {
    let (job_queue, _job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    let auth_config = music_browser::auth::AuthConfig::from_env();
    let auth_data = web::Data::new(auth_config);

    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_skip_for(vec!["/workflow".to_string()]);

    // Add pool_data like production
    let pool_data = web::Data::new(pool::init_pool("sqlite::memory:").await.unwrap());

    let app = test::init_service(
        App::new()
            .app_data(pool_data.clone())
            .app_data(auth_data.clone())
            .app_data(store_data.clone())
            .app_data(queue_data.clone())
            .wrap(middleware::Condition::new(
                false, // Disable JWT
                music_browser::auth::JwtMiddleware::new(
                    auth_data.get_ref().clone(),
                    web::Data::new(music_browser::auth::TokenVerifyCache::default()),
                ),
            ))
            .wrap(CsrfMiddleware::new(csrf_config))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::get().uri("/jobs").to_request();
    let resp = test::call_service(&app, req).await;

    println!("With pool_data - Response status: {}", resp.status());
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    println!("With pool_data - Response body: {}", body);
}

#[actix_web::test]
async fn test_jobs_endpoint_with_jobstore() {
    let (job_queue, _job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    let app = test::init_service(
        App::new()
            .app_data(store_data.clone())
            .app_data(queue_data.clone())
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::get().uri("/jobs").to_request();
    let resp = test::call_service(&app, req).await;

    println!("Response status: {}", resp.status());
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    println!("Response body: {}", body);
}

#[actix_web::test]
async fn test_job_detail_endpoint_with_full_production_config() {
    let (job_queue, _job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    let auth_config = music_browser::auth::AuthConfig::from_env();
    let auth_data = web::Data::new(auth_config.clone());

    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_skip_for(vec!["/workflow".to_string()]);

    let pool_data = web::Data::new(pool::init_pool("sqlite::memory:").await.unwrap());

    let app = test::init_service(
        App::new()
            .app_data(pool_data.clone())
            .app_data(auth_data.clone())
            .app_data(store_data.clone())
            .app_data(queue_data.clone())
            .wrap(CsrfMiddleware::new(csrf_config))
            .configure(app::configure_app),
    )
    .await;

    // First, get the login page to establish CSRF session
    let login_req = test::TestRequest::get().uri("/login").to_request();
    let login_resp = test::call_service(&app, login_req).await;

    // Extract CSRF cookie
    let csrf_cookie_name = login_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.name().to_string())
        .expect("CSRF cookie not found");
    let csrf_cookie_value = login_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.value().to_string())
        .expect("CSRF cookie value not found");

    let mut csrf_cookie = actix_web::cookie::Cookie::new(csrf_cookie_name, csrf_cookie_value);
    csrf_cookie.set_same_site(actix_web::cookie::SameSite::Lax);
    csrf_cookie.set_http_only(false);

    // Test job detail endpoint with a non-existent job ID (should return 404)
    let req = test::TestRequest::get()
        .uri("/jobs/999")
        .cookie(csrf_cookie)
        .to_request();
    let resp = test::call_service(&app, req).await;

    println!(
        "Job detail (non-existent) - Response status: {}",
        resp.status()
    );
    assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_FOUND);
}

#[actix_web::test]
async fn test_jobs_endpoint_with_csrf_middleware() {
    let (job_queue, _job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_skip_for(vec!["/jobs".to_string(), "/workflow".to_string()]);

    let app = test::init_service(
        App::new()
            .app_data(store_data.clone())
            .app_data(queue_data.clone())
            .wrap(CsrfMiddleware::new(csrf_config))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::get().uri("/jobs").to_request();
    let resp = test::call_service(&app, req).await;

    println!("With CSRF middleware - Response status: {}", resp.status());
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    println!("With CSRF middleware - Response body: {}", body);
}

#[actix_web::test]
async fn test_jobs_endpoint_with_auth_data() {
    let (job_queue, _job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    let auth_config = music_browser::auth::AuthConfig::from_env();
    let auth_data = web::Data::new(auth_config);

    let app = test::init_service(
        App::new()
            .app_data(auth_data.clone())
            .app_data(store_data.clone())
            .app_data(queue_data.clone())
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::get().uri("/jobs").to_request();
    let resp = test::call_service(&app, req).await;

    println!("With auth_data - Response status: {}", resp.status());
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    println!("With auth_data - Response body: {}", body);
}

#[actix_web::test]
async fn test_jobs_endpoint_with_csrf_no_jwt() {
    let (job_queue, _job_receiver) = JobQueue::new(256);
    let job_store = job_queue.store.clone();
    let queue_data = web::Data::new(job_queue);
    let store_data = web::Data::new(job_store);

    let auth_config = music_browser::auth::AuthConfig::from_env();
    let auth_data = web::Data::new(auth_config);

    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_skip_for(vec!["/workflow".to_string()]);

    let app = test::init_service(
        App::new()
            .app_data(auth_data.clone())
            .app_data(store_data.clone())
            .app_data(queue_data.clone())
            .wrap(middleware::Condition::new(
                false, // Disable JWT
                music_browser::auth::JwtMiddleware::new(
                    auth_data.get_ref().clone(),
                    web::Data::new(music_browser::auth::TokenVerifyCache::default()),
                ),
            ))
            .wrap(CsrfMiddleware::new(csrf_config))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::get().uri("/jobs").to_request();
    let resp = test::call_service(&app, req).await;

    println!("With CSRF no JWT - Response status: {}", resp.status());
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    println!("With CSRF no JWT - Response body: {}", body);
}
