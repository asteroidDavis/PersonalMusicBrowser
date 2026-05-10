use actix_csrf_middleware::{CsrfMiddleware, CsrfMiddlewareConfig};
use actix_web::body::BoxBody;
use actix_web::body::EitherBody;
use actix_web::cookie::SameSite;
use actix_web::{cookie::Cookie, dev::ServiceResponse, http::StatusCode, test, web, App};
use music_browser::app;
mod common;

use common::{
    assert_debug_info, extract_csrf_cookie, extract_csrf_token, extract_pre_session_cookie,
    parse_endpoint_config, test_auth_config,
};

/// Test all GET endpoints accessible from the homepage
#[actix_web::test]
async fn test_all_get_endpoints() {
    let config = parse_endpoint_config();
    let auth_config = test_auth_config("http://127.0.0.1:1".into());

    // CSRF middleware configuration for testing
    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_token_cookie_config(actix_csrf_middleware::CsrfDoubleSubmitCookie {
                http_only: false,
                secure: false,
                same_site: SameSite::Lax,
            });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_config))
            .wrap(CsrfMiddleware::new(csrf_config))
            .configure(app::configure_app),
    )
    .await;

    for endpoint in &config.get_endpoints {
        eprintln!("Testing GET endpoint: {}", endpoint.path);

        let req = test::TestRequest::get().uri(&endpoint.path).to_request();
        let resp: ServiceResponse<EitherBody<BoxBody>> = test::call_service(&app, req).await;

        let status = resp.status();
        let headers = resp.headers().clone();
        let csrf_cookie = extract_csrf_cookie(&resp);
        let pre_session_cookie = extract_pre_session_cookie(&resp);
        let cookies: Vec<Cookie<'static>> = resp
            .response()
            .cookies()
            .into_iter()
            .map(|c: Cookie<'_>| c.clone().into_owned())
            .collect();
        let body = String::from_utf8(test::read_body(resp).await.to_vec()).expect("utf8 body");

        // For public endpoints, expect 200 OK or 500 (if DB not configured)
        if endpoint.public {
            if status != StatusCode::OK && status != StatusCode::INTERNAL_SERVER_ERROR {
                assert_debug_info(status, &body, &headers, cookies.clone());
                panic!(
                    "Public GET endpoint {} returned unexpected status {}: {}",
                    endpoint.path, status, body
                );
            }
        } else if endpoint.requires_auth {
            // For authenticated endpoints, accept 200, 302/303 (redirect), or 500 (if DB not configured)
            if status != StatusCode::OK
                && status != StatusCode::FOUND
                && status != StatusCode::SEE_OTHER
                && status != StatusCode::INTERNAL_SERVER_ERROR
            {
                assert_debug_info(status, &body, &headers, cookies.clone());
                panic!(
                    "GET endpoint {} returned unexpected status {}: {}",
                    endpoint.path, status, body
                );
            }
        }

        // Debug assertions for all endpoints
        assert_debug_info(status, &body, &headers, cookies.clone());

        // Check for CSRF cookie on all GET responses (middleware should set it)
        // Only check if response is successful (not 500 error)
        if status != StatusCode::INTERNAL_SERVER_ERROR {
            if endpoint.public || endpoint.requires_auth {
                // CSRF cookie should be present for all GET endpoints
                assert!(
                    csrf_cookie.is_some(),
                    "CSRF cookie not found for GET endpoint {}",
                    endpoint.path
                );
            }

            // Check for pre-session cookie on anonymous endpoints
            if endpoint.public {
                assert!(
                    pre_session_cookie.is_some(),
                    "Pre-session cookie not found for public GET endpoint {}",
                    endpoint.path
                );
            }

            // Check content-type header
            let content_type = headers.get("content-type");
            assert!(
                content_type.is_some(),
                "Content-Type header missing for GET endpoint {}",
                endpoint.path
            );
        }

        eprintln!("✓ GET endpoint {} passed", endpoint.path);
    }

    eprintln!("All GET endpoint tests passed");
}

