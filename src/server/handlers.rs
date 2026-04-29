use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::server::AppState;
use crate::skill;
use crate::types::{parse_tool_request_payload, ToolResponse};

pub async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "dir": state.config.root_dir.display().to_string(),
        "version": "1.0.0"
    }))
}

#[derive(Deserialize)]
pub struct AuthRequest {
    token: String,
}

pub async fn auth(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AuthRequest>,
) -> impl IntoResponse {
    let valid = req.token.len() == state.config.token.len()
        && subtle_constant_eq(req.token.as_bytes(), state.config.token.as_bytes());
    Json(json!({ "valid": valid }))
}

fn subtle_constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub async fn config(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "rootDir": state.config.root_dir.display().to_string(),
        "timeout": state.config.timeout
    }))
}

pub async fn list_tools(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let tools = state.executor.list_tools();
    Json(json!({ "tools": tools }))
}

pub async fn exec(
    State(state): State<Arc<AppState>>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    tracing::info!("[OpenLink] received /exec request");

    let req = match parse_tool_request_payload(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[OpenLink] tool request parse failed: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::to_value(ToolResponse {
                    status: "error".to_string(),
                    output: String::new(),
                    error: e,
                    stop_stream: false,
                }).unwrap()),
            );
        }
    };

    tracing::info!(
        "[OpenLink] tool call: name={}, call_id={:?}, args={:?}",
        req.name,
        req.call_id,
        req.args
    );

    let resp = state.executor.execute(&req);

    tracing::info!(
        "[OpenLink] result: status={}, output_len={}",
        resp.status,
        resp.output.len()
    );
    if !resp.error.is_empty() {
        tracing::info!("[OpenLink] error: {}", resp.error);
    }

    (StatusCode::OK, Json(serde_json::to_value(resp).unwrap()))
}

pub async fn prompt(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let prompt_path = state.config.root_dir.join("prompts").join("init_prompt.txt");

    let mut content = match std::fs::read_to_string(&prompt_path) {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "init_prompt.txt not found"})),
            )
                .into_response();
        }
    };

    // Replace {{SYSTEM_INFO}}
    let system_info = build_system_info(&state.config.root_dir);
    content = content.replace("{{SYSTEM_INFO}}", &system_info);

    // Append skills list
    let skills = skill::load_infos(&state.config.root_dir);
    if !skills.is_empty() {
        let mut sb = String::from("\n\n## 当前可用 Skills\n\n");
        for sk in &skills {
            sb.push_str(&format!("- **{}**: {}\n", sk.name, sk.description));
        }
        content.push_str(&sb);
    }

    content.push_str("\n\n初始化回复：\n你好，我是 openlink，请问有什么可以帮你？");

    (StatusCode::OK, content).into_response()
}

fn build_system_info(root_dir: &PathBuf) -> String {
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S");

    format!(
        "- 操作系统: {}/{}\n- 工作目录: {}\n- 主机名: {}\n- 当前时间: {}",
        os,
        arch,
        root_dir.display(),
        hostname,
        now
    )
}

#[derive(Deserialize)]
pub struct SkillsQuery {}

pub async fn list_skills(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let skills = skill::load_infos(&state.config.root_dir);
    let items: Vec<_> = skills
        .into_iter()
        .map(|sk| json!({"name": sk.name, "description": sk.description}))
        .collect();
    Json(json!({ "skills": items }))
}

#[derive(Deserialize)]
pub struct FilesQuery {
    q: Option<String>,
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilesQuery>,
) -> impl IntoResponse {
    let q = query.q.unwrap_or_default().to_lowercase();
    if q.len() > 200 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "q too long"})),
        );
    }

    let root_real = match std::fs::canonicalize(&state.config.root_dir) {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "invalid root"})),
            );
        }
    };

    let skip_dirs: std::collections::HashSet<&str> = [
        ".git",
        "node_modules",
        ".next",
        "dist",
        "build",
        "vendor",
    ]
    .into_iter()
    .collect();

    let mut files: Vec<String> = Vec::new();

    fn walk(
        dir: &std::path::Path,
        root_real: &std::path::Path,
        root_dir: &std::path::Path,
        q: &str,
        skip_dirs: &std::collections::HashSet<&str>,
        files: &mut Vec<String>,
    ) {
        if files.len() >= 50 {
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            if files.len() >= 50 {
                return;
            }

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if entry
                .file_type()
                .map(|ft| ft.is_dir())
                .unwrap_or(false)
            {
                if skip_dirs.contains(name.as_str()) {
                    continue;
                }
                walk(&path, root_real, root_dir, q, skip_dirs, files);
                continue;
            }

            // Resolve symlinks and check they stay within root
            let real = match std::fs::canonicalize(&path) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if !real.starts_with(root_real) {
                continue;
            }

            let rel = match path.strip_prefix(root_dir) {
                Ok(r) => r.to_string_lossy().to_string(),
                Err(_) => continue,
            };

            if q.is_empty() || rel.to_lowercase().contains(q) {
                files.push(rel);
            }
        }
    }

    walk(
        &state.config.root_dir,
        &root_real,
        &state.config.root_dir,
        &q,
        &skip_dirs,
        &mut files,
    );

    (StatusCode::OK, Json(json!({ "files": files })))
}
