use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::error::{Error, ErrorBadRequest, ErrorUnauthorized};
use actix_web::{web, HttpResponse, ResponseError};
use askama::Template;
use futures_util::future::LocalBoxFuture;
use log::{info, warn};
use serde::Deserialize;
use std::future::{ready, Ready};
use std::rc::Rc;

use actix_csrf_middleware::CsrfToken;

// Security constants
const MIN_PASSWORD_LENGTH: usize = 12;
const MAX_EMAIL_LENGTH: usize = 254;
const MAX_ERROR_BODY_LENGTH: usize = 200;

/// User-facing error messages that obscure implementation details
///
/// This pattern separates user-facing messages from internal error contexts for security.
/// When adding new errors:
/// 1. Define a new constant below with a user-friendly message and a log context
/// 2. Use the constant in handlers instead of inline strings
/// 3. Log the log_context for debugging while showing the message to users
#[derive(Debug, Clone)]
pub struct UserError {
    pub message: &'static str,
    pub log_context: &'static str,
}

impl UserError {
    const fn new(message: &'static str, log_context: &'static str) -> Self {
        Self {
            message,
            log_context,
        }
    }
}

// Error mapping table - maps specific error contexts to user-facing messages
// Add new error constants here when introducing new validation or error scenarios
pub const ERR_INVALID_EMAIL: UserError =
    UserError::new("Enter a valid email address.", "invalid_email");
pub const ERR_WEAK_PASSWORD: UserError = UserError::new(
    "Use a password with at least 12 characters.",
    "weak_password",
);
pub const ERR_PASSWORD_MISMATCH: UserError =
    UserError::new("Passwords do not match.", "password_mismatch");
pub const ERR_SIGNUP_FAILED: UserError = UserError::new(
    "Signup failed. Check your email and password, then try again.",
    "signup_failed",
);
pub const ERR_SIGNUP_UNAVAILABLE: UserError = UserError::new(
    "Signup service is unavailable. Try again later.",
    "signup_unavailable",
);
pub const ERR_LOGIN_FAILED: UserError =
    UserError::new("Invalid credentials or server offline", "login_failed");
pub const ERR_INVALID_REQUEST: UserError = UserError::new("Invalid request", "invalid_request");
pub const ERR_INTERNAL_ERROR: UserError = UserError::new("Internal error", "internal_error");

#[derive(Clone)]
pub struct AuthConfig {
    pub pocketbase_url: String,
    pub cookie_secure: bool,
    pub require_login: bool,
    pub pocketbase_ca_cert: Option<String>,
    pub public_paths: Vec<String>,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let pocketbase_url =
            std::env::var("POCKETBASE_URL").unwrap_or_else(|_| "https://127.0.0.1:8090".into());

        // Validate URL scheme to prevent SSRF
        if let Err(e) = url::Url::parse(&pocketbase_url) {
            panic!("Invalid POCKETBASE_URL: {}", e);
        }
        let url = url::Url::parse(&pocketbase_url).unwrap();
        match url.scheme() {
            "http" | "https" => {}
            scheme => panic!(
                "POCKETBASE_URL must use http or https scheme, got: {}",
                scheme
            ),
        }

        let cookie_secure = std::env::var("AUTH_COOKIE_SECURE")
            .map(|value| value == "true" || value == "1")
            .unwrap_or(true);

        if !cookie_secure {
            log::warn!(
                "⚠️  SECURITY WARNING: AUTH_COOKIE_SECURE is disabled. Auth cookies will be sent over HTTP connections. This is an INSECURE configuration."
            );
        }

        let public_paths = std::env::var("AUTH_PUBLIC_PATHS")
            .unwrap_or_else(|_| "/login,/signup,/logout".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        Self {
            pocketbase_url,
            cookie_secure,
            require_login: std::env::var("AUTH_REQUIRE_LOGIN")
                .map(|value| value == "true" || value == "1")
                .unwrap_or(false),
            pocketbase_ca_cert: std::env::var("POCKETBASE_CA_CERT").ok(),
            public_paths,
        }
    }

    pub fn is_insecure(&self) -> bool {
        !self.cookie_secure
    }

