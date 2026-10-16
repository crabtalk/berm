//! A program's own SQLite database, for a host serving `berm.sql`.
//!
//! One file per program, under the directory [`Databases`] opens them in. An
//! invocation of a program keeping one, unless a query, holds the database's
//! writer for its whole run inside one transaction, which its program's SQL and
//! `berm.set` write into. A call back into a program the chain
//! already holds runs inside that transaction. A `query` reads the last commit
//! on a connection of its own, or through the writer when its chain holds it.
//! The program's SQL cannot reach the tables named `_berm_*`, where the host
//! keeps the program's `berm.get`/`berm.set` state and the migrations applied.

use crate::{Callsite, Program, Refused, syscall::sql::Statement};
use anyhow::{Context, Result, anyhow, bail};
use berm_api::Migration;
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, StatementStatus,
    fallible_iterator::FallibleIterator,
    hooks::{AuthAction, AuthContext, Authorization},
    types::{Value, ValueRef},
};
use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc, PoisonError,
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

/// The savepoint a nested call takes on each database its chain holds.
const SAVEPOINT: &str = "_berm_call";

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS _berm_state (key TEXT PRIMARY KEY, value BLOB NOT NULL);
    DROP TABLE IF EXISTS _berm_outbox;
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

/// Which chain holds each database's writer, and which database each chain is
/// waiting for the writer of.
#[derive(Default)]
struct Graph {
    holders: HashMap<Arc<str>, u64>,
    waits: HashMap<u64, Arc<str>>,
}

/// Every program's database, opened on first use at `{dir}/{program}.sqlite`.
pub struct Databases {
    dir: PathBuf,
    open: std::sync::Mutex<HashMap<String, Arc<Database>>>,
    graph: Arc<std::sync::Mutex<Graph>>,
}

impl Databases {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            open: Default::default(),
            graph: Default::default(),
        }
    }

    /// `program`'s database.
    pub fn get(&self, program: &str) -> Result<Arc<Database>> {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(database) = open.get(program) {
            return Ok(database.clone());
        }
        let database = Arc::new(Database::open(
            &self.dir.join(format!("{program}.sqlite")),
            program.into(),
            self.graph.clone(),
        )?);
        open.insert(program.to_owned(), database.clone());
        Ok(database)
    }

    /// What an invocation of `program` at `at` holds: a savepoint on each
    /// database its chain holds the writer of, and, for a program keeping a
    /// database whose writer the chain does not hold, unless a query, that
    /// writer. Waiting for a writer ends at the chain's deadline, and is
    /// refused when the chain holding it waits, through others or itself, on
    /// one this chain holds.
    pub fn enter(
        &self,
        program: &Program,
        at: &Callsite,
    ) -> impl Future<Output = Result<Held>> + Send + 'static {
        let mut held: Vec<Arc<Database>> = Vec::new();
        let mut taking = Ok(None);
        if !at.query {
            let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
            for name in &at.stack[..at.stack.len() - 1] {
                if let Some(database) = open.get(&**name)
                    && database.holds(at.chain)
                    && !held.iter().any(|kept| Arc::ptr_eq(kept, database))
                {
                    held.push(database.clone());
                }
            }
            drop(open);
            if program.manifest().database.is_some() {
                taking = self
                    .get(&program.name)
                    .map(|database| Some(database).filter(|database| !database.holds(at.chain)));
            }
        }
        let (chain, deadline) = (at.chain, at.deadline);
        async move {
            let taking = taking?;
            let mut entered = Held {
                writing: None,
                marked: Vec::with_capacity(held.len()),
            };
            for database in held {
                on(&database.jobs, |w| {
                    Ok(w.conn.execute_batch(&format!("SAVEPOINT {SAVEPOINT}"))?)
                })
                .await?;
                entered.marked.push(database);
            }
            if let Some(database) = taking {
                entered.writing = Some(database.write(chain, deadline).await?);
            }
            Ok(entered)
        }
    }
}

/// One program's database.
pub struct Database {
    path: PathBuf,
    /// The program it is the database of.
    name: Arc<str>,
    /// The writer's thread, which owns the writing connection.
    jobs: mpsc::Sender<Job>,
    /// Held by the call writing or a migration.
    writing: Arc<Mutex<()>>,
    graph: Arc<std::sync::Mutex<Graph>>,
}

