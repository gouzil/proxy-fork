# Implementation ledger — add-imgui-desktop-gui

## 交付状态

2026-10-22：本机可运行的 macOS ImGui 桌面预览；不是已签名、公证并完成全部系统/硬件验收的正式发行版。`tasks.md` 共 46 项，40 项完成，6 项保留待验收，不能据此归档为全部完成。

实现位于用户现有 checkout 的 `codex/imgui-desktop-gui` 分支，未提交、推送或创建 PR。原有 `.codegraph/`、规划文档、`proxy-fork copy.toml`、`test.py` 等未跟踪内容保留。没有写入开发机系统代理、登录 Keychain、LaunchAgent，也没有发布 Release。

运行：

```bash
cargo run --locked -p proxy-fork-gui
```

本地产物：

- Apple Silicon：`target/gui-package/aarch64-apple-darwin/release/Proxy Fork.app`
- Intel：`target/gui-package/x86_64-apple-darwin/release/Proxy Fork.app`
- 两个同目录 `proxy-fork-gui-0.1.0-<target>.app.tar.gz` 及 `.sha256`。

默认不自动启动代理、不接管系统代理、不自动检查更新。GUI 读取用户配置，CLI 保留原有 CLI > 当前目录 > 用户目录覆盖顺序。隔离运行与恢复步骤见 `docs/gui-macos-smoke.md`。

## 已实现范围与裁定

- 四个主页面：概览（状态控制）、规则管理、日志、设置；CA 向导和更新属于对话框。
- 共享 core 配置/目录/CA 与 `ProxyService`：可重复启停、先绑定监听器、串行生命周期、graceful shutdown、状态订阅、配置校验和原子保存。CLI 和 GUI 走同一生命周期。
- 规则提交使用 `save_and_replace_rules`：先构造完整 manager，再保存主配置，最后无失败地交换活动 manager；不建立多余的 `handle_event` 转发层，App 使用 `begin/finish`，Desktop 接收事件并运行后台动作。
- GUI 单个 Tokio runtime、`EventLoopProxy` 回主线程；绘制不重复执行副作用。操作互斥，退出可排队，失败时保留窗口和恢复信息。
- en/zh-CN、英文回退及缺键诊断、随包 NotoSansSC 与 OFL、运行时切换、持久草稿和稳定控件 ID。动态补字形将启动 atlas 从全字库约 64 MiB 缩至 1 MiB；真正拼音输入和混合 DPI 仍需人工验证。
- IME preedit/commit、提交去重、候选位置与剪贴板适配。纯状态测试不能替代系统输入法实测。
- winit 使用 Wait/WaitUntil 按需绘制，隐藏时不请求视觉重绘；输入后持续到 ImGui 分帧处理的输入稳定再等待，不限制为固定两帧。日志直接维护最多 2,000 条、每条最多 2,048 字符的有界历史，生产者不等待，100 ms 合并唤醒，列表裁剪及详情显示。
- macOS 托盘、LaunchAgent、Keychain 原生边界与固定来源的 GitHub 更新器；其他平台显示本地化不可用状态，不建立单实现平台 trait。
- GUI 的 OpenSSL 使用 vendored 构建；两架构包的 `otool -L` 仅包含系统库/框架，没有 Homebrew 动态库路径。

## 评审和失败路径回归

已完成一轮独立最终评审及修复，不用重复评审替代验收。修复记录：

