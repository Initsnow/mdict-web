# mdict-web 实施计划

## 1. 目标

基于官方 crates.io 上的 `mdict-rs = "0.1.4"` 实现一个：

- 高性能
- API-first
- 前后端分离
- 默认安全
- 便于多人并行开发

的 Rust MDict Web Server。

本项目优先支持本地只读词典服务：

- `*.mdx` 文本词典
- `*.mdd` 静态资源词典
- 精确查词
- 前缀联想
- 词条 HTML 渲染
- 词典资源代理

不在第一阶段范围内：

- 词典写入
- 在线上传大型词典并即时索引
- 全文检索
- 多租户权限系统

## 2. 参考库结论

`mdict-rs` 已经提供了适合作为解析内核的基础能力：

- 共享 MDX/MDD 解析核心
- 惰性解码 key block / record block
- 明确的 limits、防 panic、防越界、防 checksum 错误
- `unsafe` 禁止
- 结构化错误
- 只暴露小而稳的库 API

对 `mdict-web` 的直接意义：

1. `mdict-rs` 负责“安全解析 + 懒读取”。
2. `mdict-web` 负责“目录管理 + 索引 + 可选缓存 + HTTP API + HTML/资源重写 + 观测性”。
3. 不把 Web 逻辑硬塞回 `mdict-rs`，除非是通用且可复用的解析层增强。

当前已知缺口：

- `mdict-rs` 目前没有 Web 友好的 HTML/CSS 重写能力。
- `FileSource` 现在基于单 `Mutex<File>`，高并发同词典热点查词时可能形成读锁竞争。
- 当前库不提供 sidecar 索引；联想搜索不能直接靠现有 API 高效完成。

因此本项目必须把“sidecar 索引”作为核心设计；应用层缓存保持可选，默认关闭。

## 3. 总体架构

### 3.1 分层

后端采用 API-first 分层：

1. `config/catalog`
   负责词典清单、目录扫描、词典配对、热重载。
2. `engine`
   负责基于 `mdict-rs` 做查询、资源读取、HTML 重写、缓存协调。
3. `index`
   负责构建和读取 sidecar 前缀索引。
4. `service`
   负责用例编排：列表、详情、联想、查词、资源访问。
5. `http`
   负责路由、DTO、状态码、认证、限流、观测性。

前端完全独立，只依赖 `docs/API_CONTRACT.md` 中的接口和数据格式。

### 3.2 目标目录

第一轮代码重构后，仓库应转为：

```text
mdict-web/
  AGENTS.md
  Cargo.toml                # workspace
  .codex/
    STATUS.md
  docs/
    IMPLEMENTATION_PLAN.md
    API_CONTRACT.md
  crates/
    mdict-web-config/
    mdict-web-domain/
    mdict-web-index/
    mdict-web-engine/
    mdict-web-service/
    mdict-web-http/
    mdict-web-app/          # backend binary
  frontend/                 # 可单独开发/部署的 web client
```

说明：

- `frontend/` 不参与 Rust workspace 的编译依赖。
- 后端 API 文档先定，再允许前端与后端并行推进。
- 当前仓库还是单 `src/main.rs`，下一步先做 workspace 重构。

## 4. 技术选型

### 4.1 后端

- Runtime: `tokio`
- HTTP: `axum`
- Middleware: `tower` / `tower-http`
- Serialization: `serde`, `serde_json`
- Config: `figment` 或 `config` + `serde`
- Optional caching: `moka`
- Observability: `tracing`, `tracing-subscriber`, `metrics`
- MIME 推断: `mime_guess`
- HTML/CSS 重写: `lol_html` + 受控字符串重写

选择理由：

- `axum` 的状态注入、路由组合、错误传播对分层结构清晰，适合长期维护。
- `mdict-rs` 目前是同步文件读取模型，因此必须把词典读取放到 `spawn_blocking` 或专用阻塞线程池。
- sidecar 索引对实际性能提升会远大于单纯更换 Web 框架；应用层缓存是否值得开启，交给 benchmark 决定。

### 4.2 前端

前端独立仓库或独立目录均可，但必须遵守：

