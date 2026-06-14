[English](README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md)

# Anthro Bridge

## 中文(简体)

### 概述

Anthro Bridge 是一个代理 + GUI 管理工具，可将 Claude Desktop / Claude Code 的 API 请求路由到多个提供商的 Anthropic 兼容端点。

Anthro Bridge 读取每个请求中的 `model` 字段，并自动路由到正确的上游提供商（基于模型的路由）。仅重写 `model` 字段 — messages、thinking blocks、tool_use、tool_result 和 streaming SSE 均原样透传。

Anthro Bridge 不是 Moon Bridge 的分支、GUI 版本或配套应用，而是一个独立的 Anthropic 兼容网关。

GUI 管理工具（Tauri v2 + React 19 + TypeScript）在原生 Windows 窗口中提供启动/停止控制、配置编辑、日志查看和 API 密钥管理功能。

### 为什么需要这个网关

Claude Desktop / Claude Code 从根本上依赖 Anthropic 的 API 格式和 Claude 系列的模型名称。即使 DeepSeek、MiniMax、Kimi 等提供商提供了 Anthropic 兼容的 API，Claude Desktop / Claude Code 也无法始终直接使用它们。

特别是 **Claude Desktop 的 `inferenceModels[].name` 只接受 Anthropic 官方模型名称**。像 `claude-deepseek-v4` 或 `kimi-k2.6` 这样的网关自定义名称会被拒绝，提示 `"not an Anthropic model"`。

为了解决这个限制，Anthro Bridge **始终向 Claude Desktop 展示 Anthropic 官方模型名称（`claude-sonnet-4-6` / `claude-haiku-4-5`）作为"外壳"，而实际使用的 LLM（DeepSeek / MiniMax / Kimi）则在 GUI 中选择**。

```
Claude Desktop 侧（始终固定）
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

网关内部（根据 GUI 选择）
  DeepSeek:  Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:   Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:      Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
```

这样可以在通过 Claude Desktop 的模型名称验证的同时，自由切换 DeepSeek、MiniMax 和 Kimi。

### 运行环境

- **Windows 10/11**（支持日语环境）
- 所选提供商的 API 密钥（DeepSeek / MiniMax / Kimi — **只需一个即可**，自 v0.5.0 起）

### 快速开始

#### 1. 安装

从 [Releases](https://github.com/soheidon/anthro-bridge/releases) 下载最新安装程序并运行。

安装程序启动时会显示语言选择界面（可选 English, 日本語, 中文(简体), 中文(繁體), 한국어, Français）。

#### 2. 设置 API 密钥

设置（⚙）-> **API 密钥** 选项卡，输入提供商的 API 密钥并点击 **保存**。
密钥将持久保存为 Windows 用户环境变量。

| 提供商 | 环境变量 |
|----------|---------------------|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |

#### 3. 选择提供商

在仪表板的 **选择 LLM 提供商** 卡片中点击提供商磁贴（DeepSeek / MiniMax / Kimi）。

#### 4. 启动网关

点击标题栏中的 **Start Gateway** 按钮。代理将在 `http://127.0.0.1:4000` 上启动。

#### 5. 配置 Claude Desktop / Cowork on 3P

详细步骤请参阅 [THIRD_PARTY_INFERENCE.zh-CN.md](THIRD_PARTY_INFERENCE.zh-CN.md)。

### 端点

| Method | Path | 说明 |
|--------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/v1/models` | 公开模型列表 |
| POST | `/v1/messages` | Messages API（stream + non-stream）。基于模型的路由 |
| POST | `/v1/messages/count_tokens` | Token 计数（仅支持的提供商） |

### 路由

基于模型的路由：每个请求中的 `model` 字段决定目标提供商和上游模型。

### 语言

支持 6 种语言：English, 日本語, 中文(简体), 中文(繁體), 한국어, Français。

要添加新翻译，只需将语言文件（如 `es.ts`）放入 `gui/src/i18n/lang/` 并重新构建。
详见 [CONTRIBUTING](CONTRIBUTING.md)。

### 配置 (config.json)

提供商设置定义了每个模型的上游模型名称和功能标志。通常无需编辑。
高级用户可通过设置（⚙）-> **Gateway Config** 进行编辑。

| 键 | 说明 |
|-----|------|
| `models.<model>.upstream_model` | 发送到上游的实际模型名称（必填） |
| `models.<model>.thinking` | 当设为 `"disabled"` 时注入 thinking 抑制（可选） |
| `models.<model>.supports_vision` | 按模型的图像支持（默认回退到提供商设置） |
| `models.<model>.supports_video` | 按模型的视频支持（默认回退到提供商设置） |
| `models.<model>.visible` | 是否在 `/v1/models` 和仪表板中显示（默认 `true`） |
| `non_vision_image_policy` | 非 Vision 模型的图像处理: `replace`（占位符）/ `drop`（删除）/ `reject`（错误） |

### 项目结构

```
anthro-bridge/
├── README.md                  英语
├── README.ja.md               日语
├── README.zh-CN.md            中文(简体)
├── THIRD_PARTY_INFERENCE.md   第三方推理指南
├── THIRD_PARTY_INFERENCE.ja.md
├── THIRD_PARTY_INFERENCE.zh-CN.md
├── SPEC.md                    规格说明
├── SPEC.ja.md                 日语
├── SPEC.zh-CN.md              中文(简体)
├── LICENSE                    MIT 许可证
├── config.json                提供商配置
├── .gitignore
└── gui/
    ├── src/                   React 前端 (TypeScript)
    │   ├── components/        UI 组件
    │   ├── hooks/             自定义 Hooks
    │   └── i18n/              多语言支持
    │       └── lang/          语言文件 (en, ja, zh-CN, zh-TW, ko, fr)
    ├── src-tauri/             Tauri 后端 (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 个 Tauri 命令 + 代理生命周期
    │   │   ├── main.rs        入口点
    │   │   └── proxy.rs       axum 代理服务器
    │   ├── resources/
    │   │   └── config.json    打包配置
    │   └── Cargo.toml
    └── package.json
```

### 开发构建

```bash
cd gui
npm install
npm run tauri build    # 生产构建
npm run tauri dev      # 开发模式 (HMR)
```

需要 [Rust](https://rustup.rs/) stable 工具链和 Node.js 24+。

### 故障排除

#### 端口 4000 被占用

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### 图像/视频被拒绝

DeepSeek 不支持图像或视频。图像会自动替换为占位符文本（`non_vision_image_policy: "replace"`）。要原生使用图像，请切换到 MiniMax 或 Kimi。视频始终被拒绝。

### 许可证

MIT — 详见 [LICENSE](LICENSE)。
