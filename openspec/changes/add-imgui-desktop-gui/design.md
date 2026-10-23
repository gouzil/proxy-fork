## Context

proxy-fork 当前是一个 Rust workspace。代理启动、CA 加载、规则装配、系统代理设置和 Ctrl+C 退出逻辑集中在 `proxy-fork-cli`，配置也由 CLI 层按“CLI 参数 > 当前目录文件 > 用户目录文件”的顺序合并。这个入口适合一次性命令，但 GUI 需要可重复启动/停止、状态订阅、运行时规则更新，并且不能阻塞窗口事件循环。

本次变更新增桌面入口，首期完整交付 macOS，同时保留未来 Windows 和 Linux 的平台边界。现有 CLI、TOML 格式、规则语义和配置优先级是兼容约束。桌面应用还会接触系统代理、Keychain、自启动和更新安装，这些操作需要明确授权、可恢复状态和失败回滚。

## Goals / Non-Goals

**Goals:**

- 新增低资源占用的 `proxy-fork-gui`，提供启动/停止、状态、监听配置、规则编辑、有界日志、系统代理和 CA 控制。
- 让 CLI 和 GUI 复用同一套配置模型、校验、CA 构建、代理生命周期及系统代理恢复逻辑，并保持 CLI 的现有外部行为。
- 使用 Dear ImGui 的 Rust 绑定、winit、wgpu 和 Tokio 构建 GUI，macOS 使用 Metal 后端；代理与阻塞平台操作在后台执行，原生窗口和托盘操作保留在主线程。
- 首期完成 macOS 托盘、LaunchAgent 自启动、Keychain CA 安装向导、应用打包和 GitHub Releases 更新。
- 使用 `rust-i18n` 提供简体中文和英文，并持久化语言选择、支持运行时切换。
- 为 Windows/Linux 留下清晰的条件编译边界；未支持能力必须显式显示为不可用，不能静默失败。

**Non-Goals:**

- 本次不完成 Windows/Linux 的托盘、自启动、证书信任、签名和安装流程。
- 不增加流量抓包历史、请求编辑器、响应查看器、图表分析或远程管理。
- 不改变已有 TOML 字段、CLI 参数、配置优先级、规则匹配语义或 Hudsucker 行为。
- 不为了未来平台提前建立只有一个实现的 trait、额外 runtime crate 或通用插件系统。
- 不以 GUI 日志替代现有文件/终端日志，也不采集请求正文、完整头部或密钥等敏感数据。

## Decisions

### 1. 新增独立 GUI crate，共享能力归入 core

新增 `crates/proxy-fork-gui` workspace member，GUI 只依赖 `proxy-fork-core`，不依赖 `proxy-fork-cli`。Clap 参数和命令分派继续留在 CLI；可序列化配置模型、TOML 读写与校验、CA 构建、规则转换和可控代理生命周期移动到 core。CLI 以现有覆盖顺序生成运行时配置后调用 core，因此用户脚本和默认启动行为不变。

core 提供一个具体的 `ProxyService`，包含 `start`、`stop`、`replace_rules` 和状态订阅能力。服务状态至少区分 `Stopped`、`Starting`、`Running`、`Stopping` 和 `Failed`，以拒绝重复启动并让 UI 展示真实过渡状态。停止句柄使用 Hudsucker 的 graceful shutdown；正常停止、启动失败和应用退出都经过同一清理路径。

运行中的规则先完整校验，再一次性替换共享 `ProxyManager` 中的规则；失败时保留上一份有效规则。监听地址、CA 和系统代理开关会改变服务或系统状态，修改后标记“需要重启”，不做隐式重启。

替代方案是让 GUI 调用 CLI 或复制 `start_proxy`。前者仍受 Ctrl+C 和阻塞入口约束，后者会产生两套启动与清理逻辑，因此不采用。另一替代方案是立即拆出新的 runtime/platform crates；首期只有一个 GUI 和一个完整平台实现，额外 crate 暂无收益。

### 2. Dear ImGui 即时模式界面与按需重绘

