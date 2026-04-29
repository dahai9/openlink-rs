use std::collections::HashMap;
use std::time::Instant;

use serde_json::Value;

use super::{resolve_abs_path, Tool, ToolContext, ToolResult};
use crate::security::sandbox;

pub struct ListDirTool;

impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List directory contents"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "path": "string (required) - directory path to list"
        })
    }

    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        match args.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => Ok(()),
            _ => Err("path is required".to_string()),
        }
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let path = ctx.args.get("path").and_then(|v| v.as_str()).unwrap_or("");

        let safe_path = match if path.starts_with('/') || path.starts_with('~') {
            resolve_abs_path(path, &ctx.config.root_dir)
        } else {
            sandbox::safe_path(&ctx.config.root_dir, path)
        } {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let entries = match std::fs::read_dir(&safe_path) {
            Ok(e) => e,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let mut names = Vec::new();
        for entry in entries.flatten() {
            let mut name = entry.file_name().to_string_lossy().to_string();
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                name.push('/');
            }
            names.push(name);
        }

        let output = if names.is_empty() {
            "empty".to_string()
        } else {
            names.join("\n")
        };

        ToolResult {
            status: "success",
            output,
            error: String::new(),
            stop_stream: false,
            start_time: Instant::now(),
            end_time: Some(Instant::now()),
        }
    }
}
