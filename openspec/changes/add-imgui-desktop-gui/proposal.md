## Why

proxy-fork 目前只能通过命令行和 TOML 文件操作，启动状态、规则、日志、证书及系统代理缺少统一的可视化入口。项目需要一个低资源占用、可逐步覆盖桌面平台的 Rust GUI；首期以 macOS 为完整交付平台，同时保持现有 CLI 行为不变。

## What Changes

- 新增基于 Dear ImGui 的 `proxy-fork-gui` workspace crate：通过 `imgui-rs` 使用 Rust 编写应用，采用 winit 窗口事件循环、wgpu 渲染和 Tokio 后台运行时，macOS 使用 Metal 后端。
- 将配置模型、可控的代理生命周期及系统代理恢复逻辑整理为 CLI 与 GUI 可复用的核心能力。
- 提供代理启动/停止、运行状态、监听配置、CA 与系统代理开关、规则增删改及有界实时日志。
- 提供桌面托盘、开机自启动、CA 证书安装向导和基于 GitHub Releases 的自动更新；首期完成 macOS 实现，并为 Windows/Linux 保留明确的平台扩展边界。
- 使用 `rust-i18n` 提供简体中文和英文界面，配套 CJK 字体、中文输入法和稳定控件 ID，支持运行时切换语言。
- 使用按需重绘、隐藏窗口停止绘制和日志可见行裁剪控制常驻开销；资源占用通过发布构建实测验收。
- 增加 macOS 应用打包、更新产物校验和关键桌面流程验证。
- 不移除或改变现有 CLI 命令和配置优先级，不包含流量抓包历史、请求编辑器或图表分析。

## Capabilities

### New Capabilities

- `desktop-proxy-control`: 桌面应用中的代理生命周期、配置、规则和有界日志控制。
- `desktop-platform-services`: 托盘、自启动、CA 信任安装、自动更新及平台能力降级行为。
- `desktop-localization`: 简体中文与英文资源、语言选择持久化和运行时界面刷新。

### Modified Capabilities

无。

## Impact

- 新增 `crates/proxy-fork-gui`，并调整 `proxy-fork-core` 与 `proxy-fork-cli` 的配置和启动边界。
- 根 workspace 增加 `imgui`、`imgui-winit-support`、`imgui-wgpu`、winit、wgpu、i18n、桌面集成及更新所需依赖。
- Dear ImGui 内核由 C++ 实现，Rust 绑定包含 `imgui-sys`；开发和 CI 需要对应平台的 C/C++ 编译工具链，发行包包含字体资源及其许可证。
- macOS 需要托盘、LaunchAgent、Keychain、应用打包、签名及更新产物相关流程。
- 现有 CLI、TOML 配置格式、代理规则语义及 Hudsucker 补丁行为必须保持兼容。
