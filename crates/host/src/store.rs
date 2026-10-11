//! The host's own state, in one SQLite file: what it runs, what those
//! services committed, and who used them.

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::Connection;
use std::{path::Path, sync::Arc, time::Duration};

/// Each entry moves the schema one version on; `user_version` counts how many
/// have run. Never edit one that has shipped.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE services (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        reference TEXT NOT NULL,
        digest TEXT NOT NULL,
        version TEXT NOT NULL,
        commits INTEGER NOT NULL DEFAULT 0,
        seq INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
        PRIMARY KEY (owner_id, name)
    );
    CREATE TABLE service_state (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        key TEXT NOT NULL,
        value BLOB NOT NULL,
        PRIMARY KEY (owner_id, name, key),
        FOREIGN KEY (owner_id, name) REFERENCES services (owner_id, name) ON DELETE CASCADE
    );
    CREATE TABLE service_logs (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        seq INTEGER NOT NULL,
        topics TEXT NOT NULL,
        data TEXT NOT NULL,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        PRIMARY KEY (owner_id, name, seq),
        FOREIGN KEY (owner_id, name) REFERENCES services (owner_id, name) ON DELETE CASCADE
    );
    CREATE TABLE service_positions (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        program TEXT NOT NULL,
        seq INTEGER NOT NULL,
        PRIMARY KEY (owner_id, name, program),
        FOREIGN KEY (owner_id, name) REFERENCES services (owner_id, name) ON DELETE CASCADE
    );
    CREATE TABLE service_wakes (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        program TEXT NOT NULL,
        tool TEXT NOT NULL,
        args TEXT NOT NULL,
        due INTEGER NOT NULL,
        armed INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (owner_id, name),
        FOREIGN KEY (owner_id, name) REFERENCES services (owner_id, name) ON DELETE CASCADE
    );
    CREATE INDEX idx_service_wakes_due ON service_wakes (due);
    CREATE TABLE service_usage (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        kind TEXT NOT NULL,
        item TEXT NOT NULL,
        source TEXT NOT NULL,
        caller TEXT NOT NULL,
        outcome TEXT NOT NULL,
        day INTEGER NOT NULL,
        calls INTEGER NOT NULL,
        last INTEGER NOT NULL,
        PRIMARY KEY (owner_id, name, kind, item, source, caller, outcome, day),
        FOREIGN KEY (owner_id, name) REFERENCES services (owner_id, name) ON DELETE CASCADE
    );
    CREATE TABLE service_deprecations (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        tool TEXT NOT NULL,
        since INTEGER NOT NULL,
        sunset INTEGER,
        PRIMARY KEY (owner_id, name, tool),
        FOREIGN KEY (owner_id, name) REFERENCES services (owner_id, name) ON DELETE CASCADE
    );
    CREATE TABLE service_callers (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        tool TEXT NOT NULL,
        kind TEXT NOT NULL,
        caller TEXT NOT NULL,
        PRIMARY KEY (owner_id, name, tool, kind, caller),
        FOREIGN KEY (owner_id, name) REFERENCES services (owner_id, name) ON DELETE CASCADE
    );
    CREATE TABLE routes (
        owner_id TEXT NOT NULL,
        name TEXT NOT NULL,
        node TEXT NOT NULL,
        PRIMARY KEY (owner_id, name)
    );",
    "ALTER TABLE routes RENAME COLUMN node TO peer;",
];

/// One connection, shared. Every statement runs off the async workers.
#[derive(Clone)]
pub struct Store(Arc<Mutex<Connection>>);

impl Store {
    /// Open the state file at `path`, creating it and its directory, and run
    /// what migrations it has not.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let mut conn =
            Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let ran: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        for (at, migration) in MIGRATIONS.iter().enumerate().skip(ran as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(migration)
                .with_context(|| format!("migration {}", at + 1))?;
            tx.pragma_update(None, "user_version", at as i64 + 1)?;
            tx.commit()?;
        }
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    /// Run `work` against the connection, off the async workers.
    pub async fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T> {
        let conn = self.0.clone();
        Ok(tokio::task::spawn_blocking(move || work(&mut conn.lock())).await??)
    }
}