- 只依赖 `v1` API 契约
- 不直接假设词条 HTML 可安全插入主 DOM
- 默认通过 sandboxed iframe 或受控渲染容器展示词条内容

推荐前端形态：

- `React + Vite`
- 通过生成的 TypeScript 类型消费 API
- 开发期优先使用 `Vite` dev server；部署时默认构建 `frontend/dist` 并由 `axum` 直接同源托管
- Nix/NixOS 部署优先输出单包：Rust 二进制 + `frontend/dist`，避免额外静态站点进程

## 5. 核心领域模型

### 5.1 词典清单

每本词典不是单文件概念，而是一个 `DictionaryBundle`：

- `dictionary_id`
- 显示名、语言
- `mdx_path`
- 配置层可写 `mdd_path` 或 `mdd_paths`；内部统一归一化为有序 `mdd_paths`
- `entry_script_mode = none | original`，默认 `none`
- `theme_mode = auto | dictionary | force_auto_dark`，默认 `auto`
- 可选 passcode
- 可选额外前端展示元信息

### 5.2 查询模型

- `LookupQuery { dictionary_id, key }`
- `SuggestionQuery { dictionary_id, q, limit }`
- `ResourceQuery { dictionary_id, key }`

### 5.3 渲染模型

- `RawEntry`: `mdict-rs` 查出的原始 key + 原始 HTML
- `RenderedEntry`: 重写后的 HTML、资源基地址、缓存标签

## 6. 性能设计

### 6.1 热路径

查词请求的理想路径：

1. API 层完成参数校验。
2. `service` 直接进入 `engine`，或在显式开启缓存时先查 cache。
3. `suggest` 路径使用 sidecar 索引做规范化前缀定位；exact lookup 继续走 `mdict-rs` 原生 key lookup。
4. 需要命中正文时才调用 `mdict-rs` 读取 record。
5. 返回前做 HTML 重写；仅在显式开启缓存时写入缓存。

### 6.2 sidecar 索引

联想搜索不能靠在线遍历 `entries()`。

第一版 sidecar 设计：

- 启动或离线构建时仅扫描 key
- 生成只读前缀索引
- 至少支持：
  - 规范化 key -> ordinal postings
  - prefix suggest
  - 通过 `mdict-rs::key_at(ordinal)` 按需还原 canonical key

候选实现：

- `fst::Map` 记录 `normalized -> posting range`
- 侧边 postings 文件记录稳定 key ordinal 列表

原则：

- 线上请求不做全量 key 扫描
- 线上请求不为联想搜索解压 record block
- 线上请求最多只为返回前几个候选而按 ordinal 做少量 key-block 读取；这部分仍必须放在阻塞线程池

### 6.3 缓存策略

本项目采用低内存优先策略：

- 默认关闭应用层 entry/resource cache
- 优先依赖：
  - `mdict-rs` 的惰性读取
  - OS page cache
  - sidecar 索引
- sidecar 索引不是运行时缓存；它是查询能力的一部分，允许放磁盘或 mmap

默认常驻状态应尽量小，只保留：

1. `dictionary metadata`
   词典配置、header 摘要、健康状态、索引状态。
2. `in-flight dedupe`
   可选的同 key 并发请求合并，避免瞬时重复工作；这不是长期驻留缓存。

如果 benchmark 证明确实值得加缓存，只允许按配置显式开启：

1. `entry render cache`
   `dictionary_id + normalized_key -> RenderedEntry`。
2. `resource bytes cache`
   仅限小文件，大文件继续走 streaming。

缓存规则：

- 默认值为关闭
- 必须有大小上限
- 必须能按字节权重计量
- 必须有 eviction 策略
- 必须暴露命中率、容量、逐出次数
- 禁止无上限缓存 HTML 或二进制资源

### 6.4 并发和阻塞

`mdict-rs` 当前读取路径是同步阻塞的，因此：

- 不能在 async reactor 上直接做词典 I/O
- 必须放进专用阻塞线程池
- 必须做并发上限，避免大量热点请求把阻塞池打满

需要尽早验证的风险：

- 同一词典热点查询下，`mdict-rs::source::FileSource` 的 `Mutex<File>` 是否成为瓶颈

