use actix_web::HttpMessage;
use actix_web::{http::StatusCode, HttpRequest, HttpResponse, ResponseError};
use log::{info, warn};
use thiserror::Error;
use uuid::Uuid;

use crate::acl::{AccessLevel, CreateShare, ResourceType, Share};
use crate::auth::AuthenticatedUser;
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
    use super::best_access_for_user;
    use crate::acl::{AccessLevel, Share};

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
}
