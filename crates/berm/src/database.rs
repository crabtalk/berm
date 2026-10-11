//! A program's own SQLite database, for a host serving `berm.sql`.
//!
//! One file per program. A call that may write holds the database's writer for
//! its whole run, inside one transaction ([`Database::write`]); a `query` reads
//! the last commit on a connection of its own ([`Database::read`]). The
//! program's SQL cannot reach the tables named `_berm_*`, where the host keeps
//! the program's `berm.get`/`berm.set` state, the logs it emitted until the host
//! takes them ([`Database::outbox`]), and the migrations applied.

use crate::{Log, Refused, Write, syscall::sql::Statement};
use anyhow::{Context, Result, anyhow, bail};
use berm_api::Migration;
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, StatementStatus,
    fallible_iterator::FallibleIterator,
    hooks::{AuthAction, AuthContext, Authorization},
    types::{Value, ValueRef},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use tokio::sync::{Mutex, OwnedMutexGuard, oneshot};

/// The largest reply one statement may answer with, in bytes.
const MAX_REPLY: usize = 4 << 20;

/// What names the host's own tables.
const RESERVED: &str = "_berm_";

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS _berm_state (key TEXT PRIMARY KEY, value BLOB NOT NULL);
    CREATE TABLE IF NOT EXISTS _berm_outbox (
        id INTEGER PRIMARY KEY AUTOINCREMENT, topics TEXT NOT NULL, data TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS _berm_migrations (
        name TEXT PRIMARY KEY, sql TEXT NOT NULL,
        applied_at INTEGER NOT NULL DEFAULT (unixepoch()));";

/// Work for the writer's thread.
type Job = Box<dyn FnOnce(&mut Writer) + Send>;

/// The one connection that writes, and whether the statement it is running is
/// the program's.
struct Writer {
    conn: Connection,
    guest: Arc<AtomicBool>,
}

/// One program's database.
pub struct Database {
    path: PathBuf,
    /// The writer's thread, which owns the writing connection.
    jobs: mpsc::Sender<Job>,
    /// Held by the call writing, a migration, or taking the outbox.
    writing: Arc<Mutex<()>>,
}

/// A log the program emitted, waiting in the outbox.
#[derive(Debug)]
pub struct Outboxed {
    pub id: i64,
    pub topics: Vec<String>,
    pub data: String,
}

impl Database {
    /// Open the database at `path`, creating it and its directory if needed.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(SCHEMA)?;
        let guest = Arc::new(AtomicBool::new(false));
        guard(&conn, guest.clone())?;
        let mut writer = Writer { conn, guest };
        let (jobs, queue) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("berm-sqlite".to_owned())
            .spawn(move || {
                for job in queue {
                    job(&mut writer);
                }
            })?;
        Ok(Self {
            path: path.to_owned(),
            jobs,
            writing: Arc::new(Mutex::new(())),
        })
    }

    /// Take the writer and open a transaction, held until the [`Writing`] is
    /// committed or dropped. A dropped one rolls back.
    pub async fn write(&self) -> Result<Writing> {
        let held = self.writing.clone().lock_owned().await;
        on(&self.jobs, |w| {
            Ok(w.conn.execute_batch("BEGIN IMMEDIATE")?)
        })
        .await?;
        Ok(Writing {
            jobs: self.jobs.clone(),
            _held: held,
            open: true,
        })
    }

    /// Run the program's statement inside the open [`Writing`].
    pub async fn exec(&self, statement: Statement) -> Result<String> {
        on(&self.jobs, move |w| {
            if w.conn.is_autocommit() {
                bail!("no call holds this database's writer");
            }
            run(&w.conn, &w.guest, &statement, false)
        })
        .await
    }

    /// Run the program's statement against the last commit, refusing one that
    /// writes.
    pub async fn read(&self, statement: Statement) -> Result<String> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let (conn, guest) = reader(&path)?;
            run(&conn, &guest, &statement, true)
        })
        .await?
    }

    /// The committed value of the program's `key`.
    pub async fn get(&self, key: String) -> Result<Option<Vec<u8>>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let (conn, _) = reader(&path)?;
            Ok(conn
                .query_row(
                    "SELECT value FROM _berm_state WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .optional()?)
        })
        .await?
    }

    /// Apply the migrations not yet applied, in one transaction, answering
    /// their names. Refused, with nothing applied, when the applied ones are not
    /// the first of `migrations`, unchanged, or when one fails.
    pub async fn migrate(&self, migrations: Vec<Migration>) -> Result<Vec<String>> {
        let _held = self.writing.clone().lock_owned().await;
        on(&self.jobs, move |w| {
            let tx = w.conn.transaction()?;
            let applied: Vec<(String, String)> = tx
                .prepare("SELECT name, sql FROM _berm_migrations ORDER BY rowid")?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            for (at, (name, sql)) in applied.iter().enumerate() {
                match migrations.get(at) {
                    Some(next) if next.name == *name && next.sql == *sql => {}
                    Some(next) if next.name == *name => {
                        bail!("migration {name} was applied and has changed since")
                    }
                    _ => bail!(
                        "migration {name} was applied and is not migration {} any more",
                        at + 1
                    ),
                }
            }
            let mut ran = Vec::new();
            for next in &migrations[applied.len()..] {
                w.guest.store(true, Ordering::SeqCst);
                let applying = tx.execute_batch(&next.sql);
                w.guest.store(false, Ordering::SeqCst);
                applying.with_context(|| format!("migration {} failed", next.name))?;
                tx.execute(
                    "INSERT INTO _berm_migrations (name, sql) VALUES (?1, ?2)",
                    (&next.name, &next.sql),
                )?;
                ran.push(next.name.clone());
            }
            tx.commit()?;
            Ok(ran)
        })
        .await
    }

    /// Up to `limit` logs waiting in the outbox, oldest first.
    pub async fn outbox(&self, limit: i64) -> Result<Vec<Outboxed>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let (conn, _) = reader(&path)?;
            let rows: Vec<(i64, String, String)> = conn
                .prepare("SELECT id, topics, data FROM _berm_outbox ORDER BY id LIMIT ?1")?
                .query_map([limit], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<_, _>>()?;
            rows.into_iter()
                .map(|(id, topics, data)| {
                    Ok(Outboxed {
                        id,
                        topics: serde_json::from_str(&topics)?,
                        data,
                    })
                })
                .collect()
        })
        .await?
    }

    /// Drop the outbox's logs up to `id`, once the host has them.
    pub async fn shipped(&self, id: i64) -> Result<()> {
        let _held = self.writing.clone().lock_owned().await;
        on(&self.jobs, move |w| {
            w.conn
                .execute("DELETE FROM _berm_outbox WHERE id <= ?1", [id])?;
            Ok(())
        })
        .await
    }
}