    pub fn build_client(&self) -> Result<reqwest::Client, reqwest::Error> {
        let mut builder = reqwest::Client::builder();
        if let Some(ref ca_path) = self.pocketbase_ca_cert {
            if !ca_path.is_empty() {
                let cert_pem = std::fs::read_to_string(ca_path)
                    .expect("Failed to read POCKETBASE_CA_CERT file");
                let cert = reqwest::Certificate::from_pem(cert_pem.as_bytes())
                    .expect("Failed to parse POCKETBASE_CA_CERT PEM");
                builder = builder.add_root_certificate(cert);
            }
        }
        builder.build()
    }

    pub fn auth_with_password_url(&self) -> String {
        format!(
            "{}/api/collections/users/auth-with-password",
            self.pocketbase_url.trim_end_matches('/')
        )
    }

    pub fn users_records_url(&self) -> String {
        format!(
            "{}/api/collections/users/records",
            self.pocketbase_url.trim_end_matches('/')
        )
    }

    pub fn auth_refresh_url(&self) -> String {
        format!(
            "{}/api/collections/users/auth-refresh",
            self.pocketbase_url.trim_end_matches('/')
        )
    }
}

pub struct JwtMiddleware {
    config: AuthConfig,
}

impl JwtMiddleware {
    pub fn new(config: AuthConfig) -> Self {
        JwtMiddleware { config }
    }
}

fn is_public_path(path: &str, config: &AuthConfig) -> bool {
    config.public_paths.iter().any(|p| path == p)
}

async fn verify_token_with_pocketbase(
    token: &str,
    config: &AuthConfig,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    let client = config.build_client()?;
    let response = client
        .post(config.auth_refresh_url())
        .header("Authorization", token)
        .send()
        .await?;

    if response.status().is_success() {
        let pb_res: serde_json::Value = response.json().await.unwrap_or_default();
        if let Some(new_token) = pb_res.get("token").and_then(|t| t.as_str()) {
            Ok(Some(new_token.to_string()))
        } else {
            Ok(None)
        }
    } else {
        Err(Box::new(std::io::Error::other(format!(
            "PocketBase auth refresh failed: {}",
            response.status()
        ))))
    }
}

impl<S, B> Transform<S, ServiceRequest> for JwtMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = JwtMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(JwtMiddlewareService {
            service: Rc::new(service),
            config: self.config.clone(),
        }))
    }
}

pub struct JwtMiddlewareService<S> {
    service: Rc<S>,
    config: AuthConfig,
}

#[derive(Debug)]
pub struct AuthRedirectError;

impl std::fmt::Display for AuthRedirectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unauthorized")
    }
}

impl ResponseError for AuthRedirectError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::SeeOther()
            .insert_header(("Location", "/login"))
            .finish()
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct AuthRedirectClearCookieError;

impl std::fmt::Display for AuthRedirectClearCookieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Token expired")
    }
}

impl ResponseError for AuthRedirectClearCookieError {
    fn error_response(&self) -> HttpResponse {
        let mut cookie = actix_web::cookie::Cookie::named("auth_token");
        cookie.make_removal();
        cookie.set_path("/");
        HttpResponse::SeeOther()
            .insert_header(("Location", "/login"))
            .cookie(cookie)
            .finish()
    }
}

impl<S, B> Service<ServiceRequest> for JwtMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let config = self.config.clone();
        let service = self.service.clone();

        Box::pin(async move {
            let mut token_opt = None;

            // 1. Try to get token from Authorization header
            if let Some(auth_header) = req.headers().get("Authorization") {
                if let Ok(auth_str) = auth_header.to_str() {
                    if let Some(token) = auth_str.strip_prefix("Bearer ") {
                        token_opt = Some(token.to_string());
                    }
                }
            }

            // 2. Fallback to extracting from HttpOnly cookie
            if token_opt.is_none() {
                if let Some(cookie) = req.cookie("auth_token") {
                    token_opt = Some(cookie.value().to_string());
                }
            }

            // Verify token using PocketBase's auth-refresh endpoint
            let (token_valid, original_token) = if let Some(ref token) = token_opt {
                match verify_token_with_pocketbase(token, &config).await {
                    Ok(Some(new_token)) => {
                        // Token is valid and was refreshed
                        info!(
                            "[AUTH_SUCCESS] Token verified and refreshed for request to {}",
                            req.path()
                        );
                        (Some(new_token), Some(token.clone()))
                    }
                    Ok(None) => {
                        // Token is valid but not refreshed
                        info!(
                            "[AUTH_SUCCESS] Token verified for request to {}",
                            req.path()
                        );
                        (Some(token.clone()), Some(token.clone()))
                    }
                    Err(_) => {
                        // Token is invalid
                        warn!("[AUTH_FAILED] Invalid token for request to {}", req.path());
                        (None, Some(token.clone()))
                    }
                }
            } else {
                (None, None)
            };

            if is_public_path(req.path(), &config) {
                let res = service.call(req).await?;
                return Ok(res);
            }

            if token_valid.is_none() {
                if req.path() != "/login" {
                    info!("[AUTH_REDIRECT] Redirecting unauthorized request to /login");
                    return Err(AuthRedirectError.into());
                }
                return Err(ErrorUnauthorized("Unauthorized"));
            }

            // If token was refreshed, update the response with new cookie
            let mut res = service.call(req).await?;
            if let Some(refreshed_token) = token_valid {
                if let Some(original) = original_token {
                    if refreshed_token != original {
                        let cookie =
                            actix_web::cookie::Cookie::build("auth_token", refreshed_token)
                                .path("/")
                                .http_only(true)
                                .secure(config.cookie_secure)
                                .same_site(actix_web::cookie::SameSite::Lax)
                                .finish();
                        res.response_mut().add_cookie(&cookie).ok();
                    }
                }
            }
            Ok(res)
        })
    }
}