若 benchmark 证明存在明显竞争，按以下顺序处理：

1. 先确认 OS page cache + sidecar 是否已经足够
2. 再评估是否需要小而受控的应用层缓存
3. 最后把可复用优化回推到 `mdict-rs`
   方向包括：
   - `pread`/positioned read 风格 source
   - 多文件句柄 source
   - 可选 mmap

禁止直接在 `mdict-web` 中做一次性私有脏修补。

## 7. 安全设计

MDict 文件和其中 HTML/资源都视为不可信输入。

### 7.1 解析层

- 完全复用 `mdict-rs` 的 checked parsing 约束
- 不绕过其 limits
- 不引入 `unsafe`

### 7.2 HTML 渲染层

词条 HTML 不能直接注入前端主页面 DOM。

后端必须做：

- 重写相对资源路径到受控资源 API
- 默认移除 `<script>`、内联事件处理器、危险 URL scheme
- 对样式中的 `url(...)` 做受控重写
- 对音频资源链接改写为非导航属性，并在 entry HTML 内按需注入最小播放 runtime，阻止默认导航并原位播放

前端必须做：

- 默认在 sandboxed iframe 中渲染词条
- iframe 需要兼容 `entry_script_mode = "original"` 的脚本执行，但默认词典不导入脚本
- light/dark 主题适配优先由前端 viewer 在同源 iframe 内执行通用 runtime；不要把夜间模式实现收敛成某本词典专用 CSS
- 前端 viewer 需要遵守词典级 `theme_mode`：`auto` 做启发式检测，`dictionary` 信任词典自带暗色，`force_auto_dark` 在 dark 模式下强制启用通用 auto-dark

### 7.3 资源访问层

- 若 MDX 同目录下存在同名 `.css` / `.js` sidecar 文件，则优先于 MDD 命中；其他资源仍保持 MDD 优先，不允许扩展到任意宿主文件
- 不允许把词条中的路径解释为宿主文件系统路径
- 返回二进制时设置 `X-Content-Type-Options: nosniff`
- 大资源必须支持 streaming 和缓存控制

### 7.4 服务层

- 请求体和查询参数长度限制
- 基础限流
- 统一错误模型，不泄露本地绝对路径
- admin/reload 接口和公开查词接口分离

### 7.5 供应链和许可证

`mdict-rs` 当前是 `AGPL-3.0-only`。

因此本项目必须尽早明确许可证策略：

- 若本项目开源并兼容 AGPL，可直接依赖
- 若计划闭源部署或分发，必须确认商业授权路径

这个问题不是实现细节，而是项目级约束。

## 8. API 设计原则

API 合同以 `docs/API_CONTRACT.md` 为准。

原则：

- 所有稳定接口挂在 `/api/v1`
- JSON 元数据和 HTML/二进制内容分离
- 精确查词、联想、HTML 内容、资源访问各自独立 endpoint
- DTO 尽量稳定，避免让前端感知后端内部 crate 结构
- 缓存开关不影响 API 语义

为什么不把词条 HTML 直接塞进 JSON：

- 更容易做缓存和 `ETag`
- 更容易独立设置 `Content-Type` 和 CSP
- 更适合 iframe / 受控渲染

## 9. 里程碑

### M0. 仓库重构

- 改成 Cargo workspace
- 引入 `docs/`、`.codex/`
- 建立分层 crate 骨架
- 保持单一后端二进制入口

完成标准：

- 项目可编译
- 空服务可启动
- 基础目录和依赖方向固定

### M1. 词典目录与配置

- 定义 `DictionaryBundle` manifest
- 支持从配置文件加载词典
- 启动时校验 `mdx_path` / `mdd_paths` 和 header 元信息
- 暴露词典列表 / 详情 API

完成标准：

- 能稳定加载多个 bundle
- API 能返回 header/title/entry count 等基础信息

### M2. 精确查词与安全渲染

- 实现精确查词 API
- 实现 HTML 内容 endpoint
- 实现 MDD 资源代理 endpoint
- 完成 HTML/CSS 基础重写

完成标准：

