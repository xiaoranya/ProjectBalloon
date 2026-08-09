# API 迁移矩阵

遗留 `openapi.yaml` 是兼容性基线，而不是新 Rust 行为的规范。每个操作都处于以下状态之一：

- `UNREVIEWED`：尚未评估兼容性；
- `COMPATIBLE`：保留遗留 HTTP 契约；
- `REDESIGN`：有意变更契约并记录替代方案；
- `IMPLEMENTED`：Rust 代码存在但验证不完整；
- `VERIFIED`：契约与集成测试通过；
- `DEPRECATED`：该操作不会被迁移。

## 已评审路由清单

快照日期：2026-08-09。`{id}` 与 `{contest_id}` 等路径参数名视为等价。

| 清单 | 操作数 |
|---|---:|
| 遗留 OpenAPI 基线 | 147 |
| 当前 Axum 路由 | 208 |
| 方法与规范化路径相同 | 108 |
| 对遗留操作的显式 Rust 重设计 | 39 |
| 仅 Rust 的替代或扩展操作 | 100 |
| 未评审的遗留操作 | 0 |

更改遗留基线或 `apps/api/src/lib.rs` 后运行 `python3 scripts/check-api-compat.py --check`。计数变化会在本快照与下方分类被评审之前故意失败。

39 个非相同的遗留路由如下：

| 遗留路由族 | 操作数 | Rust 决策 |
|---|---:|---|
| `/api/resolver/runs/*`、`/api/contests/{id}/resolver/*` | 7 | REDESIGN：持久 run 资源位于 `/api/admin/resolver-runs/*` 下；公共恢复使用 `/api/public/resolver-runs/{id}/state` |
| 比赛嵌套的气球项操作 / 详情 | 5 | REDESIGN：项操作使用 `/api/balloons/{id}/*`；模糊的状态变更拆分为 `deliver` 与 `cancel`，比赛列表行提供项详情 |
| 颁奖类别、生成 / 冻结 / 导出 / 获奖者 | 12 | REDESIGN：所有变更与操作员读取都使用显式 `/api/admin/*` 命名空间；获奖者生成拆分为生成与手动工作流 |
| 单次与批量重判遗留路由 | 7 | REDESIGN：每个操作都在 `/api/admin/contests/{id}` 下按比赛限定；持久任务暴露预览、暂停与恢复，而不是有损的全局取消 |
| 全局提交详情与源码路由 | 2 | REDESIGN：队伍与管理员的详情按比赛限定并强制不同的投影；源码仅在授权的详情响应内返回 |
| 分组 / 管理员记分板与遗留记分板导出 | 3 | REDESIGN：分组选择是 `GET /api/contests/{id}/scoreboard?groupName=...`；管理员读取与 CSV 使用 `/api/admin/contests/{id}/scoreboard[.csv]` |
| Resolver 当前状态 | 1 | REDESIGN：不可变来源选择与公共当前状态按 run 限定，而不是隐式的比赛单例 |
| 题目概览 | 1 | DEPRECATED：`GET /api/contests/{id}/problems` 是更丰富的限定投影，避免第二个不一致的读模型 |
| 管理员健康 | 1 | REDESIGN：`/api/health` 是部署就绪契约，`/livez` 是进程存活 |

`GET /api/admin/contests/{contestId}/judge-queue/status` 保留其遗留方法、路径与响应字段。Rust 将 `PUBLISHING` 与 `PENDING` Outbox 行一起计数，因为已租约但未确认的任务并未排空。

`scripts/test/docker-integration.sh` 的最新一次运行通过了完整的 Docker 后端 API、PostgreSQL、Redis、RustFS、RabbitMQ 与 Judge 沙箱集成集合。它们仍被默认离线测试命令忽略，因为需要 Docker daemon 与固定镜像。

## 领域评审总结

| 领域 | 结果 | 备注 |
|---|---|---|
| 认证、工作人员账户、权限 | VERIFIED | 覆盖会话、CSRF、密码迁移 / 重置、直接账户权限与比赛范围管理 |
| 比赛、队伍、名单、生命周期、归档 | VERIFIED | 包含克隆、自动里程碑、归档前任务检查与数据库只读保护 |
| 题库与比赛题目 | VERIFIED | PostgreSQL 服务行为与文件传输路径通过集成套件；其余目录变更有意仅限超级管理员 |
| 提交、重判、Judge 传输 | VERIFIED | 存在比赛范围投影、持久 Outbox、RabbitMQ 结果幂等、批量任务、导出与队列排空状态 |
| 记分板与快照 | VERIFIED | 存在公共 / 管理员变体、分组与参赛查询筛选、freeze 行为、CSV、快照与 Redis 缓存 |
| Clarification、公告、打印、气球 | VERIFIED | 定时公告与 CUPS 投递是超出基线的 Rust 扩展 |
| Resolver、颁奖、展示、Screen、Live/OBS | REDESIGN | 功能完整的操作员流程使用显式 admin/run 资源与公共 token 限定视图 |
| 健康与运维 | REDESIGN | 就绪 / 存活、清理积压、Worker、RabbitMQ 与存储都体现在 Rust 健康模型中 |

## 题目切片

