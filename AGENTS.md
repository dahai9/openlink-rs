# AGENTS.md

This file provides guidance to AI coding assistants when working with code in this repository.

## Project Overview

openlink is a browser-local proxy that enables web-based AI assistants (Gemini/ChatGPT/DeepSeek etc.) to access the local filesystem through a sandboxed Rust server and Chrome extension.

**Architecture**: Two-component system:
1. **Rust Server** (`src/main.rs`): Axum HTTP server that executes filesystem operations within a sandboxed directory
2. **Chrome Extension** (`extension/src/content/index.ts`): Content script that intercepts AI tool calls from web pages, proxies them to the local server, and provides input completion UI

## Development Commands

### Running the Server

```bash
# Start server with default settings (current dir, port 39527)
cargo run

# Start with custom workspace and port
cargo run -- --dir=/path/to/workspace --port=39527 --timeout=60
```

### Building

```bash
# Build release binary
cargo build --release

# Run built binary
./target/release/openlink --dir=/your/workspace --port=39527
```

### Building the Extension

```bash
cd extension
npm install
npm run build   # outputs to extension/dist/
```

### Testing the Server

```bash
# Check server health
curl http://127.0.0.1:39527/health

# List available tools
curl http://127.0.0.1:39527/tools -H "Authorization: Bearer <token>"

# List available skills
curl http://127.0.0.1:39527/skills -H "Authorization: Bearer <token>"

# List files (with optional query filter)
curl "http://127.0.0.1:39527/files?q=main" -H "Authorization: Bearer <token>"

# Test command execution
curl -X POST http://127.0.0.1:39527/exec \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{"name":"exec_cmd","args":{"command":"ls -la"}}'
```

### Installing the Extension

1. Build first: `cd extension && npm run build`
2. Open Chrome: `chrome://extensions/`
3. Enable "Developer mode"
4. Click "Load unpacked"
5. Select the `extension/dist/` directory

## Code Architecture

### Request Flow

```
Web AI (Gemini/ChatGPT/DeepSeek/etc.)
  ↓ outputs <tool> tags in response
content script (extension/src/content/index.ts)
  ↓ MutationObserver detects tool tags, renders card UI
  ↓ HTTP POST to localhost:39527/exec (via background fetch)
Axum Server (src/server/)
  ↓ validates & sanitizes
Executor (src/executor.rs)
  ↓ dispatches to tool by name
Tool (src/tool/*.rs)
  ↓ executes with sandbox
Security Layer (src/security/sandbox.rs)
  ↓ path validation & command filtering
Local Filesystem
```

### Key Components

**src/types/tool_request.rs**: Core API types
- `ToolRequest`: Incoming tool call from browser (name, call_id, args)
- `ToolResponse`: Execution result (status, output, error, stop_stream)
- `parse_tool_request_payload()`: Lenient JSON body parser

**src/config.rs**: Server configuration
- `Config`: root_dir, port, timeout, token
- `load_or_create_token()`: Generates 32-byte random token, stores in `~/.openlink/settings.json`

**src/security/sandbox.rs**: Security enforcement
- `safe_path()`: Validates file paths stay within root_dir using canonicalization
- `safe_abs_path()`: Validates absolute/tilde paths against allowed roots (root_dir, ~/.claude, ~/.openlink, ~/.agent)
- `is_dangerous_command()`: Blocks dangerous commands (rm -rf, sudo, mkfs, etc.); curl/wget are explicitly allowed

**src/security/auth.rs**: Token-based auth middleware
- Axum middleware applied to all routes
- `/health` and `/auth` endpoints exempt from auth
- Constant-time token comparison to prevent timing attacks

**src/executor.rs**: Tool execution dispatcher
- `Executor::new(config)`: Creates registry, registers all 11 tools
- `Executor::execute(req)`: Looks up tool by name (exact then lowercase fallback), calls validate() then execute()
- Identity reminder injection every call; full init_prompt re-injection every 20th call

