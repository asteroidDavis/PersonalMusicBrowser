use crate::acl::{
    CreateGroup, CreateGroupMember, CreateGroupShare, CreateShare, GroupMember, GroupShare, Share,
};
use crate::auth::AuthConfig;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use thiserror::Error;

const MAX_ERROR_BODY_LENGTH: usize = 200;

#[derive(Debug, Error)]
pub enum PocketBaseClientError {
    #[error("PocketBase request failed")]
    Request(#[from] reqwest::Error),
    #[error("PocketBase returned status {status}: {body}")]
    Status { status: StatusCode, body: String },
}

impl PocketBaseClientError {
    pub fn is_missing_collection(&self) -> bool {
        match self {
            PocketBaseClientError::Status { status, body } => {
                *status == StatusCode::NOT_FOUND
                    && (body.contains("Missing collection context")
                        || body.contains("Missing or invalid collection context"))
            }
            PocketBaseClientError::Request(_) => false,
        }
    }
}

#[derive(Clone)]
pub struct PocketBaseClient {
    base_url: String,
    http_client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct PocketBaseList<T> {
    items: Vec<T>,
}

impl PocketBaseClient {
    pub fn new(base_url: String, http_client: reqwest::Client) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client,
        }
    }

    pub fn from_auth_config(config: &AuthConfig) -> Result<Self, reqwest::Error> {
        Ok(Self::new(
            config.pocketbase_url.clone(),
            config.build_client()?,
        ))
    }

    pub async fn create_share(
        &self,
        token: &str,
        share: &CreateShare,
    ) -> Result<Share, PocketBaseClientError> {
        self.post_record(token, "shares", share).await
    }

    pub async fn list_user_shares(
        &self,
        token: &str,
        user_id: &str,
    ) -> Result<Vec<Share>, PocketBaseClientError> {
        let filter = format!("user_id = '{}'", escape_filter_value(user_id));
        self.list_records(token, "shares", &filter).await
    }

    pub async fn list_resource_shares(
        &self,
        token: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<Vec<Share>, PocketBaseClientError> {
        let filter = format!(
            "resource_type = '{}' && resource_id = '{}'",
            escape_filter_value(resource_type),
            escape_filter_value(resource_id)
        );
        self.list_records(token, "shares", &filter).await
    }

    pub async fn delete_share(
        &self,
        token: &str,
        share_id: &str,
    ) -> Result<(), PocketBaseClientError> {
        let url = self.collection_record_url("shares", share_id);
        let response = self
            .http_client
            .delete(url)
            .bearer_auth(token)
            .send()
            .await?;
        self.ensure_success(response).await.map(|_| ())
    }

    pub async fn create_group(
        &self,
        token: &str,
        group: &CreateGroup,
    ) -> Result<serde_json::Value, PocketBaseClientError> {
        self.post_record(token, "groups", group).await
    }

    pub async fn add_user_to_group(
        &self,
        token: &str,
        member: &CreateGroupMember,
    ) -> Result<GroupMember, PocketBaseClientError> {
        self.post_record(token, "group_memberships", member).await
    }

    pub async fn remove_group_member(
        &self,
        token: &str,
        membership_id: &str,
    ) -> Result<(), PocketBaseClientError> {
        let url = self.collection_record_url("group_memberships", membership_id);
        let response = self
            .http_client
            .delete(url)
            .bearer_auth(token)
            .send()
            .await?;
        self.ensure_success(response).await.map(|_| ())
    }

    pub async fn get_group_members(
        &self,
        token: &str,
        group_id: &str,
    ) -> Result<Vec<GroupMember>, PocketBaseClientError> {
        let filter = format!("group_id = '{}'", escape_filter_value(group_id));
        self.list_records(token, "group_memberships", &filter).await
    }

    /// All group memberships for a user (the PocketBase list rule already
    /// restricts results to `user_id = auth`, so this returns exactly the
    /// caller's own memberships).
    pub async fn list_user_group_memberships(
        &self,
        token: &str,
        user_id: &str,
    ) -> Result<Vec<GroupMember>, PocketBaseClientError> {
        let filter = format!("user_id = '{}'", escape_filter_value(user_id));
        self.list_records(token, "group_memberships", &filter).await
    }

    /// All resources shared with a group.
    pub async fn list_group_shares(
        &self,
        token: &str,
        group_id: &str,
    ) -> Result<Vec<GroupShare>, PocketBaseClientError> {
        let filter = format!("group_id = '{}'", escape_filter_value(group_id));
        self.list_records(token, "group_shares", &filter).await
    }

    pub async fn share_with_group(
        &self,
        token: &str,
        group_share: &CreateGroupShare,
    ) -> Result<serde_json::Value, PocketBaseClientError> {
        self.post_record(token, "group_shares", group_share).await
    }

    async fn post_record<T, R>(
        &self,
        token: &str,
        collection: &str,
        data: &T,
    ) -> Result<R, PocketBaseClientError>
    where
        T: serde::Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let response = self
            .http_client
            .post(self.collection_records_url(collection))
            .bearer_auth(token)
            .json(data)
            .send()
            .await?;
        let response = self.ensure_success(response).await?;
        Ok(response.json().await?)
    }

    async fn list_records<T>(
        &self,
        token: &str,
        collection: &str,
        filter: &str,
    ) -> Result<Vec<T>, PocketBaseClientError>
    where
        T: DeserializeOwned,
    {
        let mut url = url::Url::parse(&self.collection_records_url(collection))
            .expect("PocketBase collection records URL should be valid");
        url.query_pairs_mut().append_pair("filter", filter);
        let response = self.http_client.get(url).bearer_auth(token).send().await?;
        let response = self.ensure_success(response).await?;
        let list = response.json::<PocketBaseList<T>>().await?;
        Ok(list.items)
    }

    async fn ensure_success(
        &self,
        response: reqwest::Response,
    ) -> Result<reqwest::Response, PocketBaseClientError> {
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let sanitized_body = body
            .chars()
            .take(MAX_ERROR_BODY_LENGTH)
            .collect::<String>()
            .replace('\n', " ");
        Err(PocketBaseClientError::Status {
            status,
            body: sanitized_body,
        })
    }

    fn collection_records_url(&self, collection: &str) -> String {
        format!("{}/api/collections/{}/records", self.base_url, collection)
    }

    fn collection_record_url(&self, collection: &str, id: &str) -> String {
        format!(
            "{}/api/collections/{}/records/{}",
            self.base_url, collection, id
        )
    }
}

fn escape_filter_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::escape_filter_value;

    #[test]
    fn escapes_pocketbase_filter_values() {
        assert_eq!(escape_filter_value("abc"), "abc");
        assert_eq!(escape_filter_value("a'b"), "a\\'b");
        assert_eq!(escape_filter_value("a\\b"), "a\\\\b");
    }
}
