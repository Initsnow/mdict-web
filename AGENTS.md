# mdict-web Agent Guide

## Read Order

1. Read `.codex/STATUS.md`.
2. Read `docs/IMPLEMENTATION_PLAN.md` if the task touches architecture, crate split, performance, safety, roadmap, or deployment.
3. Read `docs/API_CONTRACT.md` if the task touches routes, DTOs, HTML/resource delivery, or frontend/backend coordination.
4. Inspect the current workspace `Cargo.toml`, `crates/`, `frontend/`, and changed files before editing.

## Project Goal

Build a high-performance, safe, API-first Rust MDict web server on top of `mdict-rs`.

## Core Architecture

- `mdict-rs` stays the parser core for `.mdx` / `.mdd`.
- `mdict-web` owns config, catalog, sidecar indexing, caching, HTML/resource rewriting, HTTP API, and observability.
- Frontend and backend are separate by contract; `docs/API_CONTRACT.md` is the source of truth.

当前后端 workspace 结构：

- `crates/mdict-web-config`: TOML 配置与 `DictionaryBundle` manifest 解析
- `crates/mdict-web-domain`: DTO、错误模型、公共领域类型
- `crates/mdict-web-index`: sidecar suggest 索引
- `crates/mdict-web-engine`: `mdict-rs` 接入、阻塞 I/O、HTML/CSS 重写、可选缓存
- `crates/mdict-web-service`: catalog、reload、用例编排
- `crates/mdict-web-http`: axum 路由、中间件、内容安全头、admin/reload
- `crates/mdict-web-app`: 入口二进制与 benchmark

## Non-Negotiable Rules

- Treat dictionary files, entry HTML, CSS, and resources as untrusted input.
- Do not inject raw MDX HTML into the main frontend DOM.
- Keep blocking dictionary I/O off the async reactor; use a dedicated blocking pool.
- Do not implement online full-dictionary scans for suggest.
- Prefer low-memory-first defaults; application-level entry/resource caches are opt-in and disabled by default.
- If cache is introduced or enabled, it must be bounded, configurable, and observable.
- Prefer upstreamable improvements to `mdict-rs` over server-local parser hacks.
- Respect `mdict-rs` licensing constraints: it is `AGPL-3.0-only` unless a separate commercial license is in place.
- Do not add `unsafe` without a measured need, tight isolation, and doc updates.
- `DictionaryBundle` manifest 在程序内部统一使用 `mdd_paths = []`；配置文件层允许写单值 `mdd_path`，但必须在解析时立刻归一化为 `mdd_paths`，不要把双字段继续带进后续实现。

## Documentation Sync Rule

If your task changes any of the following, update the doc in the same task:

- architecture, scope, crate boundaries, milestones, dependency direction:
  `docs/IMPLEMENTATION_PLAN.md`
- API paths, query params, JSON fields, content types, error codes:
  `docs/API_CONTRACT.md`
- current repo state, active TODOs, benchmark findings, known risks:
  `.codex/STATUS.md`
- startup instructions or repo invariants for future sessions:
  `AGENTS.md`

If code and docs disagree, bring the docs back in sync before ending the task.

## Validation

修改后优先使用这些命令验证：

- `cargo fmt --all`
- `cargo test --workspace`
- `cargo bench -p mdict-web-app --bench lookup -- --sample-size 10 --warm-up-time 0.1 --measurement-time 0.1`

本地真实词典 smoke/benchmark 目前默认使用：

- `/home/initsnow/Documents/Dictionaries/英汉/LDOCE5++/LDOCE5++ V 2-15.mdx`
- `/home/initsnow/Documents/Dictionaries/英汉/LDOCE5++/LDOCE5++ V 2-15.mdd`

若不存在，再回退到旧路径：

- `/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdx`
- `/home/initsnow/projects/mdict-rs/tmp-dict/LDOCE5++/LDOCE5++ V 2-15.mdd`

当前二进制默认会在 `frontend/dist` 存在时由 `axum` 直接托管前端静态文件。

- 可用 `--frontend-dist` 或 `MDICT_WEB_FRONTEND_DIST` 覆盖 dist 路径
- 可用 `--no-frontend` 或 `MDICT_WEB_DISABLE_FRONTEND=true` 关闭静态前端托管
- 开发期仍优先使用 `frontend/` 下的 Vite dev server
