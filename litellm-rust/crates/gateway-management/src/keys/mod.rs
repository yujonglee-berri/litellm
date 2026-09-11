mod duration;
mod store;
mod token;
mod types;

pub use store::{AliasFilter, ExpiresFilter, KeyStore, ListFilter, MemoryStore};
pub use types::{
    AliasListResponse, BlockKeyRequest, DeleteKeyRequest, DeleteKeyResponse, GenerateKeyRequest,
    GenerateKeyResponse, KeyHealthResponse, KeyInfo, KeyInfoBatchResponse, KeyInfoResponse,
    KeyListResponse, KeyRecord, ListAliasesQuery, ListKeysQuery, RegenerateKeyRequest,
    ResetSpendRequest, UpdateKeyRequest,
};

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use litellm_gateway_auth::hash_token;
use serde_json::json;

use self::duration::duration_in_seconds;
use self::token::{
    abbreviate_api_key, generate_plaintext_key, hash_if_needed, is_key_identifier,
    validate_custom_key,
};
use crate::error::Error;

#[derive(Clone)]
pub struct KeyManager {
    store: Arc<dyn KeyStore>,
}

impl KeyManager {
    pub fn new(store: impl KeyStore + 'static) -> Self {
        Self {
            store: Arc::new(store),
        }
    }

    pub fn memory() -> Self {
        Self::new(MemoryStore::default())
    }

    pub fn generate(
        &self,
        request: GenerateKeyRequest,
        now: DateTime<Utc>,
    ) -> Result<GenerateKeyResponse, Error> {
        validate_budget(request.max_budget)?;
        validate_spend(Some(request.spend))?;
        let plaintext = plaintext_from_request(request.key.as_deref())?;
        let record = self
            .store
            .insert(record_from_generate(&request, &plaintext, now)?)?;
        Ok(GenerateKeyResponse {
            key: plaintext.clone(),
            token: plaintext,
            info: record.public_info(),
        })
    }

    pub fn generate_service_account(
        &self,
        request: GenerateKeyRequest,
        now: DateTime<Utc>,
    ) -> Result<GenerateKeyResponse, Error> {
        self.generate(
            GenerateKeyRequest {
                user_id: None,
                ..request
            },
            now,
        )
    }

