//! `berm.sql`, over one SQLite file per program under `{root}/databases`.

use crate::Service;
use anyhow::{Result, anyhow, bail};
use berm::{
    Callsite, Program, Syscall,
    database::Database,
    syscall::sql::{self, Statement},
};
use std::{
    collections::HashMap,
    path::PathBuf,
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