使用已发布且依赖相容的组合：`imgui 0.12.0`、`imgui-winit-support 0.13.0`、`imgui-wgpu 0.28.0`、`winit 0.30` 和 `wgpu 29`，实现时通过 Cargo.lock 固定解析结果并在 Rust 1.90 工具链验证。`imgui-wgpu 0.28.0` 发布包使用 wgpu 29；按对应版本文档接入，不混用 Git main 的依赖或新版 Dear ImGui API。这里的 Rust GUI 指应用代码使用 Rust，`imgui-sys 0.12` 仍编译 Dear ImGui 1.89.2 的 C++ 内核，需要 Xcode Command Line Tools 等原生编译工具链。

winit 在 macOS 主线程管理窗口、输入和事件循环，`imgui-winit-support` 对接输入与 DPI，`imgui-wgpu` 将每次 UI frame 产生的 draw data 提交给 wgpu；macOS 实际选择 Metal。渲染 surface 随窗口尺寸和 DPI 更新，零尺寸或隐藏窗口暂停绘制，surface 丢失时重新配置。首期只使用一个系统窗口和 ImGui 自带表格、输入框、菜单及弹窗，不启用 docking、多 viewport 或额外组件库。渲染器的传递依赖可能开启其他 wgpu backend features，不能把运行时选 Metal 等同于只编译 Metal。

GUI 的 `App` 持有编辑缓冲区、当前页面及任务状态，使用 `begin/finish` 管理操作；`Desktop` 接收 `AppEvent` 表达的用户动作和后台结果并更新状态。`draw(&imgui::Ui, &mut App)` 构建当前帧并收集动作，不在每一帧重新启动代理或提交副作用。进程持有一个 Tokio runtime；异步网络操作用 `spawn`，阻塞文件、证书及系统命令用 `spawn_blocking`。代理状态的 watch 通道和后台结果通过 `EventLoopProxy<AppEvent>` 唤醒 UI，主线程更新状态后请求重绘。托盘事件也使用这条路径，不用后台线程访问 ImGui context 或窗口。

空闲事件循环使用 `ControlFlow::Wait`；输入、窗口变化、服务状态变化和已合并的日志批次触发 `request_redraw`。需要光标闪烁、组合输入反馈或进行中进度动画时，用 `WaitUntil` 调度下一帧并更新 delta time，不无条件 Poll 或持续 60 FPS 绘制。窗口隐藏时取消视觉计时器和日志重绘，仅保留服务、更新和托盘所需事件；重新显示时绘制最新状态。低占用是验收目标，不据此宣称已比其他框架更省资源。

主界面分为状态控制、规则、日志和设置四个区域；证书安装与更新使用分步对话流程。启用键盘导航、可见焦点和文字状态提示，不能仅以颜色表达结果。首期采用上述具体组合以落实已确认的 ImGui 选型；独立 OpenGL 渲染方案会引入另一套上下文管理，当前不采用。

布局按用户提供的 Clash Verge 截图调整为固定左侧栏和右侧工作区，保留概览、规则管理、日志、设置四页，不照搬 Clash 的订阅、节点、TUN 或流量数据。概览采用代理服务、监听设置、转发规则、HTTPS 证书四张卡片；宽屏双列，小窗口单列滚动，保存入口与导航保持可见；各页面使用独立稳定 ID 保留自己的滚动位置。侧栏品牌仅保留 Proxy Fork。设置提供跟随系统、浅色、深色，默认跟随系统，立即应用并持久化；原生标题栏、背景、卡片、控件与弹窗同步配色，蓝色主操作保持一致。系统无法报告主题时回退浅色，手动模式不被系统外观事件覆盖。现有草稿、稳定控件 ID、重启提示与错误反馈不变；仅默认证书不存在归为“尚未配置”并保留生成入口，自定义路径、损坏证书和权限错误仍报错，不自动生成或安装信任。

概览卡片标题、说明、字段和操作按钮共用左边缘；启动/停止及管理入口使用紧凑按钮，按钮文字居中，右上角保存入口不变。侧栏品牌略放大并与导航文字左对齐；导航在实际品牌文字高度后保留 12px 间距，不用固定纵向位置为已移除的副标题留空。底部的本地服务、状态、版本号共用相同左边缘，三行紧凑分组，按实际行高定位并保留底部留白；移除侧栏退出按钮，保留托盘退出与原有清理流程。

