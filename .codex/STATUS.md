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
  - 同名 `.css` / `.js` 资源会优先使用 MDX 同目录下的 sidecar 文件，而不是 MDD 中的同名资源
  - `DictionaryBundle` manifest 现支持 `entry_script_mode = none | original`：`none` 默认移除词典脚本 / 事件属性 / `javascript:` 链接，`original` 则保留词典原始脚本并放宽词条页 CSP 到允许同源脚本
  - 词条 HTML 中的音频链接会改写到 `data-audio-href` 非导航属性；后端会按需注入极小播放 runtime，避免跳到浏览器默认媒体页
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
- `entry_script_mode = "original"` 会显著放宽词条安全边界，只适合用户显式信任的词典
- `entry_script_mode = "original"` 下，词典原始脚本还可能引用并不存在于 MDD 且也不在 MDX 同目录 `.css` / `.js` sidecar 范围内的 companion 文件；这类 404 仍需要按具体词典评估
- 词典原始脚本对音频点击的默认行为并不可靠；当前浏览器侧依赖 entry HTML 内按需注入的最小播放 runtime 来消费 `data-audio-href`

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

2026-04-04 为 `entry-html-v4` 自有折叠运行时再次执行同一命令：

- `lookup/ldoce_apple`: `1.0055 ms .. 1.0158 ms`
- `lookup/ldoce_suggest_app`: `8.6198 µs .. 8.7920 µs`
- `lookup/ldoce_entry_content_apple`: `6.6878 ms .. 7.0645 ms`

说明：

- 这次修复继续维持“移除词典原始脚本”的安全边界，但对已知的 LDOCE 折叠结构注入自有极小运行时，恢复 `LDOCE5pp_sensefold` / `lm5ppBox` 展开与收起，同时保留原有音频原位播放
- 短样本下 `lookup` 回归、`suggest` 与 `entry_content` 改善；波动仍然存在，但这次 `entry_content` 没有出现前一版音频运行时那样的更重回归

2026-04-04 为 `entry-html-v5` 二选一脚本模式再次执行同一命令：

- `lookup/ldoce_apple`: `1.0039 ms .. 1.0379 ms`
- `lookup/ldoce_suggest_app`: `8.5969 µs .. 8.7486 µs`
- `lookup/ldoce_entry_content_apple`: `6.9928 ms .. 7.0085 ms`

说明：

- 这次把“自有运行时”整块移除，改成词典级 `entry_script_mode = none | original`：`none` 完全不导入词典脚本，`original` 则原样保留词典脚本并配套放宽词条页 CSP
- 短样本下三条基准都没有统计显著变化；至少从这轮 warm path 看，去掉自有 runtime 没有带来明显的热路径损失

2026-04-04 为 `entry-html-v6` 原始脚本模式音频 URL 兼容修复再次执行同一命令：

- `lookup/ldoce_apple`: `1.1671 ms .. 1.3154 ms`
- `lookup/ldoce_suggest_app`: `9.0003 µs .. 9.2353 µs`
- `lookup/ldoce_entry_content_apple`: `5.6977 ms .. 6.0934 ms`

说明：

- `entry_script_mode = "original"` 下，部分词典脚本会根据 `href` 文本内容自行拼接音频 URL；这次把本地音频资源链接改成对词典脚本透明的 opaque query 值，并把词条渲染版本提升到 `entry-html-v6`
- 资源归一化只保留 `sound://...` 这类明确属于词典资源协议的输入，不再从任意 `http(s)` URL 反推本地资源 key；这样既保留词典原始脚本，又避免把 engine 逻辑做成词典站点兼容层。这轮短样本里 `lookup` 出现回归、`suggest` 无显著变化、`entry_content` 无显著变化，仍不把这轮结果当成稳定性能结论

2026-04-04 为 `entry-html-v7` 内置音频 runtime 再次执行同一命令：

- `lookup/ldoce_apple`: `996.16 µs .. 1.0068 ms`
- `lookup/ldoce_suggest_app`: `8.5601 µs .. 8.8191 µs`
- `lookup/ldoce_entry_content_apple`: `6.1655 ms .. 6.7431 ms`

说明：

- 这次不再依赖词典原始脚本自己正确阻止 `<a href=...>` 默认导航；后端把音频链接改写到 `data-audio-href`，并在 entry HTML 内按需注入极小 runtime 统一 `preventDefault + Audio(href).play()`，同时把渲染版本提升到 `entry-html-v7`
- 短样本下 `lookup` 变化落在噪声阈值内、`suggest` 无显著变化、`entry_content` 改善；这类结果仍然受短 measurement window 影响较大，不能直接当成稳定结论

2026-04-04 为同名 `.css` / `.js` sidecar 优先级修正再次执行同一命令：

- `lookup/ldoce_apple`: `1.0148 ms .. 1.0289 ms`
- `lookup/ldoce_suggest_app`: `8.8196 µs .. 8.9219 µs`
- `lookup/ldoce_entry_content_apple`: `4.5890 ms .. 5.5407 ms`

说明：

- 这次把资源优先级从“`MDD` 优先、sidecar 仅兜底”调整为“同名 `.css` / `.js` sidecar 优先于 `MDD`”，用来兼容 LDOCE5++ 这类本地 companion 样式覆盖包内旧样式的场景
- 短样本下 `lookup` 变化落在噪声阈值内、`suggest` 无显著变化、`entry_content` 出现改善；由于改动集中在资源访问层，这轮结果仍不应被过度解读为稳定的整体性能提升

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
