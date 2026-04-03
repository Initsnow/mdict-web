# mdict-web 状态

## 当前仓库状态

- 仓库已完成第一版 workspace 重构并可运行
- 当前后端目录：
  - `crates/mdict-web-config`
  - `crates/mdict-web-domain`
  - `crates/mdict-web-index`
  - `crates/mdict-web-engine`
  - `crates/mdict-web-service`
  - `crates/mdict-web-http`
  - `crates/mdict-web-app`
- 已有独立 `frontend/` 占位目录
- 后端已具备：
  - TOML 配置与 `DictionaryBundle` manifest
  - 词典 catalog / list / detail
  - exact lookup / suggest / entry content / resource content
  - HTML/CSS 重写与内容安全头
  - sidecar suggest 索引
  - admin reload / healthz / readyz / metrics
  - 可选 entry/resource cache，默认关闭
  - 单元测试、真实词典 HTTP smoke test、criterion benchmark

## 已锁定的设计决策

- 解析内核使用 `~/projects/mdict-rs` / `mdict-rs = "0.1"`
- 后端是 Rust API 服务，前端与后端架构分离
- API 合同以 `docs/API_CONTRACT.md` 为准
- 词条正文和 JSON 元数据分离返回
- 词条正文必须经过 HTML/CSS 重写，默认通过 sandboxed iframe 展示
- 联想搜索必须依赖 sidecar 索引，不能在线扫描全量词条
- `mdict-rs` 的同步阻塞 I/O 只能放到专用阻塞线程池
- 采用低内存优先策略：应用层 entry/resource cache 默认关闭
- 若以后启用缓存，必须能关、能调、能观测命中率

## 当前已完成项

1. Cargo workspace 与 crate 分层骨架
2. 配置加载、manifest、catalog
3. 列表 / 详情 / healthz / readyz / metrics / admin reload
4. exact lookup / entry content / resource content
5. HTML/CSS 重写与资源路径代理
6. `fst` sidecar suggest 索引
7. 可选缓存与命中指标
8. 单元测试、HTTP smoke test、criterion benchmark

## 剩余增强项

1. 若要进一步优化大文本资源返回，需要继续细化 CSS 等需重写资源的 streaming 策略
2. 补更多 benchmark 维度：启动耗时、resource path、缓存开启后的命中率与收益
3. 前端独立实现仍未开始

## 已知风险

- `mdict-rs` 当前 `FileSource` 的 `Mutex<File>` 可能在高并发热点词典下形成竞争
- 词条 HTML 的资源重写覆盖面必须实测，尤其是 CSS `url(...)`、相对路径、奇怪的资源 key
- 真实词典差异很大，必须用本地语料做回归
- `mdict-rs` 是 `AGPL-3.0-only`，许可证策略必须尽早确认
- 大二进制资源现在会按 chunk 返回，但 CSS 等需重写资源仍会整块处理
- 如果以后进一步放大应用层缓存，需要持续验证收益是否真的高于额外内存占用

## 基准记录

2026-04-03 在本地 LDOCE5++ 样本上执行：

- 命令：
  `cargo bench -p mdict-web-app --bench lookup -- --sample-size 10 --warm-up-time 0.1 --measurement-time 0.1`
- 结果：
  - `lookup/ldoce_apple`: `945.00 µs .. 993.31 µs`
  - `lookup/ldoce_suggest_app`: `8.6584 µs .. 8.8543 µs`
  - `lookup/ldoce_entry_content_apple`: `6.9579 ms .. 7.2834 ms`

说明：

- 这是 warm path 小样本结果，适合作为当前回归基线，不代表最终容量上限
- 资源与缓存收益 benchmark 仍需补齐

## 实现约束

- 不直接在 async runtime 上执行词典阻塞 I/O
- 不把未处理的 MDX HTML 注入前端主 DOM
- 不为了短期上线在 `mdict-web` 中做 `mdict-rs` 私有分叉式脏修补
- 不默认启用 entry/resource payload cache
- 若架构/API/状态变更，必须同步更新文档

## 推荐启动阅读顺序

1. `AGENTS.md`
2. `docs/IMPLEMENTATION_PLAN.md`
3. `docs/API_CONTRACT.md`
4. 当前实际代码
