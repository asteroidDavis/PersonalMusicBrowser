use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig};
use actix_web::{http::StatusCode, middleware, test, web, App, HttpResponse};
use music_browser::app;
use music_browser::auth::{self, AuthConfig};
use serde_json::Value;

fn test_auth_config(pocketbase_url: String) -> AuthConfig {
    AuthConfig {
        pocketbase_url,
        cookie_secure: false,
        require_login: false,
        pocketbase_ca_cert: None,
        public_paths: vec!["/login".into(), "/signup".into(), "/logout".into()],
        workflow_allowed_roots: vec![],
    }
}

fn setup_csrf() -> String {
    std::env::var("CSRF_SECRET")
        .unwrap_or_else(|_| "test-csrf-secret-for-integration-tests".to_string())
}

fn csrf_middleware() -> CsrfMiddleware {
    let csrf_secret = setup_csrf();
    // Match the app configuration for local development (HTTP) or HTTPS
    // For testing, we use the same configuration as the app would use with HTTPS
    // to ensure CSRF protection works correctly with secure cookies
    let csrf_config = CsrfMiddlewareConfig::double_submit_cookie(csrf_secret.as_bytes())
        .with_token_cookie_config(actix_csrf_middleware::CsrfDoubleSubmitCookie {
            http_only: false,
            same_site: actix_web::cookie::SameSite::Strict,
        });
    CsrfMiddleware::new(csrf_config)
}

#[actix_web::test]
async fn login_page_renders_signup_link() {
    setup_csrf();
    let app = test::init_service(
        App::new()
            .wrap(csrf_middleware())
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
    setup_csrf();
    let app = test::init_service(
        App::new()
            .wrap(csrf_middleware())
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
    setup_csrf();
    let app = test::init_service(
        App::new()
            .wrap(csrf_middleware())
            .app_data(web::Data::new(test_auth_config(
                "http://127.0.0.1:1".into(),
            )))
            .configure(app::configure_app),
    )
    .await;

    let get_req = test::TestRequest::get().uri("/signup").to_request();
    let get_resp = test::call_service(&app, get_req).await;

    let csrf_cookie_name = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.name().to_string())
        .expect("CSRF cookie not found");
    let csrf_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.value().to_string())
        .expect("CSRF cookie value not found");

    // Extract pre-session cookie
    // The middleware uses a pre-session cookie to derive a session ID for HMAC validation of anonymous users.
    // In a real browser, this cookie is automatically sent with requests. In tests, we must manually extract
    // and include it to replicate browser behavior. This is an acceptable divergence between test and app code
    // since actix test utilities don't automatically forward cookies like a browser does.
    //
    // SECURITY NOTE: In production, the pre-session cookie is set with http_only=true, secure=true, same_site=Strict.
    // For local development (HTTP), we configure the middleware with secure=false to allow cookies over HTTP.
    // This is an insecure configuration that should only be used for local development, not production.
    //
    // FUTURE IMPROVEMENT: Consider using a more realistic user agent (e.g., with cookie jar support) in tests
    // to automatically handle cookie forwarding, reducing the need for manual cookie extraction/inclusion.
    let pre_session_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "pre-session")
        .map(|cookie| cookie.value().to_string());

    let body = String::from_utf8(test::read_body(get_resp).await.to_vec()).expect("utf8 body");

    let csrf_token = body
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split("\"").next())
        .expect("CSRF token not found in response");

    let mut csrf_cookie = actix_web::cookie::Cookie::new(csrf_cookie_name, csrf_cookie_value);
    csrf_cookie.set_same_site(actix_web::cookie::SameSite::Lax);
    csrf_cookie.set_http_only(false);

    let req = test::TestRequest::post()
        .uri("/signup")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .insert_header(("X-CSRF-Token", csrf_token))
        .cookie(csrf_cookie);

    // Include pre-session cookie to replicate browser behavior
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

    let req = req
        .set_payload("email=user%40example.com&password=short&password_confirm=short")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    assert!(body.contains("at least 12 characters"));
}