1. **系统代理快照和恢复**：macOS 改用 SystemConfiguration 原生网络服务 UUID + 完整 Proxies XML 字典，不再用会丢失协议差异的 sysproxy 合并元组。只接管 HTTP/HTTPS，不把 HTTP 监听器写入 SOCKS；保留其他字段，接管期间暂时关闭原 PAC/自动发现。遇认证/用户/密码字段拒绝接管。恢复记录先同步到磁盘，使用排他锁及偏好锁比较预期状态；检查原生 commit/apply 返回值并重新读取，确认还原成功才删记录。切换活动网络服务后仍恢复原 UUID。纯测试覆盖字段保留、外部冲突、原生拒绝、未实际还原、记录锁和错误 UUID。
2. **登录项不能杀死当前 GUI**：取消 `bootout`，禁用时移除 plist 并 `launchctl disable`，启用时先 enable，仅未加载时 bootstrap。状态查询识别 disabled override；已加载 job 的路径更新下次登录生效。注入命令测试覆盖不自杀；GUI 的 SIGINT/SIGTERM 都进入有序退出。
3. **CA 两文件提交**：旧私钥备份保留至证书提交成功；第二个 rename 失败时回滚私钥，连回滚也失败则报告并保留备份路径。目录占用证书路径的回归先失败、修复后通过。不承诺两文件跨断电的事务原子性。
4. **CA 所有权和确认**：显示已解析的实际 Keychain 路径，确认绑定路径和指纹；执行信任前先持久记录候选归属，失败后仍能按指纹清理。不能静默丢弃旧 CA 记录，需先移除旧信任再覆盖/切换。缺路径、持久化失败、不同指纹和信任失败均有纯测试。
5. **保存结果必须符合提交边界**：主配置/路由提交后，GUI 偏好失败单独返回 warning，不再显示成“配置未保存”。普通原子保存以 rename 为逻辑提交点，后续目录 fsync 失败报告 durability warning；系统代理恢复日志仍要求在真实系统操作前完成目录 fsync。
6. **更新可恢复**：手动更新导出到持久的 `manual-update-*`，退出不会丢失已验证文件；新版本启动失败后还原并重开旧版，显示回滚提示。测试覆盖原临时目录释放后导出仍存在、替换成功保留备份、启动失败回滚和旧版重启回调。
7. **交互与边界**：托盘只在左键松开恢复窗口，悬停/移入移出不弹窗；未支持平台禁用安装操作并解释原因。CLI 继续跳过并记录无效规则，GUI 完整配置校验保持严格。修复原 host-only 精确匹配与规则热替换缓存失效，用真实回环 HTTP 请求回归。
8. **真实归档兼容**：把本次两个实际 `.app.tar.gz` 交给更新器解包时发现 AppleDouble `._Proxy Fork.app` 被安全校验拒绝。修改生产端为 `COPYFILE_DISABLE=1 tar`，保留解包路径/类型/链接限制；双架构重打包后实际解包测试均通过，发布 workflow 在上传前执行相同检查。

## 初版自动验证（2026-10-22）

以下命令均在最终代码状态执行成功；本段不是远端 CI 结果。

| 命令/检查 | 结果 |
| --- | --- |
| `cargo fmt --all --check` | 通过 |
| `cargo test --workspace --locked` | 19 个 suite，84 passed，0 failed，5 ignored |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 通过 |
| `cargo check --workspace --all-targets --locked` | 通过 |
| `cargo +1.90.0 check --workspace --all-targets --locked` | 通过 |
| `cargo tree --locked -p proxy-fork-gui -d` | 检查相容组合：imgui 0.12.0、imgui-winit-support 0.13.0、imgui-wgpu 0.28.0、winit 0.30.13、wgpu 29.0.4；未出现这些主库的多版本混用 |
| `cargo test -p proxy-fork-core --lib current_network_snapshot_round_trips --locked -- --ignored` | 显式 opt-in 的真实 macOS 只读快照往返通过；没有原生写入/授权 |
| `PROXY_FORK_GUI_TEST_ARCHIVE=<绝对归档路径> cargo test -p proxy-fork-gui --test updater_test packaged_update_archive_is_accepted --locked -- --ignored` | 最终 ARM/Intel 归档各 1 passed；不仅测试合成 fixture |
| `shasum -a 256 -c <asset>.sha256` | 两架构通过 |
| `bash -n scripts/package-gui.sh` | 通过 |
| 四个已更改 workflow 的 Ruby `YAML.parse_file` | 语法通过；本机 Ruby 有 PATH/ffi 环境警告，不是 workflow 校验失败 |
| `openspec validate add-imgui-desktop-gui --strict` | 通过；status 的 4/4 仅表示规划产物齐全，不表示 46 项实现任务全部完成 |

常规测试的 5 ignored：3 个已有文档示例、1 个真实只读网络快照、1 个实际包解包。后两项已按上表显式单独运行；没有用常规测试假装执行系统写入。

本地日志：`/private/tmp/proxy-fork-final-tests.log`、`proxy-fork-clippy.log`、`proxy-fork-final-check.log`、`proxy-fork-final-msrv.log`、`proxy-fork-package-release.log`、`proxy-fork-package-intel.log`。这些临时日志可能被系统清理；可用上面命令复现。