登录启动按用户选定的参考收成单行：左侧“登录后自动启动”，右侧 42×26 logical px 圆角开关，白色滑块，关闭灰色、开启蓝色。使用现有 ImGui 按钮保留键盘焦点与激活，状态和下次登录生效说明移入悬停提示。空闲进入设置时只读查询一次，不逐帧重试；点击已知状态请求切换，界面等待系统结果后才更新，未知状态先查询。未知且已有错误时，点击提供刷新、启用、停用恢复菜单，不把读取失败当成关闭或自动写入。忙碌及不支持平台仍禁用控制，复用现有平台处理器，不新增持久化开关字段。

设置整页按后续参考图改为三组独立卡片：左上系统设置（登录启动），右侧应用设置（语言、主题模式、保存偏好），左下更新设置（自动检查、手动检查）。内容宽度至少 656 logical px 时为两列，16px 间距、每列至少 320px；更窄时按系统、应用、更新顺序单列滚动。标题靠左，每项使用统一 48px 行高、左标签及右控件；卡片之外保留工作区底色，不再嵌入整块大白表单。语言使用现有 ImGui 下拉框，主题为浅色/深色/系统三段选择；系统即跟随系统。自动检查复用登录启动的按钮式开关，频率说明放入悬停提示；保留手动保存、更新对话框及原有忙碌/平台限制。只重新组织已有功能，不添加 Clash 专有设置，不改变其余页面。自动登录状态查询绑定页面出现，而非某张卡片进入可见范围，且不覆盖当帧显式操作；静态页面和卡片 ID 不随语言变化。

规则页沿用 Clash Verge 的紧凑列表思路，每行 76px、显示顺序号、来源和目标两行摘要，右侧提供编辑与更多菜单（上移、下移、删除）。长内容在摘要区域裁剪，悬停显示完整地址，不覆盖操作。新增后立即打开分组草稿编辑器；匹配条件、转发目标、路径重写保留全部九个字段，宽窗口前两组并列、窄窗口上下排列。编辑器按稳定规则 ID 定位，重排不会误写其他规则。关闭或 Escape 保留草稿，只有规则页“保存并应用”才校验、写入并生效；文案明确说明，不把关闭伪装成取消。

证书管理改为居中模态对话框，分状态、证书信息、信任管理和默认折叠的高级危险操作。状态刷新与缺失时生成入口独立，路径、SHA-256、已解析 Keychain 用可选择复制的只读字段展示；信任安装、重新生成和移除保留各自的确认、指纹所有权、平台及忙碌限制，不更改处理器。两个对话框与视口边缘至少留 16px，正文独立滚动，关闭按钮固定在底部；关闭证书管理清除授权。仍只有四个导航页面，GUI 仅加载用户级配置，CLI 目录级及参数覆盖不变。

随包字体使用 Noto Sans SC 的静态 Regular (400) 实例，避免 stb 默认取可变字体的 Thin 轮廓；正文 atlas 按 18 logical px 构建并适配 DPI。输入后继续绘制，直到 ImGui 分帧消费的按键、鼠标及文本输入稳定，再返回空闲等待，不能用固定两帧假定输入队列已经处理完成。

### 3. 配置兼容与 GUI 偏好分离

代理配置继续使用现有 TOML 模型。GUI 默认编辑并原子写入平台用户配置目录中的代理配置：先写同目录临时文件并刷新，再重命名替换，避免中断造成半份配置。core 的读取、校验和序列化由 CLI 与 GUI 共用；CLI 的当前目录文件和参数覆盖仍只由 CLI 入口负责。

语言、主题、窗口和更新检查等仅属于 GUI 的偏好写入独立 `gui.toml`，不污染代理配置，也不会改变 CLI 行为。语言或主题修改时立即生效，并在当前操作结束后串行保存，无论该操作成功或失败；自动保存保留原操作错误，保存也失败则追加错误，不无限重试。保存期间的新修改在其完成后再保存一次，排队退出优先并保存最新偏好。私钥不写入 GUI 偏好，已有私钥文件继续采用最小文件权限。

替代方案是在现有代理 TOML 中加入 `[gui]`。这会让纯 CLI 用户承担无关字段并扩大兼容面，因此不采用。

### 4. 生命周期集中管理系统代理恢复

