# Typemore

Typemore 是一个 Windows 桌面语音转文字工具，面向“边说边写”的日常输入场景。

核心能力：

- `右 Alt` 单键开始/结束录音
- 本地 `whisper.cpp` 小模型先做转写
- DeepSeek 对转写结果做纠错、润色和轻度结构化整理
- 优先自动粘贴到当前活动光标，不行就退回剪贴板
- 桌面底部悬浮状态窗提示录音中 / 处理中
- 托盘常驻，关闭设置窗后自动隐藏到托盘

## 技术栈

- `Tauri v2`
- `React + Vite`
- `Rust`
- `whisper.cpp`
- `DeepSeek API`

## 目录结构

```text
src/                 React 前端
src-tauri/           Rust 后端与 Tauri 配置
docs/                使用手册与开发记录
scripts/             安装与辅助脚本
```

## 环境要求

Windows 开发环境建议如下：

1. `Node.js 20+`
2. `npm 10+`
3. `Rust stable`
4. `Visual Studio C++ Build Tools`
5. `WebView2 Runtime`

## 一键安装

可以直接运行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

这个脚本会做以下事情：

1. 检查 `node` / `npm`
2. 检查 `cargo`，缺失时尝试通过 `winget` 安装 `rustup`
3. 执行 `npm install`
4. 执行 `cargo fetch`
5. 默认跑一次前端构建和后端检查

如果你只想装依赖，不想立刻构建：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -SkipBuild
```

## 本地开发

```powershell
npm run tauri dev
```

启动后：

1. 打开设置页
2. 填写 `DeepSeek API Key`
3. 点击“自动下载 Whisper 资源”
4. 保存设置

## 打包

前端构建：

```powershell
npm run build
```

Rust 检查：

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

正式桌面包：

```powershell
npm run tauri build
```

默认会生成 Windows MSI 安装包。

## 运行方式

### 热键录音

- 只监听 `右 Alt`
- 默认缓冲时间 `1500ms`
- 缓冲结束后，以最后一次指令为准

### 输出逻辑

- 当前前台是第三方应用时，优先模拟粘贴
- 不适合自动投递时，退回剪贴板
- 会弹通知提示“可直接粘贴”

### 短文本优化

- 本地转写有效字符数 `<= 3` 时，跳过大模型

## 文档

- 使用手册：[docs/USER_GUIDE.md](docs/USER_GUIDE.md)
- 开发记录：[docs/DEVELOPMENT_NOTES.md](docs/DEVELOPMENT_NOTES.md)

## 注意事项

- API Key 不应提交到仓库
- 运行时资源默认写入用户应用数据目录，不在仓库中管理
- `node_modules`、`dist`、`src-tauri/target` 等产物已加入 `.gitignore`
