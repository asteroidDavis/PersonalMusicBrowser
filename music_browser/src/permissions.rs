use actix_web::HttpMessage;
use actix_web::{http::StatusCode, web, HttpRequest, HttpResponse, ResponseError};
use log::{info, warn};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use thiserror::Error;
use uuid::Uuid;

use std::path::Path;

use crate::acl::{AccessLevel, CreateShare, GroupMember, GroupShare, ResourceType, Share};
use crate::auth::{AuthConfig, AuthenticatedUser};
use crate::jobs::TargetType;
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
    require_access_level_or_401(
        req,
        pocketbase,
        resource_type,
        resource_id,
        AccessLevel::Editor,
    )
    .await
}

/// Require `admin` access on `(resource_type, resource_id)` — used by
/// share-management endpoints where granting access is itself privileged.
/// Same skip/fail-closed semantics as `require_edit_access_or_401`.
pub async fn require_admin_access_or_401(
    req: &HttpRequest,
    pocketbase: Option<&web::Data<PocketBaseClient>>,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<(), PermissionError> {
    require_access_level_or_401(
        req,
        pocketbase,
        resource_type,
        resource_id,
        AccessLevel::Admin,
    )
    .await
}

async fn require_access_level_or_401(
    req: &HttpRequest,
    pocketbase: Option<&web::Data<PocketBaseClient>>,
    resource_type: ResourceType,
    resource_id: i64,
    required: AccessLevel,
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
    let allowed = match check_user_access_cached(req, pocketbase, &user, resource_type, resource_id)
        .await
    {
        Ok(Some(access)) => match required {
            AccessLevel::Admin => access.access_level.can_admin(),
            AccessLevel::Editor => access.access_level.can_edit(),
            AccessLevel::Viewer => true,
        },
        Ok(None) => false,
        Err(PermissionError::MissingAclCollections) => {
            warn!(
                "[ACL_CHECK_SKIPPED] PocketBase ACL collections missing for resource_type={} resource_id={} user_id={}",
                resource_type,
                resource_id,
                user.id
            );
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    if allowed {
        Ok(())
    } else {
        Err(PermissionError::NotFound)
    }
}

/// Require the request's `AuthenticatedUser` to be able to manage the given
/// `groups` record: either `owner_id` on the group, or an `owner`/`admin`
/// membership row. `NotFound` when the group doesn't exist or the user has
/// no manage rights. Same skip semantics as the resource helpers.
pub async fn require_group_manage_or_404(
    req: &HttpRequest,
    pocketbase: Option<&web::Data<PocketBaseClient>>,
    group_id: &str,
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
            "[ACL_CHECK_SKIPPED] PocketBase client missing for group_id={} user_id={}",
            group_id, user.id
        );
        return Ok(());
    };

    match user_can_manage_group(req, pocketbase, &user, group_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(PermissionError::NotFound),
        Err(PermissionError::MissingAclCollections) => {
            warn!(
                "[ACL_CHECK_SKIPPED] PocketBase ACL collections missing for group_id={} user_id={}",
                group_id, user.id
            );
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Authorize a `/api/workflows` enqueue request by target.
///
/// - Without an `AuthenticatedUser`, the usual single-tenant rule applies:
///   allowed when `AUTH_REQUIRE_LOGIN=false`, `NotAuthenticated` otherwise.
/// - `song`/`live_set` targets require edit access on the parsed id
///   (delegates to `require_edit_access_or_401`, including its skip
///   semantics for a missing PocketBase client or ACL collections).
/// - `file`/`directory` targets require the canonicalized path to live
///   under a `WORKFLOW_ALLOWED_ROOTS` entry (or the upload temp dir, which
///   the upload handler owns). An authenticated deployment with no roots
///   configured denies them — the job subprocess would otherwise run
///   against arbitrary filesystem paths.
pub async fn authorize_workflow_target(
    req: &HttpRequest,
    pocketbase: Option<&web::Data<PocketBaseClient>>,
    target_type: &TargetType,
    target_id_or_path: &str,
) -> Result<(), PermissionError> {
    require_authenticated_or_401(req)?;
    if authenticated_user(req).is_none() {
        return Ok(());
    }

    match target_type {
        TargetType::Song | TargetType::LiveSet => {
            let resource_type = match target_type {
                TargetType::Song => ResourceType::Song,
                _ => ResourceType::LiveSet,
            };
            let id = target_id_or_path
                .parse::<i64>()
                .map_err(|_| PermissionError::NotFound)?;
            require_edit_access_or_401(req, pocketbase, resource_type, id).await
        }
        TargetType::File | TargetType::Directory => {
            let mut roots: Vec<std::path::PathBuf> = req
                .app_data::<web::Data<AuthConfig>>()
                .map(|config| config.workflow_allowed_roots.clone())
                .unwrap_or_default();
            // The upload handler persists audio under this directory itself.
            roots.push(std::env::temp_dir().join("pmb_uploads"));
            if path_under_any_root(target_id_or_path, &roots) {
                Ok(())
            } else {
                Err(PermissionError::NotFound)
            }
        }
    }
}

/// Whether `path` (canonicalized) lives inside one of `roots`
/// (canonicalized when possible, compared verbatim otherwise).
fn path_under_any_root(path: &str, roots: &[std::path::PathBuf]) -> bool {
    let Ok(canonical) = Path::new(path).canonicalize() else {
        return false;
    };
    roots.iter().any(|root| {
        let root = root.canonicalize().unwrap_or_else(|_| root.clone());
        canonical.starts_with(&root)
    })
}

/// Whether `user` may manage `group_id`: `owner_id` on the group record, or
/// an `owner`/`admin` membership. `NotFound` when the group record itself
/// does not resolve.
pub async fn user_can_manage_group(
    req: &HttpRequest,
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    group_id: &str,
) -> Result<bool, PermissionError> {
    match pb_client.get_group(&user.token, group_id).await {
        Ok(group) if group.owner_id == user.id => return Ok(true),
        Ok(_) => {}
        Err(err) if err.is_missing_collection() => {
            return Err(permission_backend_error(err, "user_can_manage_group"));
        }
        Err(err) if err.is_not_found() => return Err(PermissionError::NotFound),
        Err(err) => return Err(permission_backend_error(err, "user_can_manage_group")),
    }
    Ok(user_group_memberships_cached(req, pb_client, user)
        .await?
        .iter()
        .any(|m| m.group_id == group_id && m.role.can_manage()))
}

/// Resolve which `resource_type` ids the request may see in a list view.
///
/// Returns `Some(ids)` when the response must be filtered to `ids`, `None`
/// when list filtering is disabled for this deployment (no authenticated
/// user in single-tenant mode, no `PocketBaseClient`, or missing ACL
/// collections — all warned/skipped like the mutate-path helpers). When
/// login is required but no user is present it fails closed with
/// `NotAuthenticated`.
pub async fn list_visibility(
    req: &HttpRequest,
    pocketbase: Option<&web::Data<PocketBaseClient>>,
    resource_type: ResourceType,
) -> Result<Option<Rc<HashSet<i64>>>, PermissionError> {
    let Some(user) = authenticated_user(req) else {
        return if login_required(req) {
            Err(PermissionError::NotAuthenticated)
        } else {
            Ok(None)
        };
    };
    let Some(pocketbase) = pocketbase.map(|d| d.get_ref()) else {
        warn!(
            "[ACL_LIST_SKIPPED] PocketBase client missing for resource_type={} user_id={}",
            resource_type, user.id
        );
        return Ok(None);
    };
    match accessible_resource_ids(req, pocketbase, &user, resource_type).await {
        Ok(ids) => Ok(Some(ids)),
        Err(PermissionError::MissingAclCollections) => {
            warn!(
                "[ACL_LIST_SKIPPED] PocketBase ACL collections missing for resource_type={} user_id={}",
                resource_type, user.id
            );
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

/// Retain only rows whose `id` is in `visible`, when `visible` is
/// `Some`. `None` means list filtering is disabled — keep everything.
pub fn retain_visible<T>(
    visible: &Option<Rc<HashSet<i64>>>,
    items: &mut Vec<T>,
    id_of: impl Fn(&T) -> i64,
) {
    if let Some(ids) = visible {
        items.retain(|item| ids.contains(&id_of(item)));
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

// ---------------------------------------------------------------------------
// Per-request ACL memoization
// ---------------------------------------------------------------------------

/// PocketBase ACL lookups already performed for this request, stored in
/// the request's extensions.
///
/// Multiple authorization checks within one request (e.g. a
/// parent-resource cascade that resolves a child's song and then checks
/// the song) issue a single PocketBase list request per resource/user
/// instead of one per check.
#[derive(Clone, Default)]
pub struct RequestAclCache {
    inner: Rc<RefCell<RequestAclCacheInner>>,
}

#[derive(Default)]
struct RequestAclCacheInner {
    /// `shares` rows for a resource, keyed by (resource_type, resource_id).
    resource_shares: HashMap<(ResourceType, i64), Rc<Vec<Share>>>,
    /// All `shares` rows granted to a user, keyed by user id.
    user_shares: HashMap<String, Rc<Vec<Share>>>,
    /// `group_memberships` rows for a user, keyed by user id.
    group_memberships: HashMap<String, Rc<Vec<GroupMember>>>,
    /// `group_shares` rows for a group, keyed by group id.
    group_shares: HashMap<String, Rc<Vec<GroupShare>>>,
    /// Resolved accessible id sets, keyed by (user id, resource_type).
    accessible: HashMap<(String, ResourceType), Rc<HashSet<i64>>>,
}

fn request_acl_cache(req: &HttpRequest) -> RequestAclCache {
    if let Some(cache) = req.extensions().get::<RequestAclCache>() {
        return cache.clone();
    }
    let cache = RequestAclCache::default();
    req.extensions_mut().insert(cache.clone());
    cache
}

/// `PocketBaseClient::list_resource_shares` memoized for the lifetime of
/// `req`: repeated lookups of the same resource within one request issue
/// a single PocketBase call.
pub async fn resource_shares_cached(
    req: &HttpRequest,
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<Rc<Vec<Share>>, PermissionError> {
    let cache = request_acl_cache(req);
    if let Some(shares) = cache
        .inner
        .borrow()
        .resource_shares
        .get(&(resource_type, resource_id))
    {
        return Ok(Rc::clone(shares));
    }
    let shares = Rc::new(
        pb_client
            .list_resource_shares(
                &user.token,
                resource_type.as_str(),
                &resource_id.to_string(),
            )
            .await
            .map_err(|err| permission_backend_error(err, "resource_shares_cached"))?,
    );
    cache
        .inner
        .borrow_mut()
        .resource_shares
        .insert((resource_type, resource_id), Rc::clone(&shares));
    Ok(shares)
}

/// `PocketBaseClient::list_user_shares` memoized for the lifetime of
/// `req`. Used by list-view filtering which needs the user's grants for
/// many resources of one type.
pub async fn user_shares_cached(
    req: &HttpRequest,
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
) -> Result<Rc<Vec<Share>>, PermissionError> {
    let cache = request_acl_cache(req);
    if let Some(shares) = cache.inner.borrow().user_shares.get(&user.id) {
        return Ok(Rc::clone(shares));
    }
    let shares = Rc::new(
        pb_client
            .list_user_shares(&user.token, &user.id)
            .await
            .map_err(|err| permission_backend_error(err, "user_shares_cached"))?,
    );
    cache
        .inner
        .borrow_mut()
        .user_shares
        .insert(user.id.clone(), Rc::clone(&shares));
    Ok(shares)
}

/// `list_user_group_memberships` memoized for the lifetime of `req`.
pub async fn user_group_memberships_cached(
    req: &HttpRequest,
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
) -> Result<Rc<Vec<GroupMember>>, PermissionError> {
    let cache = request_acl_cache(req);
    if let Some(memberships) = cache.inner.borrow().group_memberships.get(&user.id) {
        return Ok(Rc::clone(memberships));
    }
    let memberships = Rc::new(
        pb_client
            .list_user_group_memberships(&user.token, &user.id)
            .await
            .map_err(|err| permission_backend_error(err, "user_group_memberships_cached"))?,
    );
    cache
        .inner
        .borrow_mut()
        .group_memberships
        .insert(user.id.clone(), Rc::clone(&memberships));
    Ok(memberships)
}

/// `list_group_shares` memoized for the lifetime of `req`.
pub async fn group_shares_cached(
    req: &HttpRequest,
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    group_id: &str,
) -> Result<Rc<Vec<GroupShare>>, PermissionError> {
    let cache = request_acl_cache(req);
    if let Some(shares) = cache.inner.borrow().group_shares.get(group_id) {
        return Ok(Rc::clone(shares));
    }
    let shares = Rc::new(
        pb_client
            .list_group_shares(&user.token, group_id)
            .await
            .map_err(|err| permission_backend_error(err, "group_shares_cached"))?,
    );
    cache
        .inner
        .borrow_mut()
        .group_shares
        .insert(group_id.to_string(), Rc::clone(&shares));
    Ok(shares)
}

/// Every resource id of `resource_type` the user can access: their direct
/// `shares` unioned with `group_shares` of every group they belong to.
/// The result is memoized for the request's lifetime.
pub async fn accessible_resource_ids(
    req: &HttpRequest,
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    resource_type: ResourceType,
) -> Result<Rc<HashSet<i64>>, PermissionError> {
    let cache = request_acl_cache(req);
    let key = (user.id.clone(), resource_type);
    if let Some(ids) = cache.inner.borrow().accessible.get(&key) {
        return Ok(Rc::clone(ids));
    }

    let mut ids = HashSet::new();
    for share in user_shares_cached(req, pb_client, user).await?.iter() {
        if share.resource_type == resource_type.as_str() {
            if let Ok(id) = share.resource_id.parse::<i64>() {
                ids.insert(id);
            }
        }
    }

    for membership in user_group_memberships_cached(req, pb_client, user)
        .await?
        .iter()
    {
        for group_share in group_shares_cached(req, pb_client, user, &membership.group_id)
            .await?
            .iter()
        {
            if group_share.resource_type == resource_type.as_str() {
                if let Ok(id) = group_share.resource_id.parse::<i64>() {
                    ids.insert(id);
                }
            }
        }
    }

    let ids = Rc::new(ids);
    cache
        .inner
        .borrow_mut()
        .accessible
        .insert(key, Rc::clone(&ids));
    Ok(ids)
}

/// `check_user_access` against the request's memoized ACL cache.
pub async fn check_user_access_cached(
    req: &HttpRequest,
    pb_client: &PocketBaseClient,
    user: &AuthenticatedUser,
    resource_type: ResourceType,
    resource_id: i64,
) -> Result<Option<ResourceAccess>, PermissionError> {
    let shares = resource_shares_cached(req, pb_client, user, resource_type, resource_id).await?;
    Ok(
        best_access_for_user(&shares, &user.id).map(|access_level| ResourceAccess {
            user_id: user.id.clone(),
            access_level,
        }),
    )
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
            workflow_allowed_roots: vec![],
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