`ProxyService` 在修改系统代理前记录原始设置和即将写入的设置，并持有恢复 guard。正常停止、窗口明确退出、启动中途失败和更新重启都先执行 graceful shutdown，再读取当前系统代理；只有当前值仍等于本进程写入值时才恢复原始设置，而不是简单地“关闭代理”。若检测到外部修改，则不覆盖当前值，保留恢复记录并要求用户处理冲突。恢复成功后才清除恢复记录。

为覆盖崩溃或强制结束进程，在应用数据目录持久化包含原值和写入值的最小恢复记录。下次启动发现未清理记录时，同样先比较当前值：匹配写入值才自动恢复，否则明确提示冲突，再允许重新启用系统代理。硬终止无法保证即时执行析构，因此“退出必定恢复”限定为应用可处理的正常退出与错误路径。

替代方案是在每个 UI 事件中分别设置/关闭系统代理。它会遗漏启动失败、更新和托盘退出等路径，因此不采用。

### 5. 日志通过有界通道进入 UI

在现有 `tracing` 体系增加 GUI sink，将格式化后的等级、时间、目标和消息通过有界通道发送给界面。界面使用固定容量 `VecDeque`，达到上限时丢弃最旧记录；过滤后通过 `imgui::ListClipper` 绘制可见行，不复制无限历史。首期日志使用固定行高、单行显示，长消息按需查看详情，以保证 clipper 的行高假设成立。通道满时允许丢弃 UI 日志，并记录累计丢弃数，不能反向阻塞代理请求。日志合批最多每 100 ms 唤醒一次可见 UI；隐藏窗口继续维护有界日志而不发送逐条重绘事件。

默认不在 GUI 中记录请求正文、完整头部、证书私钥或更新凭据。需要持久日志时沿用现有日志输出，不在首期新增日志数据库。

替代方案是让 UI 读取日志文件或保存全部日志。前者带来轮转和 tail 同步问题，后者使常驻内存无界增长，因此不采用。

### 6. i18n 是界面状态，不进入代理核心

GUI 使用 `rust-i18n`，资源目录包含 `en` 和 `zh-CN`，英文作为缺失键回退。所有用户可见的应用文案使用稳定 key；来自 core 的状态和错误先转成稳定类别，再由 GUI 翻译，不在 core 返回本地化字符串。

语言切换通过 `AppEvent` 更新 locale、写入 `gui.toml` 并请求重绘，同时更新原生托盘菜单。ImGui 可见标签与控件 ID 分离：使用 `###stable-id` 或 `push_id` 配合隐藏标签，为每条规则保留不随排序和翻译变化的会话 ID。编辑缓冲区属于 `App`，不会因每帧重建界面或切换语言而丢失；内置弹窗等固定文案也须检查并替换为可翻译的应用文案。

ImGui 0.12 的默认字体不覆盖中文。应用打包有再分发许可的 CJK 字体及许可证，构建覆盖中英文文案和用户输入的字体 atlas，并处理 Retina 缩放及 DPI 变化后的纹理重建。字体 atlas 不按帧重建，也不按另一版本的动态字体 API 实现。

`imgui-winit-support 0.13` 的输入适配不足以单独保证 macOS 中文输入：应用需显式接入 winit 的 `Ime` preedit/commit、输入法启用及候选窗口位置，将确认提交的文本交给 ImGui，组合文本单独保存和展示，并避免与 `KeyboardInput.text` 重复注入。实施前用拼音输入法验证候选选择、取消组合、焦点切换、光标定位和粘贴，不能将“能显示中文字体”作为输入法已支持的证据。

替代方案是 Fluent。当前只要求中英文，不涉及复杂复数、性别或 RTL；`rust-i18n` 更小且足够，等需求出现再评估 Fluent。

### 7. 平台能力按目标系统分模块，不提前抽象 trait

平台代码放在 GUI crate 的 `platform` 模块中，通过 `#[cfg(target_os = "macos")]` 选择 macOS 实现；其他目标提供返回 `Unsupported` 的轻量边界，使核心界面可以继续编译并明确禁用尚未实现的操作。等第二个平台出现重复实现后，再按真实共同点提取 trait。

macOS 首期行为如下：

