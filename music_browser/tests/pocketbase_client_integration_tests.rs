//! Integration tests for `pocketbase_client` and `permissions` against a real
//! PocketBase instance (not a mock).
//!
//! These tests require a running PocketBase server, seeded via
//! `pocketbase/pb_migrations` (including the CI-only test-user seed
//! migration `1788730000_ci_test_seed.js`, gated behind `PB_TEST_SEED=true`),
//! and its URL provided via the `POCKETBASE_TEST_URL` env var.
//!
//! Run them with:
//!
//! ```bash
//! music_browser/scripts/run-pocketbase-integration-tests.sh
//! ```
//!
//! which spins up an ephemeral PocketBase instance, applies migrations, and
//! runs `cargo test` with `POCKETBASE_TEST_URL` set. This is wired into CI
//! (`.github/workflows/ci.yml`) and the pre-commit hook. If
//! `POCKETBASE_TEST_URL` is not set (e.g. a plain `cargo test` run), these
//! tests are skipped with a note rather than failing, so they don't block
//! unrelated local development.

use music_browser::acl::{
    AccessLevel, CreateGroup, CreateGroupMember, CreateGroupShare, CreateShare, GroupRole,
    ResourceType,
};
use music_browser::auth::AuthenticatedUser;
use music_browser::permissions::{
    grant_creator_admin_share, require_access, require_edit_access, PermissionError,
};
use music_browser::pocketbase_client::PocketBaseClient;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const TEST_USER_1_EMAIL: &str = "acl-test-user-1@example.com";
const TEST_USER_1_PASSWORD: &str = "AclTestUser1Password!";
const TEST_USER_2_EMAIL: &str = "acl-test-user-2@example.com";
const TEST_USER_2_PASSWORD: &str = "AclTestUser2Password!";

/// Returns the PocketBase base URL to test against, or `None` if the caller
/// should skip (no ephemeral instance was set up for this run).
fn pocketbase_test_url() -> Option<String> {
    std::env::var("POCKETBASE_TEST_URL").ok()
}

/// Skips the current test (with an explanatory message) unless a PocketBase
/// test instance is available.
macro_rules! require_pocketbase_or_skip {
    () => {
        match pocketbase_test_url() {
            Some(url) => url,
            None => {
                eprintln!(
                    "skipping {}: POCKETBASE_TEST_URL is not set. Run via \
                     `music_browser/scripts/run-pocketbase-integration-tests.sh` \
                     to exercise the PocketBase client integration tests.",
                    module_path!()
                );
                return;
            }
        }
    };
}

struct TestUser {
    id: String,
    token: String,
}

async fn authenticate(base_url: &str, email: &str, password: &str) -> TestUser {
    let http_client = reqwest::Client::new();
    let response = http_client
        .post(format!(
            "{base_url}/api/collections/users/auth-with-password"
        ))
        .json(&serde_json::json!({ "identity": email, "password": password }))
        .send()
        .await
        .expect("failed to reach PocketBase to authenticate seeded test user");
    assert!(
        response.status().is_success(),
        "seeded test user auth failed with status {}: is the CI test seed migration \
         (1788730000_ci_test_seed.js) applied with PB_TEST_SEED=true?",
        response.status()
    );
    let body: Value = response.json().await.expect("valid auth response JSON");
    TestUser {
        id: body["record"]["id"]
            .as_str()
            .expect("user id in auth response")
            .to_string(),
        token: body["token"]
            .as_str()
            .expect("token in auth response")
            .to_string(),
    }
}

fn unique_resource_id() -> String {
    format!("integration-test-{}", Uuid::new_v4())
}

/// A resource id in the numeric form expected by `permissions::*`, unique
/// enough per test run to avoid collisions between parallel tests sharing
/// the same PocketBase instance.
fn unique_numeric_resource_id() -> i64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    (nanos % i64::MAX as u128) as i64
}

fn pocketbase_client(base_url: &str) -> PocketBaseClient {
    PocketBaseClient::new(base_url.to_string(), reqwest::Client::new())
}

#[tokio::test]
async fn create_list_and_delete_resource_shares() {
    let base_url = require_pocketbase_or_skip!();
    let client = pocketbase_client(&base_url);
    let owner = authenticate(&base_url, TEST_USER_1_EMAIL, TEST_USER_1_PASSWORD).await;
    let grantee = authenticate(&base_url, TEST_USER_2_EMAIL, TEST_USER_2_PASSWORD).await;
    let resource_id = unique_resource_id();

    let share = client
        .create_share(
            &owner.token,
            &CreateShare {
                user_id: grantee.id.clone(),
                resource_type: ResourceType::Song.as_str().to_string(),
                resource_id: resource_id.clone(),
                access_level: AccessLevel::Editor,
                created_by: owner.id.clone(),
            },
        )
        .await
        .expect("create_share should succeed against a real PocketBase instance");
    assert_eq!(share.user_id, grantee.id);
    assert_eq!(share.access_level, AccessLevel::Editor);
    assert_eq!(share.resource_id, resource_id);

    let shares = client
        .list_resource_shares(&owner.token, ResourceType::Song.as_str(), &resource_id)
        .await
        .expect("list_resource_shares should succeed");
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].id, share.id);

    client
        .delete_share(&owner.token, &share.id)
        .await
        .expect("delete_share should succeed");

    let shares_after_delete = client
        .list_resource_shares(&owner.token, ResourceType::Song.as_str(), &resource_id)
        .await
        .expect("list_resource_shares should succeed after delete");
    assert!(shares_after_delete.is_empty());
}

