//! `berm.sql`, over one SQLite file per program under `{root}/databases`, and
//! what each invocation holds of them.

use crate::{Service, logs, store};
use anyhow::{Result, bail};
use berm::{
    Callsite, Changes, Enter, Log, Program, Scope, Syscall,
    database::Held,
    syscall::{
        emit,
        sql::{self, Statement},
    },
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Weak},
};

/// Logs copied from an outbox per pass.
const SHIP: i64 = 256;

/// Whether `program` keeps a database.
pub(crate) fn keeps(program: &Program) -> bool {
    program.manifest().database.is_some()
}

/// `berm.sql`, for programs whose manifest keeps a database.
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
                true => database.read(at.chain, statement).await,
                false => database.exec(at.chain, statement).await,
            }
        }
    })
}

/// `berm.emit`: a program keeping a database emits into its outbox.
pub(crate) fn emit(service: Weak<Service>) -> Syscall {
    emit::programs(move |at: Callsite, log: Log| {
        let kept = kept(&service, &at.program);
        async move {
            match kept {
                Some(database) => {
                    database?.emit(at.chain, log).await?;
                    Ok(true)
                }
                None => Ok(false),
            }
        }
    })
}

/// `program`'s database, when it keeps one.
pub(crate) fn kept(
    service: &Weak<Service>,
    program: &str,
) -> Option<Result<Arc<berm::database::Database>>> {
    let service = service.upgrade()?;
    service
        .get(program)
        .filter(|deployed| keeps(deployed))
        .map(|_| service.databases.get(program))
}

/// Each invocation holds what [`berm::database::Databases::enter`] gives it. One
/// that took a writer, unless the chain's first, commits on its own when it
/// returns ok.
pub(crate) fn enter(service: Weak<Service>) -> Enter {
    Arc::new(move |program: &Program, at: &Callsite| {
        let service = service.upgrade();
        let entering = service
            .as_ref()
            .map(|service| service.databases.enter(program, at));
        let first = at.depth == 1;
        Box::pin(async move {
            let (Some(service), Some(entering)) = (service, entering) else {
                bail!("the service is shutting down");
            };
            let held = entering.await?;
            Ok(Box::new(Entered {
                service,
                held,
                first,
            }) as Box<dyn Scope>)
        })
    })
}

/// One invocation's hold.
struct Entered {
    service: Arc<Service>,
    held: Held,
    /// The chain's first call, whose changes the service commits.
    first: bool,
}

impl Scope for Entered {
    fn own(&self) -> bool {
        self.held.took().is_some()
    }

    fn close(
        self: Box<Self>,
        changes: Changes,
    ) -> Pin<Box<dyn Future<Output = Result<Changes>> + Send>> {
        Box::pin(async move {
            let Entered {
                service,
                held,
                first,
            } = *self;
            let took = held.took().map(str::to_owned);
            held.commit().await?;
            let Some(program) = took else {
                return Ok(changes);
            };
            ship(&service, &program).await?;
            if first {
                return Ok(changes);
            }
            logs::append(&service.root, &changes.logs)?;
            store::commit(&service.root, &changes.writes)?;
            if !changes.logs.is_empty() {
                service.deliver();
            }
            Ok(Changes::default())
        })
    }
}

/// Append what waits in `program`'s outbox to its logs, and drop it from the
/// outbox. A crash in between appends those logs again.
pub(crate) async fn ship(service: &Service, program: &str) -> Result<()> {
    let _shipping = service.shipping.lock().await;
    let database = service.databases.get(program)?;
    let mut shipped = false;
    loop {
        let waiting = database.outbox(SHIP).await?;
        let Some(last) = waiting.last().map(|log| log.id) else {
            break;
        };
        let logs: Vec<Log> = waiting
            .into_iter()
            .map(|log| Log {
                program: program.into(),
                topics: log.topics,
                data: log.data,
            })
            .collect();
        logs::append(&service.root, &logs)?;
        database.shipped(last).await?;
        shipped = true;
    }
    if shipped {
        service.deliver();
    }
    Ok(())
}