- 托盘和窗口在 macOS 主线程创建；托盘事件通过 winit `EventLoopProxy` 转换为 `AppEvent`，与主窗口共用处理路径。关闭窗口默认隐藏到托盘并停止绘制，只有“退出”才停止服务并恢复系统代理。
- 自启动写入当前用户的 LaunchAgent，内容只引用当前签名应用路径；启用、禁用和状态查询都可重复执行，并向 UI 返回实际结果。
- CA 向导复用 core 的生成/定位能力。检测和信任安装调用 macOS `security` 命令时使用参数数组而非拼接 shell，展示将要修改的 Keychain，并在用户明确确认后执行。安装后重新检测指纹，卸载只处理本应用记录的证书。
- 平台命令返回结构化错误类别和可安全展示的 stderr 摘要；权限拒绝与能力缺失必须可区分。

替代方案是立即用统一平台服务 trait 包装所有操作。没有第二个完整实现时无法验证抽象边界，会增加样板和错误假设，因此不采用。

### 8. GitHub Releases 更新采用签名产物、暂存和可回滚替换

GUI 手动检查更新，并可在偏好中选择启动后后台检查；检查失败不影响代理运行。更新器只接受正式 GitHub Release，按版本、macOS 架构选择应用包，下载到应用数据目录的临时位置，并验证发布的 SHA-256、应用代码签名、预期 bundle identifier 和版本号。签名/公证身份必须与当前发行配置匹配，任一校验失败都删除暂存文件并停止更新。

发布身份固定为 bundle identifier `io.github.gouzil.proxy-fork`，稳定版 tag 使用 `v<version>`，macOS 更新资产使用 `proxy-fork-gui-<version>-<target>.app.tar.gz` 及同名 `.sha256`；首期目标为 `aarch64-apple-darwin` 和 `x86_64-apple-darwin`。自动检查默认关闭，由用户显式开启后每 24 小时最多检查一次，且不接受 prerelease。签名 identity、Apple Developer Team ID 和 notarization 凭据只通过发布环境注入。

安装前先停止代理并恢复系统代理。新应用包中的同一 GUI 可执行文件以内部 updater 模式等待旧进程退出，在同一文件系统内将旧 bundle 改名为备份、把新 bundle 移到目标位置、启动新版本；启动或替换失败时恢复备份。目标目录不可写时不尝试绕过权限，而是保留已验证产物并引导用户完成安装。

首期推荐把应用安装到用户可写的 `~/Applications`，从而支持完整自动替换。应用位于不可写的系统 `/Applications` 时，更新器完成检查、下载和验证后打开产物位置并提供人工替换说明，不引入高权限 helper。

替代方案是直接覆盖当前二进制或只校验 HTTPS。直接覆盖无法可靠更新完整 `.app` bundle，HTTPS 也不能证明下载产物属于预期发行者，因此不采用。

### 9. 实施与审查裁决

- macOS 的 `sysproxy 0.3` 把 HTTP、HTTPS、SOCKS 合并为单一元组，且忽略部分 `networksetup` 非零退出码，不能承担无损恢复。macOS 改用 SystemConfiguration：记录网络服务 UUID 和完整 Proxies 字典，锁定并校验预期旧值后提交/应用，恢复后重新读取确认；保留 SOCKS 与其他未修改字段，认证字段拒绝接管。其他平台保留既有适配，平台集成不算本次已验收。
- LaunchAgent 禁用使用 `launchctl disable` 加移除 plist；启用使用 `enable`，仅在未加载时 `bootstrap`。不 `bootout` 正在运行的 GUI 所属 job，避免中途 SIGTERM；已加载 job 的新路径下次登录生效。查询同时考虑 plist 路径和 disabled override。
- `App::begin/finish` 管理操作互斥与排队退出，`Desktop::user_event/finished` 分派后台结果；不再另加一层 `App::handle_event` 状态机。日志直接进入 `try_lock` 的 2,000 条有界环形缓冲，通知合批，不额外建立重复消息队列。
- 规则主配置先完整构建、保存，再替换活动 manager。随后 GUI 偏好保存失败单独显示“配置与规则已提交、偏好未保存”，不能误报主配置整体失败。原子文件以 rename 为提交点，之后的目录刷新失败仅报告耐久性警告；系统代理恢复记录另外强制刷新目录，未确认持久化前禁止修改系统设置。
- CA 指纹复用已有 OpenSSL SHA-256，不为同一功能增加 core 的 sha2 依赖。显式覆盖保留旧私钥备份直到证书替换成功，第二次替换失败则回滚；无法回滚时保留备份并给出位置。不承诺两条文件路径跨文件系统的断电原子事务。Keychain 确认绑定实际路径及指纹；先保存候选证书所有权，再改信任，拒绝用新证书覆盖仍记录的旧 CA 所有权。
- 手动更新先导出持久的 `manual-update-*` 目录再打开 Finder，退出不删除它。更新启动失败恢复并重开旧版，显示回滚结果。包提取器与本地实际产物另做 opt-in 兼容检查，不仅验证人工构造的小归档。
- CLI 继续跳过无效规则并报告错误；GUI 与 core 全量保存仍严格拒绝无效规则集。真实 HTTP 测试发现并修复了原有 host-only 精确规则不匹配实际请求路径、以及精确规则新增未清缓存的问题；这是原有匹配含义的恢复，不是新增匹配模式。
- 实际字体 atlas 只包含界面、ASCII 和已遇到的用户文本字形，按需增补；避免默认装入整个 CJK 字集。字体分发许可随包附带。

