## 1. 固化 CLI 行为并提取共享配置

- [x] 1.1 在 `crates/proxy-fork-cli/tests/config_test.rs` 和现有命令模块测试中补齐配置优先级、默认监听地址、CA 开关、规则解析及 Ctrl+C 清理的回归用例；先运行 `cargo test -p proxy-fork-cli` 记录当前通过基线。
- [x] 1.2 新建 `crates/proxy-fork-core/src/app_config.rs` 和 `crates/proxy-fork-core/src/dirs.rs`，迁入 `FileConfig`、`AppConfig`、`ListenAddr`、`ProxyManagerRuntime`、`RuleItem` 与平台目录计算；为磁盘模型补充 `Serialize`，并从 `lib.rs` 导出稳定类型。
- [x] 1.3 在 core 中实现代理配置的读取、完整校验和同目录临时文件加 `sync_all` 后重命名的原子保存；用临时目录测试有效配置往返、无效端口/缓存/规则拒绝以及失败时原文件不变。
- [x] 1.4 将规则到 `AddressPattern`/`Address` 的转换移入 core，保持当前仅接受 `http`/`https` 的语义；先构造全量新 `ProxyManager`，全部成功后再替换活动 manager，并测试无效规则不会产生部分更新。
- [x] 1.5 将 CA 文件生成函数和默认 CA 路径移动到 core 的现有 `certification` 边界，写入私钥时设置仅当前用户可读写的权限；CLI 的覆盖确认继续留在 `commands/gen_ca.rs`，并运行其现有证书生成测试。
- [x] 1.6 精简 `proxy-fork-cli` 的 `config.rs`、`dirs.rs` 和 `commands/start_proxy.rs`，让 CLI 只负责 Clap 参数覆盖与交互，继续按“CLI > 当前目录 > 用户目录”生成 core `AppConfig`；运行 `cargo test -p proxy-fork-core -p proxy-fork-cli` 验证外部行为未变。

## 2. 实现可控代理服务生命周期

