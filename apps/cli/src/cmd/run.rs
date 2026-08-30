//! `berm run` — run one of a program's tools.

use crate::Client;
use anyhow::{Context, Result};
use berm_api::Output;
use std::{
    io::{IsTerminal, Read},
    process::ExitCode,
};

/// A program that ran and said no, which is not berm failing to run it — the
/// two leave by different doors so a script can tell them apart.
const FAILED: u8 = 2;

pub fn run(client: &Client, program: &str, tool: &str, inline: Option<&str>) -> Result<ExitCode> {
    match client.run(program, tool, arguments(inline)?)? {
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
/// object: `echo '{}' | berm run` has to reach the program as the same bytes
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