代码实现和本机预览不替代硬件/系统集成验收。未执行的拼音、混合 DPI、真实 Keychain/LaunchAgent/系统代理写入、签名更新和跨平台 CI 继续记录为待验收，不因单元测试通过而关闭。

## Risks / Trade-offs

- [winit、托盘库和 macOS 主线程事件循环集成存在兼容问题] → 先做最小托盘/隐藏窗口冒烟测试，锁定验证过的依赖版本，托盘失败时仍允许普通窗口退出。
- [即时模式被实现为持续重绘，增加 CPU/GPU 和耗电] → 空闲等待事件、视觉计时器按需启用、隐藏窗口停止绘制、日志有界并裁剪可见行；以 release 构建测量资源占用。
- [Rust 绑定依赖 C++ 工具链且后端版本耦合] → 使用上述已发布兼容组合，在 CI 检查 Rust 1.90、原生编译环境和依赖树，不混用后端的 Git main。
- [中文字体和 macOS IME 并非开箱即用] → 随包提供字体与许可证，显式实现 IME 桥接，并将中文输入和 Retina 缩放作为首期验收项。
- [Dear ImGui 不提供完整原生辅助功能树] → 保留键盘导航、可见焦点和文字提示，并在支持说明中标明 VoiceOver/屏幕阅读器限制，不承诺未经实现的原生无障碍能力。
- [系统代理修改后进程被强制杀死，无法即时恢复] → 修改前持久化原始设置，正常路径统一清理，下次启动执行恢复检查，并在文档中提供手工恢复命令。
- [CA 信任安装属于高权限安全操作] → 必须显示证书指纹和目标 Keychain、要求用户确认、使用系统工具参数调用，绝不静默安装或删除非本应用证书。
- [自动更新供应链或替换失败] → 固定仓库与 bundle identity，校验 checksum 和代码签名，暂存后原子替换并保留可恢复备份；校验失败不执行任何安装。
- [从 CLI 移动配置和启动逻辑导致行为回归] → 在迁移前固化配置优先级、默认值、规则转换和 Ctrl+C 清理测试，CLI 只改为调用共享 API。
- [Windows/Linux 边界过早固化] → 非 macOS 仅提供显式不可用结果，不设计单实现 trait；第二个平台落地时再调整模块接口。
- [运行时修改规则与保存配置不同步] → 保存和应用分别显示结果；只在保存成功且校验通过后应用，失败时保留旧配置和旧运行规则。

## Migration Plan

1. 为现有 CLI 配置优先级、默认值、规则转换、启动/停止和系统代理清理补充回归验证。
2. 将共享配置、CA 和代理生命周期移动到 `proxy-fork-core`，让 CLI 改用 `ProxyService`，确认现有命令、参数和 TOML 行为不变。
3. 新增 `proxy-fork-gui` 最小窗口，接入 ImGui/winit/wgpu/Tokio，先验证 Metal、按需重绘、中文字体与输入法，再接入服务状态、配置编辑、规则应用、有界日志和中英文切换。
4. 增加 macOS 托盘、LaunchAgent 和 CA 向导；用隔离配置和测试证书完成手工/自动化冒烟验证，测试结束恢复系统状态。
5. 建立签名、公证和 GitHub Release 产物约定，再启用更新检查、校验、替换、重启和回滚流程。
6. 分发 macOS 预览版本，验证正常退出、启动失败、更新退出和崩溃后恢复场景；通过后再标记首期支持。

