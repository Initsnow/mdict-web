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
- `resolved_key`: 后端最终命中的 canonical key
- `match_type`: `exact | normalized`
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
      "tags": ["english", "learner"],
      "status": "ready"
    }
  ]
}
```

### 3.3 词典详情

`GET /api/v1/dictionaries/{dictionary_id}`

响应：`DictionaryDetail`

若词典存在但当前不可用：

- 返回 `503`
- `error.code = "dictionary_unavailable"`

### 3.4 联想搜索

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

### 3.5 精确查词

`GET /api/v1/dictionaries/{dictionary_id}/entries/lookup?key={key}`

响应：

```json
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
```

未命中时：

- 返回 `404`
- `error.code = "entry_not_found"`

### 3.6 词条 HTML 内容

`GET /api/v1/dictionaries/{dictionary_id}/entries/content?key={key}`

成功响应：

- `200 OK`
- `Content-Type: text/html; charset=utf-8`
- `ETag`
- `Cache-Control`
- 严格 CSP
- `X-Content-Type-Options: nosniff`

响应体：

- 已重写的词条 HTML
- 不包含可执行脚本

前端约定：

- 默认用 sandboxed iframe 渲染
- 不直接把返回内容插进主应用 DOM

条件请求：

- 若 `If-None-Match` 命中，返回 `304 Not Modified`

### 3.7 词典资源内容

`GET /api/v1/dictionaries/{dictionary_id}/resources/content?key={resource_key}`

说明：

- `resource_key` 是 MDD 中的原始资源 key
- 必须通过 query 参数传递，不使用路径拼接，避免反斜杠和转义问题

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