/// A call's hold on the writer and its open transaction.
pub struct Writing {
    jobs: mpsc::Sender<Job>,
    _held: OwnedMutexGuard<()>,
    open: bool,
}

impl Writing {
    /// Commit the call's statements with its `berm.set` writes and its logs,
    /// the logs into the outbox.
    pub async fn commit(mut self, writes: Vec<Write>, logs: Vec<Log>) -> Result<()> {
        let committed = on(&self.jobs, move |w| {
            for write in &writes {
                w.conn.execute(
                    "INSERT INTO _berm_state (key, value) VALUES (?1, ?2)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    (&write.key, &write.value),
                )?;
            }
            for log in &logs {
                w.conn.execute(
                    "INSERT INTO _berm_outbox (topics, data) VALUES (?1, ?2)",
                    (serde_json::to_string(&log.topics)?, &log.data),
                )?;
            }
            w.conn.execute_batch("COMMIT")?;
            Ok(())
        })
        .await;
        if committed.is_ok() {
            self.open = false;
        }
        committed
    }
}

impl Drop for Writing {
    fn drop(&mut self) {
        if self.open {
            let _ = self.jobs.send(Box::new(|w| {
                if !w.conn.is_autocommit() {
                    let _ = w.conn.execute_batch("ROLLBACK");
                }
            }));
        }
    }
}

/// Run `job` on the writer's thread.
async fn on<T: Send + 'static>(
    jobs: &mpsc::Sender<Job>,
    job: impl FnOnce(&mut Writer) -> Result<T> + Send + 'static,
) -> Result<T> {
    let (answer, answered) = oneshot::channel();
    jobs.send(Box::new(move |w| {
        let _ = answer.send(job(w));
    }))
    .map_err(|_| anyhow!("the database is closed"))?;
    answered
        .await
        .map_err(|_| anyhow!("the database is closed"))?
}

/// A connection that only reads, for one `query` or host read.
fn reader(path: &Path) -> Result<(Connection, Arc<AtomicBool>)> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", path.display()))?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let guest = Arc::new(AtomicBool::new(false));
    guard(&conn, guest.clone())?;
    Ok((conn, guest))
}

