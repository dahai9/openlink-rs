use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::time::Instant;

use regex::Regex;
use serde_json::Value;
use walkdir::WalkDir;

use super::{resolve_abs_path, Tool, ToolContext, ToolResult};
use crate::security::sandbox;

pub struct GrepTool;

impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents using regex"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "pattern": "string (required) - regex pattern to search",
            "path": "string (optional) - directory to search (default: root)",
            "include": "string (optional) - file glob filter, e.g. *.go"
        })
    }

    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        match args.get("pattern").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => {}
            _ => return Err("pattern is required".to_string()),
        }
        if let Some(inc) = args.get("include").and_then(|v| v.as_str()) {
            if inc.contains('/') || inc.contains('\\') {
                return Err("include pattern must not contain path separators".to_string());
            }
        }
        Ok(())
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
        let include = ctx.args.get("include").and_then(|v| v.as_str());

        let safe_path = match if search_path.starts_with('/') || search_path.starts_with('~') {
            resolve_abs_path(search_path, &ctx.config.root_dir)
        } else {
            sandbox::safe_path(&ctx.config.root_dir, search_path)
        } {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        // Try ripgrep first
        let output = if rg_available() {
            grep_with_rg(pattern, &safe_path, include)
        } else {
            match grep_native(pattern, &safe_path, include) {
                Ok(o) => o,
                Err(e) => return ToolResult::error(e.to_string()),
            }
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

fn rg_available() -> bool {
    std::process::Command::new("rg")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn grep_with_rg(pattern: &str, search_path: &std::path::Path, include: Option<&str>) -> String {
    let mut args = vec!["-n", "--no-heading"];
    if let Some(inc) = include {
        args.push("--glob");
        args.push(inc);
    }
    args.push("--");
    args.push(pattern);
    let path_binding = search_path.to_string_lossy();
    args.push(&path_binding);

    let output = std::process::Command::new("rg")
        .args(&args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let lines: Vec<&str> = output.split('\n').collect();
    format_grep_lines(&lines, 100)
}

fn grep_native(
    pattern: &str,
    search_path: &std::path::Path,
    include: Option<&str>,
) -> Result<String, String> {
    let re = Regex::new(pattern).map_err(|e| format!("invalid pattern: {}", e))?;

    struct Match {
        line: String,
        mtime: std::time::SystemTime,
    }

    let mut matches = Vec::new();

    for entry in WalkDir::new(search_path).follow_links(false).into_iter().flatten() {
        if entry.file_type().is_dir() {
            continue;
        }
        if let Some(inc) = include {
            let name = entry.file_name().to_string_lossy();
            if glob_match_simple(inc, &name) {
                continue; // skip if doesn't match include
            }
        }

        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        let file = match std::fs::File::open(entry.path()) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let reader = BufReader::new(file);
        let mut line_num = 0u32;
        for line_result in reader.lines() {
            line_num += 1;
            if let Ok(text) = line_result {
                if re.is_match(&text) {
                    matches.push(Match {
                        line: format!(
                            "{}:{}:{}",
                            entry.path().to_string_lossy(),
                            line_num,
                            text
                        ),
                        mtime,
                    });
                }
            }
        }
    }

    // Sort by mtime descending
    matches.sort_by(|a, b| b.mtime.cmp(&a.mtime));

    let lines: Vec<&str> = matches.iter().map(|m| m.line.as_str()).collect();
    Ok(format_grep_lines(&lines, 100))
}

fn glob_match_simple(pattern: &str, name: &str) -> bool {
    globset::GlobBuilder::new(pattern)
        .literal_separator(false)
        .build()
        .map(|g| g.compile_matcher().is_match(name))
        .unwrap_or(false)
}

fn format_grep_lines(lines: &[&str], limit: usize) -> String {
    let mut out = Vec::new();
    let mut count = 0;
    for l in lines {
        if l.is_empty() {
            continue;
        }
        out.push(l.to_string());
        count += 1;
        if count >= limit {
            out.push(format!("(结果已截断，仅显示前 {} 条)", limit));
            break;
        }
    }
    if out.is_empty() {
        "No matches found".to_string()
    } else {
        out.join("\n")
    }
}