    pub fn info(&self, key: Option<&str>) -> Result<KeyInfoResponse, Error> {
        let requested = key
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::bad_request("key is required", Some("key")))?;
        let record = self.lookup(requested)?;
        Ok(KeyInfoResponse {
            key: requested.to_string(),
            info: record.public_info(),
        })
    }

    pub fn info_many(&self, request: DeleteKeyRequest) -> Result<KeyInfoBatchResponse, Error> {
        let mut info = Vec::new();
        if let Some(keys) = request.keys.as_ref() {
            info.extend(
                keys.iter()
                    .filter_map(|key| self.lookup(key).ok().map(|record| record.public_info())),
            );
        }
        if let Some(aliases) = request.key_aliases.as_ref() {
            info.extend(aliases.iter().filter_map(|alias| {
                self.store
                    .get_by_alias(alias)
                    .ok()
                    .flatten()
                    .map(|record| record.public_info())
            }));
        }
        Ok(KeyInfoBatchResponse {
            key: request.keys,
            info,
        })
    }

    pub fn list(&self, query: ListKeysQuery, now: DateTime<Utc>) -> Result<KeyListResponse, Error> {
        let expires = match query.expires.as_deref() {
            None => None,
            Some("active") => Some(ExpiresFilter::Active),
            Some("expired") => Some(ExpiresFilter::Expired),
            Some(other) => {
                return Err(Error::bad_request(
                    format!("Invalid expires value {other:?}. Supported: 'active', 'expired'."),
                    Some("expires"),
                ));
            }
        };
        let (records, total_count) = self.store.list(&ListFilter {
            page: query.page,
            size: query.size,
            user_id: query.user_id,
            team_id: query.team_id,
            organization_id: query.organization_id,
            key_hash: query.key_hash,
            key_alias: query.key_alias,
            project_id: query.project_id,
            agent_id: query.agent_id,
            expires,
            now,
        })?;
        let keys = if query.return_full_object {
            records
                .iter()
                .map(|record| serde_json::to_value(record.public_info()).unwrap_or(json!({})))
                .collect()
        } else {
            records.iter().map(|record| json!(record.token)).collect()
        };
        Ok(KeyListResponse {
            keys,
            total_count,
            current_page: query.page.max(1),
            total_pages: total_pages(total_count, query.size),
        })
    }

    pub fn aliases(&self, query: ListAliasesQuery) -> Result<AliasListResponse, Error> {
        let (aliases, total_count) = self.store.aliases(&AliasFilter {
            page: query.page,
            size: query.size,
            search: query.search,
            team_id: query.team_id,
        })?;
        Ok(AliasListResponse {
            aliases,
            total_count,
            current_page: query.page.max(1),
            total_pages: total_pages(total_count, query.size),
            size: query.size.clamp(1, 100),
        })
    }

    pub fn update(&self, request: UpdateKeyRequest, now: DateTime<Utc>) -> Result<KeyInfo, Error> {
        validate_budget(request.max_budget)?;
        validate_spend(request.spend)?;
        let current =
            self.resolve_existing(request.key.as_deref(), request.key_alias.as_deref())?;
        let expires = match request.duration.as_deref() {
            None => current.expires,
            Some(duration) => Some(expires_at(duration, now)?),
        };
        let next = KeyRecord {
            key_alias: request.key_alias.clone().or(current.key_alias.clone()),
            spend: request.spend.unwrap_or(current.spend),
            expires,
            models: request
                .models
                .clone()
                .unwrap_or_else(|| current.models.clone()),
            user_id: request.user_id.clone().or(current.user_id.clone()),
            team_id: request.team_id.clone().or(current.team_id.clone()),
            agent_id: request.agent_id.clone().or(current.agent_id.clone()),
            organization_id: request
                .organization_id
                .clone()
                .or(current.organization_id.clone()),
            project_id: request.project_id.clone().or(current.project_id.clone()),
            budget_id: request.budget_id.clone().or(current.budget_id.clone()),
            max_budget: request.max_budget.or(current.max_budget),
            budget_duration: request
                .budget_duration
                .clone()
                .or(current.budget_duration.clone()),
            max_parallel_requests: request
                .max_parallel_requests
                .or(current.max_parallel_requests),
            metadata: request
                .metadata
                .clone()
                .unwrap_or_else(|| current.metadata.clone()),
            tpm_limit: request.tpm_limit.or(current.tpm_limit),
            rpm_limit: request.rpm_limit.or(current.rpm_limit),
            blocked: request.blocked.or(current.blocked),
            key_type: request.key_type.clone().or(current.key_type.clone()),
            allowed_routes: request
                .allowed_routes
                .clone()
                .unwrap_or_else(|| current.allowed_routes.clone()),
            tags: request.tags.clone().or(current.tags.clone()),
            updated_at: now,
            ..current.clone()
        };
        Ok(self.store.replace(&current, next)?.public_info())
    }

    pub fn delete(&self, request: DeleteKeyRequest) -> Result<DeleteKeyResponse, Error> {
        let mut deleted_keys = Vec::new();
        if let Some(keys) = request.keys.as_ref() {
            if keys.is_empty() && request.key_aliases.as_ref().is_none_or(Vec::is_empty) {
                return Err(Error::bad_request(
                    "At least one of 'keys' or 'key_aliases' must be provided.",
                    Some("keys"),
                ));
            }
            for key in keys {
                let record = self.lookup(key)?;
                self.store.remove(&record.token)?;
                deleted_keys.push(key.clone());
            }
        }
        if let Some(aliases) = request.key_aliases.as_ref() {
            if aliases.is_empty() && deleted_keys.is_empty() {
                return Err(Error::bad_request(
                    "At least one of 'keys' or 'key_aliases' must be provided.",
                    Some("key_aliases"),
                ));
            }
            for alias in aliases {
                let record = self.store.get_by_alias(alias)?.ok_or_else(|| {
                    Error::not_found(format!("Key alias '{alias}' not found."), Some("key_alias"))
                })?;
                self.store.remove(&record.token)?;
                deleted_keys.push(alias.clone());
            }
        }
        if deleted_keys.is_empty() {
            return Err(Error::bad_request(
                "At least one of 'keys' or 'key_aliases' must be provided.",
                Some("keys"),
            ));
        }
        Ok(DeleteKeyResponse { deleted_keys })
    }

    pub fn regenerate(
        &self,
        path_key: Option<&str>,
        request: RegenerateKeyRequest,
        now: DateTime<Utc>,
    ) -> Result<GenerateKeyResponse, Error> {
        let current = self.resolve_existing(
            path_key.or(request.key.as_deref()),
            request.key_alias.as_deref(),
        )?;
        let plaintext = plaintext_from_request(request.new_key.as_deref())?;
        let expires = match request.duration.as_deref() {
            None => current.expires,
            Some(duration) => Some(expires_at(duration, now)?),
        };
        let next = KeyRecord {
            token: hash_token(&plaintext),
            key_name: Some(abbreviate_api_key(&plaintext)),
            key_alias: request.key_alias.clone().or(current.key_alias.clone()),
            spend: request.spend.unwrap_or(current.spend),
            expires,
            models: request
                .models
                .clone()
                .unwrap_or_else(|| current.models.clone()),
            user_id: request.user_id.clone().or(current.user_id.clone()),
            team_id: request.team_id.clone().or(current.team_id.clone()),
            metadata: request
                .metadata
                .clone()
                .unwrap_or_else(|| current.metadata.clone()),
            max_budget: request.max_budget.or(current.max_budget),
            rotation_count: current.rotation_count.saturating_add(1),
            updated_at: now,
            ..current.clone()
        };
        let record = self.store.replace(&current, next)?;
        Ok(GenerateKeyResponse {
            key: plaintext.clone(),
            token: plaintext,
            info: record.public_info(),
        })
    }

    pub fn reset_spend(
        &self,
        key: &str,
        request: ResetSpendRequest,
        now: DateTime<Utc>,
    ) -> Result<KeyInfo, Error> {
        if !request.reset_to.is_finite() || request.reset_to < 0.0 {
            return Err(Error::bad_request(
                "reset_to must be a non-negative finite number",
                Some("reset_to"),
            ));
        }
        let current = self.lookup(key)?;
        let next = KeyRecord {
            spend: request.reset_to,
            updated_at: now,
            ..current.clone()
        };
        Ok(self.store.replace(&current, next)?.public_info())
    }

    pub fn set_blocked(
        &self,
        request: BlockKeyRequest,
        blocked: bool,
        now: DateTime<Utc>,
    ) -> Result<KeyInfo, Error> {
        if !is_key_identifier(&request.key) {
            return Err(Error::bad_request("Invalid key format.", Some("key")));
        }
        let current = self.lookup(&request.key)?;
        let next = KeyRecord {
            blocked: Some(blocked),
            updated_at: now,
            ..current.clone()
        };
        Ok(self.store.replace(&current, next)?.public_info())
    }

    pub fn health(&self) -> KeyHealthResponse {
        KeyHealthResponse { key: "healthy" }
    }

    fn resolve_existing(&self, key: Option<&str>, alias: Option<&str>) -> Result<KeyRecord, Error> {
        match (key, alias) {
            (Some(key), _) => self.lookup(key),
            (None, Some(alias)) => self.store.get_by_alias(alias)?.ok_or_else(|| {
                Error::not_found(format!("Key alias '{alias}' not found."), Some("key_alias"))
            }),
            (None, None) => Err(Error::bad_request(
                "either key or key_alias must be provided",
                Some("key"),
            )),
        }
    }

    fn lookup(&self, key: &str) -> Result<KeyRecord, Error> {
        self.store
            .get(&hash_if_needed(key))?
            .ok_or_else(|| Error::not_found("Key not found.", Some("key")))
    }

    #[cfg(test)]
    fn stores_plaintext(&self, key: &str) -> bool {
        self.store.get(key).ok().flatten().is_some()
    }
}

