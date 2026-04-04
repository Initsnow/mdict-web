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
- 已有独立 `frontend/`，实现了搜索优先首页和单词典详情页
- 后端在 `frontend/dist` 存在时可直接通过 `axum` 同源托管前端静态资源
- 词条 HTML 的 `304 Not Modified` 响应会保留与 `200` 一致的安全头，避免浏览器缓存复用旧 CSP
- 后端已具备：
  - TOML 配置与 `DictionaryBundle` manifest
  - 词典 catalog / list / detail
  - 全局多词典 aggregate suggest / lookup
  - exact lookup / suggest / entry content / resource content
  - MDX `@@@LINK=` alias 解析与最终词条跳转
  - 词条 HTML 中的 `entry://...` 交叉引用会重写到同词典 `entries/content`，不再误走资源接口
  - `sound://...` 音频资源 key 到实际 MDD 路径的服务端归一化
  - 词条 HTML 中的音频链接会被后端重写为 `data-audio-href`；仅在词条真的包含音频链接时才注入自有 iframe 运行时，在词条页内部原位播放，不再导航到默认浏览器播放器
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
8. 搜索优先首页、全局多词典搜索结果与 iframe 预览
9. 单元测试、HTTP smoke test、criterion benchmark

## 剩余增强项

1. 若要进一步优化大文本资源返回，需要继续细化 CSS 等需重写资源的 streaming 策略
2. 补更多 benchmark 维度：启动耗时、resource path、缓存开启后的命中率与收益
3. 全局搜索结果的排序策略、词典范围筛选和命中权重仍可继续细化

## 已知风险

- `mdict-rs` 当前 `FileSource` 的 `Mutex<File>` 可能在高并发热点词典下形成竞争
- 词条 HTML 的资源重写覆盖面必须实测，尤其是 CSS `url(...)`、相对路径、奇怪的资源 key
- 真实词典差异很大，必须用本地语料做回归
- `mdict-rs` 是 `AGPL-3.0-only`，许可证策略必须尽早确认
- 大二进制资源现在会按 chunk 返回，但 CSS 等需重写资源仍会整块处理
- 如果以后进一步放大应用层缓存，需要持续验证收益是否真的高于额外内存占用
- 当前全局 lookup 结果按词典遍历顺序返回，尚未引入更细的跨词典排序策略

## 基准记录

2026-04-03 在本地 LDOCE5++ 样本上执行。
样本路径现在优先使用 `~/Documents/Dictionaries/英汉/LDOCE5++/`，回退到旧的 `~/projects/mdict-rs/tmp-dict/LDOCE5++/`：

- 命令：
  `cargo bench -p mdict-web-app --bench lookup -- --sample-size 10 --warm-up-time 0.1 --measurement-time 0.1`
- 结果：
  - `lookup/ldoce_apple`: `934.91 µs .. 946.68 µs`
  - `lookup/ldoce_suggest_app`: `8.3224 µs .. 8.4150 µs`
  - `lookup/ldoce_entry_content_apple`: `5.8452 ms .. 6.4270 ms`

说明：

- 这是 warm path 小样本结果，适合作为当前回归基线，不代表最终容量上限
- 资源与缓存收益 benchmark 仍需补齐
- `cargo test --workspace` 中的真实词典 HTTP smoke test 当前已覆盖新的全局多词典搜索接口与静态前端托管路径，并在本地样本上通过

2026-04-04 为 `@@@LINK=` alias resolve 多次重复执行同一命令：

- `lookup/ldoce_apple`: `952.88 µs .. 1.1777 ms`
- `lookup/ldoce_suggest_app`: `8.0027 µs .. 8.9180 µs`
- `lookup/ldoce_entry_content_apple`: `4.6189 ms .. 7.3158 ms`

说明：

- 这次仍是短样本测量，同日多次重复的波动已经足够大，暂不把单次 change 判定直接视为稳定回归或稳定提升
- 如果后续要对 alias resolve 的性能影响做结论，需要增大 sample size / measurement time 后再看

2026-04-04 为音频链接重写与 `entry-html-v3` 再次执行同一命令：

- `lookup/ldoce_apple`: `985.41 µs .. 1.0067 ms`
- `lookup/ldoce_suggest_app`: `8.8690 µs .. 8.9863 µs`
- `lookup/ldoce_entry_content_apple`: `7.5056 ms .. 7.6596 ms`

说明：

- 这次修复把词条 HTML 中的音频链接从可导航 `href` 改写为 `data-audio-href`，并仅在需要时在词条页里注入自有音频运行时；同时提升了 `ENTRY_RENDER_VERSION`，避免浏览器继续复用旧缓存正文
- `entry_content` 短样本下出现了更明显的统计显著回归；如果要判断这次运行时注入与额外 HTML 重写逻辑的真实成本，需要放大样本后再看

2026-04-04 为 `entry://...` 交叉引用重写到 `entries/content` 再次执行同一命令：

- `lookup/ldoce_apple`: `956.18 µs .. 972.72 µs`
- `lookup/ldoce_suggest_app`: `10.451 µs .. 10.556 µs`
- `lookup/ldoce_entry_content_apple`: `7.2441 ms .. 7.7138 ms`

说明：

- 这次修复把词条 HTML 里的 `entry://...` 链接从错误的资源接口改写为同词典 `entries/content`
- `entry_content` 在短样本下没有统计显著变化；`suggest` 出现回归而 `lookup` 出现改善，这类短样本波动仍然偏大，暂不把这次结果直接当成稳定性能结论

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