回滚时停止分发 GUI 应用并从 workspace 移除 GUI member；由于 CLI 仍使用兼容的 core API 且代理 TOML 未变，用户可以继续使用原 CLI。更新替换失败时由 updater 恢复上一份 `.app`；系统代理由同一恢复记录还原。

## Open Questions

无阻塞实施的问题。Windows 和 Linux 的具体系统代理、证书存储、托盘协议、包格式和自启动实现将在对应平台变更中确定；本次只保证通用 GUI 可编译并明确报告尚未实现的平台能力。

## References

### GUI 与异步运行时

- [imgui-rs 官方仓库](https://github.com/imgui-rs/imgui-rs)
- [imgui 0.12 API](https://docs.rs/imgui/0.12.0/imgui/)
- [imgui-winit-support 0.13 发布源码](https://docs.rs/imgui-winit-support/0.13.0/src/imgui_winit_support/lib.rs.html)
- [imgui-wgpu 0.28 API](https://docs.rs/imgui-wgpu/0.28.0/imgui_wgpu/) 与[发布依赖](https://crates.io/api/v1/crates/imgui-wgpu/0.28.0/dependencies)
- [winit `ControlFlow`](https://docs.rs/winit/0.30.12/winit/event_loop/enum.ControlFlow.html) 与 [`EventLoopProxy`](https://docs.rs/winit/0.30.12/winit/event_loop/struct.EventLoopProxy.html)
- [wgpu 29 `InstanceDescriptor`](https://docs.rs/wgpu/29.0.1/wgpu/struct.InstanceDescriptor.html)
- [ImGui `ListClipper`](https://docs.rs/imgui/0.12.0/imgui/struct.ListClipper.html)
- [Clash Verge 规则管理说明](https://www.clashverge.dev/guide/rules.html)与[规则条目源码](https://github.com/clash-verge-rev/clash-verge-rev/blob/dev/src/components/rule/rule-item.tsx)：仅作为紧凑列表与操作分组参考，不复刻其订阅或策略功能。
- [Tokio `CancellationToken`](https://docs.rs/tokio-util/0.7.16/tokio_util/sync/struct.CancellationToken.html)
- [Hudsucker 0.24 API](https://docs.rs/hudsucker/0.24.0/hudsucker/)
- [`tracing-subscriber::Layer`](https://docs.rs/tracing-subscriber/0.3.20/tracing_subscriber/layer/trait.Layer.html)
- [`sysproxy` 0.3 API](https://docs.rs/sysproxy/0.3.0/sysproxy/)

### 本地化与跨平台依赖

- [`rust-i18n` 4.2 API](https://docs.rs/rust-i18n/4.2.1/rust_i18n/)
- [ImGui `FontGlyphRanges`](https://docs.rs/imgui/0.12.0/imgui/struct.FontGlyphRanges.html)
- [winit `Ime`](https://docs.rs/winit/0.30.12/winit/event/enum.Ime.html) 与 [`set_ime_allowed`](https://docs.rs/winit/0.30.12/winit/window/struct.Window.html#method.set_ime_allowed)
- [Dear ImGui 官方能力与限制](https://github.com/ocornut/imgui/blob/master/docs/README.md#the-pitch)
- [Cargo platform-specific dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#platform-specific-dependencies)

### macOS 平台集成

- [`tray-icon` 0.24 API](https://docs.rs/tray-icon/0.24.2/tray_icon/)
- [Apple: Creating Launch Daemons and Agents](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html)
- [macOS `security(1)` manual](https://keith.github.io/xcode-man-pages/security.1.html)
- [Apple: Notarizing macOS software](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)

### 更新与分发

- [GitHub REST API: Releases](https://docs.github.com/en/rest/releases/releases)
- [Reqwest 0.12 API](https://docs.rs/reqwest/0.12.23/reqwest/)
- [`cargo-packager` 0.11 API](https://docs.rs/cargo-packager/0.11.8/cargo_packager/)
