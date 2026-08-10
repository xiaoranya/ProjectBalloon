# ADR-002：Rust 后端重置

## 状态

已接受，用于 ProjectBalloon 重置。

## 背景

之前的实现是 Java/Spring 模块化单体，内置判题调度器和单独部署的判题 Worker。其外部可见行为包括大型 HTTP API、PostgreSQL 模式、SSE 通道、RabbitMQ 消息、S3 兼容对象和运维工作流。

重置的目标是产出一个地道的 Rust 系统。照搬之前的 controller/service/repository/entity 类图会保留实现意外，产生不必要的 trait 和动态分发，并掩盖事务与并发边界。

## 决策

使用一个 Cargo 工作区，包含：

- 基于 Tokio 的 Axum API 模块化单体；
- 由 API 承载的调度器和事务性 outbox 分发器；
- 可单独部署的 Tokio 判题 Worker；
- 在少量库 crate 中的纯共享领域类型和版本化线契约；
- 使用显式 PostgreSQL 查询和事务的 SQLx；
- 用于 RabbitMQ 的 Lapin、用于 Redis 的 `redis`、用于 RustFS 的 `object_store`；
- 用于从 Rust 代码生成已评审 OpenAPI 契约的 Utoipa。

Vue 前端、以 PostgreSQL 为事实来源、Redis 作为可重建状态、RabbitMQ 拓扑、RustFS 对象语义、离线部署模型和生产沙箱隔离仍然作为架构约束。

## 模块规则

- 业务功能位于 `apps/api/src/features/` 之下。
- 基础设施适配器位于所属应用之下，除非被多个二进制真正共享。
- `crates/domain` 不依赖任何框架或基础设施。
- `crates/contracts` 拥有 AMQP/SSE 载荷版本；不拥有业务编排。
- trait 表示有意义的可替换边界，而不是每张数据库表。
- HTTP DTO、数据库行、领域值和队列消息不被视为一个通用实体类型。
- 官方状态转换用枚举和受检查的转换函数表达。

## 兼容性边界

以下内容是需要显式兼容性评审的迁移输入：

- `docs/api/openapi.yaml`；
- 有效的 PostgreSQL 模式与约束；
- RabbitMQ 的 exchange、队列、路由键、ACK、重试和死信行为；
- 判题任务/结果消息中的 JSON 字段名和枚举值；
- SSE 通道授权与公共数据过滤；
- RustFS 桶/键约定和对象哈希；
- 备份、恢复、离线安装和健康检查行为。

兼容性通过行为和夹具评估，而不是匹配之前的源文件名或类。

## 持久化决策

对于全新安装，将之前的模式历史整合为一个已评审的 SQLx 基线迁移，然后创建不可变的前向迁移。

对于包含正式数据的安装，Rust 重置不承诺就地升级。将 Rust 基线部署到全新数据库；任何历史数据导出/导入必须与 SQLx 迁移链分开设计和评审。

与 RabbitMQ 发布耦合的数据库写入使用事务性 outbox。消费者是幂等的，并且仅在建立持久结果、重试或死路径后才 ACK。

## 影响

- Rust 代码围绕所有权、类型化状态、显式事务、有界并发和取消设计，而不是框架注解。
- API 保持一个可部署单元，避免过早的分布式事务和运维开销。
- 判题 Worker 保持隔离，因为其资源使用和威胁模型与业务 API 不同。
- 契约夹具和集成测试在移除之前实现后保留已评审行为。
- 构建和依赖产物必须缓存并打包，以支持完全离线安装。
