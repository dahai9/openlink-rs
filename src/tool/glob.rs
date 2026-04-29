use std::collections::HashMap;
use std::time::Instant;

use serde_json::Value;
use walkdir::WalkDir;

use super::{resolve_abs_path, Tool, ToolContext, ToolResult};
use crate::security::sandbox;

pub struct GlobTool;

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "pattern": "string (required) - glob pattern, e.g. **/*.go or *.ts",
            "path": "string (optional) - directory to search in (default: root)"
        })
    }

    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        match args.get("pattern").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => Ok(()),
            _ => Err("pattern is required".to_string()),
        }
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let pattern = ctx
            .args
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let search_path = ctx
            .args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let safe_path = match if search_path.starts_with('/') || search_path.starts_with('~') {
            resolve_abs_path(search_path, &ctx.config.root_dir)
        } else {
            sandbox::safe_path(&ctx.config.root_dir, search_path)
        } {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let base_pat = std::path::Path::new(pattern)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        let is_recursive = pattern.contains("**");

        let mut files: Vec<(String, std::time::SystemTime)> = Vec::new();

        for entry in WalkDir::new(&safe_path).follow_links(false).into_iter().flatten() {
            if entry.file_type().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let matched = if is_recursive {
                glob_match(&base_pat, &name)
            } else {
                let rel = entry
                    .path()
                    .strip_prefix(&safe_path)
                    .unwrap_or(entry.path())
                    .to_string_lossy();
                glob_match(pattern, &rel) || glob_match(&base_pat, &name)
            };

            if matched {
                let mtime = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                files.push((entry.path().to_string_lossy().to_string(), mtime));
            }
        }

        // Sort by mtime descending (newest first)
        files.sort_by(|a, b| b.1.cmp(&a.1));

        const LIMIT: usize = 100;
        let truncated = files.len() > LIMIT;
        files.truncate(LIMIT);

        let mut lines: Vec<String> = files.into_iter().map(|(p, _)| p).collect();
        if truncated {
            lines.push(format!("(结果已截断，仅显示前 {} 条)", LIMIT));
        }

        let output = if lines.is_empty() {
            "No files found".to_string()
        } else {
            lines.join("\n")
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

/// Simple glob matching supporting * and **
fn glob_match(pattern: &str, name: &str) -> bool {
    // Use globset for proper matching
    let mut builder = globset::GlobBuilder::new(pattern);
    builder.literal_separator(true);
    match builder.build() {
        Ok(glob) => glob.compile_matcher().is_match(name),
        Err(_) => false,
    }
}
