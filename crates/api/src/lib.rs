//! The wire types bermd's control API speaks.
//!
//! Here rather than in the service so a client can talk to it without linking a
//! compiler, and so the two cannot disagree about the shape.

use serde::{Deserialize, Serialize};

/// A deployed harness, as the control API reports it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Harness {
    pub name: String,
    /// sha256 of the ELF.
    pub digest: String,
    /// When to reach for this harness's tools, and how they go together.
    pub usage: String,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's arguments, as the model receives it.
    pub parameters: serde_json::Value,
}

/// What the control API returns instead of a resource when it refuses one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failed {
    pub error: String,
}
