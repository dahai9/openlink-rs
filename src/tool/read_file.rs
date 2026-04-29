use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::time::Instant;

use serde_json::Value;

use super::{resolve_abs_path, get_arg_usize, truncate::MAX_LINES, truncate::MAX_BYTES, Tool, ToolContext, ToolResult};
use crate::security::sandbox;

pub struct ReadFileTool;

impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read file contents"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "path": "string (required) - file path to read",
            "offset": "number (optional) - start line number, 1-based (default: 1)",
            "limit": "number (optional) - max lines to read (default: 2000)"
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
        let offset = get_arg_usize(&ctx.args, "offset", 1).max(1);
        let limit = get_arg_usize(&ctx.args, "limit", MAX_LINES).min(MAX_LINES);

        let safe_path = match if path.starts_with('/') || path.starts_with('~') {
            resolve_abs_path(path, &ctx.config.root_dir)
        } else {
            sandbox::safe_path(&ctx.config.root_dir, path)
        } {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let file = match std::fs::File::open(&safe_path) {
            Ok(f) => f,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let reader = BufReader::new(file);
        let mut lines = Vec::new();
        let mut total_lines: usize = 0;
        let mut byte_count: usize = 0;
        let mut truncated = false;

        for line_result in reader.lines() {
            total_lines += 1;
            if total_lines < offset {
                continue;
            }
            if lines.len() >= limit {
                truncated = true;
                break;
            }
            let line = match line_result {
                Ok(l) => l,
                Err(e) => return ToolResult::error(e.to_string()),
            };
            byte_count += line.len() + 1;
            if byte_count > MAX_BYTES {
                truncated = true;
                break;
            }
            lines.push(line);
        }

        let output = if lines.is_empty() {
            "empty".to_string()
        } else {
            lines.join("\n")
        };

        ToolResult {
            status: "success",
            output: if truncated {
                let next_offset = offset + lines.len();
                format!(
                    "{}\n[truncated, {} total lines, use offset={} to continue]",
                    output, total_lines, next_offset
                )
            } else {
                output
            },
            error: String::new(),
            stop_stream: false,
            start_time: Instant::now(),
            end_time: Some(Instant::now()),
        }
    }
}
