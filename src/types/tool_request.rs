use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub name: String,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub args: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResponse {
    pub status: String,
    pub output: String,
    pub error: String,
    pub stop_stream: bool,
}

pub fn parse_tool_request_payload(body: &[u8]) -> Result<ToolRequest, String> {
    if body.is_empty() {
        return Err("empty request body".to_string());
    }

    // Try direct JSON parse
    if let Ok(req) = serde_json::from_slice::<ToolRequest>(body) {
        return Ok(req);
    }

    // Try parsing as raw string (JSON text)
    if let Ok(s) = std::str::from_utf8(body) {
        let s = s.trim();
        if !s.is_empty() {
            if let Ok(req) = serde_json::from_str::<ToolRequest>(s) {
                return Ok(req);
            }
        }
    }

    Err("failed to parse tool request".to_string())
}
