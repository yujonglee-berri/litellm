use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use litellm_gateway_management::error::Error as ManagementError;
use litellm_gateway_management::keys::{
    AliasFilter, ExpiresFilter, KeyRecord, KeyStore, ListFilter,
};
use rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde_json::Value;

use crate::error::Error;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS verification_tokens (
    token TEXT PRIMARY KEY,
    key_name TEXT,
    key_alias TEXT,
    spend REAL NOT NULL DEFAULT 0,
    expires TEXT,
    models TEXT NOT NULL DEFAULT '[]',
    user_id TEXT,
    team_id TEXT,
    agent_id TEXT,
    organization_id TEXT,
    project_id TEXT,
    budget_id TEXT,
    max_budget REAL,
    budget_duration TEXT,
    max_parallel_requests INTEGER,
    metadata TEXT NOT NULL DEFAULT '{}',
    tpm_limit INTEGER,
    rpm_limit INTEGER,
    blocked INTEGER,
    key_type TEXT,
    allowed_routes TEXT NOT NULL DEFAULT '[]',
    tags TEXT,
    rotation_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS verification_tokens_key_alias
    ON verification_tokens (key_alias) WHERE key_alias IS NOT NULL;
CREATE INDEX IF NOT EXISTS verification_tokens_user_team
    ON verification_tokens (user_id, team_id);
CREATE INDEX IF NOT EXISTS verification_tokens_team
    ON verification_tokens (team_id);
";

const INSERT_SQL: &str = "
INSERT INTO verification_tokens (
    token, key_name, key_alias, spend, expires, models, user_id, team_id, agent_id,
    organization_id, project_id, budget_id, max_budget, budget_duration, max_parallel_requests,
    metadata, tpm_limit, rpm_limit, blocked, key_type, allowed_routes, tags, rotation_count,
    created_at, updated_at
) VALUES (
    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
    ?21, ?22, ?23, ?24, ?25
)";

#[derive(Clone)]
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ManagementError> {
        let conn = Connection::open(path).map_err(Error::database)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(Error::database)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn memory() -> Result<Self, ManagementError> {
        let conn = Connection::open_in_memory().map_err(Error::database)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(Error::database)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl KeyStore for SqliteStore {
    fn insert(&self, record: KeyRecord) -> Result<KeyRecord, ManagementError> {
        let conn = self.lock();
        if get_token(&conn, &record.token)?.is_some() {
            return Err(ManagementError::bad_request(
                "Key already exists.",
                Some("key"),
            ));
        }
        if let Some(alias) = record.key_alias.as_deref() {
            enforce_unique_alias(&conn, alias, None)?;
        }
        insert_row(&conn, &record)?;
        Ok(record)
    }

    fn get(&self, token: &str) -> Result<Option<KeyRecord>, ManagementError> {
        get_token(&self.lock(), token)
    }

    fn get_by_alias(&self, alias: &str) -> Result<Option<KeyRecord>, ManagementError> {
        self.lock()
            .query_row(
                "SELECT * FROM verification_tokens WHERE key_alias = ?1",
                [alias],
                row_to_record,
            )
            .optional()
            .map_err(Error::database)
            .map_err(ManagementError::from)
    }

    fn replace(&self, previous: &KeyRecord, next: KeyRecord) -> Result<KeyRecord, ManagementError> {
        let mut conn = self.lock();
        if get_token(&conn, &previous.token)?.is_none() {
            return Err(ManagementError::not_found("Key not found.", Some("key")));
        }
        if let Some(alias) = next.key_alias.as_deref() {
            enforce_unique_alias(&conn, alias, Some(&previous.token))?;
        }
        let tx = conn.transaction().map_err(Error::database)?;
        if previous.token != next.token {
            tx.execute(
                "DELETE FROM verification_tokens WHERE token = ?1",
                [&previous.token],
            )
            .map_err(Error::database)?;
            insert_row(&tx, &next)?;
        } else {
            update_row(&tx, &next)?;
        }
        tx.commit().map_err(Error::database)?;
        Ok(next)
    }

    fn remove(&self, token: &str) -> Result<Option<KeyRecord>, ManagementError> {
        let conn = self.lock();
        let record = get_token(&conn, token)?;
        if record.is_some() {
            conn.execute("DELETE FROM verification_tokens WHERE token = ?1", [token])
                .map_err(Error::database)?;
        }
        Ok(record)
    }

    fn list(&self, filter: &ListFilter) -> Result<(Vec<KeyRecord>, usize), ManagementError> {
        let conn = self.lock();
        let expires_kind = match filter.expires {
            None => None,
            Some(ExpiresFilter::Active) => Some("active"),
            Some(ExpiresFilter::Expired) => Some("expired"),
        };
        let now = filter.now.to_rfc3339();
        let sql_where = "
            WHERE (?1 IS NULL OR user_id = ?1)
              AND (?2 IS NULL OR team_id = ?2)
              AND (?3 IS NULL OR organization_id = ?3)
              AND (?4 IS NULL OR token = ?4)
              AND (?5 IS NULL OR key_alias = ?5)
              AND (?6 IS NULL OR project_id = ?6)
              AND (?7 IS NULL OR agent_id = ?7)
              AND (
                    ?8 IS NULL
                    OR (?8 = 'active' AND (expires IS NULL OR expires > ?9))
                    OR (?8 = 'expired' AND expires IS NOT NULL AND expires <= ?9)
              )";
        let total: usize = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM verification_tokens {sql_where}"),
                params![
                    filter.user_id,
                    filter.team_id,
                    filter.organization_id,
                    filter.key_hash,
                    filter.key_alias,
                    filter.project_id,
                    filter.agent_id,
                    expires_kind,
                    now,
                ],
                |row| row.get(0),
            )
            .map_err(Error::database)?;
        let page = filter.page.max(1);
        let size = filter.size.clamp(1, 100);
        let offset = (page - 1).saturating_mul(size);
        let mut statement = conn
            .prepare(&format!(
                "SELECT * FROM verification_tokens {sql_where} ORDER BY created_at DESC LIMIT ?10 OFFSET ?11"
            ))
            .map_err(Error::database)?;
        let keys = statement
            .query_map(
                params![
                    filter.user_id,
                    filter.team_id,
                    filter.organization_id,
                    filter.key_hash,
                    filter.key_alias,
                    filter.project_id,
                    filter.agent_id,
                    expires_kind,
                    now,
                    size,
                    offset,
                ],
                row_to_record,
            )
            .map_err(Error::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::database)?;
        Ok((keys, total))
    }

    fn aliases(&self, filter: &AliasFilter) -> Result<(Vec<String>, usize), ManagementError> {
        let conn = self.lock();
        let search = filter
            .search
            .as_ref()
            .map(|value| format!("%{}%", value.to_ascii_lowercase()));
        let sql_where = "
            WHERE key_alias IS NOT NULL
              AND key_alias != ''
              AND (?1 IS NULL OR team_id = ?1)
              AND (?2 IS NULL OR LOWER(key_alias) LIKE ?2)";
        let total: usize = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM verification_tokens {sql_where}"),
                params![filter.team_id, search],
                |row| row.get(0),
            )
            .map_err(Error::database)?;
        let page = filter.page.max(1);
        let size = filter.size.clamp(1, 100);
        let offset = (page - 1).saturating_mul(size);
        let mut statement = conn
            .prepare(&format!(
                "SELECT key_alias FROM verification_tokens {sql_where} ORDER BY key_alias ASC LIMIT ?3 OFFSET ?4"
            ))
            .map_err(Error::database)?;
        let aliases = statement
            .query_map(params![filter.team_id, search, size, offset], |row| {
                row.get(0)
            })
            .map_err(Error::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::database)?;
        Ok((aliases, total))
    }
}

fn migrate(conn: &Connection) -> Result<(), ManagementError> {
    conn.execute_batch(SCHEMA).map_err(Error::database)?;
    Ok(())
}

fn enforce_unique_alias(
    conn: &Connection,
    alias: &str,
    existing_token: Option<&str>,
) -> Result<(), ManagementError> {
    let owner: Option<String> = conn
        .query_row(
            "SELECT token FROM verification_tokens WHERE key_alias = ?1",
            [alias],
            |row| row.get(0),
        )
        .optional()
        .map_err(Error::database)?;
    match owner {
        Some(token) if existing_token != Some(token.as_str()) => Err(ManagementError::bad_request(
            format!(
                "Key with alias '{alias}' already exists. Unique key aliases across all keys are required."
            ),
            Some("key_alias"),
        )),
        _ => Ok(()),
    }
}

fn get_token(conn: &Connection, token: &str) -> Result<Option<KeyRecord>, ManagementError> {
    conn.query_row(
        "SELECT * FROM verification_tokens WHERE token = ?1",
        [token],
        row_to_record,
    )
    .optional()
    .map_err(Error::database)
    .map_err(ManagementError::from)
}

fn insert_row(conn: &Connection, record: &KeyRecord) -> Result<(), ManagementError> {
    bind_write(conn, INSERT_SQL, record)
}

fn update_row(tx: &Transaction<'_>, record: &KeyRecord) -> Result<(), ManagementError> {
    bind_write(
        tx,
        "
UPDATE verification_tokens SET
    key_name = ?2, key_alias = ?3, spend = ?4, expires = ?5, models = ?6, user_id = ?7,
    team_id = ?8, agent_id = ?9, organization_id = ?10, project_id = ?11, budget_id = ?12,
    max_budget = ?13, budget_duration = ?14, max_parallel_requests = ?15, metadata = ?16,
    tpm_limit = ?17, rpm_limit = ?18, blocked = ?19, key_type = ?20, allowed_routes = ?21,
    tags = ?22, rotation_count = ?23, created_at = ?24, updated_at = ?25
WHERE token = ?1
",
        record,
    )
}

fn bind_write(conn: &Connection, sql: &str, record: &KeyRecord) -> Result<(), ManagementError> {
    conn.execute(
        sql,
        params![
            record.token,
            record.key_name,
            record.key_alias,
            record.spend,
            record.expires.map(|value| value.to_rfc3339()),
            to_json(&Value::from(record.models.clone()))?,
            record.user_id,
            record.team_id,
            record.agent_id,
            record.organization_id,
            record.project_id,
            record.budget_id,
            record.max_budget,
            record.budget_duration,
            record.max_parallel_requests,
            to_json(&record.metadata)?,
            record.tpm_limit,
            record.rpm_limit,
            record.blocked.map(i32::from),
            record.key_type,
            to_json(&Value::from(record.allowed_routes.clone()))?,
            record
                .tags
                .as_ref()
                .map(|tags| to_json(&Value::from(tags.clone())))
                .transpose()?,
            record.rotation_count,
            record.created_at.to_rfc3339(),
            record.updated_at.to_rfc3339(),
        ],
    )
    .map_err(Error::database)?;
    Ok(())
}

fn row_to_record(row: &Row<'_>) -> rusqlite::Result<KeyRecord> {
    Ok(KeyRecord {
        token: row.get("token")?,
        key_name: row.get("key_name")?,
        key_alias: row.get("key_alias")?,
        spend: row.get("spend")?,
        expires: parse_datetime(row.get("expires")?)?,
        models: from_json(row.get("models")?)?,
        user_id: row.get("user_id")?,
        team_id: row.get("team_id")?,
        agent_id: row.get("agent_id")?,
        organization_id: row.get("organization_id")?,
        project_id: row.get("project_id")?,
        budget_id: row.get("budget_id")?,
        max_budget: row.get("max_budget")?,
        budget_duration: row.get("budget_duration")?,
        max_parallel_requests: row.get("max_parallel_requests")?,
        metadata: from_json(row.get("metadata")?)?,
        tpm_limit: row.get("tpm_limit")?,
        rpm_limit: row.get("rpm_limit")?,
        blocked: row
            .get::<_, Option<i32>>("blocked")?
            .map(|value| value != 0),
        key_type: row.get("key_type")?,
        allowed_routes: from_json(row.get("allowed_routes")?)?,
        tags: row
            .get::<_, Option<String>>("tags")?
            .map(from_json)
            .transpose()?,
        rotation_count: row.get("rotation_count")?,
        created_at: parse_datetime(Some(row.get("created_at")?))?
            .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
        updated_at: parse_datetime(Some(row.get("updated_at")?))?
            .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
    })
}

fn to_json(value: &Value) -> Result<String, ManagementError> {
    serde_json::to_string(value)
        .map_err(Error::database)
        .map_err(ManagementError::from)
}

fn from_json<T: serde::de::DeserializeOwned>(raw: String) -> rusqlite::Result<T> {
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn parse_datetime(value: Option<String>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    value
        .map(|raw| {
            DateTime::parse_from_rfc3339(&raw)
                .map(|parsed| parsed.with_timezone(&Utc))
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use litellm_gateway_management::keys::{GenerateKeyRequest, KeyManager};

    use super::SqliteStore;

    #[test]
    fn keys_survive_reopening_the_database() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("keys.sqlite");
        let now = Utc.with_ymd_and_hms(2026, 9, 11, 0, 0, 0).unwrap();
        let created = {
            let keys = KeyManager::new(SqliteStore::open(&path).expect("open"));
            keys.generate(
                GenerateKeyRequest {
                    key_alias: Some("durable".into()),
                    max_budget: Some(7.0),
                    duration: Some("1h".into()),
                    ..GenerateKeyRequest::default()
                },
                now,
            )
            .expect("generate")
        };
        let keys = KeyManager::new(SqliteStore::open(&path).expect("reopen"));
        let info = keys.info(Some(&created.key)).expect("persisted");
        assert_eq!(info.info.token_id, created.info.token_id);
        assert_eq!(info.info.key_alias.as_deref(), Some("durable"));
        assert_eq!(info.info.max_budget, Some(7.0));
        assert_eq!(info.info.expires, Some(now + Duration::hours(1)));
    }
}
