//! `berm.sql`, over one SQLite file per program under `{root}/databases`.

use crate::{Service, logs, store};
use anyhow::{Result, anyhow, bail};
use berm::{
    Callsite, Changes, Commit, Opening, Own, Program, Syscall,
    database::{Database, Writing},
    syscall::sql::{self, Statement},
};
use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
};

/// Each program's database, opened on first use.
pub(crate) struct Databases {
    root: PathBuf,
    open: Mutex<HashMap<String, Arc<Database>>>,
}

impl Databases {
    pub(crate) fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.join("databases"),
            open: Mutex::new(HashMap::new()),
        }
    }

    /// `program`'s database.
    pub(crate) fn get(&self, program: &str) -> Result<Arc<Database>> {
        let mut open = self
            .open
            .lock()
            .map_err(|_| anyhow!("databases poisoned"))?;
        if let Some(database) = open.get(program) {
            return Ok(database.clone());
        }
        let database = Arc::new(Database::open(
            &self.root.join(format!("{program}.sqlite")),
        )?);
        open.insert(program.to_owned(), database.clone());
        Ok(database)
    }
}

/// Whether `program` keeps a database.
pub(crate) fn keeps(program: &Program) -> bool {
    program.manifest().database.is_some()
}

/// `berm.sql`, for programs whose manifest keeps a database: a `query`
/// reads the last commit, any other call runs inside its own transaction.
pub(crate) fn syscall(service: Weak<Service>) -> Syscall {
    sql::programs(move |at: Callsite, statement: Statement| {
        let service = service.upgrade();
        async move {
            let Some(service) = service else {
                bail!("the service is shutting down");
            };
            if !service
                .get(&at.program)
                .is_some_and(|program| keeps(&program))
            {
                bail!("{} keeps no database", at.program);
            }
            let database = service.databases.get(&at.program)?;
            match at.query {
                true => database.read(statement).await,
                false => database.exec(statement).await,
            }
        }
    })
}

/// A nested call into a program keeping a database, unless a query, commits on
/// its own: it holds that database's writer, and its writes and logs are
/// committed when it returns ok.
pub(crate) fn own(service: Weak<Service>) -> Own {
    Arc::new(move |program: &Arc<Program>, _tool: &str, query: bool| {
        if query || !keeps(program) {
            return None;
        }
        let service = service.upgrade()?;
        let name = program.name.clone();
        let opening: Opening = Box::pin(async move {
            let writing = service.databases.get(&name)?.write().await?;
            Ok(Box::new(Nested {
                service,
                name,
                writing,
            }) as Box<dyn Commit>)
        });
        Some(opening)
    })
}

/// A nested call's hold on its program's database.
struct Nested {
    service: Arc<Service>,
    name: Arc<str>,
    writing: Writing,
}

impl Commit for Nested {
    fn commit(
        self: Box<Self>,
        changes: Changes,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(async move {
            let Nested {
                service,
                name,
                writing,
            } = *self;
            logs::append(&service.root, &changes.logs)?;
            let (own, rest): (Vec<_>, Vec<_>) = changes
                .writes
                .into_iter()
                .partition(|write| write.program == name);
            store::commit(&service.root, &rest)?;
            writing.commit(own, Vec::new()).await?;
            if !changes.logs.is_empty() {
                service.deliver();
            }
            Ok(())
        })
    }
}