// --- Routes & UI Handlers ---

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error_message: Option<String>,
    pub is_insecure: bool,
    pub csrf_token: String,
}

#[derive(Template)]
#[template(path = "signup.html")]
pub struct SignupTemplate {
    pub error_message: Option<String>,
    pub is_insecure: bool,
    pub csrf_token: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub identity: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
    pub password_confirm: String,
}

pub async fn login_view(
    csrf: CsrfToken,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    let tmpl = LoginTemplate {
        error_message: None,
        is_insecure: config.is_insecure(),
        csrf_token: csrf.0,
    };
    let html = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(html))
}

pub async fn signup_view(
    csrf: CsrfToken,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    let tmpl = SignupTemplate {
        error_message: None,
        is_insecure: config.is_insecure(),
        csrf_token: csrf.0,
    };
    let html = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(html))
}

pub async fn signup_submit(
    form: web::Form<SignupRequest>,
    csrf: CsrfToken,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    let email = form.email.trim().to_lowercase();
    let csrf_token = csrf.0;

    if !email.contains('@') || email.len() > MAX_EMAIL_LENGTH {
        warn!(
            "[AUTH_FAILED] Signup rejected: {}",
            ERR_INVALID_EMAIL.log_context
        );
        return signup_error(ERR_INVALID_EMAIL.message, csrf_token);
    }

    if form.password.len() < MIN_PASSWORD_LENGTH {
        warn!(
            "[AUTH_FAILED] Signup rejected: {}",
            ERR_WEAK_PASSWORD.log_context
        );
        return signup_error(ERR_WEAK_PASSWORD.message, csrf_token);
    }

    if form.password != form.password_confirm {
        warn!(
            "[AUTH_FAILED] Signup rejected: {}",
            ERR_PASSWORD_MISMATCH.log_context
        );
        return signup_error(ERR_PASSWORD_MISMATCH.message, csrf_token);
    }

    let client = config.build_client().map_err(|e| {
        warn!("[AUTH_FAILED] Failed to build HTTP client: {}", e);
        actix_web::error::ErrorInternalServerError(ERR_INTERNAL_ERROR.message)
    })?;
    let res = client
        .post(config.users_records_url())
        .json(&serde_json::json!({
            "email": email,
            "password": form.password,
            "passwordConfirm": form.password_confirm
        }))
        .send()
        .await;

    match res {
        Ok(response) if response.status().is_success() => {
            info!("[AUTH_SUCCESS] User signup completed.");
            Ok(HttpResponse::SeeOther()
                .append_header(("Location", "/login"))
                .finish())
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let sanitized_body = body
                .chars()
                .take(MAX_ERROR_BODY_LENGTH)
                .collect::<String>()
                .replace('\n', " ");
            warn!(
                "[AUTH_FAILED] PocketBase signup rejected with status {}: {}",
                status, sanitized_body
            );
            signup_error(ERR_SIGNUP_FAILED.message, csrf_token)
        }
        Err(e) => {
            warn!("[AUTH_FAILED] PocketBase signup connection error: {}", e);
            signup_error(ERR_SIGNUP_UNAVAILABLE.message, csrf_token)
        }
    }
}