打包命令 `bash scripts/package-gui.sh <target> release` 两架构通过。Mach-O 分别为 arm64/x86_64，Info.plist 均为 `io.github.gouzil.proxy-fork` / `0.1.0`。ARM `.app` 约 29 MiB、压缩包 17,979,197 bytes；Intel `.app` 约 31 MiB、压缩包 17,755,380 bytes。字体源与许可证约 17 MiB（已嵌入，不是再额外加到包大小）。正式发布模式缺签名凭据时提前失败的 gate 已验证；没有执行签名、公证、上传或真实在线更新。

## 实际窗口和资源观察

使用隔离的 XDG 配置/数据目录，无 CA、无系统代理；开发期监听端口 `18798`。已观察实际 Metal 窗口四页、中英文切换、中文粘贴、跨页保留草稿、代理启动及停止的监听出现/消失。最终 release 窗口中文标签正确，缺字问题已修复。输入验证包含开发期包，不代表每次 release 构建都重跑全部人工步骤。

以下为开发机观察，不是干净机器的标准基准，也不能用于断言 ImGui 比 iced/GPUI 更省资源：

| 场景 | 时间和实际绘制 | 进程采样 |
| --- | --- | --- |
| 可见 release，包含页面/设置操作 | 2026-10-22 07:34:19Z 至 07:42:41Z，约 8m22s，79 帧，atlas 1,048,576 bytes | RSS 81,952 → 103,856 KiB，累计 CPU 0.55 → 0.91s；采样瞬时 CPU 0.0% |
| 隐藏 release，`--background` | 07:49:13Z 至 08:03:23Z，约 14m10s，0 帧，atlas 1,048,576 bytes | 运行 5m19s 时 RSS 66,800 KiB / CPU 0.32s；9m52s 时 RSS 45,632 KiB / CPU 0.39s；采样瞬时 CPU 0.0% |

退出日志为 `/private/tmp/proxy-fork-gui-final-visible.log` 和 `proxy-fork-gui-final-hidden.log`。两个实例均通过 SIGTERM 正常结束；收尾检查没有 GUI 测试进程、端口 `18798` 没有监听。隐藏启动 0 帧不等于已实测托盘隐藏→恢复全过程；`ps` 的 CPU 值也不能证明 GPU/能耗为零。

## 保留待验收和已知限制

- **3.8**：字体、许可证、动态 atlas、稳定 ID/草稿已实现；混合 DPI、切语言/排序后真实键盘焦点与光标连续性未完成硬件验收。
- **3.9**：IME 适配与去重测试已实现；真正系统拼音的候选选择、取消和焦点切换尚未实测。
- **6.4**：Keychain 参数/解析/归属测试已实现；实际登录 Keychain 信任、取消授权、按指纹移除需要明确人工授权，未执行。
- **7.4**：替换/备份/启动回滚及重开旧版已有测试；不可写安装目录的完整验收和同团队签名真实更新未完成。首次 target→backup rename 失败时旧 bundle 保持原位，但 helper 当前不自动重开旧版，需人工启动。
- **8.1**：macOS/Windows/Linux workflow 和 Rust 1.90 job 已配置；本地 macOS/MSRV 通过，远端 CI、Windows/Linux 实际编译/测试与平台集成尚无结果。Intel 只完成交叉构建和产物检查，没有 Intel 硬件运行。
- **8.5**：已有上表开发机观察；干净机器、持续日志/代理负载、真正 IME、GPU/能耗的正式基准未完成。

此外，实际系统代理写入、授权取消、网络切换恢复、LaunchAgent 登录循环及托盘隐藏恢复仍以手工清单为准；没有因纯测试通过而宣称系统验收通过。自动更新启动时若已有互斥操作在执行，本次自动检查可被跳过，仍可手动检查。Dear ImGui 无完整原生辅助功能树，键盘导航不是 VoiceOver 支持。

## Clash Verge 布局调整（2026-10-23）

