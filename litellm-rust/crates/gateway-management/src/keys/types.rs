use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
pub struct KeyRecord {
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_alias: Option<String>,
    pub spend: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<DateTime<Utc>>,
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_budget: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_parallel_requests: Option<i32>,
    pub metadata: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_type: Option<String>,
    pub allowed_routes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    pub rotation_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl KeyRecord {
    pub fn public_info(&self) -> KeyInfo {
        KeyInfo {
            token_id: self.token.clone(),
            key_name: self.key_name.clone(),
            key_alias: self.key_alias.clone(),
            spend: self.spend,
            expires: self.expires,
            models: self.models.clone(),
            user_id: self.user_id.clone(),
            team_id: self.team_id.clone(),
            agent_id: self.agent_id.clone(),
            organization_id: self.organization_id.clone(),
            project_id: self.project_id.clone(),
            budget_id: self.budget_id.clone(),
            max_budget: self.max_budget,
            budget_duration: self.budget_duration.clone(),
            max_parallel_requests: self.max_parallel_requests,
            metadata: self.metadata.clone(),
            tpm_limit: self.tpm_limit,
            rpm_limit: self.rpm_limit,
            blocked: self.blocked,
            key_type: self.key_type.clone(),
            allowed_routes: self.allowed_routes.clone(),
            tags: self.tags.clone(),
            rotation_count: self.rotation_count,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyInfo {
    pub token_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_alias: Option<String>,
    pub spend: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<DateTime<Utc>>,
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_budget: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_duration: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_parallel_requests: Option<i32>,
    pub metadata: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm_limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_type: Option<String>,
    pub allowed_routes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    pub rotation_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GenerateKeyRequest {
    pub key: Option<String>,
    pub key_alias: Option<String>,
    pub duration: Option<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub spend: f64,
    pub max_budget: Option<f64>,
    pub user_id: Option<String>,
    pub team_id: Option<String>,
    pub agent_id: Option<String>,
    pub organization_id: Option<String>,
    pub project_id: Option<String>,
    pub budget_id: Option<String>,
    pub max_parallel_requests: Option<i32>,
    #[serde(default)]
    pub metadata: Value,
    pub tpm_limit: Option<i64>,
    pub rpm_limit: Option<i64>,
    pub budget_duration: Option<String>,
    pub blocked: Option<bool>,
    pub key_type: Option<String>,
    #[serde(default)]
    pub allowed_routes: Vec<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GenerateKeyResponse {
    pub key: String,
    pub token: String,
    #[serde(flatten)]
    pub info: KeyInfo,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct UpdateKeyRequest {
    pub key: Option<String>,
    pub key_alias: Option<String>,
    pub duration: Option<String>,
    pub models: Option<Vec<String>>,
    pub spend: Option<f64>,
    pub max_budget: Option<f64>,
    pub user_id: Option<String>,
    pub team_id: Option<String>,
    pub agent_id: Option<String>,
    pub organization_id: Option<String>,
    pub project_id: Option<String>,
    pub budget_id: Option<String>,
    pub max_parallel_requests: Option<i32>,
    pub metadata: Option<Value>,
    pub tpm_limit: Option<i64>,
    pub rpm_limit: Option<i64>,
    pub budget_duration: Option<String>,
    pub blocked: Option<bool>,
    pub key_type: Option<String>,
    pub allowed_routes: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct DeleteKeyRequest {
    pub keys: Option<Vec<String>>,
    pub key_aliases: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeleteKeyResponse {
    pub deleted_keys: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RegenerateKeyRequest {
    pub key: Option<String>,
    pub new_key: Option<String>,
    pub duration: Option<String>,
    pub spend: Option<f64>,
    pub metadata: Option<Value>,
    pub key_alias: Option<String>,
    pub models: Option<Vec<String>>,
    pub max_budget: Option<f64>,
    pub user_id: Option<String>,
    pub team_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ResetSpendRequest {
    pub reset_to: f64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BlockKeyRequest {
    pub key: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListKeysQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_list_size")]
    pub size: u32,
    pub user_id: Option<String>,
    pub team_id: Option<String>,
    pub organization_id: Option<String>,
    pub key_hash: Option<String>,
    pub key_alias: Option<String>,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    #[serde(default)]
    pub return_full_object: bool,
    pub expires: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListAliasesQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_alias_size")]
    pub size: u32,
    pub search: Option<String>,
    pub team_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyListResponse {
    pub keys: Vec<Value>,
    pub total_count: usize,
    pub current_page: u32,
    pub total_pages: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct AliasListResponse {
    pub aliases: Vec<String>,
    pub total_count: usize,
    pub current_page: u32,
    pub total_pages: u32,
    pub size: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyInfoResponse {
    pub key: String,
    pub info: KeyInfo,
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyInfoBatchResponse {
    pub key: Option<Vec<String>>,
    pub info: Vec<KeyInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KeyHealthResponse {
    pub key: &'static str,
}

impl Default for ListKeysQuery {
    fn default() -> Self {
        Self {
            page: default_page(),
            size: default_list_size(),
            user_id: None,
            team_id: None,
            organization_id: None,
            key_hash: None,
            key_alias: None,
            project_id: None,
            agent_id: None,
            return_full_object: false,
            expires: None,
        }
    }
}

impl Default for ListAliasesQuery {
    fn default() -> Self {
        Self {
            page: default_page(),
            size: default_alias_size(),
            search: None,
            team_id: None,
        }
    }
}

fn default_page() -> u32 {
    1
}

fn default_list_size() -> u32 {
    10
}

fn default_alias_size() -> u32 {
    50
}
