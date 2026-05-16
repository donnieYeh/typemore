# Typemore

Typemore 是一个 Windows 桌面语音转文字工具，面向“边说边写”的输入场景。

核心能力：
- 单独按 `右 Alt` 开始和结束录音
- 本地 `whisper.cpp` 模型先做转写
- DeepSeek 对结果做纠错、润色和轻度结构化整理
- 优先自动粘贴到当前活动光标，不行则退回剪贴板
- 桌面底部悬浮窗提示“录音中 / 处理中”
- 托盘常驻，关闭主窗口后自动隐藏到托盘

## 用户使用

### 安装

如果你只是想把 Typemore 装到本机使用，推荐直接运行：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-local.ps1
```

它会自动：
1. 检查开发/构建依赖
2. 生成 `MSI` 安装包
3. 自动拉起安装程序

如果你只想生成安装包，不立刻安装：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-installer.ps1
```

生成后的安装包默认位于：

```text
src-tauri\target\release\bundle\msi\
```

### 启动

正式安装完成后，可以通过以下方式启动：

1. 从开始菜单打开 `Typemore`
2. 或双击桌面快捷方式（如果安装时创建了）

启动后：
- 主窗口会打开
- 系统托盘会出现 Typemore 图标
- 关闭主窗口不会退出程序，而是隐藏到托盘

### 首次配置

首次启动后，请按这个顺序完成初始化：

1. 打开设置窗口
2. 填写 `DeepSeek API Key`
3. 点击“自动下载 Whisper 资源”
4. 保存设置

### 日常使用

1. 在任意输入框中放好光标
2. 单独按一次 `右 Alt`
3. 开始说话
4. 再单独按一次 `右 Alt`
5. 等待程序转写、修复并自动粘贴

更完整的用户手册见：[docs/USER_GUIDE.md](docs/USER_GUIDE.md)

## 开发使用

如果你要参与开发，先准备开发环境：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\setup-dev.ps1
```

开发模式启动：

```powershell
npm run tauri dev
```

常用命令：

```powershell
npm run build
cargo check --manifest-path .\src-tauri\Cargo.toml
npm run tauri build
```

开发记录和架构说明见：[docs/DEVELOPMENT_NOTES.md](docs/DEVELOPMENT_NOTES.md)

## 目录

```text
src/                 React 前端
src-tauri/           Rust 后端与 Tauri 配置
docs/                用户手册与开发记录
scripts/             开发初始化、打包与安装脚本
```

## 注意事项

- API Key 不应提交到仓库
- 运行时资源默认写入用户应用数据目录，不在仓库中管理
- `node_modules`、`dist`、`src-tauri/target` 等产物已加入 `.gitignore`