#[actix_web::test]
async fn signup_rejects_password_mismatch_without_calling_pocketbase() {
    setup_csrf();
    let app = test::init_service(
        App::new()
            .wrap(csrf_middleware())
            .app_data(web::Data::new(test_auth_config(
                "http://127.0.0.1:1".into(),
            )))
            .configure(app::configure_app),
    )
    .await;

    // First, get the CSRF token from the signup page
    let get_req = test::TestRequest::get().uri("/signup").to_request();
    let get_resp = test::call_service(&app, get_req).await;

    // Extract CSRF cookie name and value as strings (to avoid borrow checker error)
    let csrf_cookie_name = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.name().to_string())
        .expect("CSRF cookie not found");
    let csrf_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.value().to_string())
        .expect("CSRF cookie value not found");

    // Extract pre-session cookie
    // The middleware uses a pre-session cookie to derive a session ID for HMAC validation of anonymous users.
    // In a real browser, this cookie is automatically sent with requests. In tests, we must manually extract
    // and include it to replicate browser behavior. This is an acceptable divergence between test and app code
    // since actix test utilities don't automatically forward cookies like a browser does.
    //
    // SECURITY NOTE: In production, the pre-session cookie is set with http_only=true, secure=true, same_site=Strict.
    // For local development (HTTP), we configure the middleware with secure=false to allow cookies over HTTP.
    // This is an insecure configuration that should only be used for local development, not production.
    //
    // FUTURE IMPROVEMENT: Consider using a more realistic user agent (e.g., with cookie jar support) in tests
    // to automatically handle cookie forwarding, reducing the need for manual cookie extraction/inclusion.
    let pre_session_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "pre-session")
        .map(|cookie| cookie.value().to_string());

    let body = String::from_utf8(test::read_body(get_resp).await.to_vec()).expect("utf8 body");

    // Extract CSRF token from the response
    let csrf_token = body
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split("\"").next())
        .expect("CSRF token not found in response");

    // Reconstruct the cookie with proper attributes
    let mut csrf_cookie = actix_web::cookie::Cookie::new(csrf_cookie_name, csrf_cookie_value);
    csrf_cookie.set_same_site(actix_web::cookie::SameSite::Lax);
    csrf_cookie.set_http_only(false); // Must be false for double-submit cookie pattern

    let req = test::TestRequest::post()
        .uri("/signup")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .cookie(csrf_cookie);

    // Include pre-session cookie to replicate browser behavior
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

    let req = req.set_payload(format!("email=user%40example.com&password=averylongpassword&password_confirm=differentlongpassword&csrf_token={}", csrf_token)).to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    assert!(body.contains("Passwords do not match"));
}

