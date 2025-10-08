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

```bash
# 使用默认配置运行
proxy-fork-cli

# 指定监听地址
proxy-fork-cli --listen 0.0.0.0:7898

# 启用系统代理
proxy-fork-cli --enable-sysproxy

# 禁用 CA 证书（无证书模式）
proxy-fork-cli --noca

# 启用调试日志
proxy-fork-cli -d  # DEBUG 级别
proxy-fork-cli -dd # TRACE 级别
```

### 开发时使用（需要源码）

如果你在开发环境中，可以使用以下命令：

```bash
# 构建项目
cargo build --release

# 运行
./target/release/proxy-fork-cli

# 或者直接运行（会自动编译）
cargo run -p proxy-fork-cli
```

### 配置

支持从多个来源加载配置，优先级如下（后者覆盖前者）：

1. 用户目录配置：`~/.config/proxy-fork/config.toml`
2. 当前目录配置：`./proxy-fork.toml` 或 `./config.toml`
3. CLI 参数

#### 示例配置文件

```toml
[server]
listen = "127.0.0.1:8080"
ca_cert = "~/.mitmproxy/mitmproxy-ca-cert.cer"
ca_key = "~/.mitmproxy/mitmproxy-ca.pem"

[rules]
# 精确匹配规则
exact = [
    { pattern = "example.com", target = "http://127.0.0.1:8081" }
]

# 模式匹配规则
pattern = [
    { pattern = "*.google.com", target = "http://127.0.0.1:8082" },
    { pattern = "api.*.com", target = "http://127.0.0.1:8083" }
]
```

### 动态规则管理

支持运行时添加和移除代理规则：

```bash
# 添加精确匹配规则
proxy-fork-cli --add-exact example.com=http://127.0.0.1:8081

# 添加模式匹配规则
proxy-fork-cli --add-pattern "*.google.com=http://127.0.0.1:8082"

# 移除规则
proxy-fork-cli --remove-exact example.com
proxy-fork-cli --remove-pattern "*.google.com"
```

## 开发

### 项目结构

```
crates/
├── proxy-fork-cli/     # CLI 工具
└── proxy-fork-core/    # 核心代理逻辑
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
