use std::path::{Path, PathBuf};

use chrono::{Datelike, Local, NaiveDate, Timelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::vault;

#[derive(Clone)]
pub struct Db {
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StoredKey {
    pub id: i64,
    pub provider: String,
    pub label: String,
    pub encrypted_key: String,
}

#[derive(Debug, Clone)]
pub struct LocalIdentityRecord {
    pub peer_id: String,
    pub encrypted_private_key: String,
    pub display_name: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerRecord {
    pub peer_id: String,
    pub display_name: String,
    pub timezone: String,
    pub multiaddr: Option<String>,
    pub online: bool,
    pub last_seen: Option<String>,
    pub availability_state: String,
    pub daily_limit_tokens: i64,
    pub daily_used_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenMessageRecord {
    pub id: i64,
    pub ts: String,
    pub direction: String,
    pub peer_id: String,
    pub msg_type: String,
    pub amount: i64,
    pub granted: Option<bool>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VaultKeyDto {
    pub id: i64,
    pub provider: String,
    pub label: String,
    pub masked_key: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_tokens_today: i64,
    pub requests_today: i64,
    pub active_key_label: Option<String>,
    pub proxy_running: bool,
    pub proxy_port: u16,
}

#[derive(Debug, Clone)]
pub struct RequestInsert {
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub source: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TransactionInsert {
    pub tx_type: String,
    pub peer_id: Option<String>,
    pub provider: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub request_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionRecord {
    pub id: i64,
    pub ts: String,
    pub tx_type: String,
    pub peer_id: Option<String>,
    pub provider: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub request_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FairUseRecord {
    pub peer_id: String,
    pub contributed_tokens: i64,
    pub consumed_tokens: i64,
    pub balance_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolStatusRecord {
    pub peer_id: String,
    pub display_name: String,
    pub availability_state: String,
    pub online: bool,
    pub timezone: String,
    pub daily_limit_tokens: i64,
    pub daily_used_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleConfig {
    pub timezone: String,
    pub weekly_active_bitmap: String,
    pub sharing_override: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditLogRecord {
    pub id: i64,
    pub ts: String,
    pub direction: String,
    pub peer_id: Option<String>,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub request_nonce: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityEventRecord {
    pub id: i64,
    pub ts: String,
    pub event_type: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RateLimitStat {
    pub peer_id: String,
    pub max_requests_per_min: i64,
    pub current_window_count: i64,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct LedgerGossipRecord {
    pub entry_id: String,
    pub signer_peer_id: String,
    pub lamport_ts: i64,
    pub cipher_b64: String,
    pub nonce: String,
    pub signature_b64: String,
    pub signer_pubkey_b64: String,
    pub observed_ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthSessionRecord {
    pub provider: String,
    pub account_label: Option<String>,
    pub scopes: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Db {
    pub fn new(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        let db = Self {
            path: path.as_ref().to_path_buf(),
        };
        db.init()?;
        Ok(db)
    }

    fn connection(&self) -> rusqlite::Result<Connection> {
        Connection::open(&self.path)
    }

    fn init(&self) -> rusqlite::Result<()> {
        let conn = self.connection()?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS requests (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                model TEXT,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL,
                request_id TEXT
            );

            CREATE TABLE IF NOT EXISTS keys (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                encrypted_key TEXT NOT NULL,
                label TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS ux_keys_provider ON keys(provider);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS identity (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                peer_id TEXT NOT NULL,
                encrypted_private_key TEXT NOT NULL,
                display_name TEXT NOT NULL,
                timezone TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS peers (
                peer_id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                timezone TEXT NOT NULL,
                multiaddr TEXT,
                online INTEGER NOT NULL DEFAULT 0,
                last_seen TEXT,
                availability_state TEXT NOT NULL DEFAULT 'unknown',
                daily_limit_tokens INTEGER NOT NULL DEFAULT 100000,
                daily_used_tokens INTEGER NOT NULL DEFAULT 0,
                daily_used_date TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS peer_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                direction TEXT NOT NULL,
                peer_id TEXT NOT NULL,
                msg_type TEXT NOT NULL,
                amount INTEGER NOT NULL DEFAULT 0,
                granted INTEGER,
                reason TEXT
            );

            CREATE TABLE IF NOT EXISTS local_sharing_state (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                availability_state TEXT NOT NULL,
                sharing_override TEXT NOT NULL,
                daily_limit_tokens INTEGER NOT NULL,
                daily_used_tokens INTEGER NOT NULL,
                daily_used_date TEXT,
                schedule_timezone TEXT NOT NULL,
                weekly_active_bitmap TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                type TEXT NOT NULL,
                peer_id TEXT,
                provider TEXT NOT NULL,
                model TEXT,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                request_hash TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS ix_transactions_ts ON transactions(ts);
            CREATE UNIQUE INDEX IF NOT EXISTS ux_transactions_hash_type_peer ON transactions(request_hash, type, COALESCE(peer_id, 'self'));

            CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                direction TEXT NOT NULL,
                peer_id TEXT,
                model TEXT,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                request_nonce TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS seen_nonces (
                nonce TEXT PRIMARY KEY,
                ts_epoch INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS peer_rate_limits (
                peer_id TEXT PRIMARY KEY,
                max_requests_per_min INTEGER NOT NULL DEFAULT 30,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS peer_request_window (
                peer_id TEXT NOT NULL,
                window_minute INTEGER NOT NULL,
                request_count INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(peer_id, window_minute)
            );

            CREATE TABLE IF NOT EXISTS device_info (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                device_salt TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS security_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                event_type TEXT NOT NULL,
                detail TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS circle_keys (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                encrypted_circle_key TEXT NOT NULL,
                key_version INTEGER NOT NULL DEFAULT 1,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ledger_gossip (
                entry_id TEXT PRIMARY KEY,
                signer_peer_id TEXT NOT NULL,
                lamport_ts INTEGER NOT NULL,
                cipher_b64 TEXT NOT NULL,
                nonce TEXT NOT NULL,
                signature_b64 TEXT NOT NULL,
                signer_pubkey_b64 TEXT NOT NULL,
                observed_ts TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS ix_ledger_gossip_lamport ON ledger_gossip(lamport_ts);

            CREATE TABLE IF NOT EXISTS oauth_sessions (
                provider TEXT PRIMARY KEY,
                encrypted_access_token TEXT NOT NULL,
                encrypted_refresh_token TEXT,
                expires_at TEXT,
                scopes TEXT,
                account_label TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;

        if self.get_setting("proxy_port")?.is_none() {
            self.set_setting("proxy_port", "47821")?;
        }
        if self.get_setting("peer_default_rate_limit_per_min")?.is_none() {
            self.set_setting("peer_default_rate_limit_per_min", "30")?;
        }
        if self.get_setting("blocked_model_patterns")?.is_none() {
            self.set_setting("blocked_model_patterns", "")?;
        }
        if self.get_setting("log_content")?.is_none() {
            self.set_setting("log_content", "false")?;
        }
        if self.get_setting("relay_mode")?.is_none() {
            self.set_setting("relay_mode", "self_hosted")?;
        }
        if self.get_setting("lamport_clock")?.is_none() {
            self.set_setting("lamport_clock", "0")?;
        }

        self.init_local_sharing_defaults()?;
        self.ensure_daily_reset()?;
        let _ = self.get_or_create_device_salt()?;
        let _ = self.ensure_circle_key()?;

        Ok(())
    }

    fn init_local_sharing_defaults(&self) -> rusqlite::Result<()> {
        let conn = self.connection()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM local_sharing_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;

        if count == 0 {
            let now = Local::now().to_rfc3339();
            let date = Local::now().date_naive().to_string();
            let timezone = std::env::var("TZ").unwrap_or_else(|_| "UTC".to_string());
            let bitmap = "1".repeat(7 * 24);
            conn.execute(
                "INSERT INTO local_sharing_state(singleton, availability_state, sharing_override, daily_limit_tokens, daily_used_tokens, daily_used_date, schedule_timezone, weekly_active_bitmap, updated_at) VALUES(1, 'available', 'auto', 100000, 0, ?1, ?2, ?3, ?4)",
                params![date, timezone, bitmap, now],
            )?;
        }

        Ok(())
    }

    pub fn ensure_daily_reset(&self) -> rusqlite::Result<()> {
        let today = Local::now().date_naive().to_string();
        let conn = self.connection()?;
        conn.execute(
            "UPDATE local_sharing_state SET daily_used_tokens = CASE WHEN daily_used_date = ?1 THEN daily_used_tokens ELSE 0 END, daily_used_date = ?1 WHERE singleton = 1",
            params![today],
        )?;
        conn.execute(
            "UPDATE peers SET daily_used_tokens = CASE WHEN daily_used_date = ?1 THEN daily_used_tokens ELSE 0 END, daily_used_date = ?1",
            params![today],
        )?;
        Ok(())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
            "#,
            params![key, value, now],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.connection()?;
        conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
    }

    pub fn get_or_create_device_salt(&self) -> rusqlite::Result<String> {
        let conn = self.connection()?;
        if let Some(existing) = conn
            .query_row(
                "SELECT device_salt FROM device_info WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(existing);
        }
        let now = Local::now().to_rfc3339();
        let salt = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO device_info(singleton, device_salt, updated_at) VALUES(1, ?1, ?2)",
            params![salt, now],
        )?;
        Ok(salt)
    }

    pub fn get_schedule_config(&self) -> rusqlite::Result<ScheduleConfig> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT schedule_timezone, weekly_active_bitmap, sharing_override FROM local_sharing_state WHERE singleton = 1",
            [],
            |row| {
                Ok(ScheduleConfig {
                    timezone: row.get(0)?,
                    weekly_active_bitmap: row.get(1)?,
                    sharing_override: row.get(2)?,
                })
            },
        )
    }

    pub fn set_schedule_config(
        &self,
        timezone: &str,
        weekly_active_bitmap: &str,
        sharing_override: &str,
    ) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "UPDATE local_sharing_state SET schedule_timezone = ?1, weekly_active_bitmap = ?2, sharing_override = ?3, updated_at = ?4 WHERE singleton = 1",
            params![timezone, weekly_active_bitmap, sharing_override, now],
        )?;
        Ok(())
    }

    pub fn get_local_availability_state(&self) -> rusqlite::Result<String> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT availability_state FROM local_sharing_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
    }

    pub fn set_local_availability_state(&self, state: &str) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "UPDATE local_sharing_state SET availability_state = ?1, updated_at = ?2 WHERE singleton = 1",
            params![state, now],
        )?;
        Ok(())
    }

    pub fn set_local_daily_limit(&self, limit_tokens: i64) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "UPDATE local_sharing_state SET daily_limit_tokens = ?1, updated_at = ?2 WHERE singleton = 1",
            params![limit_tokens, now],
        )?;
        Ok(())
    }

    pub fn local_can_lend(&self, amount: i64) -> rusqlite::Result<(bool, String)> {
        self.ensure_daily_reset()?;
        let conn = self.connection()?;
        let (availability, override_mode, limit, used): (String, String, i64, i64) = conn.query_row(
            "SELECT availability_state, sharing_override, daily_limit_tokens, daily_used_tokens FROM local_sharing_state WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;

        if availability == "paused" || override_mode == "paused" {
            return Ok((false, "paused".to_string()));
        }

        if availability == "sleeping" {
            return Ok((false, "sleeping".to_string()));
        }

        if used + amount > limit {
            return Ok((false, "daily limit reached".to_string()));
        }

        Ok((true, "available".to_string()))
    }

    pub fn increment_local_daily_used(&self, amount: i64) -> rusqlite::Result<()> {
        self.ensure_daily_reset()?;
        let conn = self.connection()?;
        conn.execute(
            "UPDATE local_sharing_state SET daily_used_tokens = daily_used_tokens + ?1 WHERE singleton = 1",
            params![amount.max(0)],
        )?;
        Ok(())
    }

    pub fn evaluate_schedule_availability(&self) -> rusqlite::Result<String> {
        self.ensure_daily_reset()?;
        let cfg = self.get_schedule_config()?;
        if cfg.sharing_override == "share_now" {
            self.set_local_availability_state("available")?;
            return Ok("available".to_string());
        }
        if cfg.sharing_override == "paused" {
            self.set_local_availability_state("paused")?;
            return Ok("paused".to_string());
        }

        let now = Local::now();
        let day_index = now.weekday().num_days_from_monday() as usize;
        let hour = now.hour() as usize;
        let idx = day_index * 24 + hour;
        let active = cfg
            .weekly_active_bitmap
            .chars()
            .nth(idx)
            .map(|c| c == '1')
            .unwrap_or(true);

        let state = if active { "available" } else { "sleeping" };
        self.set_local_availability_state(state)?;
        Ok(state.to_string())
    }

    pub fn set_local_identity(
        &self,
        peer_id: &str,
        encrypted_private_key: &str,
        display_name: &str,
        timezone: &str,
    ) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            r#"
            INSERT INTO identity(singleton, peer_id, encrypted_private_key, display_name, timezone, created_at, updated_at)
            VALUES (1, ?1, ?2, ?3, ?4, ?5, ?5)
            ON CONFLICT(singleton) DO UPDATE SET
              peer_id = excluded.peer_id,
              encrypted_private_key = excluded.encrypted_private_key,
              display_name = excluded.display_name,
              timezone = excluded.timezone,
              updated_at = excluded.updated_at
            "#,
            params![peer_id, encrypted_private_key, display_name, timezone, now],
        )?;
        Ok(())
    }

    pub fn get_local_identity(&self) -> rusqlite::Result<Option<LocalIdentityRecord>> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT peer_id, encrypted_private_key, display_name, timezone FROM identity WHERE singleton = 1",
            [],
            |row| {
                Ok(LocalIdentityRecord {
                    peer_id: row.get(0)?,
                    encrypted_private_key: row.get(1)?,
                    display_name: row.get(2)?,
                    timezone: row.get(3)?,
                })
            },
        )
        .optional()
    }

    pub fn update_local_profile(&self, display_name: &str, timezone: &str) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "UPDATE identity SET display_name = ?1, timezone = ?2, updated_at = ?3 WHERE singleton = 1",
            params![display_name, timezone, now],
        )?;
        Ok(())
    }

    pub fn upsert_peer(
        &self,
        peer_id: &str,
        display_name: &str,
        timezone: &str,
        multiaddr: Option<&str>,
        online: bool,
    ) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let today = Local::now().date_naive().to_string();
        let conn = self.connection()?;
        conn.execute(
            r#"
            INSERT INTO peers(peer_id, display_name, timezone, multiaddr, online, last_seen, availability_state, daily_limit_tokens, daily_used_tokens, daily_used_date, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'unknown', 100000, 0, ?7, ?6, ?6)
            ON CONFLICT(peer_id) DO UPDATE SET
              display_name = excluded.display_name,
              timezone = excluded.timezone,
              multiaddr = COALESCE(excluded.multiaddr, peers.multiaddr),
              online = excluded.online,
              last_seen = excluded.last_seen,
              updated_at = excluded.updated_at
            "#,
            params![peer_id, display_name, timezone, multiaddr, online as i64, now, today],
        )?;
        Ok(())
    }

    pub fn set_peer_status(&self, peer_id: &str, online: bool) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "UPDATE peers SET online = ?1, last_seen = ?2, updated_at = ?2 WHERE peer_id = ?3",
            params![online as i64, now, peer_id],
        )?;
        Ok(())
    }

    pub fn clear_all_peers(&self) -> rusqlite::Result<()> {
        let conn = self.connection()?;
        conn.execute("DELETE FROM peers", [])?;
        conn.execute("DELETE FROM peer_rate_limits", [])?;
        conn.execute("DELETE FROM peer_request_window", [])?;
        Ok(())
    }

    pub fn update_peer_availability(
        &self,
        peer_id: &str,
        availability_state: &str,
        daily_limit_tokens: Option<i64>,
        daily_used_tokens: Option<i64>,
    ) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "UPDATE peers SET availability_state = ?1, daily_limit_tokens = COALESCE(?2, daily_limit_tokens), daily_used_tokens = COALESCE(?3, daily_used_tokens), updated_at = ?4 WHERE peer_id = ?5",
            params![availability_state, daily_limit_tokens, daily_used_tokens, now, peer_id],
        )?;
        Ok(())
    }

    pub fn list_online_peers(&self) -> rusqlite::Result<Vec<PeerRecord>> {
        let all = self.list_peers()?;
        Ok(all.into_iter().filter(|p| p.online).collect())
    }

    pub fn list_peers(&self) -> rusqlite::Result<Vec<PeerRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT peer_id, display_name, timezone, multiaddr, online, last_seen, availability_state, daily_limit_tokens, daily_used_tokens FROM peers ORDER BY online DESC, updated_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();

        while let Some(row) = rows.next()? {
            out.push(PeerRecord {
                peer_id: row.get(0)?,
                display_name: row.get(1)?,
                timezone: row.get(2)?,
                multiaddr: row.get(3)?,
                online: row.get::<_, i64>(4)? == 1,
                last_seen: row.get(5)?,
                availability_state: row.get(6)?,
                daily_limit_tokens: row.get(7)?,
                daily_used_tokens: row.get(8)?,
            });
        }

        Ok(out)
    }

    pub fn insert_peer_message(
        &self,
        direction: &str,
        peer_id: &str,
        msg_type: &str,
        amount: i64,
        granted: Option<bool>,
        reason: Option<&str>,
    ) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            r#"
            INSERT INTO peer_messages(ts, direction, peer_id, msg_type, amount, granted, reason)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![now, direction, peer_id, msg_type, amount, granted.map(|v| v as i64), reason],
        )?;
        Ok(())
    }

    pub fn list_recent_peer_messages(&self, limit: i64) -> rusqlite::Result<Vec<TokenMessageRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, ts, direction, peer_id, msg_type, amount, granted, reason FROM peer_messages ORDER BY id DESC LIMIT ?1",
        )?;

        let mut rows = stmt.query(params![limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(TokenMessageRecord {
                id: row.get(0)?,
                ts: row.get(1)?,
                direction: row.get(2)?,
                peer_id: row.get(3)?,
                msg_type: row.get(4)?,
                amount: row.get(5)?,
                granted: row.get::<_, Option<i64>>(6)?.map(|v| v == 1),
                reason: row.get(7)?,
            });
        }
        Ok(out)
    }

    pub fn set_provider_key(&self, provider: &str, label: &str, encrypted_key: &str) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            r#"
            INSERT INTO keys(provider, encrypted_key, label, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4, ?4)
            ON CONFLICT(provider) DO UPDATE SET
              encrypted_key = excluded.encrypted_key,
              label = excluded.label,
              updated_at = excluded.updated_at
            "#,
            params![provider, encrypted_key, label, now],
        )?;
        Ok(())
    }

    pub fn upsert_oauth_session(
        &self,
        provider: &str,
        encrypted_access_token: &str,
        encrypted_refresh_token: Option<&str>,
        expires_at: Option<&str>,
        scopes: Option<&str>,
        account_label: Option<&str>,
    ) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            r#"
            INSERT INTO oauth_sessions(provider, encrypted_access_token, encrypted_refresh_token, expires_at, scopes, account_label, created_at, updated_at)
            VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(provider) DO UPDATE SET
              encrypted_access_token = excluded.encrypted_access_token,
              encrypted_refresh_token = excluded.encrypted_refresh_token,
              expires_at = excluded.expires_at,
              scopes = excluded.scopes,
              account_label = excluded.account_label,
              updated_at = excluded.updated_at
            "#,
            params![
                provider,
                encrypted_access_token,
                encrypted_refresh_token,
                expires_at,
                scopes,
                account_label,
                now
            ],
        )?;
        Ok(())
    }

    pub fn get_oauth_session_encrypted(
        &self,
        provider: &str,
    ) -> rusqlite::Result<Option<(String, Option<String>, Option<String>, Option<String>, Option<String>)>> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT encrypted_access_token, encrypted_refresh_token, expires_at, scopes, account_label FROM oauth_sessions WHERE provider = ?1",
            params![provider],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
    }

    pub fn list_oauth_sessions(&self) -> rusqlite::Result<Vec<OAuthSessionRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT provider, account_label, scopes, expires_at, created_at, updated_at FROM oauth_sessions ORDER BY updated_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(OAuthSessionRecord {
                provider: row.get(0)?,
                account_label: row.get(1)?,
                scopes: row.get(2)?,
                expires_at: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            });
        }
        Ok(out)
    }

    pub fn delete_oauth_session(&self, provider: &str) -> rusqlite::Result<()> {
        let conn = self.connection()?;
        conn.execute("DELETE FROM oauth_sessions WHERE provider = ?1", params![provider])?;
        Ok(())
    }

    pub fn no_content_logging_hardcoded(&self) -> bool {
        false
    }

    pub fn ensure_circle_key(&self) -> rusqlite::Result<String> {
        if let Some(existing) = self.get_circle_key_plain()? {
            return Ok(existing);
        }
        let raw = Uuid::new_v4().to_string().replace('-', "");
        self.set_circle_key_plain(&raw, 1)?;
        Ok(raw)
    }

    pub fn set_circle_key_plain(&self, key_plain: &str, version: i64) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let salt = self.get_or_create_device_salt()?;
        let encrypted = vault::encrypt_api_key(
            key_plain,
            &format!("tokenunion-circle-key::{salt}"),
            &salt,
        )
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO circle_keys(singleton, encrypted_circle_key, key_version, updated_at) VALUES(1, ?1, ?2, ?3) ON CONFLICT(singleton) DO UPDATE SET encrypted_circle_key = excluded.encrypted_circle_key, key_version = excluded.key_version, updated_at = excluded.updated_at",
            params![encrypted, version, now],
        )?;
        Ok(())
    }

    pub fn get_circle_key_plain(&self) -> rusqlite::Result<Option<String>> {
        let conn = self.connection()?;
        let encrypted: Option<String> = conn
            .query_row(
                "SELECT encrypted_circle_key FROM circle_keys WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let Some(encrypted) = encrypted else {
            return Ok(None);
        };
        let salt = self.get_or_create_device_salt()?;
        let decrypted = vault::decrypt_api_key(
            &encrypted,
            &format!("tokenunion-circle-key::{salt}"),
            &salt,
        )
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        Ok(Some(decrypted))
    }

    pub fn rotate_circle_key(&self) -> rusqlite::Result<String> {
        let conn = self.connection()?;
        let current_version: i64 = conn
            .query_row(
                "SELECT COALESCE(key_version, 1) FROM circle_keys WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(1);
        let new_key = Uuid::new_v4().to_string().replace('-', "");
        self.set_circle_key_plain(&new_key, current_version + 1)?;
        Ok(new_key)
    }

    pub fn next_lamport(&self) -> rusqlite::Result<i64> {
        let current: i64 = self
            .get_setting("lamport_clock")?
            .unwrap_or_else(|| "0".to_string())
            .parse()
            .unwrap_or(0);
        let next = current + 1;
        self.set_setting("lamport_clock", &next.to_string())?;
        Ok(next)
    }

    pub fn observe_lamport(&self, remote_lamport: i64) -> rusqlite::Result<i64> {
        let current: i64 = self
            .get_setting("lamport_clock")?
            .unwrap_or_else(|| "0".to_string())
            .parse()
            .unwrap_or(0);
        let next = current.max(remote_lamport) + 1;
        self.set_setting("lamport_clock", &next.to_string())?;
        Ok(next)
    }

    pub fn max_lamport(&self) -> rusqlite::Result<i64> {
        let conn = self.connection()?;
        conn.query_row("SELECT COALESCE(MAX(lamport_ts), 0) FROM ledger_gossip", [], |row| row.get(0))
    }

    pub fn upsert_ledger_gossip(&self, item: &LedgerGossipRecord) -> rusqlite::Result<bool> {
        let conn = self.connection()?;
        let existing: Option<i64> = conn
            .query_row(
                "SELECT lamport_ts FROM ledger_gossip WHERE entry_id = ?1",
                params![item.entry_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(curr) = existing {
            if curr >= item.lamport_ts {
                return Ok(false);
            }
        }
        conn.execute(
            "INSERT INTO ledger_gossip(entry_id, signer_peer_id, lamport_ts, cipher_b64, nonce, signature_b64, signer_pubkey_b64, observed_ts) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(entry_id) DO UPDATE SET signer_peer_id=excluded.signer_peer_id, lamport_ts=excluded.lamport_ts, cipher_b64=excluded.cipher_b64, nonce=excluded.nonce, signature_b64=excluded.signature_b64, signer_pubkey_b64=excluded.signer_pubkey_b64, observed_ts=excluded.observed_ts",
            params![
                item.entry_id,
                item.signer_peer_id,
                item.lamport_ts,
                item.cipher_b64,
                item.nonce,
                item.signature_b64,
                item.signer_pubkey_b64,
                item.observed_ts
            ],
        )?;
        Ok(true)
    }

    pub fn list_ledger_gossip_since(&self, lamport_cursor: i64, limit: i64) -> rusqlite::Result<Vec<LedgerGossipRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT entry_id, signer_peer_id, lamport_ts, cipher_b64, nonce, signature_b64, signer_pubkey_b64, observed_ts FROM ledger_gossip WHERE lamport_ts > ?1 ORDER BY lamport_ts ASC LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![lamport_cursor, limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(LedgerGossipRecord {
                entry_id: row.get(0)?,
                signer_peer_id: row.get(1)?,
                lamport_ts: row.get(2)?,
                cipher_b64: row.get(3)?,
                nonce: row.get(4)?,
                signature_b64: row.get(5)?,
                signer_pubkey_b64: row.get(6)?,
                observed_ts: row.get(7)?,
            });
        }
        Ok(out)
    }

    pub fn count_prompt_leaks(&self, probe: &str) -> rusqlite::Result<i64> {
        let conn = self.connection()?;
        let probe_like = format!("%{probe}%");
        let mut total = 0_i64;
        total += conn.query_row(
            "SELECT COUNT(*) FROM requests WHERE COALESCE(model,'') LIKE ?1 OR COALESCE(source,'') LIKE ?1 OR COALESCE(request_id,'') LIKE ?1",
            params![probe_like.clone()],
            |row| row.get::<_, i64>(0),
        )?;
        total += conn.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE COALESCE(model,'') LIKE ?1 OR COALESCE(request_nonce,'') LIKE ?1",
            params![probe_like.clone()],
            |row| row.get::<_, i64>(0),
        )?;
        total += conn.query_row(
            "SELECT COUNT(*) FROM transactions WHERE COALESCE(model,'') LIKE ?1 OR COALESCE(request_hash,'') LIKE ?1",
            params![probe_like],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(total)
    }

    pub fn set_anthropic_key(&self, label: &str, encrypted_key: &str) -> rusqlite::Result<()> {
        self.set_provider_key("anthropic", label, encrypted_key)
    }

    pub fn get_provider_key(&self, provider: &str) -> rusqlite::Result<Option<StoredKey>> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT id, provider, label, encrypted_key FROM keys WHERE provider = ?1 LIMIT 1",
            params![provider],
            |row| {
                Ok(StoredKey {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    label: row.get(2)?,
                    encrypted_key: row.get(3)?,
                })
            },
        )
        .optional()
    }

    pub fn list_keys(&self) -> rusqlite::Result<Vec<VaultKeyDto>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, provider, label, encrypted_key, created_at FROM keys ORDER BY updated_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let encrypted: String = row.get(3)?;
            out.push(VaultKeyDto {
                id: row.get(0)?,
                provider: row.get(1)?,
                label: row.get(2)?,
                masked_key: mask_ciphertext(&encrypted),
                created_at: row.get(4)?,
            });
        }
        Ok(out)
    }

    pub fn delete_key(&self, id: i64) -> rusqlite::Result<()> {
        let conn = self.connection()?;
        conn.execute("DELETE FROM keys WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn insert_request(&self, req: &RequestInsert) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            r#"
            INSERT INTO requests(ts, model, input_tokens, output_tokens, source, request_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                now,
                req.model,
                req.input_tokens,
                req.output_tokens,
                req.source,
                req.request_id
            ],
        )?;
        Ok(())
    }

    pub fn insert_audit_log(
        &self,
        direction: &str,
        peer_id: Option<&str>,
        model: Option<&str>,
        input_tokens: i64,
        output_tokens: i64,
        request_nonce: &str,
    ) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO audit_log(ts, direction, peer_id, model, input_tokens, output_tokens, request_nonce) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![now, direction, peer_id, model, input_tokens, output_tokens, request_nonce],
        )?;
        Ok(())
    }

    pub fn list_audit_log(&self, limit: i64) -> rusqlite::Result<Vec<AuditLogRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, ts, direction, peer_id, model, input_tokens, output_tokens, request_nonce FROM audit_log ORDER BY id DESC LIMIT ?1",
        )?;
        let mut rows = stmt.query(params![limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(AuditLogRecord {
                id: row.get(0)?,
                ts: row.get(1)?,
                direction: row.get(2)?,
                peer_id: row.get(3)?,
                model: row.get(4)?,
                input_tokens: row.get(5)?,
                output_tokens: row.get(6)?,
                request_nonce: row.get(7)?,
            });
        }
        Ok(out)
    }

    pub fn remember_nonce_if_fresh(&self, nonce: &str, ts_epoch: i64) -> rusqlite::Result<bool> {
        let now = Utc::now().timestamp();
        if (now - ts_epoch).abs() > 30 {
            return Ok(false);
        }
        let conn = self.connection()?;
        conn.execute(
            "DELETE FROM seen_nonces WHERE ts_epoch < ?1",
            params![now - 30],
        )?;
        let inserted = conn.execute(
            "INSERT OR IGNORE INTO seen_nonces(nonce, ts_epoch) VALUES(?1, ?2)",
            params![nonce, ts_epoch],
        )?;
        Ok(inserted == 1)
    }

    pub fn check_and_increment_peer_rate_limit(&self, peer_id: &str) -> rusqlite::Result<(bool, i64, i64)> {
        let now = Utc::now().timestamp();
        let current_window = now / 60;
        let default_limit: i64 = self
            .get_setting("peer_default_rate_limit_per_min")?
            .unwrap_or_else(|| "30".to_string())
            .parse()
            .unwrap_or(30);

        let conn = self.connection()?;
        let max_limit = conn
            .query_row(
                "SELECT max_requests_per_min FROM peer_rate_limits WHERE peer_id = ?1",
                params![peer_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(default_limit);

        conn.execute(
            "DELETE FROM peer_request_window WHERE window_minute < ?1",
            params![current_window - 1],
        )?;
        conn.execute(
            "INSERT INTO peer_request_window(peer_id, window_minute, request_count) VALUES(?1, ?2, 1) ON CONFLICT(peer_id, window_minute) DO UPDATE SET request_count = request_count + 1",
            params![peer_id, current_window],
        )?;
        let count: i64 = conn.query_row(
            "SELECT request_count FROM peer_request_window WHERE peer_id = ?1 AND window_minute = ?2",
            params![peer_id, current_window],
            |row| row.get(0),
        )?;
        Ok((count <= max_limit, count, max_limit))
    }

    pub fn set_peer_rate_limit(&self, peer_id: &str, max_per_min: i64) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO peer_rate_limits(peer_id, max_requests_per_min, updated_at) VALUES(?1, ?2, ?3) ON CONFLICT(peer_id) DO UPDATE SET max_requests_per_min = excluded.max_requests_per_min, updated_at = excluded.updated_at",
            params![peer_id, max_per_min, now],
        )?;
        Ok(())
    }

    pub fn list_rate_limit_stats(&self) -> rusqlite::Result<Vec<RateLimitStat>> {
        let now = Utc::now().timestamp() / 60;
        let default_limit: i64 = self
            .get_setting("peer_default_rate_limit_per_min")?
            .unwrap_or_else(|| "30".to_string())
            .parse()
            .unwrap_or(30);
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT p.peer_id,
                   COALESCE(r.max_requests_per_min, ?1) as max_requests_per_min,
                   COALESCE(w.request_count, 0) as current_window_count
            FROM peers p
            LEFT JOIN peer_rate_limits r ON r.peer_id = p.peer_id
            LEFT JOIN peer_request_window w ON w.peer_id = p.peer_id AND w.window_minute = ?2
            ORDER BY p.peer_id
            "#,
        )?;
        let mut rows = stmt.query(params![default_limit, now])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(RateLimitStat {
                peer_id: row.get(0)?,
                max_requests_per_min: row.get(1)?,
                current_window_count: row.get(2)?,
            });
        }
        Ok(out)
    }

    pub fn insert_security_event(&self, event_type: &str, detail: &str) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            "INSERT INTO security_events(ts, event_type, detail) VALUES(?1, ?2, ?3)",
            params![now, event_type, detail],
        )?;
        Ok(())
    }

    pub fn list_security_events(&self, limit: i64) -> rusqlite::Result<Vec<SecurityEventRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, ts, event_type, detail FROM security_events ORDER BY id DESC LIMIT ?1",
        )?;
        let mut rows = stmt.query(params![limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(SecurityEventRecord {
                id: row.get(0)?,
                ts: row.get(1)?,
                event_type: row.get(2)?,
                detail: row.get(3)?,
            });
        }
        Ok(out)
    }

    pub fn insert_transaction(&self, tx: &TransactionInsert) -> rusqlite::Result<()> {
        let now = Local::now().to_rfc3339();
        let conn = self.connection()?;
        conn.execute(
            r#"
            INSERT OR IGNORE INTO transactions(ts, type, peer_id, provider, model, input_tokens, output_tokens, request_hash)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                now,
                tx.tx_type,
                tx.peer_id,
                tx.provider,
                tx.model,
                tx.input_tokens,
                tx.output_tokens,
                tx.request_hash
            ],
        )?;
        Ok(())
    }

    pub fn list_transactions(&self, limit: i64) -> rusqlite::Result<Vec<TransactionRecord>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, ts, type, peer_id, provider, model, input_tokens, output_tokens, request_hash FROM transactions ORDER BY id DESC LIMIT ?1",
        )?;
        let mut rows = stmt.query(params![limit])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(TransactionRecord {
                id: row.get(0)?,
                ts: row.get(1)?,
                tx_type: row.get(2)?,
                peer_id: row.get(3)?,
                provider: row.get(4)?,
                model: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                request_hash: row.get(8)?,
            });
        }
        Ok(out)
    }

    pub fn fair_use_7d(&self) -> rusqlite::Result<Vec<FairUseRecord>> {
        let since = (Local::now() - chrono::Duration::days(7)).to_rfc3339();
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            r#"
            WITH contributed AS (
                SELECT COALESCE(peer_id, '') AS peer_id, SUM(input_tokens + output_tokens) AS tokens
                FROM transactions
                WHERE type = 'lent' AND ts >= ?1
                GROUP BY COALESCE(peer_id, '')
            ),
            consumed AS (
                SELECT COALESCE(peer_id, '') AS peer_id, SUM(input_tokens + output_tokens) AS tokens
                FROM transactions
                WHERE type = 'borrowed' AND ts >= ?1
                GROUP BY COALESCE(peer_id, '')
            )
            SELECT p.peer_id,
                   COALESCE(c.tokens, 0) AS contributed,
                   COALESCE(d.tokens, 0) AS consumed
            FROM peers p
            LEFT JOIN contributed c ON c.peer_id = p.peer_id
            LEFT JOIN consumed d ON d.peer_id = p.peer_id
            ORDER BY (COALESCE(c.tokens,0) - COALESCE(d.tokens,0)) DESC
            "#,
        )?;

        let mut rows = stmt.query(params![since])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let contributed: i64 = row.get(1)?;
            let consumed: i64 = row.get(2)?;
            out.push(FairUseRecord {
                peer_id: row.get(0)?,
                contributed_tokens: contributed,
                consumed_tokens: consumed,
                balance_tokens: contributed - consumed,
            });
        }
        Ok(out)
    }

    pub fn pool_status(&self, local_peer_id: &str, local_name: &str, local_timezone: &str) -> rusqlite::Result<Vec<PoolStatusRecord>> {
        let mut out = Vec::new();
        let local_state = self.get_local_availability_state().unwrap_or_else(|_| "unknown".to_string());
        out.push(PoolStatusRecord {
            peer_id: local_peer_id.to_string(),
            display_name: local_name.to_string(),
            availability_state: local_state,
            online: true,
            timezone: local_timezone.to_string(),
            daily_limit_tokens: self.local_limit_tokens().unwrap_or(100000),
            daily_used_tokens: self.local_used_tokens().unwrap_or(0),
        });
        for peer in self.list_peers()? {
            out.push(PoolStatusRecord {
                peer_id: peer.peer_id,
                display_name: peer.display_name,
                availability_state: peer.availability_state,
                online: peer.online,
                timezone: peer.timezone,
                daily_limit_tokens: peer.daily_limit_tokens,
                daily_used_tokens: peer.daily_used_tokens,
            });
        }
        Ok(out)
    }

    pub fn local_limit_tokens(&self) -> rusqlite::Result<i64> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT daily_limit_tokens FROM local_sharing_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
    }

    pub fn local_used_tokens(&self) -> rusqlite::Result<i64> {
        self.ensure_daily_reset()?;
        let conn = self.connection()?;
        conn.query_row(
            "SELECT daily_used_tokens FROM local_sharing_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
    }

    pub fn today_stats(&self) -> rusqlite::Result<(i64, i64)> {
        let today: NaiveDate = Local::now().date_naive();
        let start = format!("{}T00:00:00", today.format("%Y-%m-%d"));
        let conn = self.connection()?;
        let result = conn.query_row(
            r#"
            SELECT COALESCE(SUM(input_tokens + output_tokens), 0) AS total_tokens,
                   COUNT(*) AS requests
            FROM requests
            WHERE ts >= ?1
            "#,
            params![start],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(result)
    }

    pub fn active_key_label(&self) -> rusqlite::Result<Option<String>> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT label FROM keys WHERE provider = 'anthropic' LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
    }
}

