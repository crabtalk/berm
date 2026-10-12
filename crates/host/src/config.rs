//! How a host is sized.

use berm::syscall::call::DEFAULT_CALL_DEPTH;
use serde::Deserialize;
use std::path::PathBuf;

/// A missing field takes its default.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Guests live at once across every service, nested calls included.
    #[serde(default = "instances")]
    pub instances: u32,
    /// The most wasm instructions one invocation may execute.
    #[serde(default = "fuel")]
    pub fuel: u64,
    /// The most rows one invocation may read and write together.
    #[serde(default = "rows")]
    pub rows: u64,
    /// How deep a chain of services calling services may go.
    #[serde(default = "depth")]
    pub depth: u32,
    /// Where compiled code is cached between processes.
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,
    /// Where the databases of the services that keep one live.
    #[serde(default = "databases")]
    pub databases: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            instances: instances(),
            fuel: fuel(),
            rows: rows(),
            depth: depth(),
            cache_dir: None,
            databases: databases(),
        }
    }
}

fn instances() -> u32 {
    berm::Config::default().instances
}

fn fuel() -> u64 {
    berm::Config::default().fuel
}

fn rows() -> u64 {
    berm::Config::default().rows
}

fn depth() -> u32 {
    DEFAULT_CALL_DEPTH
}

fn databases() -> PathBuf {
    PathBuf::from("databases")
}
