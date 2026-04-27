# OpenLink Extension

## 安装依赖

```bash
npm install
```

## 开发构建

```bash
npm run dev
```

## Chrome 构建

```bash
npm run build
```

打包后的文件在 `dist/` 目录。

## Firefox 构建

```bash
npm run build:firefox
```

同样输出到 `dist/`，其中会自动写入 Firefox 专用 `manifest.json`。

## Firefox 打包为 .xpi

```bash
npm run package:firefox
```

生成文件：`openlink-firefox.xpi`

## 加载到 Chrome

1. 打开 `chrome://extensions/`
2. 开启"开发者模式"
3. 点击"加载已解压的扩展程序"
4. 选择 `dist/` 目录

## 加载到 Firefox（临时安装）

1. 打开 `about:debugging#/runtime/this-firefox`
2. 点击「临时载入附加组件」
3. 选择 `dist/manifest.json` 或 `openlink-firefox.xpi`

> 注意：如果在 `about:addons` 里直接安装未签名的 `.xpi`，Firefox 常会提示 "appears to be corrupt"。
> 开发调试请使用上面的「临时载入附加组件」。

## 网页调试（Gemini 抓不到 Tool Call 时）

1. 打开插件弹窗，开启「调试日志」开关
2. 在 `https://gemini.google.com` 打开 DevTools Console
3. 过滤关键词：`[OpenLink]` 或 `[OpenLink][debug]`
4. 复现一次问题后，导出并提供以下信息：
   - Console 中的 OpenLink 日志
   - Network 中对应会话请求的 Response（是否包含 `tool_call:`）
   - OpenLink 本地服务端日志（终端输出）

说明：Gemini 页面如果不渲染 tool 文本，插件会自动启用流式拦截回退通道捕捉 tool call。