| 遗留操作 | 状态 | Rust 方向 |
|---|---|---|
| `GET /api/problems` | VERIFIED | 强制超级管理员有界题库列表与 `contestId` 限定的比赛管理员目录访问；拒绝无范围比赛管理员读取与外来范围 |
| `POST /api/problems` | VERIFIED | 校验限制与封闭的 P0 语言集；返回 `201`；仅超级管理员创建由 PostgreSQL 集成测试覆盖 |
| `GET /api/problems/{id}` | VERIFIED | 存在超级管理员与全分配限定的比赛管理员读取；外来或未分配访问隐藏为未找到 |
| `PATCH /api/problems/{id}` | VERIFIED | 乐观并发、全分配比赛管理员范围与 freeze 检查通过 PostgreSQL 集成测试 |
| `DELETE /api/problems/{id}` | VERIFIED | 题目分配给比赛时阻止软删除；未分配删除由 PostgreSQL 集成测试覆盖 |
| `PUT /api/problems/{id}/statements/{langCode}` | VERIFIED | 存储有界 Markdown，返回服务端渲染的净化 HTML，并强制全分配比赛管理员范围与 freeze 检查 |
| `GET /api/problems/{id}/statements` | VERIFIED | 为限定编辑器返回持久化 Markdown 描述，不暴露队伍面向投影 |
| `DELETE /api/problems/{id}/statements/{langCode}` | VERIFIED | 在与描述更新相同的范围与 freeze 规则下删除持久化语言描述 |
| `GET /api/problems/{id}/attachments` | VERIFIED | Rust 扩展为限定编辑器列出持久化附件元数据，不暴露对象键 |
| `GET /api/problems/{id}/testdata` | VERIFIED | 限定工作人员下载当前归档返回安全响应头，不暴露对象键；流式传输保留 |
| `POST /api/problems/{id}/testdata` | VERIFIED | 有界上传、深度 ZIP 安全与 `.in`/`.out` 配对校验、不可变版本、SHA-256、过期写入保护、freeze 复查与补偿通过测试 |
| `POST /api/problems/{id}/attachments` | VERIFIED | 有界 multipart 上传、S3 对象写入、SHA-256 元数据、全分配范围、freeze 复查与补偿通过 PostgreSQL 集成测试 |
| `GET /api/problems/{id}/attachments/{attachmentId}` | VERIFIED | 父题目授权、名单 / 生命周期检查、安全响应头与不透明存储元数据通过 PostgreSQL 集成测试 |
| `DELETE /api/problems/{id}/attachments/{attachmentId}` | VERIFIED | 仅 DRAFT 元数据删除、审计与 best-effort 对象清理通过 PostgreSQL 集成测试；这是 Rust 重设计扩展 |
| `GET /api/contests/{contestId}/problems` | VERIFIED | 限定工作人员视图与名单队伍发布视图通过 PostgreSQL 集成测试 |
| `POST /api/contests/{contestId}/problems` | VERIFIED | 强制比赛管理员范围、DRAFT 生命周期、别名、顺序与颜色校验；限定分配由 PostgreSQL 集成测试覆盖 |
| `PATCH /api/contests/{contestId}/problems/{problemId}` | VERIFIED | 锁定比赛生命周期、映射确定性唯一性冲突，并由比赛管理编辑器暴露 |
| `DELETE /api/contests/{contestId}/problems/{problemId}` | VERIFIED | 仅允许在 DRAFT 中限定移除，并拒绝带提交的分配 |
| `PUT /api/contests/{contestId}/problems/reorder` | VERIFIED | 完整集合校验与延迟唯一性提供原子位置交换 |
| `GET /api/contests/{contestId}/problems/overview` | DEPRECATED | 由更丰富的限定 `GET /api/contests/{contestId}/problems` 投影替代 |
| `POST /api/contests/{contestId}/submissions` | VERIFIED | 显式队伍身份、RUNNING 时间窗口、名单 / 题目 / 语言检查、64 KiB 源码上传、精确滚动限流、初始判定、JudgeTask Outbox、实时事件与补偿通过 PostgreSQL 测试 |
| `GET /api/contests/{contestId}/submissions` | REDESIGN | 实现队伍私有与授权工作人员投影，带有限光标分页 |
| `GET /api/contests/{contestId}/submissions/{submissionId}` | REDESIGN | 隐藏其他队伍的源码与敏感 Judge 输出；单独暴露工作人员详情 |

测试数据与附件传输现在有已验证的有界实现。失败的附件、测试数据、提交源码与打印 PDF 补偿删除会持久化，用于多实例安全的后台重试。附件元数据删除在同一个数据库事务中注册其清理任务，关闭提交到 RustFS 删除之间的崩溃窗口。附件 HTTP 下载现在直接使用 S3 流；测试数据下载在验证不可变 SHA-256 时仍会缓冲。双向桶到数据库对账现在持久化缺失引用发现，并在运维上暴露其未解决计数。Worker 已执行有界、策略兼容的解压。

Judge Task 分发与 Judge Result 消费在 `PROJECT_BALLOON_RABBITMQ_ENABLED` 之后得到验证：存在持久拓扑声明、稳定消息 ID、强制 Publisher Confirm、多实例 Outbox 租约、指数重试、过期租约恢复、事务性结果幂等、提交后 ACK、死信拒绝与就绪投影。实时 Docker 验证覆盖 confirm、任务重试 TTL、结果 ACK、重复重放与畸形结果死信。broker 重启测试存在，但必须作为生产彩排的一部分执行，而不是默认测试套件。
