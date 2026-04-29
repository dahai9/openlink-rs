use std::collections::HashMap;
use std::io::Write;
use std::time::Instant;

use serde_json::Value;

use super::{resolve_abs_path, Tool, ToolContext, ToolResult};
use crate::security::sandbox;

pub struct WriteFileTool;

impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to file"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "path": "string (required) - file path to write",
            "content": "string (required) - content to write",
            "mode": "string (optional) - 'append' or 'overwrite' (default)"
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
        let content = ctx.args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let mode = ctx.args.get("mode").and_then(|v| v.as_str()).unwrap_or("overwrite");

        let safe_path = match if path.starts_with('/') || path.starts_with('~') {
            resolve_abs_path(path, &ctx.config.root_dir)
        } else {
            sandbox::safe_path(&ctx.config.root_dir, path)
        } {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Create parent directories
        if let Some(parent) = safe_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return ToolResult::error(e.to_string());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755));
            }
        }

        let result = if mode == "append" {
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&safe_path)
                .and_then(|mut f| f.write_all(content.as_bytes()))
        } else {
            std::fs::write(&safe_path, content)
        };

        if let Err(e) = result {
            return ToolResult::error(e.to_string());
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&safe_path, std::fs::Permissions::from_mode(0o644));
        }

        ToolResult {
            status: "success",
            output: "写入成功".to_string(),
            error: String::new(),
            stop_stream: true,
            start_time: Instant::now(),
            end_time: Some(Instant::now()),
        }
    }
}