#[actix_web::test]
async fn signup_posts_to_pocketbase_and_redirects_to_login() {
    setup_csrf();
    let pb = actix_test::start(|| {
        App::new().route(
            "/api/collections/users/records",
            web::post().to(|body: web::Json<Value>| async move {
                assert_eq!(body["email"], "user@example.com");
                assert_eq!(body["password"], "averylongpassword");
                assert_eq!(body["passwordConfirm"], "averylongpassword");
                HttpResponse::Ok().json(serde_json::json!({}))
            }),
        )
    });

    let app = test::init_service(
        App::new()
            .wrap(csrf_middleware())
            .app_data(web::Data::new(test_auth_config(pb.url(""))))
            .configure(app::configure_app),
    )
    .await;

    // First, get the CSRF token from the signup page
    let get_req = test::TestRequest::get().uri("/signup").to_request();
    let get_resp = test::call_service(&app, get_req).await;

    // Extract CSRF cookie name and value as strings (to avoid borrow checker error)
    let csrf_cookie_name = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.name().to_string())
        .expect("CSRF cookie not found");
    let csrf_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.value().to_string())
        .expect("CSRF cookie value not found");

    // Extract pre-session cookie
    // The middleware uses a pre-session cookie to derive a session ID for HMAC validation of anonymous users.
    // In a real browser, this cookie is automatically sent with requests. In tests, we must manually extract
    // and include it to replicate browser behavior. This is an acceptable divergence between test and app code
    // since actix test utilities don't automatically forward cookies like a browser does.
    //
    // SECURITY NOTE: In production, the pre-session cookie is set with http_only=true, secure=true, same_site=Strict.
    // For local development (HTTP), we configure the middleware with secure=false to allow cookies over HTTP.
    // This is an insecure configuration that should only be used for local development, not production.
    //
    // FUTURE IMPROVEMENT: Consider using a more realistic user agent (e.g., with cookie jar support) in tests
    // to automatically handle cookie forwarding, reducing the need for manual cookie extraction/inclusion.
    let pre_session_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "pre-session")
        .map(|cookie| cookie.value().to_string());

    let body = String::from_utf8(test::read_body(get_resp).await.to_vec()).expect("utf8 body");

    // Extract CSRF token from the response
    let csrf_token = body
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split("\"").next())
        .expect("CSRF token not found in response");

    // Reconstruct the cookie with proper attributes
    let mut csrf_cookie = actix_web::cookie::Cookie::new(csrf_cookie_name, csrf_cookie_value);
    csrf_cookie.set_same_site(actix_web::cookie::SameSite::Lax);
    csrf_cookie.set_http_only(false); // Must be false for double-submit cookie pattern

    let req = test::TestRequest::post()
        .uri("/signup")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .cookie(csrf_cookie);

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

    let req = req.set_payload(format!("email=USER%40EXAMPLE.COM&password=averylongpassword&password_confirm=averylongpassword&csrf_token={}", csrf_token)).to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(resp.headers().get("Location").unwrap(), "/login");
}

#[actix_web::test]
async fn signup_returns_unavailable_message_when_pocketbase_cannot_be_reached() {
    setup_csrf();
    let app = test::init_service(
        App::new()
            .wrap(csrf_middleware())
            .app_data(web::Data::new(test_auth_config(
                "http://127.0.0.1:1".into(),
            )))
            .configure(app::configure_app),
    )
    .await;

    // First, get the CSRF token from the signup page
    let get_req = test::TestRequest::get().uri("/signup").to_request();
    let get_resp = test::call_service(&app, get_req).await;

    // Extract CSRF cookie name and value as strings (to avoid borrow checker error)
    let csrf_cookie_name = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.name().to_string())
        .expect("CSRF cookie not found");
    let csrf_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.value().to_string())
        .expect("CSRF cookie value not found");

    // Extract pre-session cookie
    // The middleware uses a pre-session cookie to derive a session ID for HMAC validation of anonymous users.
    // In a real browser, this cookie is automatically sent with requests. In tests, we must manually extract
    // and include it to replicate browser behavior. This is an acceptable divergence between test and app code
    // since actix test utilities don't automatically forward cookies like a browser does.
    //
    // SECURITY NOTE: In production, the pre-session cookie is set with http_only=true, secure=true, same_site=Strict.
    // For local development (HTTP), we configure the middleware with secure=false to allow cookies over HTTP.
    // This is an insecure configuration that should only be used for local development, not production.
    //
    // FUTURE IMPROVEMENT: Consider using a more realistic user agent (e.g., with cookie jar support) in tests
    // to automatically handle cookie forwarding, reducing the need for manual cookie extraction/inclusion.
    let pre_session_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "pre-session")
        .map(|cookie| cookie.value().to_string());

    let body = String::from_utf8(test::read_body(get_resp).await.to_vec()).expect("utf8 body");

    // Extract CSRF token from the response
    let csrf_token = body
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split("\"").next())
        .expect("CSRF token not found in response");

    // Reconstruct the cookie with proper attributes
    let mut csrf_cookie = actix_web::cookie::Cookie::new(csrf_cookie_name, csrf_cookie_value);
    csrf_cookie.set_same_site(actix_web::cookie::SameSite::Lax);
    csrf_cookie.set_http_only(false); // Must be false for double-submit cookie pattern

    let req = test::TestRequest::post()
        .uri("/signup")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .cookie(csrf_cookie);

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

    let req = req.set_payload(format!("email=user%40example.com&password=averylongpassword&password_confirm=averylongpassword&csrf_token={}", csrf_token)).to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");
    assert!(body.contains("Signup service is unavailable. Try again later."));
}

