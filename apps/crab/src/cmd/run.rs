//! `crab run` — run one of a program's tools.

use crate::utils;
use anyhow::{Context, Result};
use berm_api::{Dry, DryRun, Output, host::Caller};
use crabtalk_sdk::client::Client;
use std::{
    io::{IsTerminal, Read},
    process::ExitCode,
};

/// A program that ran and said no, which is not berm failing to run it — the
/// two leave by different doors so a script can tell them apart.
const FAILED: u8 = 2;

/// Run `tool` of `program`, dry when `dry` is set.
pub fn run(
    client: &Client,
    program: &str,
    tool: &str,
    inline: Option<&str>,
    caller: Option<&Caller>,
    dry: Option<Dry>,
) -> Result<ExitCode> {
    let arguments = arguments(inline)?;
    let (output, meter) = match dry {
        Some(dry) => {
            // Arguments that are not JSON go across as their text.
            let args = serde_json::from_slice(&arguments).unwrap_or_else(|_| {
                serde_json::Value::String(String::from_utf8_lossy(&arguments).into_owned())
            });
            let args = match arguments.is_empty() {
                true => serde_json::Value::Null,
                false => args,
            };
            utils::wait(client.dry(program, tool, &DryRun { args, dry }, caller))?
        }
        None => utils::wait(client.run(program, tool, arguments, caller))?,
    };
    if let Some(meter) = meter {
        eprintln!(
            "{} instructions, {} rows read, {} written, {} KiB memory, {} bytes proxied",
            meter.instructions,
            meter.read,
            meter.written,
            meter.memory / 1024,
            meter.proxied
        );
    }
    match output {
        Output::Done(result) => {
            println!("{result}");
            Ok(ExitCode::SUCCESS)
        }
        Output::Failed(failure) => {
            eprintln!("{program}.{tool} failed: {failure}");
            Ok(ExitCode::from(FAILED))
        }
    }
}

/// What was typed, what was piped in, or nothing — which is what a program
/// already sees from MCP when a call carries no arguments.
///
/// Trimmed, because a shell's trailing newline is not part of the argument
/// object: `echo '{}' | crab run` has to reach the program as the same bytes
/// typing it does, and as the compact object MCP sends.
fn arguments(inline: Option<&str>) -> Result<Vec<u8>> {
    let raw = match inline {
        Some(arguments) => arguments.as_bytes().to_vec(),
        None => {
            let stdin = std::io::stdin();
            if stdin.is_terminal() {
                return Ok(Vec::new());
            }
            let mut piped = Vec::new();
            stdin
                .lock()
                .read_to_end(&mut piped)
                .context("cannot read arguments from stdin")?;
            piped
        }
    };
    Ok(raw.trim_ascii().to_vec())
}
