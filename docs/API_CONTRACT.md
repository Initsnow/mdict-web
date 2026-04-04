# mdict-web API Contract v1

本文档是前后端并行开发的唯一接口基准。

约束：

- 稳定前缀：`/api/v1`
- 所有 JSON 使用 `application/json; charset=utf-8`
- 所有字符串使用 UTF-8
- 时间字段统一 RFC 3339 UTC
- 前端只依赖此文档，不依赖后端内部 crate 结构

## 1. 通用规则

### 1.1 字段命名

- JSON 一律 `snake_case`
- 枚举值一律小写字符串

### 1.2 错误模型

所有错误返回：

```json
{
  "error": {
    "code": "dictionary_not_found",
    "message": "dictionary `oald10` was not found",
    "request_id": "01H...",
    "details": {}
  }
}
```

字段说明：

- `code`: 稳定机读错误码
- `message`: 可读错误信息
- `request_id`: 用于日志关联
- `details`: 可选附加信息

预留错误码：

- `bad_request`
- `dictionary_not_found`
- `entry_not_found`
- `resource_not_found`
- `dictionary_unavailable`
- `rate_limited`
- `unauthorized`
- `internal_error`

### 1.3 内容安全

- 词条正文不通过 JSON 内联返回
- 词条正文通过单独 HTML endpoint 返回
- 资源通过单独二进制 endpoint 返回

## 2. 核心对象

### 2.1 DictionarySummary

```json
{
  "dictionary_id": "oald10",
  "display_name": "Oxford Advanced Learner's Dictionary",
  "description": "English learner dictionary",
  "source_lang": "en",
  "target_lang": "en",
  "entry_count": 125000,
  "has_resources": true,
  "theme_mode": "auto",
  "tags": ["english", "learner"],
  "status": "ready"
}
```

### 2.2 DictionaryDetail

```json
{
  "dictionary_id": "oald10",
  "display_name": "Oxford Advanced Learner's Dictionary",
  "description": "English learner dictionary",
  "source_lang": "en",
  "target_lang": "en",
  "entry_count": 125000,
  "has_resources": true,
  "theme_mode": "auto",
  "tags": ["english", "learner"],
  "status": "ready",
  "header": {
    "title": "Oxford Advanced Learner's Dictionary",
    "description": "....",
    "generated_by_engine_version": "2.0",
    "required_engine_version": "2.0",
    "encoding_label": "UTF-8"
  }
}
```

说明：

- `theme_mode`: 词典级夜间模式策略，取值为 `auto | dictionary | force_auto_dark`
- `theme_mode = auto`: 前端 viewer 在 dark 模式下按渲染结果做启发式 auto-dark；若词条本身已是暗色，则跳过反相
- `theme_mode = dictionary`: 前端 viewer 信任词典自身主题，不再做 auto-dark 兜底
- `theme_mode = force_auto_dark`: 前端 viewer 在 dark 模式下强制启用通用 auto-dark，适合明确只有亮色样式的词典

### 2.3 SuggestionItem

```json
{
  "key": "apple",
  "label": "apple",
  "match_type": "prefix"
}
```

### 2.4 LookupResult

```json
{
  "dictionary_id": "oald10",
  "query_key": "build-up",
  "resolved_key": "build up",
  "redirected_from": "build-up",
  "match_type": "exact",
  "has_resources": true,
  "content_url": "/api/v1/dictionaries/oald10/entries/content?key=build%20up",
  "resource_url_template": "/api/v1/dictionaries/oald10/resources/content?key={resource_key}",
  "etag": "\"entry:oald10:build up:...\""
}
```

非 redirect 命中示例：

```json
{
  "dictionary_id": "oald10",
  "query_key": "apple",
  "resolved_key": "apple",
  "match_type": "exact",
  "has_resources": true,
  "content_url": "/api/v1/dictionaries/oald10/entries/content?key=apple",
  "resource_url_template": "/api/v1/dictionaries/oald10/resources/content?key={resource_key}",
  "etag": "\"entry:oald10:apple:...\""
}
```

说明：

- `query_key`: 用户原始查询
- `resolved_key`: 后端最终用于返回内容的 key；若命中 `@@@LINK=` alias，则这里是最终跳转目标
- `redirected_from`: 可选；当命中 alias record 并跳到其他词条时返回原 alias key
- `match_type`: `exact | normalized`，描述用户查询与首次命中的词典 key 之间的匹配关系，不描述 redirect
- `content_url`: 词条 HTML 内容地址
- `resource_url_template`: 前端或重写逻辑可参考的资源模板

## 3. Public API

### 3.1 健康检查

`GET /healthz`

响应：

```json
{
  "status": "ok"
}
```

响应头：