/// While `guest` is set, refuse transaction control, `ATTACH`, `DETACH`, a
/// `PRAGMA` that sets a value, and anything reaching a `_berm_*` table.
fn guard(conn: &Connection, guest: Arc<AtomicBool>) -> Result<()> {
    conn.authorizer(Some(move |context: AuthContext<'_>| {
        if !guest.load(Ordering::SeqCst) {
            return Authorization::Allow;
        }
        let table = match context.action {
            AuthAction::Transaction { .. }
            | AuthAction::Savepoint { .. }
            | AuthAction::Attach { .. }
            | AuthAction::Detach { .. }
            | AuthAction::Pragma {
                pragma_value: Some(_),
                ..
            } => return Authorization::Deny,
            AuthAction::CreateIndex { table_name, .. }
            | AuthAction::CreateTable { table_name, .. }
            | AuthAction::CreateTempIndex { table_name, .. }
            | AuthAction::CreateTempTable { table_name, .. }
            | AuthAction::CreateTempTrigger { table_name, .. }
            | AuthAction::CreateTrigger { table_name, .. }
            | AuthAction::Delete { table_name, .. }
            | AuthAction::DropIndex { table_name, .. }
            | AuthAction::DropTable { table_name, .. }
            | AuthAction::DropTempIndex { table_name, .. }
            | AuthAction::DropTempTable { table_name, .. }
            | AuthAction::DropTempTrigger { table_name, .. }
            | AuthAction::DropTrigger { table_name, .. }
            | AuthAction::Insert { table_name, .. }
            | AuthAction::Read { table_name, .. }
            | AuthAction::Update { table_name, .. }
            | AuthAction::AlterTable { table_name, .. }
            | AuthAction::Analyze { table_name, .. }
            | AuthAction::CreateVtable { table_name, .. }
            | AuthAction::DropVtable { table_name, .. } => table_name,
            _ => return Authorization::Allow,
        };
        match table.starts_with(RESERVED) {
            true => Authorization::Deny,
            false => Authorization::Allow,
        }
    }))?;
    Ok(())
}

/// Run one of the program's statements and answer the reply's JSON.
fn run(
    conn: &Connection,
    guest: &AtomicBool,
    statement: &Statement,
    read_only: bool,
) -> Result<String> {
    let params: Vec<serde_json::Value> =
        serde_json::from_str(&statement.params).context("the parameters are not a JSON array")?;
    guest.store(true, Ordering::SeqCst);
    let ran = (|| {
        let mut batch = rusqlite::Batch::new(conn, &statement.sql);
        let Some(mut prepared) = batch.next()? else {
            bail!("there is no statement to run");
        };
        if batch.next()?.is_some() {
            return Err(Refused("one statement per call".to_owned()).into());
        }
        if read_only && !prepared.readonly() {
            return Err(Refused("a query cannot write".to_owned()).into());
        }
        let columns: Vec<String> = prepared
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();
        let mut rows = Vec::new();
        let mut size = 0;
        let mut found =
            prepared.query(rusqlite::params_from_iter(params.into_iter().map(value)))?;
        while let Some(row) = found.next()? {
            let mut out = Vec::with_capacity(columns.len());
            for at in 0..columns.len() {
                let cell = json(row.get_ref(at)?);
                size += cell.to_string().len();
                out.push(cell);
            }
            if size > MAX_REPLY {
                bail!("the statement answered more than {MAX_REPLY} bytes");
            }
            rows.push(out);
        }
        drop(found);
        let read = prepared.get_status(StatementStatus::RowsRead);
        let written = prepared.get_status(StatementStatus::RowsWritten);
        Ok(serde_json::json!({ "columns": columns, "rows": rows, "read": read, "written": written })
            .to_string())
    })();
    guest.store(false, Ordering::SeqCst);
    ran
}

/// A JSON parameter as SQLite binds it: arrays and objects as their JSON text.
fn value(param: serde_json::Value) -> Value {
    match param {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(flag) => Value::Integer(flag as i64),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(integer) => Value::Integer(integer),
            None => Value::Real(number.as_f64().unwrap_or_default()),
        },
        serde_json::Value::String(text) => Value::Text(text),
        other => Value::Text(other.to_string()),
    }
}

/// A cell as JSON: a blob as its bytes.
fn json(cell: ValueRef<'_>) -> serde_json::Value {
    match cell {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(integer) => integer.into(),
        ValueRef::Real(real) => real.into(),
        ValueRef::Text(text) => String::from_utf8_lossy(text).into_owned().into(),
        ValueRef::Blob(bytes) => bytes.to_vec().into(),
    }
}