- [x] 2.1 在 `crates/proxy-fork-core/tests/proxy_service_test.rs` 先添加 `Stopped → Starting → Running → Stopping → Stopped`、重复启动拒绝、端口占用进入 `Failed` 的异步测试，测试配置使用无 CA、无系统代理和临时监听端口。
- [x] 2.2 为 workspace 的 `tokio-util` 增加 `rt` feature，新建 `crates/proxy-fork-core/src/proxy_service.rs`，实现具体的 `ProxyService`、`ProxyServiceState`、`ProxyServiceError`、`start`、`stop`、`replace_rules` 和 `subscribe`；由服务先绑定 `TcpListener`、取得实际地址，再交给 [Hudsucker](https://docs.rs/hudsucker/0.24.0/hudsucker/)，并用 [`CancellationToken`](https://docs.rs/tokio-util/0.7.16/tokio_util/sync/struct.CancellationToken.html) 驱动 graceful shutdown。
- [x] 2.3 macOS 使用 SystemConfiguration 保存网络服务 UUID 与完整代理字典，仅在监听成功后接管 HTTP/HTTPS，并保留 SOCKS 等原值、暂时停用 PAC/自动发现；其他平台保留 [`sysproxy` API](https://docs.rs/sysproxy/0.3.0/sysproxy/) 适配。原值和本进程写入值先持久化至 `system-proxy-restore.toml`，在原生偏好锁内比较当前状态、检查提交/应用结果并读回验证，确认恢复成功后才删除记录。
- [x] 2.4 添加恢复记录测试：覆盖遗留记录匹配时恢复、当前代理已被外部修改时不覆盖、系统拒绝恢复时保留记录和错误状态；测试使用可注入的临时记录路径与代理读写函数，禁止修改开发机真实系统代理。
- [x] 2.5 让 CLI 使用 `ProxyService` 启动并等待 Ctrl+C，再调用 `stop`；运行 CLI 集成测试，确认监听、规则、CA、系统代理提示和退出日志仍由同一条路径产生。

## 3. 建立 ImGui 应用壳与本地化

- [x] 3.1 参考 [imgui 0.12 API](https://docs.rs/imgui/0.12.0/imgui/)、[imgui-wgpu 0.28 API](https://docs.rs/imgui-wgpu/0.28.0/imgui_wgpu/) 和 [Cargo 平台依赖](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#platform-specific-dependencies)，新建 `crates/proxy-fork-gui/Cargo.toml` 与 `src/main.rs`；采用 `imgui 0.12.0`、`imgui-winit-support 0.13.0`、`imgui-wgpu 0.28.0`、`winit 0.30`、`wgpu 29` 和 `rust-i18n 4.2.1`，通过 Cargo.lock 固定相容组合，检查 Rust 1.90 与 C++ 工具链；不启用 docking 或多 viewport。
- [x] 3.2 新建 `src/app.rs`、`src/event.rs` 和 `src/view/{dashboard,rules,logs,settings}.rs`，实现 `App`、`AppEvent`、`App::begin/finish` 与 Desktop 事件处理，以及基于 `imgui::Ui` 的即时模式四区导航；编辑缓冲区持久保存在 App，控件使用稳定 ID，启用键盘导航、可见焦点和文字状态，副作用只在动作发生时提交一次。
- [x] 3.3 新建 `src/preferences.rs`，把语言、窗口状态、自动更新选择和上次检查时间保存到独立 `gui.toml`；使用原子写入和显式默认值测试，禁止保存私钥或代理恢复数据。
- [x] 3.4 新建 `locales/app.yml`，按 [`rust-i18n` 4.2 API](https://docs.rs/rust-i18n/4.2.1/rust_i18n/) 提供 `en` 与 `zh-CN`；实现系统 locale 首次选择、英文回退、运行时切换和持久化，并添加测试扫描视图使用的翻译键在英文中全部存在。
- [x] 3.5 在 GUI 中持有单个 Tokio runtime 和共享 `ProxyService`，监听 `watch::Receiver<ProxyServiceState>` 并将状态及后台任务结果通过 winit [`EventLoopProxy<AppEvent>`](https://docs.rs/winit/0.30.12/winit/event_loop/struct.EventLoopProxy.html) 送回主线程；阻塞工作使用 `spawn_blocking`，测试事件到达后状态更新、重复操作被拒绝且绘制不会重新启动任务。
- [x] 3.6 新建 `src/render.rs`，用 winit 管理主线程窗口并通过 `imgui-winit-support` 接入输入/DPI，`imgui-wgpu` 提交 draw data；macOS 选择 wgpu Metal，覆盖 resize、DPI 变化、零尺寸暂停和 surface 重建，在最小窗口验证成功后再接入业务页面。
- [x] 3.7 依据 [`ControlFlow`](https://docs.rs/winit/0.30.12/winit/event_loop/enum.ControlFlow.html) 实现按需绘制：空闲 Wait，输入或后台结果 request_redraw，仅光标闪烁/组合输入/进行中动画使用 WaitUntil；隐藏窗口停止绘制及视觉计时器，恢复显示时绘制最新状态，并验证无持续 Poll 忙循环。
- [ ] 3.8 将可再分发的 CJK 字体及许可证放入 `crates/proxy-fork-gui/assets/fonts/`，按 [`FontGlyphRanges`](https://docs.rs/imgui/0.12.0/imgui/struct.FontGlyphRanges.html) 构建中英文字形 atlas，处理 Retina DPI 和纹理重建；测试翻译标签变化及规则排序后稳定 ID、焦点和未保存文本保持一致。
- [ ] 3.9 新建 `src/ime.rs`，补齐发布版输入适配器缺少的 winit [`Ime`](https://docs.rs/winit/0.30.12/winit/event/enum.Ime.html) 处理：启用输入法、更新候选位置、暂存并显示 preedit、仅一次提交 commit 文本；测试字符去重和焦点切换，并在 macOS 拼音输入法实测候选选择、取消组合、中文编辑及粘贴。

## 4. 完成代理控制、规则和有界日志

- [x] 4.1 实现 Dashboard 的监听地址、CA、系统代理和缓存配置表单；复用 core 校验，运行期间修改需重启的字段只标记待应用，不执行隐式重启。
- [x] 4.2 实现 Rules 页面的新增、编辑、删除和排序；`save_and_replace_rules` 先准备完整 manager，再原子保存主配置，最后无失败地替换活动规则，准备/保存失败时保留旧文件和路由。单独保存 GUI 偏好失败时明确报告“主配置已保存、偏好保存失败”，不误报路由未提交。
- [x] 4.3 新建 `src/logging.rs`，通过 [`tracing-subscriber::Layer`](https://docs.rs/tracing-subscriber/0.3.20/tracing_subscriber/layer/trait.Layer.html) 增加有界 GUI sink；保留容量 2,000 的 `VecDeque`，溢出累计 dropped 数且不阻塞代理，最多每 100 ms 合并唤醒一次可见 UI，隐藏窗口时继续维护有界记录但不请求绘制。
- [x] 4.4 实现 Logs 页面的级别过滤、自动滚动开关、清空和 dropped 计数，过滤后用 [`imgui::ListClipper`](https://docs.rs/imgui/0.12.0/imgui/struct.ListClipper.html) 仅绘制固定行高的可见行，长消息提供详情查看；验证过滤不会复制完整历史，日志不包含请求正文、完整头部、私钥或更新凭据。
- [x] 4.5 为 `App::begin/finish` 与 Desktop 事件处理中的启动、停止、保存配置、保存规则、日志清空和并发操作禁用逻辑增加无窗口单元测试，并运行 `cargo test -p proxy-fork-gui`。

## 5. 接入 macOS 托盘和自启动

- [x] 5.1 新建 `src/platform/mod.rs`、`src/platform/macos/mod.rs` 和 `src/platform/unsupported.rs`；macOS 之外返回结构化 `Unsupported`，并用条件编译保证通用 GUI 不引用 macOS 类型，不建立单实现 platform trait。
- [x] 5.2 使用 [`tray-icon 0.24.2`](https://docs.rs/tray-icon/0.24.2/tray_icon/) 在 macOS 主线程创建菜单栏图标和“显示窗口、启动/停止、退出”菜单，通过 winit `EventLoopProxy` 投递现有 `AppEvent`；窗口关闭默认隐藏并停止绘制，托盘初始化失败时保留普通窗口退出能力，语言切换同步更新托盘文案。
- [x] 5.3 为“退出”实现单一异步关闭流程：禁用重复退出、停止代理、恢复系统代理、保存偏好，全部完成后才结束事件循环；添加状态机测试覆盖代理已停、正在启动和恢复失败三种情况。
- [x] 5.4 新建 `src/platform/macos/launch_agent.rs`，依照 [Apple LaunchAgent 指南](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html)，使用 `plist 1.10` 生成 bundle identifier 为 `io.github.gouzil.proxy-fork` 的用户级 LaunchAgent；使用 `launchctl enable/disable`，仅未加载时 `bootstrap`，禁止 `bootout` 杀死当前 GUI。已加载任务的路径更新在下次登录生效；用临时目录及注入命令测试 plist、禁用覆盖和幂等操作。
- [x] 5.5 在 Settings 接入实际自启动状态与错误反馈；禁止对未安装或路径已变化的 `.app` 写入失效 LaunchAgent，并提供重新注册操作。

## 6. 实现 macOS CA 安装向导

- [x] 6.1 复用 core 已有 OpenSSL SHA-256，让 CA 生成结果返回证书路径、私钥路径和指纹；增加“文件已存在不覆盖”、显式覆盖及第二个文件提交失败时旧私钥回滚测试，日志仅记录证书路径与指纹，不为此增加 core 哈希依赖。
- [x] 6.2 新建 `src/platform/macos/keychain.rs`，根据 macOS [`security(1)`](https://keith.github.io/xcode-man-pages/security.1.html) 使用参数数组构造 `security default-keychain`、`find-certificate`、`add-trusted-cert` 和按指纹删除本应用证书的命令；把权限拒绝、未找到和解析失败映射为不同错误类别。
- [x] 6.3 实现“选择或生成 → 显示指纹与目标 Keychain → 用户确认 → 系统授权 → 重新检测”的 CA 向导；取消授权时保留文件且不修改代理配置，删除操作只能匹配本应用记录的证书指纹。
- [ ] 6.4 为命令参数和输出解析增加纯单元测试；真实 Keychain 冒烟测试必须使用单独测试证书、要求显式人工确认，并在同一检查清单中按指纹移除测试证书，自动测试不得修改登录 Keychain。

## 7. 实现安全更新和 macOS 打包

- [x] 7.1 新建 `src/updater/{mod.rs,github.rs,verify.rs,install.rs}`，使用 `semver 1.0`、GUI 的 `sha2 0.11`、`flate2 1.1` 和 `tar 0.4`，固定仓库 `gouzil/proxy-fork`、bundle identifier `io.github.gouzil.proxy-fork`、稳定版 `v<version>` tag 和 `proxy-fork-gui-<version>-<target>.app.tar.gz(.sha256)` 资产规则；自动检查默认关闭，开启后 24 小时内最多检查一次且排除 prerelease。
- [x] 7.2 使用 [Reqwest 0.12](https://docs.rs/reqwest/0.12.23/reqwest/) 调用 [GitHub Releases API](https://docs.github.com/en/rest/releases/releases)，按当前 macOS 架构和 SemVer 选取资产；通过本地 HTTP 测试服务覆盖最新版、无更新、错误架构、prerelease、限流和离线场景，检查失败不得影响代理。
- [x] 7.3 下载到应用数据目录的临时文件，验证 SHA-256 后解包；随后以参数数组运行 `codesign --verify --deep --strict`、`spctl --assess` 并校验 Info.plist 中的 identifier/version，任一失败都删除暂存文件且不停止当前代理；发布配置遵循 [Apple notarization 指南](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)。
- [ ] 7.4 实现内部 updater 模式：确认目标可写后停止代理并恢复系统代理，等待旧进程退出，在同一文件系统将旧 `.app` 改名为备份、移动新 bundle、启动新版本；用临时假 bundle 测试成功替换、目标不可写和启动失败回滚。
- [x] 7.5 在 Settings 和更新对话框接入手动检查、自动检查 opt-in、版本说明、下载进度、确认安装和失败恢复；不可写的 `/Applications` 只打开已验证产物位置并给出人工替换说明，不加入高权限 helper。
- [x] 7.6 新增 `crates/proxy-fork-gui/packager.toml`、macOS 应用/托盘图标资源和 `.github/workflows/release-gui.yml`，使用 [`cargo-packager 0.11.8`](https://docs.rs/cargo-packager/0.11.8/cargo_packager/) 为 `aarch64-apple-darwin`、`x86_64-apple-darwin` 生成 `.app` 与更新归档、SHA-256，随包包含 CJK 字体及许可证，CI 配置 `imgui-sys` 所需原生工具链；从 CI secrets 注入签名和 notarization 凭据且不打印凭据。
- [x] 7.7 在本机执行未签名打包 dry-run，验证 bundle identifier、版本、两种目标的资产命名和 checksum 生成；正式发布 job 在缺少签名凭据时必须失败而不能上传未签名产物。

## 8. 跨平台边界、文档与最终验证

- [ ] 8.1 更新 CI，使 `proxy-fork-gui` 在 macOS 执行测试和打包检查，在 Windows/Linux 至少执行通用 GUI `cargo check` 与单元测试；配置各平台 `imgui-sys` C++ 工具链与 winit/wgpu 依赖，并检查 GUI 的 Rust 1.90 兼容性；未实现的平台服务显示本地化不可用状态。
- [x] 8.2 添加 macOS 手工冒烟清单，覆盖 Metal 渲染、Retina/缩放、中文字体与拼音 IME、切语言保留焦点和输入、键盘导航、窗口隐藏后停止绘制与托盘恢复，以及正常退出恢复系统代理、端口占用、遗留恢复记录、LaunchAgent、CA 授权取消、离线更新和更新回滚；每项注明验证后如何恢复系统状态。
- [x] 8.3 更新根 `README.md` 的 workspace 结构、ImGui Rust 绑定及 C++ 编译前置条件、GUI 构建运行方式、macOS 首期支持范围、VoiceOver 限制和 `~/Applications` 更新约束；Windows/Linux 明确标注平台集成尚未交付。
- [x] 8.4 执行 `cargo fmt --all --check`、`cargo test --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings` 和 `cargo check --workspace --all-targets --locked`；检查 `cargo tree -p proxy-fork-gui -d` 中 imgui/winit/wgpu 的实际版本组合、日志缓冲上限，确认现有 CLI 与 core 测试通过。
- [ ] 8.5 在干净 macOS 环境记录 release 应用包与字体 atlas 大小、RSS、CPU/GPU 占用和实际绘制次数，分别测量可见空闲、编辑/IME、持续日志及隐藏托盘场景；验证无输入/计时器时事件循环等待、隐藏时停止绘制，不根据框架名称推定资源占用。
