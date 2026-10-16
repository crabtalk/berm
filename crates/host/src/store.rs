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
    "DROP TABLE service_callers;
    ALTER TABLE services RENAME COLUMN owner_id TO scope;
    ALTER TABLE service_state RENAME COLUMN owner_id TO scope;
    ALTER TABLE service_logs RENAME COLUMN owner_id TO scope;
    ALTER TABLE service_positions RENAME COLUMN owner_id TO scope;
    ALTER TABLE service_wakes RENAME COLUMN owner_id TO scope;
    ALTER TABLE service_usage RENAME COLUMN owner_id TO scope;
    ALTER TABLE service_deprecations RENAME COLUMN owner_id TO scope;
    ALTER TABLE routes RENAME COLUMN owner_id TO scope;",
    "CREATE TABLE service_meters (
        scope TEXT NOT NULL,
        name TEXT NOT NULL,
        day INTEGER NOT NULL,
        calls INTEGER NOT NULL DEFAULT 0,
        instructions INTEGER NOT NULL DEFAULT 0,
        memory INTEGER NOT NULL DEFAULT 0,
        read INTEGER NOT NULL DEFAULT 0,
        written INTEGER NOT NULL DEFAULT 0,
        stored INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (scope, name, day),
        FOREIGN KEY (scope, name) REFERENCES services (scope, name) ON DELETE CASCADE
    );",
    "CREATE TABLE service_budgets (
        scope TEXT NOT NULL,
        name TEXT NOT NULL,
        tool TEXT NOT NULL,
        instructions INTEGER NOT NULL,
        rows INTEGER NOT NULL,
        PRIMARY KEY (scope, name, tool),
        FOREIGN KEY (scope, name) REFERENCES services (scope, name) ON DELETE CASCADE
    );",
    "ALTER TABLE service_meters ADD COLUMN proxied INTEGER NOT NULL DEFAULT 0;",
    "CREATE TABLE service_terms (
        scope TEXT NOT NULL,
        name TEXT NOT NULL,
        tool TEXT NOT NULL,
        instructions INTEGER,
        rows INTEGER,
        price_micros INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (scope, name, tool),
        FOREIGN KEY (scope, name) REFERENCES services (scope, name) ON DELETE CASCADE
    );
    INSERT INTO service_terms (scope, name, tool, instructions, rows)
        SELECT scope, name, tool, instructions, rows FROM service_budgets;
    DROP TABLE service_budgets;",
    "ALTER TABLE service_meters ADD COLUMN overage INTEGER NOT NULL DEFAULT 0;",
    "CREATE TABLE held_scopes (scope TEXT PRIMARY KEY);",
    "CREATE TABLE service_calls (
        id INTEGER PRIMARY KEY,
        scope TEXT NOT NULL,
        name TEXT NOT NULL,
        at INTEGER NOT NULL,
        tool TEXT NOT NULL,
        source TEXT NOT NULL,
        caller TEXT NOT NULL,
        outcome TEXT NOT NULL,
        ms INTEGER NOT NULL,
        meter TEXT,
        price_micros INTEGER NOT NULL,
        FOREIGN KEY (scope, name) REFERENCES services (scope, name) ON DELETE CASCADE
    );
    CREATE INDEX idx_service_calls_service ON service_calls (scope, name);
    CREATE INDEX idx_service_calls_tool ON service_calls (scope, name, tool);
    CREATE INDEX idx_service_calls_at ON service_calls (at);",
    "ALTER TABLE service_wakes ADD COLUMN failures INTEGER NOT NULL DEFAULT 0;",
    "ALTER TABLE service_logs ADD COLUMN call INTEGER;
    ALTER TABLE service_logs ADD COLUMN at INTEGER NOT NULL DEFAULT 0;
    UPDATE service_logs SET at = CAST(strftime('%s', 'now') AS INTEGER) * 1000;
    CREATE INDEX idx_service_logs_call ON service_logs (scope, name, call);
    CREATE INDEX idx_service_logs_at ON service_logs (at);
    DROP TABLE service_positions;",
    "ALTER TABLE service_meters ADD COLUMN logged INTEGER NOT NULL DEFAULT 0;",
    // `slot` is '' for the wake a service arms, and a scheduled tool's name
    // for that tool's: `every` is its period, `declared` the one the last
    // deploy's config set, and `due` is null while it has none.
    "CREATE TABLE service_wakes_new (
        scope TEXT NOT NULL,
        name TEXT NOT NULL,
        slot TEXT NOT NULL DEFAULT '',
        program TEXT NOT NULL,
        tool TEXT NOT NULL,
        args TEXT NOT NULL,
        due INTEGER,
        armed INTEGER NOT NULL DEFAULT 1,
        failures INTEGER NOT NULL DEFAULT 0,
        every INTEGER,
        declared INTEGER,
        PRIMARY KEY (scope, name, slot),
        FOREIGN KEY (scope, name) REFERENCES services (scope, name) ON DELETE CASCADE
    );
    INSERT INTO service_wakes_new (scope, name, program, tool, args, due, armed, failures)
        SELECT scope, name, program, tool, args, due, armed, failures FROM service_wakes;
    DROP TABLE service_wakes;
    ALTER TABLE service_wakes_new RENAME TO service_wakes;
    CREATE INDEX idx_service_wakes_due ON service_wakes (due);",
    // A cron tool's `cron` and `declared` are cron expressions. A period a
    // five-field expression says turns into it; any other leaves the tool
    // without one, and not called.
    "ALTER TABLE service_wakes ADD COLUMN cron TEXT;
    ALTER TABLE service_wakes ADD COLUMN declared_cron TEXT;
    UPDATE service_wakes SET
        cron = CASE
            WHEN every = 60000 THEN '* * * * *'
            WHEN every % 60000 = 0 AND every < 3600000 AND 60 % (every / 60000) = 0
                THEN '*/' || (every / 60000) || ' * * * *'
            WHEN every = 3600000 THEN '0 * * * *'
            WHEN every % 3600000 = 0 AND every < 86400000 AND 24 % (every / 3600000) = 0
                THEN '0 */' || (every / 3600000) || ' * * *'
            WHEN every = 86400000 THEN '0 0 * * *'
        END,
        declared_cron = CASE
            WHEN declared = 60000 THEN '* * * * *'
            WHEN declared % 60000 = 0 AND declared < 3600000 AND 60 % (declared / 60000) = 0
                THEN '*/' || (declared / 60000) || ' * * * *'
            WHEN declared = 3600000 THEN '0 * * * *'
            WHEN declared % 3600000 = 0 AND declared < 86400000 AND 24 % (declared / 3600000) = 0
                THEN '0 */' || (declared / 3600000) || ' * * *'
            WHEN declared = 86400000 THEN '0 0 * * *'
        END
     WHERE slot <> '';
    UPDATE service_wakes SET due = NULL WHERE slot <> '' AND cron IS NULL;
    ALTER TABLE service_wakes DROP COLUMN every;
    ALTER TABLE service_wakes DROP COLUMN declared;
    ALTER TABLE service_wakes RENAME COLUMN declared_cron TO declared;",
    "ALTER TABLE service_terms DROP COLUMN price_micros;
    ALTER TABLE service_calls DROP COLUMN price_micros;
    ALTER TABLE service_meters DROP COLUMN overage;",
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