fn record_from_generate(
    request: &GenerateKeyRequest,
    plaintext: &str,
    now: DateTime<Utc>,
) -> Result<KeyRecord, Error> {
    let expires = match request.duration.as_deref() {
        None => None,
        Some(duration) => Some(expires_at(duration, now)?),
    };
    let metadata = if request.metadata.is_null() {
        json!({})
    } else {
        request.metadata.clone()
    };
    Ok(KeyRecord {
        token: hash_token(plaintext),
        key_name: Some(abbreviate_api_key(plaintext)),
        key_alias: empty_to_none(request.key_alias.as_deref()),
        spend: request.spend,
        expires,
        models: request.models.clone(),
        user_id: empty_to_none(request.user_id.as_deref()),
        team_id: empty_to_none(request.team_id.as_deref()),
        agent_id: empty_to_none(request.agent_id.as_deref()),
        organization_id: empty_to_none(request.organization_id.as_deref()),
        project_id: empty_to_none(request.project_id.as_deref()),
        budget_id: empty_to_none(request.budget_id.as_deref()),
        max_budget: request.max_budget,
        budget_duration: request.budget_duration.clone(),
        max_parallel_requests: request.max_parallel_requests,
        metadata,
        tpm_limit: request.tpm_limit,
        rpm_limit: request.rpm_limit,
        blocked: request.blocked,
        key_type: request
            .key_type
            .clone()
            .or_else(|| Some("default".to_string())),
        allowed_routes: request.allowed_routes.clone(),
        tags: request.tags.clone(),
        rotation_count: 0,
        created_at: now,
        updated_at: now,
    })
}

