use actix_web::body::BoxBody;
use actix_web::body::EitherBody;
use actix_web::{cookie::Cookie, dev::ServiceResponse, http::header::HeaderMap, http::StatusCode};
use music_browser::auth::AuthConfig;
use serde::{Deserialize, Serialize};
use std::fs;

/// Endpoint configuration from YAML
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EndpointConfig {
    pub get_endpoints: Vec<GetEndpoint>,
    pub form_groups: Vec<FormGroup>,
    pub button_endpoints: Vec<ButtonGroup>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetEndpoint {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub requires_auth: bool,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FormGroup {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub endpoints: Vec<FormEndpoint>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FormEndpoint {
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub requires_auth: bool,
    #[serde(default)]
    pub requires_csrf: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ButtonGroup {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub endpoints: Vec<ButtonEndpoint>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ButtonEndpoint {
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub requires_auth: bool,
    #[serde(default)]
    pub requires_csrf: bool,
    #[serde(default)]
    pub requires_song: bool,
}

/// Parse the endpoints.yaml configuration file
pub fn parse_endpoint_config() -> EndpointConfig {
    let yaml_content =
        fs::read_to_string("tests/endpoints.yaml").expect("Failed to read endpoints.yaml");
    serde_yaml::from_str(&yaml_content).expect("Failed to parse endpoints.yaml")
}

/// Create a test auth config for testing
pub fn test_auth_config(pocketbase_url: String) -> AuthConfig {
    AuthConfig {
        pocketbase_url,
        pocketbase_ca_cert: None,
        cookie_secure: false,
        require_login: false,
        public_paths: vec!["/login".into(), "/signup".into(), "/logout".into()],
        workflow_allowed_roots: vec![],
    }
}

/// Extract CSRF token from response body
pub fn extract_csrf_token(body: &str) -> Option<String> {
    body.split("name=\"csrf_token\" value=\"")
        .nth(1)
        .and_then(|s| s.split("\"").next())
        .map(|s| s.to_string())
}

/// Extract CSRF cookie from response
pub fn extract_csrf_cookie<B>(resp: &ServiceResponse<B>) -> Option<Cookie<'static>> {
    resp.response()
        .cookies()
        .into_iter()
        .find(|cookie| cookie.name() == "CSRF-ANON")
        .map(|c| c.clone().into_owned())
}

/// Extract pre-session cookie from response
pub fn extract_pre_session_cookie<B>(resp: &ServiceResponse<B>) -> Option<Cookie<'static>> {
    resp.response()
        .cookies()
        .into_iter()
        .find(|cookie| cookie.name() == "pre-session")
        .map(|c| c.clone().into_owned())
}

/// Extract auth token cookie from response
pub fn extract_auth_token_cookie<B>(resp: &ServiceResponse<B>) -> Option<Cookie<'static>> {
    resp.response()
        .cookies()
        .into_iter()
        .find(|cookie| cookie.name() == "auth_token")
        .map(|c| c.clone().into_owned())
}

/// Assert debug information for a response
pub fn assert_debug_info(
    status: StatusCode,
    body: &str,
    headers: &HeaderMap,
    cookies: Vec<Cookie<'_>>,
) {
    eprintln!("=== DEBUG INFO ===");
    eprintln!("Status: {}", status);
    eprintln!("Headers:");
    for (name, value) in headers.iter() {
        eprintln!("  {}: {:?}", name, value);
    }
    eprintln!("Cookies:");
    for cookie in cookies {
        eprintln!(
            "  {}: http_only={:?}, secure={:?}, same_site={:?}",
            cookie.name(),
            cookie.http_only(),
            cookie.secure(),
            cookie.same_site()
        );
    }
    eprintln!("Body (first 500 chars):");
    eprintln!("  {}", &body.chars().take(500).collect::<String>());
    eprintln!("==================");
}
