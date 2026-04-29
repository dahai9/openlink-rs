use std::collections::HashMap;
use std::io::Read;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{get_arg_string, truncate::truncate, Tool, ToolContext, ToolResult};
use crate::security::sandbox;

pub struct ExecCmdTool;

impl Tool for ExecCmdTool {
    fn name(&self) -> &str {
        "exec_cmd"
    }

    fn description(&self) -> &str {
        "Execute shell command in sandbox"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "command": "string (required) - shell command to execute"
        })
    }

    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        let cmd = get_arg_string(args, "command", "cmd").unwrap_or("");
        if cmd.is_empty() {
            return Err("command is required".to_string());
        }
        if sandbox::is_dangerous_command(cmd) {
            return Err("dangerous command blocked".to_string());
        }
        Ok(())
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let cmd = get_arg_string(&ctx.args, "command", "cmd").unwrap_or("");
        let timeout_secs = ctx.config.timeout;
        let root_dir = ctx.config.root_dir.clone();
        let cmd_owned = cmd.to_string();

        let result = tokio::task::block_in_place(|| {
            let (shell, flag) = get_shell();

            let mut child = match std::process::Command::new(shell)
                .arg(flag)
                .arg(&cmd_owned)
                .current_dir(&root_dir)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => return Err(e.to_string()),
            };

            // Poll with timeout
            let deadline = Instant::now() + Duration::from_secs(timeout_secs);
            let success = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status.success(),
                    Ok(None) => {
                        if Instant::now() >= deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err("execution timeout".to_string());
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(e) => return Err(e.to_string()),
                }
            };

            let mut stdout_buf = Vec::new();
            let mut stderr_buf = Vec::new();
            if let Some(ref mut out) = child.stdout {
                let _ = out.read_to_end(&mut stdout_buf);
            }
            if let Some(ref mut err) = child.stderr {
                let _ = err.read_to_end(&mut stderr_buf);
            }

            Ok((stdout_buf, stderr_buf, success))
        });

        match result {
            Err(e) if e == "execution timeout" => ToolResult::error("execution timeout"),
            Err(e) => ToolResult::error(e),
            Ok((stdout, stderr, success)) => {
                let combined = format!(
                    "{}{}",
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                );
                let (output_str, _) = truncate(&combined);
                let output_str = if output_str.is_empty() {
                    "empty".to_string()
                } else {
                    output_str
                };

                if success {
                    ToolResult {
                        status: "success",
                        output: format!("command: {}\n\n{}", cmd, output_str),
                        error: String::new(),
                        stop_stream: false,
                        start_time: Instant::now(),
                        end_time: Some(Instant::now()),
                    }
                } else {
                    ToolResult {
                        status: "error",
                        output: format!("command: {}\n\n{}", cmd, output_str),
                        error: String::new(),
                        stop_stream: false,
                        start_time: Instant::now(),
                        end_time: Some(Instant::now()),
                    }
                }
            }
        }
    }
}

fn get_shell() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("cmd.exe", "/C")
    } else {
        ("sh", "-c")
    }
}
