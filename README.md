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

更多用法可以查看 [USAGE](./crates/proxy-fork-cli/USAGE.md)

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