- 返回 `X-Request-Id`

### 3.1.1 就绪检查

`GET /readyz`

响应：

```json
{
  "status": "ready",
  "ready_dictionaries": 1,
  "unavailable_dictionaries": []
}
```

说明：

- 当有 bundle 加载失败时，`status = "degraded"`
- `unavailable_dictionaries` 返回不可用 `dictionary_id` 列表

### 3.2 词典列表

`GET /api/v1/dictionaries`

响应：

```json
{
  "items": [
    {
      "dictionary_id": "oald10",
      "display_name": "Oxford Advanced Learner's Dictionary",
      "description": "English learner dictionary",
      "source_lang": "en",
      "target_lang": "en",
      "entry_count": 125000,
      "has_resources": true,
      "theme_mode": "auto",
      "tags": ["english", "learner"],
      "status": "ready"
    }
  ]
}
```

### 3.3 全局联想搜索

`GET /api/v1/search/suggest?q={query}&limit={n}&dictionary_id={id}`

约束：

- `q` 必填
- `limit` 默认 `20`
- `limit` 最大 `50`
- `dictionary_id` 可选，可重复传多个；省略时搜索所有 `ready` 词典

响应：

```json
{
  "query": "app",
  "items": [
    {
      "dictionary_id": "oald10",
      "key": "app",
      "label": "app",
      "match_type": "prefix"
    },
    {
      "dictionary_id": "ldoce6",
      "key": "apple",
      "label": "apple",
      "match_type": "prefix"
    }
  ]
}
```

说明：

- 返回聚合后的平铺联想列表，不内联词典正文
- `limit` 作用于总返回项数，不是单词典配额

性能约定：

- 该接口必须基于每本词典的 sidecar 索引
- 不允许请求时扫描全量词条

### 3.4 全局聚合查词

`GET /api/v1/search/lookup?key={key}&dictionary_id={id}`

约束：

- `key` 必填
- `dictionary_id` 可选，可重复传多个；省略时搜索所有 `ready` 词典

响应：

```json
{
  "query_key": "Apple",
  "items": [
    {
      "dictionary_id": "oald10",
      "query_key": "Apple",
      "resolved_key": "apple",
      "match_type": "normalized",
      "has_resources": true,
      "content_url": "/api/v1/dictionaries/oald10/entries/content?key=apple",
      "resource_url_template": "/api/v1/dictionaries/oald10/resources/content?key={resource_key}",
      "etag": "\"entry:oald10:apple:6f91...\""
    }
  ]
}
```

说明：

- 只返回命中的词典
- 不通过该接口直接返回词条 HTML

未命中时：

- 返回 `404`
- `error.code = "entry_not_found"`

### 3.5 词典详情

`GET /api/v1/dictionaries/{dictionary_id}`

响应：`DictionaryDetail`

若词典存在但当前不可用：

- 返回 `503`
- `error.code = "dictionary_unavailable"`

### 3.6 联想搜索

`GET /api/v1/dictionaries/{dictionary_id}/suggest?q={query}&limit={n}`

约束：

- `q` 必填
- `limit` 默认 `20`
- `limit` 最大 `50`

响应：

```json
{
  "dictionary_id": "oald10",
  "query": "app",
  "items": [
    {
      "key": "app",
      "label": "app",
      "match_type": "prefix"
    },
    {
      "key": "apple",
      "label": "apple",
      "match_type": "prefix"
    }
  ]
}
```

性能约定：

- 该接口必须基于 sidecar 索引
- 不允许请求时扫描全量词条

### 3.7 精确查词

`GET /api/v1/dictionaries/{dictionary_id}/entries/lookup?key={key}`

响应：

```json
{
  "dictionary_id": "oald10",
  "query_key": "build-up",
  "resolved_key": "build up",
  "redirected_from": "build-up",
  "match_type": "exact",
  "has_resources": true,
  "content_url": "/api/v1/dictionaries/oald10/entries/content?key=build%20up",
  "resource_url_template": "/api/v1/dictionaries/oald10/resources/content?key={resource_key}",
  "etag": "\"entry:oald10:build up:6f91...\""
}
```

未命中时：

- 返回 `404`
- `error.code = "entry_not_found"`

说明：

- 若首次命中的 record 正文是 `@@@LINK={target}`，后端会在有界深度内解析 redirect 链并返回最终目标词条
- 这不是 HTTP 302；客户端始终收到 JSON lookup 结果

### 3.8 词条 HTML 内容

`GET /api/v1/dictionaries/{dictionary_id}/entries/content?key={key}`

成功响应：

- `200 OK`
- `Content-Type: text/html; charset=utf-8`
- `ETag`
- `Cache-Control`
- 严格 CSP（允许同源父页面通过 iframe 承载，`frame-ancestors 'self'`）
- `X-Content-Type-Options: nosniff`