fn plaintext_from_request(key: Option<&str>) -> Result<String, Error> {
    match key {
        None => Ok(generate_plaintext_key()),
        Some(custom) => {
            validate_custom_key(custom)?;
            Ok(custom.to_string())
        }
    }
}

fn expires_at(duration: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>, Error> {
    let seconds = duration_in_seconds(duration)
        .map_err(|message| Error::bad_request(message, Some("duration")))?;
    Ok(now + Duration::seconds(seconds))
}

fn validate_budget(max_budget: Option<f64>) -> Result<(), Error> {
    match max_budget {
        Some(value) if !value.is_finite() || value < 0.0 => Err(Error::bad_request(
            format!("max_budget must be a non-negative finite number. Received: {value}"),
            Some("max_budget"),
        )),
        _ => Ok(()),
    }
}

fn validate_spend(spend: Option<f64>) -> Result<(), Error> {
    match spend {
        Some(value) if !value.is_finite() || value < 0.0 => Err(Error::bad_request(
            format!("spend must be a non-negative finite number. Received: {value}"),
            Some("spend"),
        )),
        _ => Ok(()),
    }
}

fn empty_to_none(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn total_pages(total: usize, size: u32) -> u32 {
    let size = size.clamp(1, 100) as usize;
    let pages = total.div_ceil(size);
    u32::try_from(pages).unwrap_or(u32::MAX).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use litellm_gateway_auth::hash_token;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 11, 0, 0, 0).unwrap()
    }

    fn manager() -> KeyManager {
        KeyManager::memory()
    }

    #[test]
    fn generate_stores_hash_and_returns_plaintext_once() {
        let keys = manager();
        let response = keys
            .generate(
                GenerateKeyRequest {
                    key_alias: Some("prod".into()),
                    models: vec!["gpt-4".into()],
                    max_budget: Some(10.0),
                    duration: Some("1h".into()),
                    ..GenerateKeyRequest::default()
                },
                now(),
            )
            .expect("generate");

        assert!(response.key.starts_with("sk-"));
        assert_eq!(response.token, response.key);
        assert_eq!(response.info.token_id, hash_token(&response.key));
        assert_eq!(response.info.key_alias.as_deref(), Some("prod"));
        assert_eq!(response.info.models, vec!["gpt-4".to_string()]);
        assert_eq!(response.info.max_budget, Some(10.0));
        assert_eq!(response.info.expires, Some(now() + Duration::hours(1)));
        assert!(!keys.stores_plaintext(&response.key));
        assert_eq!(
            keys.info(Some(&response.key)).expect("info").info.token_id,
            response.info.token_id
        );
    }

    #[test]
    fn custom_key_and_duplicate_alias_are_rejected() {
        let keys = manager();
        assert!(
            keys.generate(
                GenerateKeyRequest {
                    key: Some("not-secret".into()),
                    ..GenerateKeyRequest::default()
                },
                now(),
            )
            .is_err()
        );
        keys.generate(
            GenerateKeyRequest {
                key_alias: Some("shared".into()),
                ..GenerateKeyRequest::default()
            },
            now(),
        )
        .expect("first alias");
        let error = keys
            .generate(
                GenerateKeyRequest {
                    key_alias: Some("shared".into()),
                    ..GenerateKeyRequest::default()
                },
                now(),
            )
            .expect_err("duplicate alias");
        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn update_delete_block_and_regenerate_change_the_stored_key() {
        let keys = manager();
        let created = keys
            .generate(
                GenerateKeyRequest {
                    key_alias: Some("ops".into()),
                    spend: 4.2,
                    ..GenerateKeyRequest::default()
                },
                now(),
            )
            .expect("generate");

        let updated = keys
            .update(
                UpdateKeyRequest {
                    key_alias: Some("ops".into()),
                    max_budget: Some(25.0),
                    models: Some(vec!["gpt-4o".into()]),
                    ..UpdateKeyRequest::default()
                },
                now(),
            )
            .expect("update");
        assert_eq!(updated.max_budget, Some(25.0));
        assert_eq!(updated.models, vec!["gpt-4o".to_string()]);

        let blocked = keys
            .set_blocked(
                BlockKeyRequest {
                    key: created.key.clone(),
                },
                true,
                now(),
            )
            .expect("block");
        assert_eq!(blocked.blocked, Some(true));

        let rotated = keys
            .regenerate(
                None,
                RegenerateKeyRequest {
                    key: Some(created.key.clone()),
                    ..RegenerateKeyRequest::default()
                },
                now(),
            )
            .expect("regenerate");
        assert_ne!(rotated.key, created.key);
        assert_eq!(rotated.info.rotation_count, 1);
        assert_eq!(rotated.info.key_alias.as_deref(), Some("ops"));
        assert!(keys.info(Some(&created.key)).is_err());
        assert!(keys.info(Some(&rotated.key)).is_ok());

        let reset = keys
            .reset_spend(&rotated.key, ResetSpendRequest { reset_to: 0.0 }, now())
            .expect("reset");
        assert_eq!(reset.spend, 0.0);

        keys.delete(DeleteKeyRequest {
            key_aliases: Some(vec!["ops".into()]),
            ..DeleteKeyRequest::default()
        })
        .expect("delete");
        assert!(keys.info(Some(&rotated.key)).is_err());
    }

    #[test]
    fn list_filters_and_never_returns_plaintext() {
        let keys = manager();
        let first = keys
            .generate(
                GenerateKeyRequest {
                    user_id: Some("alice".into()),
                    key_alias: Some("alice-prod".into()),
                    duration: Some("1s".into()),
                    ..GenerateKeyRequest::default()
                },
                now(),
            )
            .expect("first");
        keys.generate(
            GenerateKeyRequest {
                user_id: Some("bob".into()),
                key_alias: Some("bob-prod".into()),
                ..GenerateKeyRequest::default()
            },
            now(),
        )
        .expect("second");

        let hashes = keys
            .list(
                ListKeysQuery {
                    user_id: Some("alice".into()),
                    page: 1,
                    size: 10,
                    ..ListKeysQuery::default()
                },
                now(),
            )
            .expect("list");
        assert_eq!(hashes.total_count, 1);
        assert_eq!(hashes.keys, vec![json!(hash_token(&first.key))]);
        assert!(
            !hashes
                .keys
                .iter()
                .any(|value| value.as_str() == Some(first.key.as_str()))
        );

        let expired = keys
            .list(
                ListKeysQuery {
                    expires: Some("expired".into()),
                    page: 1,
                    size: 10,
                    ..ListKeysQuery::default()
                },
                now() + Duration::seconds(2),
            )
            .expect("expired");
        assert_eq!(expired.total_count, 1);

        let listed_aliases = keys
            .aliases(ListAliasesQuery {
                search: Some("alice".into()),
                page: 1,
                size: 50,
                ..ListAliasesQuery::default()
            })
            .expect("aliases");
        assert_eq!(listed_aliases.aliases, vec!["alice-prod".to_string()]);
    }
}
