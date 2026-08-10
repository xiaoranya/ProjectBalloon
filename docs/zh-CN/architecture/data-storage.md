# 数据存储

本文档定义存储所有权与第一版的持久化规则。

## 存储职责

| 存储 | 职责 |
|---|---|
| PostgreSQL | 权威的关系型业务数据 |
| Redis | 可重建的缓存、计数器与热点实时数据 |
| RabbitMQ | Judge 任务传输与重试 / 死信队列 |
| RustFS | 大型文件对象与生成的产物 |

PostgreSQL 是比赛结果的真相来源。Redis 与 RabbitMQ 绝不能成为最终比赛状态的唯一持久副本。

## PostgreSQL 核心表

初始表分组：

身份与比赛设置：

- `users`
- `teams`
- `team_members`
- `contests`
- `contest_teams`
- `problems`
- `contest_problems`

判题与记分板：

- `submissions`
- `judgements`
- `runs`
- `contest_scoreboard_cells`
- `contest_scoreboard_rows`
- `scoreboard_snapshots`

两个实时记分板投影表以 PostgreSQL 为权威，并在接受最终 Judge 结果的同一事务内按受影响的队伍 / 题目重建。投影将 AC 之前的 WA、TLE、MLE、RE 与 OLE 计为 20 分钟的错误尝试；编译错误、系统错误与取消不计罚时。快照作为独立的不可变产物保留，供 freeze、Resolver 与颁奖工作流使用。

每个记分板快照记录其 `PUBLIC` 或 `ADMIN` 变体、可选的组与参赛筛选、选择器内的单调版本、完整的序列化记分板、其 SHA-256 摘要以及创建它的工作人员用户。PostgreSQL 唯一性约束与基于 advisory lock 的分配可防止并发创建时出现重复版本。数据库触发器拒绝更新与删除，因此后续 Judge 结果或重判可以在不改变历史 Resolver 或颁奖输入的情况下重建实时投影。

实时管理员记分板读取这些投影表。当比赛处于 `freeze_at` 与 `end_at` 之间时，公共记分板会从提交时间戳早于 `freeze_at` 的活动终态判定中重建相同的 ICPC 单元；这防止 freeze 后的结果改变公共排名，同时不会让 Redis 成为真相来源。

启用时，Redis 在带修订版本号的键下存储完整渲染的记分板。修订版本从 PostgreSQL 连同比赛日程读取，并由数据库触发器针对每个会改变排名或展示的已持久化输入推进。缓存条目还区分 public/admin、live/frozen/final 阶段、组与参赛类型。Redis 失败时读写降级到 PostgreSQL；旧修订版本通过 TTL 过期，绝不再被选中。

沟通与现场运营：

- `clarifications`
- `announcements`
- `print_requests`
- `balloon_tasks`
- `balloon_colors`

Resolver 与颁奖：

- `resolver_runs`
- `resolver_snapshots`
- `resolver_events`
- `award_categories`
- `award_rules`
- `award_recipients`

Screen、Live 与审计：

- `screen_instances`
- `screen_groups`
- `screen_commands`
- `broadcast_tokens`
- `audit_logs`

## RustFS 对象类别

RustFS 用作离线部署的 S3 兼容对象存储服务。应用程序代码在可行时应依赖对象存储语义与 S3 兼容 API，而非实现特定行为。

RustFS 存储：

- 题目附件。
- 测试数据。
- 提交源文件。
- 编译日志。
- Judge 日志。
- 导出文件。
- 打印 PDF。

建议的桶布局：

```text
problems/
testdata/
submissions/
judge-logs/
exports/
prints/
backups/
```

对象键在适用时应包含比赛 ID 或题目 ID。上传的测试数据应包含哈希与版本元数据。

Rust API 目前规范化以下键：

```text
problems/{problemId}/attachments/{sha256}/{uuid}-{safeFilename}
problems/{problemId}/testdata/v{version}/{uuid}.zip
submissions/{contestId}/{teamId}/{uuid}.{languageExtension}
```

桶名属于部署配置，绝不嵌入数据库对象键或通过公共 API 返回。

## 数据版本化

影响正式比赛结果的对象与行必须进行版本化或冻结。

对版本敏感的数据：

- 题目描述。
- 测试数据包。
- 时间与内存限制。
- 语言配置。
- 比赛题目别名与顺序。
- 气球颜色。
- 记分板快照。
- Resolver 快照。

比赛配置冻结后，变更应需要特权操作并记录审计。

## 架构迁移

全新 Rust 安装从 `migrations/20260719000000_initial_baseline.sql` 开始。该基线代表先前迁移历史的有效架构；它不复制先前框架的实体模型。

API 将 SQLx 迁移嵌入其可执行文件，并在接受流量前应用它们。已应用的迁移不可变。Rust 版本仅支持全新安装；先前实现的现有安装不得重放全新安装基线。任何历史数据传输都是独立的导出 / 导入项目，而不是升级保证。

## 审计要求

审计日志应记录：

- 操作者用户 ID 与用户类型。
- 操作类型。
- 目标资源。
- 合理情况下的变更前后值。
- 请求 IP。
- 时间戳。
- 结果状态。

关键的审计操作：

- 权限与账户变更。
- 比赛时间变更。
- 题目 / 测试数据变更。
- 重判操作。
- 记分板快照生成。
- Resolver 快照生成。
- 颁奖冻结。
- 打印任务取消 / 拒绝。
- 备份与恢复操作。

## Realtime Outbox

源自已提交业务变更的实时通知首先存储在 PostgreSQL `realtime_outbox` 中。业务事务绝不能在任何提交之前直接向 Redis 发布，也不能依赖提交后的 best-effort 回调。

每行包含唯一事件 ID、比赛、事件类型、受众范围、带版本的 JSON 载荷、可用时间、尝试次数与投递状态。分发器可以安全重试，Redis 仍然是扇出传输而非持久真相来源。

分发器停止时出现待处理行是预期现象。运维就绪性与监控必须区分小的瞬时积压与停滞或耗尽的重试队列。

认领使用有时间限制的 `PUBLISHING` 租约。替代分发器会将过期认领改为 `FAILED` 并重试，因此崩溃可能重复一个失效事件，但不会使其静默搁浅。浏览器消费者使用 `event_id` 去重，并继续周期性的 REST 轮询以收敛。TEAM 行需要 `team_id`；数据库约束同时拒绝无收件人的 TEAM 事件以及附加到更广范围上的收件人 ID。

Redis Pub/Sub 是多个 API 副本共享的投递传输，但它不是持久队列。分发器仅在 Redis 接受 `PUBLISH` 后确认 Outbox 行；连接失败会将行移至 `FAILED` 进行有界指数重试。每个 Redis 信封携带源实例 UUID，使发布副本可以在不处理自身 Pub/Sub 回声的情况下执行立即的本地投递。

## 备份范围

正式比赛备份必须包含：

- PostgreSQL dump 或基础备份。
- RustFS 对象数据与元数据。
- ProjectBalloon 二进制部署配置、包版本与校验和。
- 重建 PostgreSQL、Redis、RabbitMQ、对象存储、代理、打印、沙箱与可观测性服务所需的用户管理服务配置与版本清单。
- 需要时的已生成导出。

Redis 备份对恢复是可选的，因为 Redis 状态应可重建。RabbitMQ 队列状态在实时比赛期间很重要；备份策略应侧重于在维护前优雅停止或排空。
