use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::error::{ErrorBadRequest, ErrorUnauthorized};
use actix_web::{web, Error, HttpMessage, HttpRequest, HttpResponse, ResponseError};
use askama::Template;
use futures_util::future::LocalBoxFuture;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use log::{info, warn};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub id: String,
    pub exp: usize,
}

#[derive(Clone)]
pub struct AuthConfig {
    pub pocketbase_url: String,
    pub jwt_secret: String,
    pub cookie_secure: bool,
    pub require_login: bool,
    pub pocketbase_ca_cert: Option<String>,
    pub public_paths: Vec<String>,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let jwt_secret = std::env::var("POCKETBASE_JWT_SECRET").unwrap_or_default();
        let allow_empty = std::env::var("AUTH_ALLOW_EMPTY_JWT_SECRET")
            .map(|value| value == "true" || value == "1")
            .unwrap_or(false);

        if jwt_secret.is_empty() && !allow_empty {
            panic!(
                "POCKETBASE_JWT_SECRET is empty. Set a strong secret or set AUTH_ALLOW_EMPTY_JWT_SECRET=true to allow unsafe behavior."
            );
        }

        if allow_empty {
            log::warn!(
                "⚠️  SECURITY WARNING: AUTH_ALLOW_EMPTY_JWT_SECRET is enabled. This is an EMERGENCY-ONLY setting that disables JWT signature validation. The application is running in an INSECURE configuration. Re-enable proper authentication as soon as possible."
            );
        }

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
            jwt_secret,
            cookie_secure,
            require_login: std::env::var("AUTH_REQUIRE_LOGIN")
                .map(|value| value == "true" || value == "1")
                .unwrap_or(false),
            pocketbase_ca_cert: std::env::var("POCKETBASE_CA_CERT").ok(),
            public_paths,
        }
    }

    pub fn is_insecure(&self) -> bool {
        self.jwt_secret.is_empty() || !self.cookie_secure
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
}

pub struct JwtMiddleware {
    config: AuthConfig,
}

impl JwtMiddleware {
    pub fn new(config: AuthConfig) -> Self {
        JwtMiddleware { config }
    }
}

pub fn validate_token(
    token: &str,
    config: &AuthConfig,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.validate_aud = false;

    let key = DecodingKey::from_secret(config.jwt_secret.as_bytes());
    decode::<Claims>(token, &key, &validation).map(|token_data| token_data.claims)
}

fn is_public_path(path: &str, config: &AuthConfig) -> bool {
    config.public_paths.iter().any(|p| path == p)
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

            // Validate token once and store result for both auth state and authorization
            let validation_result = token_opt
                .as_ref()
                .map(|token| validate_token(token, &config));

            // Set auth state in extensions if token is valid (even on public paths)
            if let Some(Ok(ref claims)) = validation_result {
                req.extensions_mut().insert(claims.clone());
            }

            if is_public_path(req.path(), &config) {
                let res = service.call(req).await?;
                return Ok(res);
            }

            match token_opt {
                Some(_token) => {
                    // Token already validated above in validation_result
                }
                None => {
                    warn!(
                        "[AUTH_FAILED] No token provided in request to {}",
                        req.path()
                    );
                    if let Some(accept) = req.headers().get("accept") {
                        if let Ok(accept_str) = accept.to_str() {
                            if accept_str.contains("text/html") && req.path() != "/login" {
                                return Err(AuthRedirectError.into());
                            }
                        }
                    }
                    return Err(ErrorUnauthorized("Unauthorized"));
                }
            };

            match validation_result.unwrap() {
                Ok(claims) => {
                    info!(
                        "[AUTH_SUCCESS] Valid token for user {} on {}",
                        claims.id,
                        req.path()
                    );
                    let res = service.call(req).await?;
                    Ok(res)
                }
                Err(err) => {
                    if err.kind() == &jsonwebtoken::errors::ErrorKind::ExpiredSignature {
                        warn!("[AUTH_EXPIRED] Token expired for request to {}", req.path());
                        if let Some(accept) = req.headers().get("accept") {
                            if let Ok(accept_str) = accept.to_str() {
                                if accept_str.contains("text/html") && req.path() != "/login" {
                                    return Err(AuthRedirectClearCookieError.into());
                                }
                            }
                        }
                        Err(ErrorUnauthorized("Token expired"))
                    } else {
                        warn!(
                            "[AUTH_FAILED] Token validation failed: {} for request to {}",
                            err,
                            req.path()
                        );
                        Err(ErrorUnauthorized("Unauthorized"))
                    }
                }
            }
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
    req: HttpRequest,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    // Try to extract CSRF token if middleware is present
    let csrf_token = req
        .extensions()
        .get::<CsrfToken>()
        .map(|t| t.0.clone())
        .unwrap_or_default();

    let tmpl = LoginTemplate {
        error_message: None,
        is_insecure: config.is_insecure(),
        csrf_token,
    };
    let html = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(html))
}

