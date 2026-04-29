use std::collections::HashMap;
use std::time::Instant;

use serde_json::Value;

use super::{Tool, ToolContext, ToolResult};

pub struct InvalidTool;

impl Tool for InvalidTool {
    fn name(&self) -> &str {
        "invalid"
    }

    fn description(&self) -> &str {
        "Catches unknown tool calls"
    }

    fn parameters(&self) -> Value {
        Value::Null
    }

    fn validate(&self, _args: &HashMap<String, Value>) -> Result<(), String> {
        Ok(())
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let tool_name = ctx
            .args
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        ToolResult {
            status: "error",
            output: String::new(),
            error: format!(
                "工具 '{}' 不存在。可用工具: exec_cmd, read_file, write_file, list_dir, glob, grep, edit, web_fetch, todo_write, question, skill",
                tool_name
            ),
            stop_stream: false,
            start_time: Instant::now(),
            end_time: None,
        }
    }
}
