# frontend

前端保持独立开发与部署，只通过 `docs/API_CONTRACT.md` 约定的 `/api/v1` 接口与后端交互。

词条内容默认通过 sandboxed iframe 渲染，不直接注入主应用 DOM。