#[tokio::test]
async fn list_user_shares_returns_shares_granted_to_that_user() {
    let base_url = require_pocketbase_or_skip!();
    let client = pocketbase_client(&base_url);
    let owner = authenticate(&base_url, TEST_USER_1_EMAIL, TEST_USER_1_PASSWORD).await;
    let grantee = authenticate(&base_url, TEST_USER_2_EMAIL, TEST_USER_2_PASSWORD).await;
    let resource_id = unique_resource_id();

    client
        .create_share(
            &owner.token,
            &CreateShare {
                user_id: grantee.id.clone(),
                resource_type: ResourceType::Album.as_str().to_string(),
                resource_id: resource_id.clone(),
                access_level: AccessLevel::Viewer,
                created_by: owner.id.clone(),
            },
        )
        .await
        .expect("create_share should succeed");

    let shares = client
        .list_user_shares(&grantee.token, &grantee.id)
        .await
        .expect("list_user_shares should succeed for the grantee");
    assert!(shares
        .iter()
        .any(|s| s.resource_id == resource_id && s.resource_type == ResourceType::Album.as_str()));
}

#[tokio::test]
async fn group_membership_and_group_share_workflow() {
    let base_url = require_pocketbase_or_skip!();
    let client = pocketbase_client(&base_url);
    let owner = authenticate(&base_url, TEST_USER_1_EMAIL, TEST_USER_1_PASSWORD).await;
    let member = authenticate(&base_url, TEST_USER_2_EMAIL, TEST_USER_2_PASSWORD).await;

    let group = client
        .create_group(
            &owner.token,
            &CreateGroup {
                name: format!("Integration Test Group {}", Uuid::new_v4()),
                description: "created by pocketbase_client_integration_tests".to_string(),
                owner_id: owner.id.clone(),
            },
        )
        .await
        .expect("create_group should succeed");
    let group_id = group["id"]
        .as_str()
        .expect("created group should have an id")
        .to_string();

    let membership = client
        .add_user_to_group(
            &owner.token,
            &CreateGroupMember {
                group_id: group_id.clone(),
                user_id: member.id.clone(),
                role: GroupRole::Member,
            },
        )
        .await
        .expect("add_user_to_group should succeed");
    assert_eq!(membership.user_id, member.id);
    assert_eq!(membership.group_id, group_id);

    // group_memberships' list rule only exposes records where user_id
    // matches the requester, so the member fetches their own membership
    // using their own token.
    let member_memberships = client
        .get_group_members(&member.token, &group_id)
        .await
        .expect("get_group_members should succeed for the member themselves");
    assert!(member_memberships.iter().any(|m| m.id == membership.id));

    let resource_id = unique_resource_id();
    let group_share = client
        .share_with_group(
            &owner.token,
            &CreateGroupShare {
                group_id: group_id.clone(),
                resource_type: ResourceType::Instrument.as_str().to_string(),
                resource_id: resource_id.clone(),
                access_level: AccessLevel::Viewer,
                created_by: owner.id.clone(),
            },
        )
        .await
        .expect("share_with_group should succeed");
    assert_eq!(group_share["group_id"], group_id);
    assert_eq!(group_share["resource_id"], resource_id);

    client
        .remove_group_member(&owner.token, &membership.id)
        .await
        .expect("remove_group_member should succeed");
}

#[tokio::test]
async fn permissions_module_resolves_real_share_grants() {
    let base_url = require_pocketbase_or_skip!();
    let client = pocketbase_client(&base_url);
    let owner = authenticate(&base_url, TEST_USER_1_EMAIL, TEST_USER_1_PASSWORD).await;
    let other = authenticate(&base_url, TEST_USER_2_EMAIL, TEST_USER_2_PASSWORD).await;
    let resource_id = unique_numeric_resource_id();

    let owner_user = AuthenticatedUser {
        id: owner.id.clone(),
        token: owner.token.clone(),
    };
    let other_user = AuthenticatedUser {
        id: other.id.clone(),
        token: other.token.clone(),
    };

    // No shares exist yet for this resource: both users should be denied.
    let err = require_access(&client, &owner_user, ResourceType::Song, resource_id)
        .await
        .expect_err("no share should mean no access yet");
    assert!(matches!(err, PermissionError::NotFound));

    // Creating a resource grants the creator admin access.
    grant_creator_admin_share(&client, &owner_user, ResourceType::Song, resource_id)
        .await
        .expect("grant_creator_admin_share should succeed");

    let owner_access = require_access(&client, &owner_user, ResourceType::Song, resource_id)
        .await
        .expect("owner should now have access");
    assert_eq!(owner_access.access_level, AccessLevel::Admin);
    require_edit_access(&client, &owner_user, ResourceType::Song, resource_id)
        .await
        .expect("admin access should satisfy edit access");

    // The other user still has no access.
    let other_err = require_access(&client, &other_user, ResourceType::Song, resource_id)
        .await
        .expect_err("other user should not have access without a share");
    assert!(matches!(other_err, PermissionError::NotFound));

    // Grant the other user viewer access directly and confirm the
    // permissions module reflects it, including the viewer/editor boundary.
    client
        .create_share(
            &owner.token,
            &CreateShare {
                user_id: other.id.clone(),
                resource_type: ResourceType::Song.as_str().to_string(),
                resource_id: resource_id.to_string(),
                access_level: AccessLevel::Viewer,
                created_by: owner.id.clone(),
            },
        )
        .await
        .expect("create_share should succeed");

    let other_access = require_access(&client, &other_user, ResourceType::Song, resource_id)
        .await
        .expect("other user should now have viewer access");
    assert_eq!(other_access.access_level, AccessLevel::Viewer);

    let edit_err = require_edit_access(&client, &other_user, ResourceType::Song, resource_id)
        .await
        .expect_err("viewer access should not satisfy edit access");
    assert!(matches!(edit_err, PermissionError::NotFound));
}
