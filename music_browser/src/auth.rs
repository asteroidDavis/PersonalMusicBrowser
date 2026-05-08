use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::error::{ErrorBadRequest, ErrorUnauthorized};
use actix_web::{web, Error, HttpMessage, HttpResponse, ResponseError};
use askama::Template;
use futures_util::future::LocalBoxFuture;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::future::{ready, Ready};
use std::rc::Rc;

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

        Self {
            pocketbase_url: std::env::var("POCKETBASE_URL")
                .unwrap_or_else(|_| "https://127.0.0.1:8090".into()),
            jwt_secret,
            cookie_secure: std::env::var("AUTH_COOKIE_SECURE")
                .map(|value| value == "true" || value == "1")
                .unwrap_or(true),
        }
    }

    pub fn auth_with_password_url(&self) -> String {
        format!(
            "{}/api/collections/users/auth-with-password",
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
                    if auth_str.starts_with("Bearer ") {
                        token_opt = Some(auth_str[7..].to_string());
                    }
                }
            }

            // 2. Fallback to extracting from HttpOnly cookie
            if token_opt.is_none() {
                if let Some(cookie) = req.cookie("auth_token") {
                    token_opt = Some(cookie.value().to_string());
                }
            }

            let token = match token_opt {
                Some(t) => t,
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

            match validate_token(&token, &config) {
                Ok(claims) => {
                    info!(
                        "[AUTH_SUCCESS] Valid token for user {} on {}",
                        claims.id,
                        req.path()
                    );
                    req.extensions_mut().insert(claims);
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
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub identity: String,
    pub password: String,
}

pub async fn login_view() -> actix_web::Result<HttpResponse> {
    let tmpl = LoginTemplate {
        error_message: None,
    };
    let html = tmpl
        .render()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().content_type("text/html").body(html))
}

pub async fn login_submit(
    form: web::Form<LoginRequest>,
    config: web::Data<AuthConfig>,
) -> actix_web::Result<HttpResponse> {
    if form.identity.trim().is_empty() || form.password.is_empty() {
        warn!("[AUTH_FAILED] Login rejected for empty identity or password.");
        return Err(ErrorBadRequest("Invalid login request"));
    }

    let client = reqwest::Client::new();
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

                return Ok(HttpResponse::SeeOther()
                    .append_header(("Location", "/"))
                    .cookie(cookie)
                    .finish());
            }
        }
        _ => {
            warn!("[AUTH_FAILED] PocketBase login failed or unreachable.");
        }
    }

    let tmpl = LoginTemplate {
        error_message: Some("Invalid credentials or server offline".into()),
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

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/login"))
        .cookie(cookie)
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
}
