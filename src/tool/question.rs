use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::time::Instant;

use serde_json::Value;

use super::{Tool, ToolContext, ToolResult};

pub struct QuestionTool;

impl Tool for QuestionTool {
    fn name(&self) -> &str {
        "question"
    }

    fn description(&self) -> &str {
        "Ask the user a question and wait for input"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "question": "string (required) - the question to ask",
            "options": "array (optional) - list of choices to present"
        })
    }

    fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        match args.get("question").and_then(|v| v.as_str()) {
            Some(q) if !q.is_empty() => Ok(()),
            _ => Err("question is required".to_string()),
        }
    }

    fn execute(&self, ctx: &ToolContext) -> ToolResult {
        let question = ctx
            .args
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let mut sb = String::from("[需要您的输入]\n\n");
        sb.push_str(question);

        if let Some(options) = ctx.args.get("options").and_then(|v| v.as_array()) {
            if !options.is_empty() {
                sb.push_str("\n\n可选项：");
                for (i, opt) in options.iter().enumerate() {
                    let _ = write!(sb, "\n  {}. {}", i + 1, opt);
                }
                sb.push_str("\n\n请输入您的选择或回答：");
            }
        }

        ToolResult {
            status: "success",
            output: sb,
            error: String::new(),
            stop_stream: false,
            start_time: Instant::now(),
            end_time: Some(Instant::now()),
        }
    }
}
