# OpenLink Extension

## 安装依赖

```bash
npm install
```

## 开发

```bash
npm run dev
```

## 打包

```bash
npm run build
```

打包后的文件在 `dist/` 目录。

## Firefox 打包

```bash
npm run build:firefox
```

这会生成 Firefox 可直接加载的 `dist/` 目录。

如果需要 `.xpi`：

```bash
npm run package:firefox
```

会生成 `openlink-firefox.xpi`。

## 加载到 Chrome

1. 打开 `chrome://extensions/`
2. 开启"开发者模式"
3. 点击"加载已解压的扩展程序"
4. 选择 `dist/` 目录

## 加载到 Firefox

1. 打开 `about:debugging#/runtime/this-firefox`
2. 点击「临时载入附加组件」
3. 选择 `dist/manifest.json` 或 `openlink-firefox.xpi`

如果在 `about:addons` 直接安装未签名的 `.xpi` 提示损坏，改用上面的临时载入方式。