- 固定侧栏、浅色工作区、双列/单列响应式概览卡片；保留四页与中英文，不新增网络客户端功能或依赖。
- 静态 Regular 字体修复 Thin 默认轮廓，正文增大到 18 logical px；补充字体来源/生成方式与回归测试。
- 连续三次点击的真实 ImGui 回归先失败（仅到 Rules，未到 Settings），修复后通过；另测连续按键、滚轮、文本输入及重新进入空闲状态。字体与卡片几何回归也已通过。
- 当前代码重新执行 `cargo test --workspace --locked`：21 suites、88 passed、0 failed、5 ignored；fmt、workspace 严格 Clippy、all-target check 和 Rust 1.90 check 通过。
- Apple Silicon release 重新构建，通过现有 bundle 刷新二进制、adhoc 签名与归档；`codesign --verify --deep --strict`、真实归档解包回归及 SHA-256 校验通过。当前 PATH 缺少 cargo-packager，因此不把这次手动刷新记成完整打包脚本通过。Intel 仍是初版产物，未刷新本轮视觉修改；没有正式签名、公证或发布。
- 源图与实际窗口均按 940 × 700 logical（含标题栏）、2× Retina 比较；最终整体与局部对照见根目录 `design-qa.md`。640 × 480 最终字体的顶部没有裁切；上一字号已实际滚动到底部，最终包再次滚动/点击受原生自动化窗口激活限制，未计为完整原生交互复验。
- 为用户保留隔离配置的停止状态概览预览；端口 18798 没有监听，没有写入日常配置、系统代理、Keychain 或 LaunchAgent。本节替代初版“收尾没有预览进程”的描述。

## 主题、品牌与 CA 空状态（2026-10-23）

- 设置增加跟随系统、浅色、深色；旧偏好默认跟随系统，选择立即应用并通过现有串行偏好保存流程持久化。系统主题事件只影响跟随模式；原生标题栏、ImGui 控件、卡片、侧栏和弹窗同步配色，不新增依赖。
- 最终审查发现忙碌操作失败会跳过偏好保存；改由 App 在成功/失败后统一决定排队动作，退出优先。自动保存保留原错误、追加保存失败；保存期间再次修改会再存一次，没有新修改时不自旋重试。5 个真实模型/磁盘回归先复现缺失排队与错误丢失，再通过；复审未发现本轮剩余 actionable 问题。
- 删除左上角“本地代理工作台”及翻译键，仅保留 Proxy Fork。修复共享页面 child ID 导致的跨页滚动继承，并按两个最小卡片宽度计算双列阈值，避免设置页滚动条把概览锁在单列。
- 只有默认 CA 的 NotFound 被解释为“尚未配置”，保留生成入口；自定义路径、证书损坏、NotADirectory、PermissionDenied 继续报错。刷新前清理旧指纹/目标/确认，缺失状态仍保留记录的证书所有权，不自动生成或安装信任。
- 新增回归先复现旧偏好无主题、检查失败仍保留信任确认、686px 可用宽度误判为单列，以及滚动设置后概览 Start 按钮位置被继承。修复及排队保存收尾后全工作区 23 suites、99 passed、0 failed、5 ignored；fmt、严格 all-target Clippy、all-target check、Rust 1.90 check 通过。主题往返、解析/覆盖、配色切换/对比度、缺失/损坏/权限错误也有覆盖。
- 最终 ARM release 已重新构建，现有 `.app` 刷新并 adhoc 签名；签名校验、无 Homebrew 动态库检查、真实更新归档解包和 checksum 通过。归档 SHA-256：`8cf929d011096a8a336ff1c6691596a939846ed56357c6f83ed312e580e71ca2`。仍未刷新 Intel，没有运行完整 cargo-packager 或正式签名/公证/发布。
- 实际隔离窗口验证了深浅色即时切换、标题栏、重启保留 Dark、中英文切换、从设置返回概览的双列布局，以及“尚未配置”对话框。最终截图 `32`–`36` 见 `design-qa.md`。电脑锁屏中止后续原生操作；最终运行时切回跟随系统、其余页面/最小窗口完整复验仍未完成。没有改变系统外观、系统代理、Keychain 或 LaunchAgent；没有生成 CA，端口 18798 未监听。
- 上述截图早于最后的排队保存修正；最终包未在锁屏期间重新打开，新增失败路径以自动测试为证据。替换包前已通过 SIGTERM 正常停止隔离预览（39 帧，atlas 524,288 bytes），收尾无 GUI 测试进程或 18798 监听。本节替代上一轮保留预览进程的状态。当前 shell 缺少 OpenSpec CLI，没有宣称本轮重新执行规划验证。

