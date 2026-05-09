use actix_web::{http::StatusCode, middleware, test, web, App, HttpResponse};
use music_browser::app;
use music_browser::auth::{self, AuthConfig};
use serde_json::Value;

fn test_auth_config(pocketbase_url: String) -> AuthConfig {
    AuthConfig {
        pocketbase_url,
        jwt_secret: "test-secret".into(),
        cookie_secure: false,
        require_login: true,
        pocketbase_ca_cert: None,
        public_paths: vec!["/login".into(), "/signup".into(), "/logout".into()],
    }
}

#[actix_web::test]
async fn login_page_renders_signup_link() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(test_auth_config(
                "http://127.0.0.1:1".into(),
            )))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::get().uri("/login").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    assert!(body.contains("/signup"));
    assert!(body.contains("Create an account"));
}

#[actix_web::test]
async fn signup_page_renders_password_requirements() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(test_auth_config(
                "http://127.0.0.1:1".into(),
            )))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::get().uri("/signup").to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    assert!(body.contains("Create Account"));
    assert!(body.contains("Use at least 12 characters."));
    assert!(body.contains("/login"));
}

#[actix_web::test]
async fn signup_rejects_weak_password_without_calling_pocketbase() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(test_auth_config(
                "http://127.0.0.1:1".into(),
            )))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/signup")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload("email=user%40example.com&password=short&password_confirm=short")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    assert!(body.contains("at least 12 characters"));
}

#[actix_web::test]
async fn signup_rejects_password_mismatch_without_calling_pocketbase() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(test_auth_config(
                "http://127.0.0.1:1".into(),
            )))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/signup")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload("email=user%40example.com&password=averylongpassword&password_confirm=differentlongpassword")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    assert!(body.contains("Passwords do not match"));
}

#[actix_web::test]
async fn signup_posts_to_pocketbase_and_redirects_to_login() {
    let pb = actix_test::start(|| {
        App::new().route(
            "/api/collections/users/records",
            web::post().to(|body: web::Json<Value>| async move {
                assert_eq!(body["email"], "user@example.com");
                assert_eq!(body["password"], "averylongpassword");
                assert_eq!(body["passwordConfirm"], "averylongpassword");
                HttpResponse::Ok().json(serde_json::json!({ "id": "generated-user-id" }))
            }),
        )
    });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(test_auth_config(pb.url(""))))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/signup")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload("email=USER%40EXAMPLE.COM&password=averylongpassword&password_confirm=averylongpassword")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("Location").unwrap(), "/login");
}

#[actix_web::test]
async fn signup_returns_unavailable_message_when_pocketbase_cannot_be_reached() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(test_auth_config(
                "http://127.0.0.1:1".into(),
            )))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/signup")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload("email=user%40example.com&password=averylongpassword&password_confirm=averylongpassword")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    assert!(body.contains("Signup service is unavailable. Try again later."));
}

#[actix_web::test]
async fn login_posts_to_pocketbase_sets_cookie_and_redirects_home() {
    let pb = actix_test::start(|| {
        App::new().route(
            "/api/collections/users/auth-with-password",
            web::post().to(|body: web::Json<Value>| async move {
                assert_eq!(body["identity"], "user@example.com");
                assert_eq!(body["password"], "averylongpassword");
                HttpResponse::Ok().json(serde_json::json!({ "token": "pocketbase-token" }))
            }),
        )
    });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(test_auth_config(pb.url(""))))
            .configure(app::configure_app),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/login")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .set_payload("identity=user%40example.com&password=averylongpassword")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("Location").unwrap(), "/");
    let cookie = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "auth_token");
    let cookie = cookie.expect("auth_token cookie should be set");
    assert_eq!(cookie.value(), "pocketbase-token");
    assert!(cookie.http_only().unwrap_or(false));
}

#[actix_web::test]
async fn auth_middleware_allows_public_auth_routes_and_redirects_protected_html() {
    let config = test_auth_config("http://127.0.0.1:1".into());
    let app = test::init_service(
        App::new()
            .wrap(middleware::Condition::new(
                true,
                auth::JwtMiddleware::new(config.clone()),
            ))
            .app_data(web::Data::new(config))
            .configure(app::configure_app),
    )
    .await;

    let signup_req = test::TestRequest::get().uri("/signup").to_request();
    let signup_resp = test::call_service(&app, signup_req).await;
    assert_eq!(signup_resp.status(), StatusCode::OK);

    let protected_req = test::TestRequest::get().uri("/songs").to_request();
    let protected_resp = test::try_call_service(&app, protected_req)
        .await
        .expect_err("protected HTML request without token should be rejected");
    let protected_resp = protected_resp.error_response();
    assert_eq!(protected_resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(protected_resp.headers().get("Location").unwrap(), "/login");
}
