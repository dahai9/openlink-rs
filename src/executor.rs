use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing;

use crate::config::Config;
use crate::tool::{self, Tool, ToolContext};
use crate::types::{ToolRequest, ToolResponse};

pub struct Executor {
    config: Arc<Config>,
    registry: tool::Registry,
    call_count: AtomicU64,
}

impl Executor {
    pub fn new(config: Arc<Config>) -> Self {
        let mut registry = tool::Registry::new();

        // Register all tools
        let tools: Vec<Arc<dyn tool::Tool>> = vec![
            Arc::new(tool::exec_cmd::ExecCmdTool),
            Arc::new(tool::list_dir::ListDirTool),
            Arc::new(tool::read_file::ReadFileTool),
            Arc::new(tool::write_file::WriteFileTool),
            Arc::new(tool::glob::GlobTool),
            Arc::new(tool::grep::GrepTool),
            Arc::new(tool::edit::EditTool),
            Arc::new(tool::web_fetch::WebFetchTool),
            Arc::new(tool::question::QuestionTool),
            Arc::new(tool::skill_tool::SkillTool),
            Arc::new(tool::todo_write::TodoWriteTool),
        ];

        for t in tools {
            let _ = registry.register(t);
        }

        Self {
            config,
            registry,
            call_count: AtomicU64::new(0),
        }
    }

    pub fn execute(&self, req: &ToolRequest) -> ToolResponse {
        tracing::info!("[Executor] executing tool: {}", req.name);

        // Try exact name first, then lowercase fallback
        let tool = self
            .registry
            .get(&req.name)
            .or_else(|| self.registry.get(&req.name.to_lowercase()));

        let tool = match tool {
            Some(t) => t,
            None => {
                let invalid = tool::invalid::InvalidTool;
                let mut args = req.args.clone();
                args.insert(
                    "tool".to_string(),
                    serde_json::Value::String(req.name.clone()),
                );
                let ctx = ToolContext {
                    args,
                    config: self.config.clone(),
                };
                let result = invalid.execute(&ctx);
                return ToolResponse {
                    status: "error".to_string(),
                    output: result.error.clone(),
                    error: result.error,
                    stop_stream: false,
                };
            }
        };

        // Validate
        if let Err(e) = tool.validate(&req.args) {
            let msg = format!("validation failed: {}", e);
            return ToolResponse {
                status: "error".to_string(),
                output: msg.clone(),
                error: msg,
                stop_stream: false,
            };
        }

        // Execute
        let ctx = ToolContext {
            args: req.args.clone(),
            config: self.config.clone(),
        };
        let result = tool.execute(&ctx);

        let mut resp = ToolResponse {
            status: result.status.to_string(),
            output: result.output.clone(),
            error: result.error.clone(),
            stop_stream: result.stop_stream,
        };
        if result.status == "error" && result.output.is_empty() {
            resp.output = result.error.clone();
        }

        // Append identity reminder; re-inject full prompt every 20 calls
        let n = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        const REINJECT_EVERY: u64 = 20;
        const REMINDER: &str =
            "\n\n[系统提示] 请记住你是 openlink，严格遵循工具调用规范，不要忘记自己的身份和指令。";

        if n % REINJECT_EVERY == 0 {
            let prompt_path = self.config.root_dir.join("init_prompt.txt");
            if let Ok(data) = std::fs::read_to_string(&prompt_path) {
                resp.output.push_str("\n\n[系统重新注入提示词]\n");
                resp.output.push_str(&data);
            }
        } else {
            resp.output.push_str(REMINDER);
        }

        resp
    }

    pub fn list_tools(&self) -> Vec<tool::ToolInfo> {
        self.registry.list()
    }
}