- 一个包含图片/CSS 的实际词典可在前端正常展示
- 默认不导入词典脚本；如显式开启 `entry_script_mode = "original"`，则仅在 sandboxed iframe 内执行

### M3. sidecar 联想索引

- 构建 prefix suggest sidecar
- 实现 suggest API
- 增加索引构建/校验流程

完成标准：

- 联想请求不扫描全词典
- 大词典下延迟稳定

### M4. 性能与硬化

- 基于 benchmark 决定是否启用可选缓存
- 增加 benchmark
- 增加 malformed input / mutation / regression tests
- 加 tracing、metrics、request id

完成标准：

- 有可重复 benchmark
- 若开启缓存，默认仍可关闭，且可观测命中率
- 有针对恶意输入和高并发的回归测试

### M5. 运维能力

- 热重载或显式 reload
- 健康检查和 readiness
- 运行参数与日志完善
- 提供 flake package 与 NixOS module，默认复用同一个 `axum` 进程托管前端页面

完成标准：

- 可用于长期运行的单机服务部署

## 10. 测试策略

测试必须分层：

1. 单元测试
   HTML 重写、DTO 校验、配置解析、cache key、mime 推断。
2. 集成测试
   API 响应、状态码、缓存命中、资源返回。
3. 回归测试
   真实本地词典样本。
4. 硬化测试
   恶意 HTML、异常资源 key、超长 query、损坏词典。
5. benchmark
   启动耗时、lookup p50/p95、suggest p50/p95、热点资源命中率。

参考 `mdict-rs` 的已有思路：

- malformed-input tests
- mutation-style regression tests
- corpus lookup tests

## 11. 验收指标

在本机 NVMe 和真实中型词典样本上，至少测量：

- 冷启动扫描耗时
- 精确查词 p50 / p95
- suggest p50 / p95
- 缓存开启时的命中率
- 资源返回吞吐

建议初始目标：

- warm exact lookup: p50 < 20ms
- warm suggest: p50 < 10ms
- 若启用 entry cache，HTML 命中缓存时接近纯 HTTP 开销

这些值是目标，不是先验保证；必须通过 benchmark 验证。

## 12. 最近实施顺序

接下来按这个顺序推进：

1. 把仓库改成 workspace 并建立 crate skeleton
2. 先做配置加载、catalog、词典列表 API
3. 再做 exact lookup + entry content + resource content
4. 然后补 sidecar suggest
5. 最后做性能、硬化、运维

## 13. 文档同步规则

发生以下变更时，必须同任务更新文档：

- 架构、crate 拆分、依赖方向、里程碑变更：
  更新 `docs/IMPLEMENTATION_PLAN.md`
- API 路径、JSON 字段、内容类型、错误模型变更：
  更新 `docs/API_CONTRACT.md`
- 当前实现状态、已知风险、待办顺序变更：
  更新 `.codex/STATUS.md`
- Agent 启动读物、规则、约束变更：
  更新 `AGENTS.md`

## 14. 当前实现状态

截至 2026-04-03，仓库已落地第一版可运行实现：

- Cargo workspace 已完成，crate 边界与依赖方向固定
- 已实现 TOML 配置加载与 `DictionaryBundle` manifest；配置层可写 `mdd_path` 或 `mdd_paths`，内部统一为有序 `mdd_paths`
- 已实现 catalog、词典列表、详情、healthz、readyz
- 已实现 exact lookup、entry HTML content、resource content
- 已实现 HTML/CSS 重写、按词典配置切换的脚本保留/移除、资源 URL 重写
- 已实现基于 `fst` 的 sidecar suggest 索引，请求路径不再在线扫描全词典
- 已实现 admin reload、request id、基础全局限流、Prometheus metrics
- 已实现可选 entry/resource cache，默认关闭，带容量上限和命中计数
- 已补充单元测试、真实词典 HTTP smoke test、criterion benchmark

当前仍需持续跟进但不阻塞首版上线的点：

- 大二进制 resource 已通过上游化的分块读取接口实现 chunked HTTP 返回；仍会整块处理的主要是需要重写的 CSS 一类文本资源
- benchmark 目前已覆盖 warm lookup / suggest / entry content，后续可继续补启动耗时、资源路径、缓存开启命中率