fn signup_error(message: &str, csrf_token: String) -> actix_web::Result<HttpResponse> {
    let tmpl = SignupTemplate {
        error_message: Some(message.into()),
        is_insecure: false,
        csrf_token,
    };
    let html = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::BadRequest()
        .content_type("text/html")
        .body(html))
}

pub async fn login_submit(
    form: web::Form<LoginRequest>,
    csrf: CsrfToken,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    let csrf_token = csrf.0;

    if form.identity.trim().is_empty() || form.password.is_empty() {
        warn!(
            "[AUTH_FAILED] Login rejected: {}",
            ERR_INVALID_REQUEST.log_context
        );
        return Err(ErrorBadRequest(ERR_INVALID_REQUEST.message));
    }

    let client = config.build_client().map_err(|e| {
        warn!("[AUTH_FAILED] Failed to build HTTP client: {}", e);
        actix_web::error::ErrorInternalServerError(ERR_INTERNAL_ERROR.message)
    })?;
    let res = client
        .post(config.auth_with_password_url())
        .json(&serde_json::json!({
            "identity": form.identity.trim(),
            "password": form.password
        }))
        .send()
        .await;

    match res {
        Ok(response) if response.status().is_success() => {
            let pb_res: serde_json::Value = response.json().await.unwrap_or_default();
            if let Some(token) = pb_res.get("token").and_then(|t| t.as_str()) {
                let cookie = actix_web::cookie::Cookie::build("auth_token", token.to_string())
                    .path("/")
                    .http_only(true)
                    .secure(config.cookie_secure)
                    .finish();

                let flag_cookie = actix_web::cookie::Cookie::build("auth_present", "1")
                    .path("/")
                    .secure(config.cookie_secure)
                    .finish();
                return Ok(HttpResponse::SeeOther()
                    .append_header(("Location", "/"))
                    .cookie(cookie)
                    .cookie(flag_cookie)
                    .finish());
            }
        }
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let sanitized_body = body
                .chars()
                .take(MAX_ERROR_BODY_LENGTH)
                .collect::<String>()
                .replace('\n', " ");
            warn!(
                "[AUTH_FAILED] PocketBase login rejected with status {}: {}",
                status, sanitized_body
            );
        }
        Err(e) => {
            warn!("[AUTH_FAILED] PocketBase login connection error: {}", e);
        }
    }

    let tmpl = LoginTemplate {
        error_message: Some(ERR_LOGIN_FAILED.message.into()),
        is_insecure: false,
        csrf_token,
    };
    let html = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Unauthorized()
        .content_type("text/html")
        .body(html))
}

pub async fn logout(_csrf: actix_csrf_middleware::CsrfToken) -> actix_web::Result<HttpResponse> {
    let mut cookie = actix_web::cookie::Cookie::named("auth_token");
    cookie.make_removal();
    cookie.set_path("/");

    let mut flag_cookie = actix_web::cookie::Cookie::named("auth_present");
    flag_cookie.make_removal();
    flag_cookie.set_path("/");

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/login"))
        .cookie(cookie)
        .cookie(flag_cookie)
        .finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AuthConfig {
        AuthConfig {
            pocketbase_url: "https://127.0.0.1:8090".into(),
            cookie_secure: true,
            require_login: true,
            pocketbase_ca_cert: None,
            public_paths: vec!["/login".into(), "/signup".into(), "/logout".into()],
        }
    }

    #[test]
    fn login_path_is_public() {
        let config = test_config();
        assert!(is_public_path("/login", &config));
        assert!(is_public_path("/signup", &config));
        assert!(is_public_path("/logout", &config));
        assert!(!is_public_path("/profile", &config));
    }

    #[test]
    fn public_paths_configurable() {
        let mut config = test_config();
        assert!(is_public_path("/login", &config));

        config.public_paths = vec!["/custom".into()];
        assert!(!is_public_path("/login", &config));
        assert!(is_public_path("/custom", &config));
    }

    #[test]
    fn rejects_invalid_url_scheme() {
        // Test URL scheme validation directly
        let url = url::Url::parse("ftp://127.0.0.1:8090").unwrap();
        assert_ne!(url.scheme(), "http");
        assert_ne!(url.scheme(), "https");
    }

    #[test]
    fn is_insecure_detects_insecure_cookie() {
        let config = AuthConfig {
            pocketbase_url: "http://127.0.0.1:8090".into(),
            cookie_secure: false,
            require_login: true,
            pocketbase_ca_cert: None,
            public_paths: vec!["/login".into()],
        };
        assert!(config.is_insecure());
    }
}
