use std::collections::HashMap;
use std::time::Instant;

use serde_json::Value;

use super::{Tool, ToolContext, ToolResult};

pub struct TodoWriteTool;

impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Write task list to .todos.json"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "todos": "array (required) - full list of todo items to save"
        })
    }

    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        if args.get("todos").is_none() {
            return Err("todos is required".to_string());
        }
        Ok(())
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let todos = match ctx.args.get("todos") {
            Some(v) => v,
            None => return ToolResult::error("todos is required"),
        };

        let data = match serde_json::to_string_pretty(todos) {
            Ok(d) => d,
            Err(e) => return ToolResult::error(e.to_string()),
        };

        let path = ctx.config.root_dir.join(".todos.json");
        if let Err(e) = std::fs::write(&path, data) {
            return ToolResult::error(e.to_string());
        }

        let count = todos.as_array().map(|a| a.len()).unwrap_or(0);

        ToolResult {
            status: "success",
            output: format!("已保存 {} 个任务", count),
            error: String::new(),
            stop_stream: false,
            start_time: Instant::now(),
            end_time: Some(Instant::now()),
        }
    }
}