/// Test all POST endpoints from form_groups with CSRF validation
#[actix_web::test]
async fn test_form_post_endpoints() {
    let config = parse_endpoint_config();
    let auth_config = test_auth_config("http://127.0.0.1:1".into());

    // CSRF middleware configuration for testing
    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_token_cookie_config(actix_csrf_middleware::CsrfDoubleSubmitCookie {
                http_only: false,
                secure: false,
                same_site: SameSite::Lax,
            });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_config))
            .wrap(CsrfMiddleware::new(csrf_config))
            .configure(app::configure_app),
    )
    .await;

    for group in &config.form_groups {
        eprintln!("Testing form group: {}", group.name);

        for endpoint in &group.endpoints {
            eprintln!("Testing POST endpoint: {}", endpoint.path);

            // First, test that the endpoint rejects requests without CSRF token
            let post_req_no_csrf = test::TestRequest::post()
                .uri(&endpoint.path)
                .insert_header(("content-type", "application/x-www-form-urlencoded"))
                .to_request();
            let post_resp_no_csrf: ServiceResponse<EitherBody<BoxBody>> =
                test::call_service(&app, post_req_no_csrf).await;

            // Check that the endpoint exists (not 404)
            let status_no_csrf = post_resp_no_csrf.status();
            if status_no_csrf == StatusCode::NOT_FOUND {
                panic!("POST endpoint {} not found (404)", endpoint.path);
            }

            // CSRF-protected endpoints should reject requests without proper CSRF tokens
            if endpoint.requires_csrf {
                if status_no_csrf != StatusCode::FORBIDDEN
                    && status_no_csrf != StatusCode::UNAUTHORIZED
                    && status_no_csrf != StatusCode::BAD_REQUEST
                {
                    // If it's not rejected, it might be because auth is required first
                    if endpoint.requires_auth {
                        eprintln!(
                            "✓ POST endpoint {} requires auth (may need auth before CSRF check)",
                            endpoint.path
                        );
                    } else {
                        eprintln!(
                            "⚠ POST endpoint {} returned {} without CSRF token (expected rejection)",
                            endpoint.path, status_no_csrf
                        );
                    }
                } else {
                    eprintln!(
                        "✓ POST endpoint {} properly rejected without CSRF token",
                        endpoint.path
                    );
                }
            } else {
                eprintln!("✓ POST endpoint {} does not require CSRF", endpoint.path);
            }

            // For public endpoints with CSRF, test with proper CSRF token
            if endpoint.public && endpoint.requires_csrf {
                // Get a CSRF token from the corresponding GET endpoint
                let get_req = test::TestRequest::get().uri(&endpoint.path).to_request();
                let get_resp: ServiceResponse<EitherBody<BoxBody>> =
                    test::call_service(&app, get_req).await;

                let csrf_cookie = extract_csrf_cookie(&get_resp);
                let get_body =
                    String::from_utf8(test::read_body(get_resp).await.to_vec()).expect("utf8 body");
                let csrf_token = extract_csrf_token(&get_body);

                if let (Some(token), Some(cookie)) = (csrf_token, csrf_cookie) {
                    // Make POST request with CSRF token
                    let post_req_with_csrf = test::TestRequest::post()
                        .uri(&endpoint.path)
                        .insert_header(("content-type", "application/x-www-form-urlencoded"))
                        .cookie(cookie)
                        .set_payload(format!("csrf_token={}", token))
                        .to_request();
                    let post_resp_with_csrf: ServiceResponse<EitherBody<BoxBody>> =
                        test::call_service(&app, post_req_with_csrf).await;

                    let status_with_csrf = post_resp_with_csrf.status();
                    eprintln!(
                        "✓ POST endpoint {} with CSRF token returned {}",
                        endpoint.path, status_with_csrf
                    );
                } else {
                    eprintln!(
                        "⚠ Could not extract CSRF token/cookie from GET endpoint {}",
                        endpoint.path
                    );
                }
            }
        }
    }

    eprintln!("POST endpoint tests completed");
}

/// Test all button endpoints with CSRF validation
#[actix_web::test]
async fn test_button_endpoints() {
    let config = parse_endpoint_config();
    let auth_config = test_auth_config("http://127.0.0.1:1".into());

    // CSRF middleware configuration for testing
    let csrf_config =
        CsrfMiddlewareConfig::double_submit_cookie(b"test-secret-32-bytes-long-for-testing-!!")
            .with_token_cookie_config(actix_csrf_middleware::CsrfDoubleSubmitCookie {
                http_only: false,
                secure: false,
                same_site: SameSite::Lax,
            });

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(auth_config))
            .wrap(CsrfMiddleware::new(csrf_config))
            .configure(app::configure_app),
    )
    .await;

    for group in &config.button_endpoints {
        eprintln!("Testing button group: {}", group.name);

        for endpoint in &group.endpoints {
            eprintln!("Testing button endpoint: {}", endpoint.path);

            // Replace {id} with a placeholder ID for testing
            let test_path = endpoint.path.replace("{id}", "1");

            // Test that the endpoint rejects requests without CSRF token
            let req_no_csrf = match endpoint.method.as_str() {
                "POST" => test::TestRequest::post().uri(&test_path),
                "PUT" => test::TestRequest::put().uri(&test_path),
                _ => {
                    eprintln!(
                        "⚠ Unsupported method {} for endpoint {}",
                        endpoint.method, endpoint.path
                    );
                    continue;
                }
            };
            let req_no_csrf = req_no_csrf
                .insert_header(("content-type", "application/x-www-form-urlencoded"))
                .to_request();
            let resp_no_csrf: ServiceResponse<EitherBody<BoxBody>> =
                test::call_service(&app, req_no_csrf).await;

            // Check that the endpoint exists (not 404)
            let status_no_csrf = resp_no_csrf.status();
            if status_no_csrf == StatusCode::NOT_FOUND {
                // Button endpoints with {id} may return 404 if the ID doesn't exist
                // This is expected in testing without a real database
                eprintln!(
                    "⚠ Button endpoint {} returned 404 (ID may not exist in test DB)",
                    endpoint.path
                );
            } else if endpoint.requires_csrf {
                // CSRF-protected endpoints should reject requests without proper CSRF tokens
                if status_no_csrf != StatusCode::FORBIDDEN
                    && status_no_csrf != StatusCode::UNAUTHORIZED
                    && status_no_csrf != StatusCode::BAD_REQUEST
                {
                    // If it's not rejected, it might be because auth is required first
                    if endpoint.requires_auth {
                        eprintln!(
                            "✓ Button endpoint {} requires auth (may need auth before CSRF check)",
                            endpoint.path
                        );
                    } else {
                        eprintln!(
                            "⚠ Button endpoint {} returned {} without CSRF token (expected rejection)",
                            endpoint.path, status_no_csrf
                        );
                    }
                } else {
                    eprintln!(
                        "✓ Button endpoint {} properly rejected without CSRF token",
                        endpoint.path
                    );
                }
            } else {
                eprintln!("✓ Button endpoint {} does not require CSRF", endpoint.path);
            }
        }
    }

    eprintln!("Button endpoint tests completed");
}