fn mask_ciphertext(cipher: &str) -> String {
    let shown = cipher.chars().take(6).collect::<String>();
    format!("{}******", shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_nonce_rejected() {
        let path = std::env::temp_dir().join(format!("tokenunion-test-{}.db", Uuid::new_v4()));
        let db = Db::new(&path).unwrap();
        let now = Utc::now().timestamp();

        let first = db.remember_nonce_if_fresh("nonce-abc", now).unwrap();
        let second = db.remember_nonce_if_fresh("nonce-abc", now).unwrap();
        assert!(first);
        assert!(!second, "second use of same nonce should be rejected");
    }

    #[test]
    fn prompt_content_never_persisted_in_logs() {
        let path = std::env::temp_dir().join(format!("tokenunion-test-{}.db", Uuid::new_v4()));
        let db = Db::new(&path).unwrap();
        let probe = "PROMPT_SECRET_123";
        db.insert_request(&RequestInsert {
            model: Some("claude-opus-4".to_string()),
            input_tokens: 10,
            output_tokens: 20,
            source: "local_proxy".to_string(),
            request_id: Some("req-1".to_string()),
        })
        .unwrap();
        db.insert_audit_log("outbound", Some("peer"), Some("claude-opus-4"), 10, 20, "nonce-1")
            .unwrap();
        db.insert_transaction(&TransactionInsert {
            tx_type: "self".to_string(),
            peer_id: None,
            provider: "anthropic".to_string(),
            model: Some("claude-opus-4".to_string()),
            input_tokens: 10,
            output_tokens: 20,
            request_hash: "hash-1".to_string(),
        })
        .unwrap();
        let leaks = db.count_prompt_leaks(probe).unwrap();
        assert_eq!(leaks, 0, "prompt probe should never appear in persisted telemetry");
    }
}
