# proxy-fork-cli 使用说明

本工具支持从多个来源加载配置，并遵循如下优先级（后者覆盖前者）：

1. 用户目录配置：`~/.config/proxy-fork/config.toml`（Windows/macOS/Linux 路径自动适配）
2. 当前目录配置：`./proxy-fork.toml`（若无则尝试 `./config.toml`）
3. CLI 参数：命令行传参（最高优先级）

## 运行

- 默认运行（使用结构体默认监听地址 + 配置文件规则）

```bash
cargo run -p proxy-fork-cli
```

- 指定监听地址（覆盖文件配置）

```bash
cargo run -p proxy-fork-cli -- --listen 0.0.0.0:7898
```

- 指定 CA 证书与私钥（覆盖文件配置）

```bash
cargo run -p proxy-fork-cli -- \
  --ca-cert ~/.mitmproxy/mitmproxy-ca-cert.cer \
  --ca-key  ~/.mitmproxy/mitmproxy-ca.pem
```

- 启用系统代理

```bash
cargo run -p proxy-fork-cli -- --enable-sysproxy
```

- 禁用 CA 证书（无证书模式）

```bash
cargo run -p proxy-fork-cli -- --noca
```

- 启用调试日志（-d 或 -dd）

```bash
cargo run -p proxy-fork-cli -- -d  # DEBUG 级别
cargo run -p proxy-fork-cli -- -dd # TRACE 级别
```

- 通过 CLI 添加规则（可多次传参）

```bash
cargo run -p proxy-fork-cli -- \
  --rule 'protocol=https,host=api.example.com,path=/console/api/*,target_host=localhost,target_port=5001,target_protocol=http' \
  --rule 'protocol=https,host=*.example.com,path=/api/*,target_host=127.0.0.1,target_port=8080,target_protocol=http,path_transform=prepend,target_path=/local'
```

## 配置文件示例（TOML）

可在当前目录创建 `proxy-fork.toml`，或放置到 `~/.config/proxy-fork/config.toml`。

```toml
# 监听地址（可选；如不设置，则使用默认 127.0.0.1:7898）
listen = "127.0.0.1:7898"

# CA 证书与私钥（可选；可被 CLI 覆盖）
cert = "/Users/you/.mitmproxy/mitmproxy-ca-cert.cer"
key  = "/Users/you/.mitmproxy/mitmproxy-ca.pem"

# 禁用 CA 证书（无证书模式；可选；默认 false）
noca = false

[proxy_manager]
# LRU 缓存大小（可选；默认 1000）
cache_size = 1000

# 规则列表
rules = [
  # 示例1：保留路径
  { protocol = "https", host = "service.example.com", path = "/console/api/*", target_host = "localhost", target_port = 5001, target_protocol = "http" },

  # 示例2：前缀拼接
  { protocol = "https", host = "*.example.com", path = "/api/*", target_host = "127.0.0.1", target_port = 8080, target_protocol = "http", path_transform = "prepend", target_path = "/local" },

  # 示例3：前缀替换
  { protocol = "https", host = "service.example.com", path = "/v1/*", target_host = "127.0.0.1", target_port = 9090, target_protocol = "http", path_transform = "replace", target_path = "/v2" }
]
```

## 规则格式说明（CLI 与 TOML 通用字段）

- protocol: http | https（必填）
- host: 匹配的主机（必填）；支持：
  - 精确匹配：example.com
  - 通配符：*.example.com 或 /api/*（路径）
  - 正则：以 `re:` 前缀，例如 `re:^api/v[0-9]+/users$`
- path: 匹配路径（可选）；支持精确/通配符/正则
- port: 匹配端口（可选）
- target_protocol: 目标协议（默认 http）
- target_host: 目标主机（必填）
- target_port: 目标端口（可选）
- path_transform: preserve | prepend | replace（可选；默认 preserve）
- target_path: 当 path_transform 为 prepend/replace 时使用的新前缀

## 备注

- 监听地址、ProxyManager 缓存大小等默认值写在对应结构体上（derive_builder 默认），无需在配置中显式指定。
- CLI 提供的参数与规则会覆盖/追加到文件配置。
