use actix_web::HttpMessage;
use actix_web::{http::StatusCode, web, HttpRequest, HttpResponse, ResponseError};
use log::{info, warn};
use thiserror::Error;
use uuid::Uuid;

use crate::acl::{AccessLevel, CreateShare, ResourceType, Share};
use crate::auth::{AuthConfig, AuthenticatedUser};
use crate::pocketbase_client::{PocketBaseClient, PocketBaseClientError};

#[derive(Debug, Error)]
pub enum PermissionError {
    #[error("not authenticated")]
    NotAuthenticated,
    #[error("resource not found")]
    NotFound,
    #[error("PocketBase ACL collections are not configured")]
    MissingAclCollections,
    #[error("permission backend unavailable")]
    BackendUnavailable { trace_id: String },
}

impl ResponseError for PermissionError {
    fn status_code(&self) -> StatusCode {
        match self {
            PermissionError::NotAuthenticated => StatusCode::UNAUTHORIZED,
            PermissionError::NotFound => StatusCode::NOT_FOUND,
            PermissionError::MissingAclCollections => StatusCode::SERVICE_UNAVAILABLE,
            PermissionError::BackendUnavailable { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            PermissionError::NotAuthenticated => HttpResponse::Unauthorized().finish(),
            PermissionError::NotFound => HttpResponse::NotFound().finish(),
            PermissionError::MissingAclCollections => HttpResponse::ServiceUnavailable()
                .body("PocketBase ACL collections are not configured"),
            PermissionError::BackendUnavailable { trace_id } => HttpResponse::InternalServerError()
                .insert_header(("x-trace-id", trace_id.clone()))
                .body(format!("Internal error. Trace ID: {trace_id}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceAccess {
    pub user_id: String,
    pub access_level: AccessLevel,
}

pub fn authenticated_user(req: &HttpRequest) -> Option<AuthenticatedUser> {
    req.extensions().get::<AuthenticatedUser>().cloned()
}

pub async fn check_user_access(
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<Option<ResourceAccess>, PermissionError> {
    let shares = pb_client
        .list_resource_shares(
            &user.token,
            resource_type.as_str(),
            &resource_id.to_string(),
        )
        .await
        .map_err(|err| permission_backend_error(err, "check_user_access"))?;

    let best_access = best_access_for_user(&shares, &user.id);
    Ok(best_access.map(|access_level| ResourceAccess {
        user_id: user.id.clone(),
        access_level,
    }))
}

pub async fn require_access(
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<ResourceAccess, PermissionError> {
    check_user_access(pb_client, user, resource_type, resource_id)
        .await?
        .ok_or(PermissionError::NotFound)
}

pub async fn require_edit_access(
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<ResourceAccess, PermissionError> {
    let access = require_access(pb_client, user, resource_type, resource_id).await?;
    if access.access_level.can_edit() {
        Ok(access)
    } else {
        Err(PermissionError::NotFound)
    }
}

pub async fn grant_creator_admin_share(
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<Share, PermissionError> {
    let share = CreateShare {
        user_id: user.id.clone(),
        resource_type: resource_type.as_str().to_string(),
        resource_id: resource_id.to_string(),
        access_level: AccessLevel::Admin,
        created_by: user.id.clone(),
    };
    let created_share = pb_client
        .create_share(&user.token, &share)
        .await
        .map_err(|err| permission_backend_error(err, "grant_creator_admin_share"))?;
    info!(
        "[ACL_SHARE_CREATED] resource_type={} resource_id={} user_id={} access_level={}",
        resource_type,
        resource_id,
        user.id,
        AccessLevel::Admin
    );
    Ok(created_share)
}

// ---------------------------------------------------------------------------
// Request-scoped authorization helpers — the single call-site pattern every
// mutating handler uses. They centralize the "authenticated or single-tenant"
// and "PocketBase optionally configured" semantics so individual handlers stay
// a one-line call.
// ---------------------------------------------------------------------------

/// Whether this deployment requires login (`AUTH_REQUIRE_LOGIN`).
///
/// Reads the `AuthConfig` from application data; defaults to `false` when no
/// config is registered (unit tests / single-tenant local-dev mode).
fn login_required(req: &HttpRequest) -> bool {
    req.app_data::<web::Data<AuthConfig>>()
        .map(|config| config.require_login)
        .unwrap_or(false)
}

/// Require the request to carry an `AuthenticatedUser`.
///
/// In single-tenant mode (`AUTH_REQUIRE_LOGIN=false`) anonymous requests are
/// allowed so the app keeps working without PocketBase identities. When login
/// is required, a missing user fails closed — the JWT middleware should have
/// rejected the request already, so reaching this point indicates
/// misconfiguration (e.g. the route was added to `public_paths`).
pub fn require_authenticated_or_401(req: &HttpRequest) -> Result<(), PermissionError> {
    if authenticated_user(req).is_none() && login_required(req) {
        return Err(PermissionError::NotAuthenticated);
    }
    Ok(())
}

/// Require the request's `AuthenticatedUser` to hold edit access (an `editor`
/// or `admin` share) on `(resource_type, resource_id)`.
///
/// - No user + `AUTH_REQUIRE_LOGIN=false` → allowed (single-tenant local dev).
/// - No user + login required → `NotAuthenticated` (fail closed).
/// - No `PocketBaseClient` configured, or the ACL collections are missing →
///   allowed with a warning, so a deployment without the sharing collections
///   keeps working.
/// - A denied share check reports `NotFound` so resources the caller cannot
///   access stay invisible.
pub async fn require_edit_access_or_401(
    req: &HttpRequest,
    pocketbase: Option<&web::Data<PocketBaseClient>>,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<(), PermissionError> {
    let Some(user) = authenticated_user(req) else {
        return if login_required(req) {
            Err(PermissionError::NotAuthenticated)
        } else {
            Ok(())
        };
    };
    let Some(pocketbase) = pocketbase.map(|d| d.get_ref()) else {
        warn!(
            "[ACL_CHECK_SKIPPED] PocketBase client missing for resource_type={} resource_id={} user_id={}",
            resource_type,
            resource_id,
            user.id
        );
        return Ok(());
    };
    match require_edit_access(pocketbase, &user, resource_type, resource_id).await {
        Ok(_) => Ok(()),
        Err(PermissionError::MissingAclCollections) => {
            warn!(
                "[ACL_CHECK_SKIPPED] PocketBase ACL collections missing for resource_type={} resource_id={} user_id={}",
                resource_type,
                resource_id,
                user.id
            );
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Grant the request's `AuthenticatedUser` a creator `admin` share on a
/// freshly created resource. Best-effort bookkeeping: skips silently (with a
/// warning) when there is no authenticated user, no `PocketBaseClient`, or no
/// ACL collections.
pub async fn grant_creator_admin_share_if_authenticated(
    req: &HttpRequest,
    pocketbase: Option<&web::Data<PocketBaseClient>>,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<(), PermissionError> {
    let Some(user) = authenticated_user(req) else {
        return Ok(());
    };
    let Some(pocketbase) = pocketbase.map(|d| d.get_ref()) else {
        warn!(
            "[ACL_SHARE_SKIPPED] PocketBase client missing for resource_type={} resource_id={} user_id={}",
            resource_type,
            resource_id,
            user.id
        );
        return Ok(());
    };
    match grant_creator_admin_share(pocketbase, &user, resource_type, resource_id).await {
        Ok(_) => Ok(()),
        Err(PermissionError::MissingAclCollections) => {
            warn!(
                "[ACL_SHARE_SKIPPED] PocketBase ACL collections missing for resource_type={} resource_id={} user_id={}",
                resource_type,
                resource_id,
                user.id
            );
            Ok(())
        }
        Err(err) => Err(err),
    }
}

fn best_access_for_user(shares: &[Share], user_id: &str) -> Option<AccessLevel> {
    let mut best = None;
    for share in shares.iter().filter(|share| share.user_id == user_id) {
        best = Some(match (best, share.access_level) {
            (Some(AccessLevel::Admin), _) | (_, AccessLevel::Admin) => AccessLevel::Admin,
            (Some(AccessLevel::Editor), _) | (_, AccessLevel::Editor) => AccessLevel::Editor,
            _ => AccessLevel::Viewer,
        });
    }
    best
}

fn permission_backend_error(err: PocketBaseClientError, context: &str) -> PermissionError {
    if err.is_missing_collection() {
        warn!(
            "[ACL_COLLECTIONS_MISSING] context={} error={}",
            context, err
        );
        return PermissionError::MissingAclCollections;
    }
    let trace_id = Uuid::new_v4().to_string();
    warn!(
        "[ACL_BACKEND_FAILED] trace_id={} context={} error={}",
        trace_id, context, err
    );
    PermissionError::BackendUnavailable { trace_id }
}

#[cfg(test)]
mod tests {
    use super::{
        authenticated_user, best_access_for_user, grant_creator_admin_share_if_authenticated,
        require_authenticated_or_401, require_edit_access_or_401, PermissionError,
    };
    use crate::acl::{AccessLevel, ResourceType, Share};
    use crate::auth::{AuthConfig, AuthenticatedUser};
    use actix_web::{test::TestRequest, web, HttpMessage, HttpRequest};

    fn share(user_id: &str, access_level: AccessLevel) -> Share {
        Share {
            id: format!("{user_id}-{access_level}"),
            user_id: user_id.to_string(),
            resource_type: "song".to_string(),
            resource_id: "1".to_string(),
            access_level,
            created_by: "owner".to_string(),
        }
    }

    #[test]
    fn best_access_uses_highest_access_for_user() {
        let shares = vec![
            share("user-a", AccessLevel::Viewer),
            share("user-b", AccessLevel::Admin),
            share("user-a", AccessLevel::Editor),
        ];
        assert_eq!(
            best_access_for_user(&shares, "user-a"),
            Some(AccessLevel::Editor)
        );
    }

    #[test]
    fn best_access_ignores_other_users() {
        let shares = vec![share("user-b", AccessLevel::Admin)];
        assert_eq!(best_access_for_user(&shares, "user-a"), None);
    }

    fn auth_config(require_login: bool) -> AuthConfig {
        AuthConfig {
            pocketbase_url: "http://127.0.0.1:1".to_string(),
            cookie_secure: false,
            require_login,
            pocketbase_ca_cert: None,
            public_paths: vec![],
        }
    }

    fn request(require_login: bool, user: Option<AuthenticatedUser>) -> HttpRequest {
        let req = TestRequest::default()
            .app_data(web::Data::new(auth_config(require_login)))
            .to_http_request();
        if let Some(user) = user {
            req.extensions_mut().insert(user);
        }
        req
    }

    fn user() -> AuthenticatedUser {
        AuthenticatedUser {
            id: "user-a".to_string(),
            token: "token".to_string(),
        }
    }

    #[test]
    fn authenticated_user_reads_extensions() {
        let req = request(false, Some(user()));
        assert_eq!(
            authenticated_user(&req).map(|u| u.id),
            Some("user-a".into())
        );
        let req = request(false, None);
        assert!(authenticated_user(&req).is_none());
    }

    #[test]
    fn require_authenticated_allows_anonymous_in_single_tenant_mode() {
        let req = request(false, None);
        assert!(require_authenticated_or_401(&req).is_ok());
    }

    #[test]
    fn require_authenticated_fails_closed_when_login_required() {
        let req = request(true, None);
        assert!(matches!(
            require_authenticated_or_401(&req),
            Err(PermissionError::NotAuthenticated)
        ));
    }

    #[test]
    fn require_authenticated_allows_authenticated_user() {
        let req = request(true, Some(user()));
        assert!(require_authenticated_or_401(&req).is_ok());
    }

    #[tokio::test]
    async fn edit_check_allows_anonymous_in_single_tenant_mode() {
        let req = request(false, None);
        assert!(
            require_edit_access_or_401(&req, None, ResourceType::Song, 1)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn edit_check_fails_closed_when_login_required() {
        let req = request(true, None);
        assert!(matches!(
            require_edit_access_or_401(&req, None, ResourceType::Song, 1).await,
            Err(PermissionError::NotAuthenticated)
        ));
    }

    #[tokio::test]
    async fn edit_check_skips_when_pocketbase_client_missing() {
        let req = request(true, Some(user()));
        assert!(
            require_edit_access_or_401(&req, None, ResourceType::Song, 1)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn creator_share_skips_without_authenticated_user() {
        let req = request(true, None);
        assert!(
            grant_creator_admin_share_if_authenticated(&req, None, ResourceType::Song, 1)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn creator_share_skips_when_pocketbase_client_missing() {
        let req = request(true, Some(user()));
        assert!(
            grant_creator_admin_share_if_authenticated(&req, None, ResourceType::Song, 1)
                .await
                .is_ok()
        );
    }
}