响应体：

- 已重写的词条 HTML；若 `key` 命中的是 `@@@LINK=` alias，则直接返回最终目标词条的 HTML
- 行为取决于词典 manifest 的 `entry_script_mode = none | original`；默认是 `none`
- `entry_script_mode = none` 时，后端会移除词典自带 `<script>`、内联事件处理器和 `javascript:` 链接，只保留重写后的 HTML/CSS/资源 URL
- `entry_script_mode = original` 时，后端会保留词典原始 `<script>`、内联事件处理器和 `javascript:` 链接，但仍通过受控资源 API 重写相对资源路径
- 词条中的音频资源链接会把真实播放地址改写到非导航属性 `data-audio-href`；后端会在返回的 entry HTML 内按需注入极小 runtime，统一拦截点击后原位播放，避免浏览器直接导航到媒体响应
- 词条中的 `entry://{target}` 链接会重写为同词典的 `entries/content?key={target}`，若原链接带 `#fragment` 则一并保留

前端约定：

- 默认用 sandboxed iframe 渲染
- iframe 需要允许脚本执行，因为部分词典可显式启用 `entry_script_mode = original`
- iframe 需要允许后端注入的最小音频 runtime 执行
- 前端 viewer 可以在同源 iframe 内注入自有 theme runtime，同步父页面 light/dark 状态；夜间模式适配应优先走通用 iframe 运行时，而不是为单本词典写死选择器
- 前端 viewer 需要遵守词典摘要中的 `theme_mode`：`auto` 做启发式检测，`dictionary` 跳过 auto-dark，`force_auto_dark` 在 dark 模式下强制启用通用 auto-dark
- 不直接把返回内容插进主应用 DOM

条件请求：

- 若 `If-None-Match` 命中，返回 `304 Not Modified`
- `304` 响应仍返回与 `200` 一致的词条安全头，确保浏览器缓存不会继续沿用旧 CSP

### 3.9 词典资源内容

`GET /api/v1/dictionaries/{dictionary_id}/resources/content?key={resource_key}`

说明：

- `resource_key` 是 MDD 中的原始资源 key
- 必须通过 query 参数传递，不使用路径拼接，避免反斜杠和转义问题
- 后端也会兼容词条 HTML 中出现的 `sound://...` 音频资源引用，并在服务端归一化到实际的 MDD key 变体
- 若 `resource_key` 对应的 MDX 同目录下存在同名 `.css` / `.js` sidecar 文件，则本地 sidecar 优先于 MDD；其他后缀仍保持 MDD 优先，且当前不扩展到更多 sidecar 类型

成功响应：

- `200 OK`
- `Content-Type` 由后端推断
- 大二进制资源允许分块返回
- `ETag`
- `Cache-Control`
- `X-Content-Type-Options: nosniff`

未命中时：

- 返回 `404`
- `error.code = "resource_not_found"`

条件请求：

- 若 `If-None-Match` 命中，返回 `304 Not Modified`

## 4. 状态码约定

- `200 OK`: 正常返回
- `400 Bad Request`: 参数非法或长度超限
- `404 Not Found`: 词典 / 词条 / 资源不存在
- `429 Too Many Requests`: 限流
- `503 Service Unavailable`: 词典正在加载、损坏或不可用
- `500 Internal Server Error`: 未分类服务端错误

## 5. 缓存约定

### 5.1 HTML 内容

- 支持 `ETag`
- 支持条件请求
- `Cache-Control` 默认允许短期缓存

### 5.2 资源内容

- 静态资源可长缓存
- 若词典 reload，`ETag` 必须变化

## 6. 运维接口

### 6.1 指标接口

`GET /metrics`

说明：

- 默认暴露 Prometheus 文本格式指标
- 路径可由配置调整，但默认值是 `/metrics`

### 6.2 Admin Reload

`POST /api/v1/admin/reload`

认证：

- 必须带 `Authorization: Bearer <reload_token>`
- 若服务端未配置 `reload_token` 或 token 不匹配，返回 `401`
- `error.code = "unauthorized"`

成功响应：

```json
{
  "status": "reloaded",
  "dictionary_count": 1
}
```

## 7. 向后兼容规则

`v1` 期间允许：

- 新增非必填字段
- 新增 endpoint

`v1` 期间不允许：

- 删除已有字段
- 修改已有字段含义
- 修改错误码语义
- 把 HTML endpoint 改成 JSON 内嵌正文

## 8. 预留但暂不实现的接口

以下路径先保留命名空间，不承诺当前版本实现：

- `GET /api/v1/admin/index/jobs/{job_id}`
