# Typemore 开发记录

这份文档只面向开发者，记录实现方式、设计决策和后续可优化方向。

## 1. 项目结构

- 前端：`React + Vite`
- 后端：`Rust`
- 桌面壳：`Tauri v2`
- 本地转写：`whisper.cpp`
- 云端修复：`DeepSeek API`

## 2. 关键实现决策

### 为什么选 Tauri

- Windows 桌面能力够用
- Rust 后端适合做音频采集、输入模拟和系统交互
- 打包链路成熟

### 为什么本地转写走 whisper.cpp CLI

- 降低 Rust 进程内模型绑定复杂度
- 更容易替换模型和 CLI 版本
- Windows 分发更稳

### 为什么热键只监听右 Alt

`Alt` 单独作为标准全局快捷键并不稳定，通用快捷键插件也不适合处理这种语义。

当前方案：
- 只监听 `右 Alt`
- 通过轮询按键状态判断单独按下与释放
- 提供可配置缓冲时间，避免快速连按导致抖动切换

### 为什么要有短文本跳过大模型

极短的本地转写：
- 继续送大模型收益很低
- 反而增加延迟
- 有时还会过度改写

所以当前规则是：有效字符数 `<= 3` 时直接输出本地结果。

### 为什么悬浮窗做成独立窗口

- 不依赖主窗口是否显示
- 更接近系统级录音浮层体验
- 容易实现透明、置顶、无任务栏显示

### 为什么关闭主窗口时改成隐藏

Typemore 是常驻桌面工具，不适合把关闭动作等同于退出程序。

当前行为：
- 主窗口关闭时隐藏到托盘
- 托盘点击时恢复主窗口
- 如果主窗口真的不存在，则重新创建

## 3. 文本处理策略

最终文本处理链路：

1. 录音结束
2. 本地 Whisper 转写
3. 生成拼音提示
4. DeepSeek 做纠错、润色和轻度结构化整理
5. 自动粘贴或剪贴板兜底

大模型目标不是“重写内容”，而是：
- 修正同音字和明显误识别
- 去掉口头卡顿、重复、语气词、自我修正
- 补全标点和断句
- 在适合时做轻度结构化整理

## 4. 输入投递策略

当前优先级：

1. 优先尝试向第三方前台窗口直接粘贴
2. 如果不适合或失败，则退回剪贴板
3. 保证结果不丢

这是为了兼容：
- 普通输入框
- 终端
- 一些 UI Automation 识别不稳定的窗口

## 5. 常用开发命令

准备开发环境：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\setup-dev.ps1
```

启动开发环境：

```powershell
npm run tauri dev
```

前端构建：

```powershell
npm run build
```

Rust 检查：

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

正式打包：

```powershell
npm run tauri build
```

## 6. 脚本职责

- `scripts/setup-dev.ps1`：准备开发环境
- `scripts/build-installer.ps1`：生成 MSI 安装包
- `scripts/install-local.ps1`：生成并启动本地安装
- `scripts/install.ps1`：兼容旧入口，内部转发到 `setup-dev.ps1`

## 7. 后续可继续优化

- 麦克风设备选择更细化
- 本地模型切换
- 多语言模式
- DeepSeek prompt 风格切换
- 更细的悬浮窗动画
- 正式 MSI 安装器和自动资源下载整合