#[actix_web::test]
async fn login_posts_to_pocketbase_sets_cookie_and_redirects_home() {
    setup_csrf();
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
            .wrap(csrf_middleware())
            .app_data(web::Data::new(test_auth_config(pb.url(""))))
            .configure(app::configure_app),
    )
    .await;

    let get_req = test::TestRequest::get().uri("/login").to_request();
    let get_resp = test::call_service(&app, get_req).await;

    let csrf_cookie_name = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.name().to_string())
        .expect("CSRF cookie not found");
    let csrf_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|cookie| cookie.value().to_string())
        .expect("CSRF cookie value not found");

    // Extract pre-session cookie
    // The middleware uses a pre-session cookie to derive a session ID for HMAC validation of anonymous users.
    // In a real browser, this cookie is automatically sent with requests. In tests, we must manually extract
    // and include it to replicate browser behavior. This is an acceptable divergence between test and app code
    // since actix test utilities don't automatically forward cookies like a browser does.
    //
    // SECURITY NOTE: In production, the pre-session cookie is set with http_only=true, secure=true, same_site=Strict.
    // For local development (HTTP), we configure the middleware with secure=false to allow cookies over HTTP.
    // This is an insecure configuration that should only be used for local development, not production.
    //
    // FUTURE IMPROVEMENT: Consider using a more realistic user agent (e.g., with cookie jar support) in tests
    // to automatically handle cookie forwarding, reducing the need for manual cookie extraction/inclusion.
    let pre_session_cookie_value = get_resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "pre-session")
        .map(|cookie| cookie.value().to_string());

    let body = String::from_utf8(test::read_body(get_resp).await.to_vec()).expect("utf8 body");

    let csrf_token = body
        .split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split("\"").next())
        .expect("CSRF token not found in response");

    let mut csrf_cookie = actix_web::cookie::Cookie::new(csrf_cookie_name, csrf_cookie_value);
    csrf_cookie.set_same_site(actix_web::cookie::SameSite::Lax);
    csrf_cookie.set_http_only(false);

    let req = test::TestRequest::post()
        .uri("/login")
        .insert_header(("Content-Type", "application/x-www-form-urlencoded"))
        .insert_header(("X-CSRF-Token", csrf_token))
        .cookie(csrf_cookie);

    // Include pre-session cookie to replicate browser behavior
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

    let req = req
        .set_payload("identity=user%40example.com&password=averylongpassword")
        .to_request();
    let resp = test::call_service(&app, req).await;

    let status = resp.status();
    let location = resp
        .headers()
        .get("Location")
        .map(|v| v.to_str().unwrap().to_string());
    let auth_token_cookie_name = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "auth_token")
        .map(|c| c.name().to_string());
    let auth_token_cookie_value = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "auth_token")
        .map(|c| c.value().to_string());
    let auth_token_http_only = resp
        .response()
        .cookies()
        .find(|cookie| cookie.name() == "auth_token")
        .and_then(|c| c.http_only());
    let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");

    eprintln!("Response status: {}", status);
    eprintln!("Response body: {}", body);

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location.unwrap(), "/");
    assert_eq!(auth_token_cookie_name.unwrap(), "auth_token");
    assert_eq!(auth_token_cookie_value.unwrap(), "pocketbase-token");
    assert!(auth_token_http_only.unwrap());
}

#[actix_web::test]
async fn auth_middleware_allows_public_auth_routes_and_redirects_protected_html() {
    setup_csrf();
    let config = test_auth_config("http://127.0.0.1:1".into());
    let app = test::init_service(
        App::new()
            .wrap(csrf_middleware())
            .wrap(middleware::Condition::new(
                true,
                auth::JwtMiddleware::new(
                    config.clone(),
                    web::Data::new(auth::TokenVerifyCache::default()),
                ),
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