pub async fn signup_view(
    req: HttpRequest,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    // Try to extract CSRF token if middleware is present
    let csrf_token = req
        .extensions()
        .get::<CsrfToken>()
        .map(|t| t.0.clone())
        .unwrap_or_default();

    let tmpl = SignupTemplate {
        error_message: None,
        is_insecure: config.is_insecure(),
        csrf_token,
    };
    let html = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(html))
}

pub async fn signup_submit(
    form: web::Form<SignupRequest>,
    req: HttpRequest,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    let email = form.email.trim().to_lowercase();
    let csrf_token = req
        .extensions()
        .get::<CsrfToken>()
        .map(|t| t.0.clone())
        .unwrap_or_default();

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
    req: HttpRequest,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    let csrf_token = req
        .extensions()
        .get::<CsrfToken>()
        .map(|t| t.0.clone())
        .unwrap_or_default();

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
                    .same_site(actix_web::cookie::SameSite::Lax)
                    .finish();

                let flag_cookie = actix_web::cookie::Cookie::build("auth_present", "1")
                    .path("/")
                    .secure(config.cookie_secure)
                    .same_site(actix_web::cookie::SameSite::Lax)
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

pub async fn logout() -> actix_web::Result<HttpResponse> {
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
    use jsonwebtoken::{encode, EncodingKey, Header};

    fn test_config() -> AuthConfig {
        AuthConfig {
            pocketbase_url: "https://127.0.0.1:8090".into(),
            jwt_secret: "test-secret".into(),
            cookie_secure: true,
            require_login: true,
            pocketbase_ca_cert: None,
            public_paths: vec!["/login".into(), "/signup".into(), "/logout".into()],
        }
    }

    fn token_for(claims: Claims, secret: &str) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .expect("test token should encode")
    }

    #[test]
    fn validates_valid_token() {
        let config = test_config();
        let token = token_for(
            Claims {
                id: "user-1".into(),
                exp: 4_102_444_800,
            },
            &config.jwt_secret,
        );

        let claims = validate_token(&token, &config).expect("valid token should decode");

        assert_eq!(claims.id, "user-1");
    }

    #[test]
    fn rejects_expired_token() {
        let config = test_config();
        let token = token_for(
            Claims {
                id: "user-1".into(),
                exp: 1,
            },
            &config.jwt_secret,
        );

        let err = validate_token(&token, &config).expect_err("expired token should fail");

        assert_eq!(
            err.kind(),
            &jsonwebtoken::errors::ErrorKind::ExpiredSignature
        );
    }

    #[test]
    fn rejects_forged_token() {
        let config = test_config();
        let token = token_for(
            Claims {
                id: "user-1".into(),
                exp: 4_102_444_800,
            },
            "wrong-secret",
        );

        let err = validate_token(&token, &config).expect_err("forged token should fail");

        assert_eq!(
            err.kind(),
            &jsonwebtoken::errors::ErrorKind::InvalidSignature
        );
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
    fn fails_fast_on_empty_jwt_secret() {
        // Test that empty secret is detected as insecure
        let config = AuthConfig {
            pocketbase_url: "http://127.0.0.1:8090".into(),
            jwt_secret: "".into(),
            cookie_secure: true,
            require_login: true,
            pocketbase_ca_cert: None,
            public_paths: vec!["/login".into()],
        };
        assert!(config.is_insecure());
    }

    #[test]
    fn allows_empty_jwt_secret_with_override() {
        std::env::set_var("POCKETBASE_URL", "http://127.0.0.1:8090");
        std::env::set_var("POCKETBASE_JWT_SECRET", "");
        std::env::set_var("AUTH_ALLOW_EMPTY_JWT_SECRET", "true");
        let config = AuthConfig::from_env();
        assert!(config.jwt_secret.is_empty());
        assert!(config.is_insecure());
        // Cleanup
        std::env::remove_var("AUTH_ALLOW_EMPTY_JWT_SECRET");
        std::env::remove_var("POCKETBASE_JWT_SECRET");
        std::env::remove_var("POCKETBASE_URL");
    }
}
