use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};

use super::types::KeyRecord;
use crate::error::Error;

pub trait KeyStore: Send + Sync {
    fn insert(&self, record: KeyRecord) -> Result<KeyRecord, Error>;
    fn get(&self, token: &str) -> Result<Option<KeyRecord>, Error>;
    fn get_by_alias(&self, alias: &str) -> Result<Option<KeyRecord>, Error>;
    fn replace(&self, previous: &KeyRecord, next: KeyRecord) -> Result<KeyRecord, Error>;
    fn remove(&self, token: &str) -> Result<Option<KeyRecord>, Error>;
    fn list(&self, filter: &ListFilter) -> Result<(Vec<KeyRecord>, usize), Error>;
    fn aliases(&self, filter: &AliasFilter) -> Result<(Vec<String>, usize), Error>;
}

#[derive(Clone, Default)]
pub struct MemoryStore {
    inner: Arc<RwLock<StoreInner>>,
}

#[derive(Default)]
struct StoreInner {
    by_token: HashMap<String, KeyRecord>,
    by_alias: HashMap<String, String>,
}

impl KeyStore for MemoryStore {
    fn insert(&self, record: KeyRecord) -> Result<KeyRecord, Error> {
        let mut inner = self.write();
        if inner.by_token.contains_key(&record.token) {
            return Err(Error::bad_request("Key already exists.", Some("key")));
        }
        if let Some(alias) = record.key_alias.as_deref() {
            enforce_unique_alias(&inner, alias, None)?;
            inner
                .by_alias
                .insert(alias.to_string(), record.token.clone());
        }
        inner.by_token.insert(record.token.clone(), record.clone());
        Ok(record)
    }

    fn get(&self, token: &str) -> Result<Option<KeyRecord>, Error> {
        Ok(self.read().by_token.get(token).cloned())
    }

    fn get_by_alias(&self, alias: &str) -> Result<Option<KeyRecord>, Error> {
        let inner = self.read();
        Ok(inner
            .by_alias
            .get(alias)
            .and_then(|token| inner.by_token.get(token).cloned()))
    }

    fn replace(&self, previous: &KeyRecord, next: KeyRecord) -> Result<KeyRecord, Error> {
        let mut inner = self.write();
        if !inner.by_token.contains_key(&previous.token) {
            return Err(Error::not_found("Key not found.", Some("key")));
        }
        if let Some(alias) = next.key_alias.as_deref() {
            enforce_unique_alias(&inner, alias, Some(&previous.token))?;
        }
        if previous.token != next.token {
            inner.by_token.remove(&previous.token);
        }
        if let Some(old_alias) = previous.key_alias.as_deref() {
            inner.by_alias.remove(old_alias);
        }
        if let Some(alias) = next.key_alias.as_deref() {
            inner.by_alias.insert(alias.to_string(), next.token.clone());
        }
        inner.by_token.insert(next.token.clone(), next.clone());
        Ok(next)
    }

    fn remove(&self, token: &str) -> Result<Option<KeyRecord>, Error> {
        let mut inner = self.write();
        let record = inner.by_token.remove(token);
        if let Some(alias) = record
            .as_ref()
            .and_then(|record| record.key_alias.as_deref())
        {
            inner.by_alias.remove(alias);
        }
        Ok(record)
    }

    fn list(&self, filter: &ListFilter) -> Result<(Vec<KeyRecord>, usize), Error> {
        let inner = self.read();
        let mut matched: Vec<KeyRecord> = inner
            .by_token
            .values()
            .filter(|record| filter.matches(record))
            .cloned()
            .collect();
        matched.sort_by_key(|record| std::cmp::Reverse(record.created_at));
        let total = matched.len();
        let page = filter.page.max(1);
        let size = filter.size.clamp(1, 100);
        let start = usize::try_from((page - 1).saturating_mul(size)).unwrap_or(usize::MAX);
        let keys = matched
            .into_iter()
            .skip(start)
            .take(size as usize)
            .collect();
        Ok((keys, total))
    }

    fn aliases(&self, filter: &AliasFilter) -> Result<(Vec<String>, usize), Error> {
        let inner = self.read();
        let mut aliases: Vec<String> = inner
            .by_token
            .values()
            .filter(|record| filter.matches(record))
            .filter_map(|record| record.key_alias.clone())
            .collect();
        aliases.sort();
        let total = aliases.len();
        let page = filter.page.max(1);
        let size = filter.size.clamp(1, 100);
        let start = usize::try_from((page - 1).saturating_mul(size)).unwrap_or(usize::MAX);
        let page_aliases = aliases
            .into_iter()
            .skip(start)
            .take(size as usize)
            .collect();
        Ok((page_aliases, total))
    }
}

impl MemoryStore {
    fn read(&self) -> std::sync::RwLockReadGuard<'_, StoreInner> {
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, StoreInner> {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn enforce_unique_alias(
    inner: &StoreInner,
    alias: &str,
    existing_token: Option<&str>,
) -> Result<(), Error> {
    match inner.by_alias.get(alias) {
        Some(token) if existing_token != Some(token.as_str()) => Err(Error::bad_request(
            format!(
                "Key with alias '{alias}' already exists. Unique key aliases across all keys are required."
            ),
            Some("key_alias"),
        )),
        _ => Ok(()),
    }
}

pub struct ListFilter {
    pub page: u32,
    pub size: u32,
    pub user_id: Option<String>,
    pub team_id: Option<String>,
    pub organization_id: Option<String>,
    pub key_hash: Option<String>,
    pub key_alias: Option<String>,
    pub project_id: Option<String>,
    pub agent_id: Option<String>,
    pub expires: Option<ExpiresFilter>,
    pub now: DateTime<Utc>,
}

impl ListFilter {
    fn matches(&self, record: &KeyRecord) -> bool {
        self.user_id
            .as_ref()
            .is_none_or(|value| record.user_id.as_ref() == Some(value))
            && self
                .team_id
                .as_ref()
                .is_none_or(|value| record.team_id.as_ref() == Some(value))
            && self
                .organization_id
                .as_ref()
                .is_none_or(|value| record.organization_id.as_ref() == Some(value))
            && self
                .key_hash
                .as_ref()
                .is_none_or(|value| &record.token == value)
            && self
                .key_alias
                .as_ref()
                .is_none_or(|value| record.key_alias.as_ref() == Some(value))
            && self
                .project_id
                .as_ref()
                .is_none_or(|value| record.project_id.as_ref() == Some(value))
            && self
                .agent_id
                .as_ref()
                .is_none_or(|value| record.agent_id.as_ref() == Some(value))
            && match self.expires {
                None => true,
                Some(ExpiresFilter::Active) => {
                    !record.expires.is_some_and(|expires| expires <= self.now)
                }
                Some(ExpiresFilter::Expired) => {
                    record.expires.is_some_and(|expires| expires <= self.now)
                }
            }
    }
}

#[derive(Clone, Copy)]
pub enum ExpiresFilter {
    Active,
    Expired,
}

pub struct AliasFilter {
    pub page: u32,
    pub size: u32,
    pub search: Option<String>,
    pub team_id: Option<String>,
}

impl AliasFilter {
    fn matches(&self, record: &KeyRecord) -> bool {
        let Some(alias) = record.key_alias.as_deref() else {
            return false;
        };
        self.team_id
            .as_ref()
            .is_none_or(|value| record.team_id.as_ref() == Some(value))
            && self.search.as_ref().is_none_or(|search| {
                alias
                    .to_ascii_lowercase()
                    .contains(&search.to_ascii_lowercase())
            })
    }
}