## 对齐与侧栏间距收尾（2026-10-23）

- 卡片的标题、说明和表单仍左对齐；启动/停止改为 96px 紧凑按钮，管理规则/证书按中英文标签自然宽度绘制，按钮在卡片左侧，文字居中。右上角保存、忙碌禁用和实际动作不变。
- 品牌左边缘与导航文字对齐，字号比例 1.2 → 1.35。导航由固定 y=106 改为实际品牌高度后加 12px，消除删除副标题后残留的大空隙；无新依赖或抽象。
- 真实 ImGui 回归先复现旧固定导航位置和铺满按钮的空白区域误触，再验证中英文空白区域不触发操作、按钮仍正确启动/进入规则/检查 CA；更新连续输入和独立页面滚动测试的点击位置。复审未发现本轮 actionable 问题。
- 最终代码重新执行 `cargo test --workspace --locked`：23 suites、99 passed、0 failed、5 ignored；`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、all-target check 与 Rust 1.90 check 通过。
- ARM release 已重新构建并刷新 `.app`，adhoc 签名校验通过；新归档显式执行 `packaged_update_archive_is_accepted -- --ignored` 通过，并通过 `.sha256` 校验。当前 SHA-256：`fd0694d14d7962f21554466266b918ab777572d98fb4ba00f042c7c4129f7cbc`。Intel 未刷新，完整 cargo-packager、正式签名、公证和发布未执行。
- 最终包已重新打开：`37`/`38` 是 940×700 logical、2× 的中英文概览，`39` 是配置为 640×480 内容区后的实际顶部；放大的品牌未裁切，导航、退出和固定保存入口可见。截图见 `design-qa.md`，本轮这些观察替代上一节的“最终包未重新打开”状态。
- 原生拖动未成功改变尺寸；最小尺寸是退出后修改隔离偏好并重新启动验证，不是拖动缩放验收。滚动及 Raise 后仍无到底部的画面变化，故不计为底部滚动通过；不以自动测试替代原生交互验收。隔离预览通过 SIGTERM 正常停止（最小窗口 23 帧、atlas 1,048,576 bytes），无 GUI 测试进程或 18798 监听，隔离窗口偏好恢复为 940×668；未触碰日常配置、系统外观/代理、Keychain 或 LaunchAgent，未生成 CA。OpenSpec CLI 仍不可用，没有宣称重新验证。

## 侧栏底部清理（2026-10-23）

- 删除左下角“退出”按钮和侧栏不再需要的动作参数，唯一调用点同步调整；保留 macOS 托盘的退出入口、翻译、忙碌禁用及原有退出清理流程。
- 本地服务、状态、版本号放在同一文本分组，三行与导航及品牌的 x=32 左边缘对齐，不再在换行后退回 x=14；行距收紧为 6px，按实际行高锚定底部并保留留白，不再为退出按钮预留高度。
- 真实 ImGui 点击回归先失败：旧底部区域发出 `[Exit]`；删除后，中英文在 940×700、640×480 的该区域均不产生动作。保持现有草稿、导航、独立滚动和卡片按钮回归。全工作区 23 suites、99 passed、0 failed、5 ignored；fmt、严格 all-target Clippy、all-target check、Rust 1.90 all-target check 通过。局部审查未发现问题。
- ARM release 重新构建，刷新现有 `.app` 并通过 adhoc 签名校验；实际更新归档解包回归和 checksum 通过。当前归档 SHA-256：`3c503178493488fe4d5f5030b70e99471e4039f97abd2fb57493e2912bdf27f5`。Intel 未刷新，没有正式签名、公证、发布或完整 cargo-packager 运行。
- 原生截图 `40-sidebar-footer-wide-zh.jpg`（1880×1400、中文 940×668 内容区）和 `41-sidebar-footer-min-en.jpg`（1280×1024、英文 640×480 内容区）均为 2×，三行对齐、无退出按钮；英文最小窗口无换行、裁切或导航重叠。最小窗口通过隔离偏好重启设置，不作为拖动缩放验收。截图路径见 `design-qa.md`。
- 两次隔离预览均通过 SIGTERM 正常退出，收尾无 GUI 测试进程或 18798 监听；隔离偏好恢复为中文 940×668。没有改变日常配置、系统外观/代理、Keychain 或 LaunchAgent。托盘退出未在本轮实际调用，其他平台和既有完整原生交互清单仍待验收；没有重新执行 OpenSpec CLI 验证。

## 登录启动单行开关（2026-10-23）

- 按用户第二张参考图，将登录启动收成左标题、右侧 42×26 logical px 圆角开关；关闭灰色、开启沿用蓝色强调色，白色圆形滑块。状态及下次登录生效说明移入悬停提示，移除常驻的启用/停用/刷新按钮；中英文与现有键盘焦点保留，不新增依赖、配置字段或平台处理器。
- 空闲进入设置时只读检查一次，已知状态请求相反操作，系统返回前不提前改变显示状态；普通未知状态点击仍只读检查，不按帧重试。审查发现旧注册路径失效时可能永远无法离开未知态，已补上错误未知态的显式刷新/启用/停用菜单；用户不选择操作就不写设置。复核未发现剩余 Critical/Important 问题。
- 真实 ImGui 回归先复现旧界面没有开关，以及错误未知态重复失败查询的缺口，再验证中英文开启/关闭动作、确认结果前不变、忙碌无动作、未知只读及恢复菜单三项点击。输入结算夹具同步允许设置页出现时的检查，并区分中途仅绘制按键测试帧造成的页面重新出现，不放宽成逐帧查询。最终全工作区 23 suites、99 passed、0 failed、5 ignored；fmt、严格 all-target Clippy、all-target check、Rust 1.90 all-target check 通过。
- ARM release 重新构建，刷新现有 `.app`，adhoc 签名校验通过；实际归档解包回归另行 1 passed，SHA-256 校验通过。当前归档 SHA-256：`46090b14072bcfab2214a89e7a20506da7892414b26a43138aab162ff8626fce`。未刷新 Intel，没有完整 cargo-packager、正式签名、公证或发布。
- 原生截图 `42-autostart-switch-settings-zh.jpg`（1880×1400、中文 940×668 内容区）与 `43-autostart-switch-settings-en.jpg`（1280×1400、英文 640×668 内容区）均为 2×，显示浅色关闭态、标题与开关同一行，窄窗口无文字重叠或裁切。英文通过隔离偏好重启获取；本轮点击语言选项未得到变化，不宣称运行时切语言通过。未点击真实开关；开启态动作及错误菜单由无平台写入的 UI 测试验证。完整最小高度滚动、拖动缩放、原生键盘及既有系统/硬件清单仍待验收。
- 两次预览通过 SIGTERM 正常退出（43/90 帧，atlas 均为 1,048,576 bytes）；隔离偏好恢复为 zh-CN、940×668，无 GUI 测试进程或 18798 监听。没有启停登录项、改变系统外观/代理、Keychain 或日常配置。截图路径见 `design-qa.md`；当前 shell 无 OpenSpec CLI，没有新的 CLI 验证结论。

## 设置页分组卡片（2026-10-23）

- 按用户后续整页参考，将设置改为系统、应用、更新三张独立卡片；宽屏系统/更新位于左列、应用位于右列，可用宽度小于 656 logical px 时按系统/应用/更新单列排列。统一 48px 行高、左侧标签、右侧控件及 16px 卡片间距，设置页外层改用工作区底色；其他页面不变。
- 语言改用真实下拉框，主题为浅色/深色/系统三段选择；自动检查复用已有圆角开关，频率说明移入悬停提示。保存偏好、手动检查、更新对话框、登录状态错误恢复及忙碌/平台限制保留，不新增依赖或配置字段。页面出现时的只读登录检查不依赖卡片是否被裁剪，并让当帧显式动作优先；页面与卡片 ID 不随语言变化。
- 真实 ImGui 点击回归先复现旧布局无法在新行位置操作，再通过；覆盖中英文登录开关、错误恢复、三段主题、自动检查、手动保存/检查、忙碌禁用、下拉切语言及窄窗重排。切换语言不重建页面、不重复查询登录状态。全工作区 23 suites、99 passed、0 failed、5 ignored；fmt、严格 all-target Clippy、all-target check、Rust 1.90 all-target check 均通过。局部只读代码审查没有发现明确问题；原生字体/裁剪/键盘等不能据此视为通过。
- ARM release 重新构建并刷新现有 `.app`，adhoc 签名验证通过，实际更新归档解包回归另行 1 passed，SHA-256 校验通过。当前归档 SHA-256：`36c144ecc24f38d1b46b5d0a41f8703a283a7e556845ad7ece336a85abc59470`。未刷新 Intel，没有正式签名、公证、发布、完整 cargo-packager 或新的 OpenSpec CLI 验证。
- 原生新截图 `44-settings-cards-light-zh.jpg`（1880×1400、中文 940×668 内容区、2×）显示浅色关闭态的三张卡片，无标签/控件重叠。运行时下拉和主题点击未得到稳定变化；后续英文宽窗口、中文 640×760 窄窗口重启均仅捕获空白内容，退出记录都是 0 帧，尚未导航至设置页。对应 `45-settings-cards-en-capture-blocked.jpg`、`46-settings-cards-narrow-capture-blocked.jpg` 是阻塞证据，不是布局通过证据；原因未确诊，不将其归因于语言、布局或捕获工具。完整英文/窄窗原生复验、滚动、拖动缩放、键盘及既有系统/硬件清单仍未完成。
- 三次隔离预览均已 SIGTERM 正常退出（46/0/0 帧，atlas 均 1,048,576 bytes）；隔离偏好恢复为 zh-CN、浅色、940×668。未实际启停登录项、检查在线更新、修改系统代理/外观、Keychain 或日常配置。截图路径与整体验收阻塞状态见 `design-qa.md`。

## 规则列表与证书分组弹窗（2026-10-23）

- GUI 继续只加载用户级配置，本轮不改变配置入口、CLI、core 或系统处理器；保留四页、中英文及既有主题。规则页改为 76px 双行摘要、顺序号、编辑与更多菜单，新增/编辑用稳定 ID 打开分组草稿弹窗，关闭保留草稿，保存并应用才校验、写入和生效。
- 证书管理改为居中模态窗口，分状态、可复制只读信息、信任管理、默认折叠的高级危险操作；正文滚动、底部关闭固定。授权、已解析 Keychain、指纹所有权、忙碌和平台限制沿用原逻辑，关闭清除确认；不自动生成或安装。
- 单次独立源码审查没有安全/数据丢失发现，指出原生字号下 64px 编辑按钮文字空间偏紧。本轮按视觉任务的重要问题处理，将按钮扩为 96px、同步摘要预留区；使用随包 Noto Sans SC 18px 和原生内边距的真实输入回归先失败再通过。原生预览另发现旧式 columns 的分隔线跨列导致标题错位，删除两处分隔线后源/目标首字段对齐，新增真实字段输入验证；诊断打印已移除。
- 最新完整工作区验证：23 suites、99 passed、0 failed、5 ignored；fmt、严格 all-target Clippy、all-target check、Rust 1.90 all-target check 通过。覆盖全部九个字段、草稿保持、重排/删除、长摘要、两语言、忙碌、640×480 关闭入口以及 CA 生成/刷新/授权/覆盖/移除边界；只收集动作，不执行系统写入。
- 最终 ARM 原生截图 `49`–`52`（中文/浅色、940×668 内容区、2×）确认规则列表、分组编辑器、证书信息与信任分区、正文滚动及固定关闭。`47` 为之前空态、`48` 为修正前的标题错位证据。没有把这些截图扩展为所有语言/主题、最小尺寸、键盘、IME、混合 DPI 或已配置证书状态都已通过；完整验收边界见 `design-qa.md`。
- ARM release 重新构建，刷新现有 `.app` 并通过 adhoc 签名检查；独立二进制与 release 相同。实际更新归档解包回归另行 1 passed、SHA-256 校验通过，当前归档为 `9ad7d4bd9c4e9811b84d96f3ba5e1369205eafccb2550217b63665ef83b03ab6`。未刷新 Intel，未执行完整 cargo-packager、正式签名、公证、发布或新的 OpenSpec CLI 验证。
- 两个隔离预览均正常退出（39/321 帧），临时示例规则已移除，隔离偏好保持 zh-CN/light/940×668，无预览进程或 18798 监听。未生成 CA、修改 Keychain、系统代理/外观、LaunchAgent 或日常配置；无关脏改动保留，未提交或推送。

## 高精度滚动输入修复（2026-10-23）

- 确认 `imgui-winit-support 0.13.0` 将 PixelDelta 仅按正负转换为整格，放大触控板小位移。在统一事件入口先按当前 DPI 还原逻辑像素，参照 Dear ImGui 原生 macOS 后端乘以 `0.01`，以小数 LineDelta 交给现有适配层；保留普通鼠标步长、事件阶段和按需重绘，不增加依赖、配置或常驻帧循环。
- 回归先失败再通过：两轴/方向、小位移、零值及 1×/1.5×/2× DPI；普通鼠标不随 DPI 缩放。现有单 Context 测试补上真实无窗口 ImGui 小幅内容滚动和有限帧后恢复空闲。全工作区 23 suites、101 passed、0 failed、5 ignored；fmt、严格 all-target Clippy、Rust 1.90 all-target check 和 ARM release 构建通过。
- 刷新现有 ARM `.app`、独立二进制及归档，adhoc 签名验证、实际更新器解包回归另行 1 passed、checksum 通过。当前归档 SHA-256：`66eead97d28b6836932e8893df7fe4b92856934bee1c45f3c67cb136ce923104`。未刷新 Intel，未执行完整 cargo-packager、正式签名、公证或发布。
- 本轮没有启动 GUI，没有真实触控板/鼠标手感或帧时间采样；无窗口测试不等同于实机滚动流畅度验收。未触碰日常配置、系统代理/外观、Keychain、LaunchAgent 或用户运行中的实例；未提交或推送。

## 悬浮提示换行修复（2026-10-23）

- 定位规则摘要的自动定宽提示框与 `text_wrapped` 当前内容宽度之间的反馈：随包 18px 字体下，短地址提示实际被算为 42px 宽、470px 高。统一提示函数在首次布局前设置独立的换行位置，文字宽度上限为 420 logical px，并给视口边缘与内边距预留空间；完整源/目标文本不变。规则摘要及设置页两处同类提示复用此函数，普通短标签提示保持原有处理。
- 现有单 Context UI 测试扩展真实悬停及提示绘制顶点范围检查；短地址、长 ASCII 主机名和中文路径分别覆盖中英文、940×700 / 640×480。旧实现先失败，修复后均通过。全工作区 23 suites、101 passed、0 failed、5 ignored；fmt、严格 all-target Clippy、Rust 1.90 all-target check 和 ARM release 构建通过。
- 现有 ARM `.app` 和独立二进制以新文件替换刷新，adhoc 签名验证通过；实际更新归档解包另行 1 passed、checksum 通过。当前归档 SHA-256：`76ebd637b6c14c6d0937efba7df691935f0b7ff8b7cc5c18ec546f2f4e7e092c`。未刷新 Intel，未执行完整 cargo-packager、正式签名、公证、发布或新的 OpenSpec CLI 验证。
- 本轮没有启动原生 GUI 或采集新截图，绘制数据回归不等同于完整原生视觉验收。没有终止或操作用户正在运行的窗口，也未读取/修改日常代理配置、系统代理/外观、Keychain 或 LaunchAgent；无关脏改动保留，未提交或推送。

## 主题选项圆角（2026-10-23）

- 仅调整设置页三个主题选项的样式：6px 圆角、4px 间距，按间距扣除按钮宽度，保持总宽 180 logical px；配色、标签、配置及切换逻辑不变，无新增依赖。
- 真实无窗口 ImGui 绘制回归先在旧版方角失败，再验证中英文、深浅配色下三个选中项的圆角，以及空隙点击不切换主题。全工作区 23 suites、101 passed、0 failed、5 ignored；fmt、严格 all-target Clippy 和 ARM release 构建通过。
- ARM `.app`、独立二进制及归档已刷新；adhoc 签名、实际更新器解包回归另行 1 passed、checksum 校验通过。最新归档 SHA-256：`92dc82c51929b751a3fe4e5b62159b7e18bb21985d036aeb218053d690a88898`。没有原生窗口截图复验，未刷新 Intel，未执行完整 cargo-packager、正式签名、公证或发布；没有触碰日常配置、系统设置或运行中的实例，未提交或推送。
