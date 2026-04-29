# 开发指南

## 环境要求

- Rust 1.85+ (edition 2024)
- Node.js 18+
- Chrome 或 Firefox 浏览器

## 项目结构

```
openlink/
├── src/
│   ├── main.rs            # 服务端入口
│   ├── lib.rs             # 库根模块
│   ├── config.rs          # 配置与 Token 管理
│   ├── executor.rs        # 工具执行器
│   ├── security/
│   │   ├── auth.rs        # Token 认证中间件
│   │   └── sandbox.rs     # 路径沙箱与命令过滤
│   ├── server/
│   │   ├── mod.rs         # 路由与 AppState
│   │   └── handlers.rs    # HTTP 处理函数
│   ├── skill/
│   │   └── mod.rs         # Skill 加载器
│   ├── tool/
│   │   ├── mod.rs         # Tool trait 与 Registry
│   │   ├── edit.rs        # 字符串替换
│   │   ├── exec_cmd.rs    # Shell 命令执行
│   │   ├── glob.rs        # 文件模式匹配
│   │   ├── grep.rs        # 内容搜索
│   │   ├── list_dir.rs    # 目录列表
│   │   ├── read_file.rs   # 文件读取
│   │   ├── write_file.rs  # 文件写入
│   │   ├── web_fetch.rs   # 网页抓取
│   │   ├── skill_tool.rs  # Skill 加载
│   │   ├── question.rs    # 用户提问
│   │   ├── todo_write.rs  # 任务列表
│   │   └── truncate.rs    # 输出截断
│   └── types/
│       └── tool_request.rs # API 类型定义
├── prompts/               # 内置初始化提示词
├── extension/             # 浏览器扩展（TypeScript）
│   ├── src/
│   │   ├── content/       # 内容脚本（工具调用拦截）
│   │   ├── popup/         # 扩展弹窗 UI
│   │   └── background/    # Service Worker
│   └── public/            # manifest.json 等静态资源
├── Cargo.toml
├── install.sh             # Linux/macOS 安装脚本
└── install.ps1            # Windows 安装脚本
```

## 本地开发

### 启动服务端

```bash
cargo run -- --dir=/your/workspace
```

### 构建服务端

```bash
# Debug 构建
cargo build

# Release 构建
cargo build --release
./target/release/openlink --dir=/your/workspace --port=39527
```

### 开发扩展

```bash
cd extension
npm install
npm run build      # 生产构建
npm run dev        # 监听模式（改动自动重新构建）
npm run build:firefox
npm run package:firefox
```

构建产物在 `extension/dist/`：

- `npm run build` 或 `npm run build:chrome` 生成 Chrome 版本
- `npm run build:firefox` 生成 Firefox 版本（Firefox 专用 manifest）
- `npm run package:firefox` 额外生成 `extension/openlink-firefox.xpi`

浏览器加载方式：

- Chrome: `chrome://extensions/` -> 加载已解压的扩展程序 -> `extension/dist/`
- Firefox: `about:debugging#/runtime/this-firefox` -> 临时载入附加组件 -> `extension/dist/manifest.json`

### 运行测试

```bash
cargo test
```

## 发布新版本

推送 tag 后 GitHub Actions 自动构建并发布：

```bash
git tag v1.0.0
git push origin v1.0.0
```

发布产物包含：

- 各平台二进制（linux/darwin/windows × amd64/arm64）
- 扩展压缩包 `extension.zip`

## 添加新 AI 平台支持

在 `extension/src/content/index.ts` 的 `getSiteConfig()` 中添加新站点配置：

```typescript
if (h.includes("example.com"))
  return {
    editor: "textarea#input", // 输入框选择器
    sendBtn: 'button[type="submit"]', // 发送按钮选择器
    stopBtn: null,
    fillMethod: "value", // paste | execCommand | value | prosemirror
    useObserver: true, // 是否用 DOM Observer 检测工具调用
    responseSelector: ".response", // 响应容器选择器（useObserver=true 时必填）
    supported: true, // 显示初始化按钮
  };
```

同时在 `extension/public/manifest.json` 的 `content_scripts.matches` 和 `web_accessible_resources.matches` 中添加对应域名。

## 添加新工具

1. 在 `src/tool/` 下创建新文件，实现 `Tool` trait（`name()`, `description()`, `parameters()`, `validate()`, `execute()`）
2. 在 `src/executor.rs` 的 `Executor::new()` 中注册新工具
3. 所有文件路径操作必须通过 `sandbox::safe_path()` 验证