impl Database {
    /// Open the database at `path`, creating it and its directory if needed.
    fn open(path: &Path, name: Arc<str>, graph: Arc<std::sync::Mutex<Graph>>) -> Result<Self> {
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
            name,
            jobs,
            writing: Arc::new(Mutex::new(())),
            graph,
        })
    }

    /// Whether `chain` holds the writer.
    fn holds(&self, chain: u64) -> bool {
        self.graph
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .holders
            .get(&self.name)
            == Some(&chain)
    }

    /// Take the writer for `chain` and open a transaction, held until the
    /// [`Writing`] is committed or dropped. A dropped one rolls back.
    async fn write(&self, chain: u64, deadline: std::time::Instant) -> Result<Writing> {
        let waiting = {
            let mut graph = self.graph.lock().unwrap_or_else(PoisonError::into_inner);
            let mut wanted = self.name.clone();
            for _ in 0..=graph.waits.len() {
                let Some(&holder) = graph.holders.get(&wanted) else {
                    break;
                };
                if holder == chain {
                    return Err(Refused(format!(
                        "{}'s database is held by a chain waiting on this one",
                        self.name
                    ))
                    .into());
                }
                let Some(next) = graph.waits.get(&holder) else {
                    break;
                };
                wanted = next.clone();
            }
            graph.waits.insert(chain, self.name.clone());
            Waiting {
                graph: self.graph.clone(),
                chain,
            }
        };
        let taken = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            self.writing.clone().lock_owned(),
        )
        .await;
        drop(waiting);
        let Ok(held) = taken else {
            bail!(
                "the call's deadline passed waiting for {}'s database",
                self.name
            );
        };
        self.graph
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .holders
            .insert(self.name.clone(), chain);
        let writing = Writing {
            jobs: self.jobs.clone(),
            name: self.name.clone(),
            graph: self.graph.clone(),
            _held: held,
            open: true,
        };
        on(&self.jobs, |w| {
            Ok(w.conn.execute_batch("BEGIN IMMEDIATE")?)
        })
        .await?;
        Ok(writing)
    }

    /// Run the program's statement inside the transaction `chain` holds.
    pub async fn exec(&self, chain: u64, statement: Statement) -> Result<Answer> {
        self.held(chain)?;
        on(&self.jobs, move |w| {
            run(&w.conn, &w.guest, &statement, false)
        })
        .await
    }

    /// Run the program's statement, refusing one that writes: inside the
    /// transaction `chain` holds, or against the last commit.
    pub async fn read(&self, chain: u64, statement: Statement) -> Result<Answer> {
        if self.holds(chain) {
            return on(&self.jobs, move |w| {
                run(&w.conn, &w.guest, &statement, true)
            })
            .await;
        }
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let (conn, guest) = reader(&path)?;
            run(&conn, &guest, &statement, true)
        })
        .await?
    }

    /// The program's `key`: inside the transaction `chain` holds, or as last
    /// committed.
    pub async fn get(&self, chain: u64, key: String) -> Result<Option<Vec<u8>>> {
        let state = |conn: &Connection, key: String| -> Result<Option<Vec<u8>>> {
            Ok(conn
                .query_row(
                    "SELECT value FROM _berm_state WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .optional()?)
        };
        if self.holds(chain) {
            return on(&self.jobs, move |w| state(&w.conn, key)).await;
        }
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let (conn, _) = reader(&path)?;
            state(&conn, key)
        })
        .await?
    }

    /// Set the program's `key` inside the transaction `chain` holds.
    pub async fn set(&self, chain: u64, key: String, value: Vec<u8>) -> Result<()> {
        self.held(chain)?;
        on(&self.jobs, move |w| {
            w.conn.execute(
                "INSERT INTO _berm_state (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                (&key, &value),
            )?;
            Ok(())
        })
        .await
    }

    /// Refuse a write from a chain not holding the writer.
    fn held(&self, chain: u64) -> Result<()> {
        match self.holds(chain) {
            true => Ok(()),
            false => bail!("this chain does not hold {}'s database", self.name),
        }
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
}

/// What one invocation holds, from [`Databases::enter`]. Dropped without
/// [`Held::commit`], it rolls back the writer it took and every savepoint.
pub struct Held {
    writing: Option<Writing>,
    /// The databases held before the invocation began, each under a savepoint.
    marked: Vec<Arc<Database>>,
}

impl Held {
    /// The program whose writer the invocation took, if it took one.
    pub fn took(&self) -> Option<&str> {
        self.writing.as_ref().map(|writing| &*writing.name)
    }

    /// Commit the writer the invocation took, and release every savepoint.
    pub async fn commit(mut self) -> Result<()> {
        if let Some(writing) = self.writing.take() {
            writing.commit().await?;
        }
        for database in std::mem::take(&mut self.marked) {
            on(&database.jobs, |w| {
                Ok(w.conn.execute_batch(&format!("RELEASE {SAVEPOINT}"))?)
            })
            .await?;
        }
        Ok(())
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        for database in &self.marked {
            let _ = database.jobs.send(Box::new(|w| {
                let _ = w
                    .conn
                    .execute_batch(&format!("ROLLBACK TO {SAVEPOINT}; RELEASE {SAVEPOINT}"));
            }));
        }
    }
}

/// A chain's hold on the writer and its open transaction.
struct Writing {
    jobs: mpsc::Sender<Job>,
    name: Arc<str>,
    graph: Arc<std::sync::Mutex<Graph>>,
    _held: OwnedMutexGuard<()>,
    open: bool,
}

impl Writing {
    async fn commit(mut self) -> Result<()> {
        on(&self.jobs, |w| Ok(w.conn.execute_batch("COMMIT")?)).await?;
        self.open = false;
        Ok(())
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
        self.graph
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .holders
            .remove(&self.name);
    }
}

/// A chain's place in the waits of the [`Graph`], taken out when dropped.
struct Waiting {
    graph: Arc<std::sync::Mutex<Graph>>,
    chain: u64,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.graph
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .waits
            .remove(&self.chain);
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

/// What one statement answered: the reply's JSON, and the rows it read and
/// wrote.
pub struct Answer {
    pub json: String,
    pub read: u64,
    pub written: u64,
}

/// Run one of the program's statements.
fn run(
    conn: &Connection,
    guest: &AtomicBool,
    statement: &Statement,
    read_only: bool,
) -> Result<Answer> {
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
        let read = prepared.get_status(StatementStatus::RowsRead) as u64;
        let written = prepared.get_status(StatementStatus::RowsWritten) as u64;
        Ok(Answer {
            json: serde_json::json!({ "columns": columns, "rows": rows, "read": read, "written": written })
                .to_string(),
            read,
            written,
        })
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
