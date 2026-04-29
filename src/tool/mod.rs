pub mod edit;
pub mod exec_cmd;
pub mod glob;
pub mod grep;
pub mod invalid;
pub mod list_dir;
pub mod question;
pub mod read_file;
pub mod skill_tool;
pub mod todo_write;
pub mod truncate;
pub mod web_fetch;
pub mod write_file;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::Value;

use crate::config::Config;
use crate::security::sandbox;

pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String>;
    fn execute(&self, ctx: &ToolContext) -> ToolResult;
}

pub struct ToolContext {
    pub args: HashMap<String, Value>,
    pub config: Arc<Config>,
}

pub struct ToolResult {
    pub status: &'static str,
    pub output: String,
    pub error: String,
    pub stop_stream: bool,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
}

impl ToolResult {
    pub fn success() -> Self {
        Self {
            status: "success",
            output: String::new(),
            error: String::new(),
            stop_stream: false,
            start_time: Instant::now(),
            end_time: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            status: "error",
            output: String::new(),
            error: msg.into(),
            stop_stream: false,
            start_time: Instant::now(),
            end_time: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

pub struct Registry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<()> {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            bail!("tool '{}' already registered", name);
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<ToolInfo> {
        self.tools
            .values()
            .map(|t| ToolInfo {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: Some(t.parameters()),
            })
            .collect()
    }
}

/// Resolves an absolute path against root_dir + allowed home sub-roots.
pub fn resolve_abs_path(path: &str, root_dir: &Path) -> Result<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let claude_dir = home.join(".claude");
    let openlink_dir = home.join(".openlink");
    let agent_dir = home.join(".agent");
    let allowed: Vec<&Path> = vec![
        root_dir,
        claude_dir.as_path(),
        openlink_dir.as_path(),
        agent_dir.as_path(),
    ];
    sandbox::safe_abs_path(path, &allowed)
}

/// Helper to extract a string arg from a value map, trying multiple keys.
pub fn get_arg_string<'a>(args: &'a HashMap<String, Value>, key1: &str, key2: &str) -> Option<&'a str> {
    args.get(key1)
        .and_then(|v| v.as_str())
        .or_else(|| args.get(key2).and_then(|v| v.as_str()))
}

/// Helper to extract a numeric arg as usize, defaulting to `default`.
pub fn get_arg_usize(args: &HashMap<String, Value>, key: &str, default: usize) -> usize {
    args.get(key)
        .and_then(|v| {
            if let Some(n) = v.as_u64() {
                Some(n as usize)
            } else {
                v.as_f64().map(|n| n as usize)
            }
        })
        .unwrap_or(default)
}
