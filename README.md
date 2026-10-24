# proxy-fork

一个高性能的 Rust 实现的 HTTP 代理工具，支持动态规则配置、MITM 拦截和系统代理管理。

## 特性

- 🚀 高性能异步代理服务器
- 🔧 动态规则配置（支持精确匹配和模式匹配）
- 🛡️ MITM（中间人）拦截支持
- 🌐 系统代理自动配置
- 📁 多级配置支持（用户目录、项目目录、CLI 参数）
- 🔍 详细的日志和调试选项
- 📊 性能基准测试
- 🖥️ Rust ImGui 桌面界面：状态控制、规则管理、日志、设置，中英文本地化

## 安装

### 方式一：直接从 Git 安装（推荐）

```bash
cargo install --git https://github.com/gouzil/proxy-fork.git proxy-fork-cli
```

安装完成后直接运行：

```bash
proxy-fork-cli --help
```

### 方式二：从源码构建

确保你已安装 Rust 1.90.0 或更高版本：

```bash
git clone https://github.com/gouzil/proxy-fork.git
cd proxy-fork
cargo build --release
```

然后运行：

```bash
./target/release/proxy-fork-cli
```

### 方式三：本地安装

如果你已经克隆了仓库，可以本地安装：

```bash
cargo install --path crates/proxy-fork-cli
```

然后可以直接使用：

```bash
proxy-fork-cli
```

## 用法

### 基本运行

更多用法可以查看 [USAGE](./crates/proxy-fork-cli/USAGE.md)

### 桌面界面（macOS 首期）

需要 Rust 1.90+、Xcode Command Line Tools（`xcode-select --install`）及 C/C++ 编译器。
界面使用 [imgui-rs](https://github.com/imgui-rs/imgui-rs)、winit 0.30、wgpu 29；macOS 使用 Metal。
Dear ImGui 的 C++ 源码由 `imgui-sys` 编译，不需要另装 ImGui。GUI 内置 OpenSSL，不依赖 Homebrew 动态库。

```bash
cargo run --locked -p proxy-fork-gui
```

首次使用可在状态页关闭 CA，以普通 HTTP/CONNECT 代理启动；HTTPS 内容改写需要先生成并手动信任 CA。
系统代理默认关闭，CA 信任、自启动和自动更新均需主动启用。请勿将生成的私钥提交到版本库。
关闭窗口会隐藏到菜单栏；使用托盘“退出”才会停止代理并恢复本应用接管的系统代理。

配置目录沿用 CLI 的平台目录（macOS/Linux 默认 `~/.config/proxy-fork`，尊重 `XDG_CONFIG_HOME`）：
`config.toml` 保存代理配置，`gui.toml` 单独保存语言、窗口尺寸等桌面偏好；GUI 不读取当前工作目录的项目配置。
概览字段在回车、失焦或离开页面后自动校验并写入，开关立即保存；语言、主题和自动检查更新也自动保存，不再提供单独保存按钮。监听地址/CA/系统代理选项仍需手动重启代理才生效；尚未生成 CA 时也可保存设置，启动时仍严格检查证书。规则在字段结束编辑、关闭弹窗、删除或排序后独立保存；有效规则整体替换活动规则，已有连接继续使用原规则。规则和概览不会互相提交未完成的草稿；无效输入或写入失败保留旧配置与活动规则。
日志采用顶部操作栏、级别与文字筛选、双行记录列表；支持暂停显示、时间正/倒序、详情和清空，继续只采集有界的生命周期日志，不记录请求内容。
规则优先级沿用核心：精确规则优先，模式规则按列表顺序匹配；相同精确键后写覆盖前写。

本地打包（在 Mac 上运行，输出到 `target/gui-package/<target>/<profile>/`）：

```bash
cargo install cargo-packager --version 0.11.8 --locked
bash scripts/package-gui.sh aarch64-apple-darwin release
# Intel Mac 使用 x86_64-apple-darwin
```

将生成的 `Proxy Fork.app` 放到 `~/Applications` 再启用登录自启动。移动应用后需重新注册自启动。
应用可写目录支持更新替换；不可写的 `/Applications` 会打开已验证产物供人工替换，不安装提权 helper。
自动更新默认关闭；开启后启动时最多每天检查一次 GitHub 稳定版。更新必须通过 checksum、Developer ID 签名、
notarization、bundle id、版本和相同签名团队校验；本地未签名测试包不能自动更新。
正式 tag 发布要求配置签名和 notarization secrets，缺少凭据会失败，不会发布未签名包。

Windows/Linux 已保留通用 GUI 编译与测试入口，但托盘、系统集成、证书向导及更新安装仅交付 macOS 实现。
Linux 构建需要 `pkg-config`、C++/Clang、`libxkbcommon-dev` 和 `libwayland-dev`。
支持键盘导航，但 Dear ImGui 尚不提供完整的 VoiceOver 辅助功能树，不能宣称完整无障碍支持。
验证范围、资源占用与需人工执行的系统集成检查见 [macOS 冒烟清单](docs/gui-macos-smoke.md)。

## 开发

### WebSocket 代理补丁说明

项目在 `Cargo.toml` 中通过 `[patch.crates-io]` 引入本地 `crates/hudsucker` 补丁版本。

这样做的原因是：WebSocket 代理需要等上游握手完成后，才能知道实际协商出的 `Sec-WebSocket-Protocol`。代理不能直接回显客户端请求里的第一个子协议，否则客户端和上游可能看到不同的协议选择。

当前补丁保持最小范围：

- `hudsucker` 会先连接上游 WebSocket，再把上游实际返回的 `Sec-WebSocket-Protocol` 同步到给客户端的 `101 Switching Protocols` 响应
- WebSocket 规则改写 URI 后会同步更新上游握手请求的 `Host` 头

后续维护建议：

- 如果 Hudsucker 上游发布等价修复，优先回到官方版本并移除本地 patch
- 升级依赖后请重点回归测试：`wss` 握手、子协议协商、消息收发稳定性

### 项目结构

```
crates/
├── hudsucker/          # 本地上游补丁，HTTP/WebSocket 传输
├── proxy-fork-cli/     # CLI 工具
├── proxy-fork-core/    # 共享配置、规则、CA、可控代理服务
└── proxy-fork-gui/     # ImGui 桌面界面、macOS 集成、更新
```

### 运行测试

```bash
cargo test
```

### 性能基准测试

```bash
cargo bench
```

## 贡献

欢迎提交 Issue 和 Pull Request！
