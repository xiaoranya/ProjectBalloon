# ProjectBalloon 文档

本目录是 ProjectBalloon Rust 实现的文档根目录。之前的 Java 实现已完成迁移并移除；兼容性基线保留在已评审的契约、迁移和 API 迁移矩阵中。

## 状态

- 架构、需求、运维和现有 OpenAPI 契约已提升到工作区根目录。
- 后端实现决策现以 Rust 2024、Tokio、Axum、SQLx、Redis、RabbitMQ 和 S3 兼容对象存储为目标。
- `api/openapi.yaml` 是从之前实现捕获的兼容性基线。在端点对等性评审完成之前，不得将其重新生成为新的 Rust 契约。
- 运行中的 Rust API 在 `/api/openapi.json` 提供其已评审、代码生成的 OpenAPI 3.1 契约，并在 `/api/docs` 提供内置 Swagger UI。
- 判题语言列表中对 Java 的引用指参赛者提交的 Java 代码，而非后端实现语言。

## 文档地图

- `requirements/`：产品与竞赛需求。保留在仓库中用于追溯；目前不包含在已发布的文档网站中。
- `architecture/`：系统边界、数据所有权、判题、安全和架构决策，包括 `ADR-002-rust-backend-reset.md`。
- `api/`：外部可观察的 HTTP 契约和 Rust 实现说明。
- `dev/`：Rust 与前端开发规范。
- `ops/`：快速开始、配置参考、离线安装、运维、故障排查、压测与恢复。
- `user/`：面向选手、管理员和现场运营人员的按角色使用手册（新内容；作为「使用手册」分区发布）。

## 语言

本目录中的每个文档在 `docs/` 下都有对应的英文原文，目录结构一一对应；英文版本为规范版本。`api/openapi.yaml` 是机器生成的契约，不做镜像。需求文档保留在 `docs/requirements/`（中文原版在 `docs/zh-CN/requirements/`）用于追溯，不包含在已发布的文档网站中。

## 源码布局

```text
apps/api/              Rust 模块化单体 API
apps/judge-worker/     Rust 判题 Worker
crates/domain/         纯领域类型与状态机
crates/contracts/      版本化 AMQP 与事件线格式契约
crates/test-support/   共享集成测试支持
migrations/            SQLx PostgreSQL 迁移
frontend/web/          Vue 3 前端
deploy/judge/runtimes/ 参赛语言镜像
deploy/                离线部署定义
scripts/               开发与运维命令
apps/*/tests/          应用集成与验收测试
```

## 文档规则

- 将需求文档、数据库行为、队列契约和已评审的 OpenAPI 操作视为迁移输入，而不是照搬 Java 类结构的指令。
- 架构、契约、安全、部署或运维工作流变更时，应在同一变更中更新相关文档。
- 命令和文件引用使用仓库根目录相对路径。