**src/tool/**: Individual tool implementations (11 tools + 1 invalid handler)

| Tool | File | Description |
|------|------|-------------|
| exec_cmd | exec_cmd.rs | Shell command execution with timeout, uses `std::process::Command` in `block_in_place` |
| read_file | read_file.rs | Read file with offset/limit pagination (max 2000 lines, 50KB) |
| write_file | write_file.rs | Write/append to file, creates parent dirs, sets 0o644 on Unix |
| edit | edit.rs | String replacement with 10-step normalization cascade for AI-generated content |
| list_dir | list_dir.rs | Directory listing with `/` suffix for dirs |
| glob | glob.rs | File pattern matching via walkdir, sorted by mtime desc, limit 100 |
| grep | grep.rs | Regex content search, prefers ripgrep if available, else native Rust, limit 100 |
| web_fetch | web_fetch.rs | HTTP GET with HTML stripping, SSRF protection, 30s timeout, 1MB body limit |
| question | question.rs | Ask user a question with optional choices |
| skill | skill_tool.rs | Load a skill by name, or list available skills |
| todo_write | todo_write.rs | Write task list to `.todos.json` in root_dir |
| (invalid) | invalid.rs | Catch-all for unknown tool names, lists valid tools |

**src/tool/mod.rs**: Tool trait and registry
- `Tool` trait: name(), description(), parameters(), validate(), execute()
- `Registry`: HashMap-based tool lookup
- `resolve_abs_path()`: Resolves paths against allowed roots
- `get_arg_string()` / `get_arg_usize()`: Argument extraction helpers

**src/tool/truncate.rs**: Output truncation (shared by exec_cmd, read_file, web_fetch)
- Limits to 2000 lines or 50KB; saves full output to `~/.openlink/tool-output/`

**src/server/mod.rs**: Axum router setup
- `AppState`: Holds Arc<Config> and Executor
- `create_router()`: Wires all routes with auth middleware

**src/server/handlers.rs**: HTTP handlers

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Server status and version |
| POST | `/auth` | Validate Bearer token |
| GET | `/config` | Current configuration (rootDir, timeout) |
| GET | `/tools` | List all registered tools with parameters |
| POST | `/exec` | Execute a tool request |
| GET | `/prompt` | Init prompt with system info and skills list |
| GET | `/skills` | List available skills (name + description) |
| GET | `/files?q=` | List files matching query (max 50, skips .git/node_modules/etc.) |

**src/skill/mod.rs**: Skills loader
- Scans 7 directories for SKILL.md files (see Skills System below)
- YAML frontmatter parsing for name + description
- `find_skill()`: Case-insensitive lookup by name, rejects path traversal

**prompts/init_prompt.txt**: Default system prompt injected into AI on initialization
- Contains tool definitions, usage rules, and `{{SYSTEM_INFO}}` placeholder

### Supported AI Platforms

| Platform | fillMethod | useObserver | Notes |
|----------|-----------|-------------|-------|
| Google AI Studio | value | true | Recommended; writes to System Instructions |
| Google Gemini | execCommand | true | |
| ChatGPT | prosemirror | true | |
| 通义千问 (Qwen) | value | true | |
| DeepSeek | paste | false | Uses injected.js |
| Kimi | execCommand | false | |
| Mistral | execCommand | false | |
| Perplexity | execCommand | false | |
| Arena.ai | value | true | |
| OpenRouter | value | false | |
| Grok | value | false | |
| GitHub Copilot | value | false | |
| t3.chat | value | false | |
| z.ai | value | false | |

### Security Model

**Sandbox Isolation**: All file operations restricted to configured root_dir
- Path traversal blocked by canonicalization and prefix check
- Symlinks resolved before validation in both executor and `/files` endpoint
- Absolute/tilde paths validated against allowed roots: root_dir, ~/.claude, ~/.openlink, ~/.agent

**Command Filtering**: Dangerous commands blocked before execution
- Destructive: `rm -rf`, `mkfs`, `dd`, `format`
- Privilege: `sudo`, `chmod 777`
- System: `kill -9`, `reboot`, `shutdown`
- Network: `curl` and `wget` are **allowed** (unlike Go version)

**Token Auth**: All API endpoints protected by Bearer token (stored in `~/.openlink/settings.json` with 0o600 permissions)

**Timeout Control**: All commands timeout after configured duration (default 60s), implemented via `try_wait` polling in 50ms intervals

**SSRF Protection**: web_fetch blocks requests to private/internal IPs (10.x, 172.16-31.x, 192.168.x, 127.x, 169.254.x, IPv6 link-local/loopback/ULA)

**Manual Confirmation**: Extension renders tool card UI; user clicks to run each tool call

### Input Completion (extension)

The content script attaches an `input` event listener to the AI platform's editor element:

- Typing `/` triggers skill completion: fetches `GET /skills`, shows picker, inserts `<tool name="skill">` XML on select
- Typing `@` triggers file completion: fetches `GET /files?q=<query>`, shows picker, inserts file path on select
- Picker supports ↑/↓ navigation, Enter to confirm, Escape to dismiss
- Results are cached (skills: 30s, files: 5s) to avoid excessive requests
- Race conditions prevented via `inputVersion` counter

### Skills System

Skills are Markdown files that extend AI capabilities for specific domains. Scanned directories (in priority order):

```
<rootDir>/.skills/
<rootDir>/.openlink/skills/
<rootDir>/.agent/skills/
<rootDir>/.claude/skills/
~/.openlink/skills/
~/.agent/skills/
~/.claude/skills/
```

Each skill is a subdirectory containing `SKILL.md` with YAML frontmatter (`name`, `description`).

## Module Information

- **Crate**: `openlink` v1.0.0
- **Edition**: Rust 2024
- **Framework**: Axum 0.8 + Tokio async runtime
- **HTTP Client**: reqwest 0.12 with rustls-tls (no OpenSSL dependency)
- **Extension**: TypeScript, Manifest V3, built with esbuild/webpack