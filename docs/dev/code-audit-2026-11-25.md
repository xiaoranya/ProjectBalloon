---
title: 代码审计问题清单
description: 2026-11-25 全量代码通读的审计产出，按严重性分级，含去重合并、核验状态与修复批次建议。
---

# 代码审计问题清单（2026-11-25）

## 审计范围与方法

- 工作区：`ProjectBalloon` v0.1.0-alpha.5（Rust 2024 workspace 63,780 行 / 230 文件；前端 Vue 3 手写约 39,000 行，跳过 `src/api/types.generated.ts`）。
- 方法：8 路并行逐文件通读生产代码，对照 `docs/dev/coding-style.md`、`docs/architecture/` 各 ADR 与设计文档、`docs/requirements/` 需求文档、`migrations/0001_initial.sql`。
- 去重：同一问题被多个模块交叉发现时合并为一条，并在「位置」列出全部落点。
- 未修改任何文件。

## 核验状态说明

| 标记 | 含义 |
| --- | --- |
| ✅ 已核验 | 审计者回读源码确认，关键项另查 sqlx 0.9.0 源码与迁移列类型交叉验证 |
| ⚠️ 待复核 | 逻辑链已在报告中给出，但未逐行复核；修复前建议先按「位置」自行确认一行 |

**P0 共 6 条，全部 ✅ 已核验。** P1 中 7 条已核验（P1-01、02、16、23、25、35、36），其余为待复核。P2/P3 均为待复核。

P1 的严重性经审计者统一校准过：认证类问题（会话撤销、KEEPTTL 复活、账号大小写、bootstrap 抢占）在各分报告中为 P2，因其为确定性安全缺口或账号混淆面，此处上调为 P1。

## 统计

| 严重性 | 条数 |
| --- | --- |
| P0 | 6 |
| P1 | 42 |
| P2 | 104 |
| P3 | 100 |
| **合计** | **252** |

正文中 P0/P1 按「问题域」分节（认证、实时、判题、计分、题目、竞赛、基础设施、展示、前端、沙箱纵深），P2/P3 按「所属模块」分节。

---

# P0 — 可导致沙箱逃逸、判题操纵、数据损坏或系统性失效

## P0-01 选手可通过符号链接实现宿主任意文件写 ✅

- **位置**：`apps/judge-worker/src/sandbox/runner.rs:277`（Docker）、`apps/judge-worker/src/sandbox/bwrap.rs:331`（bwrap）
- **问题**：每个测试点都用 `tokio::fs::copy(data_dir/N.in, work_dir/current.in)` 在**宿主机**写入。`work_dir` 以可写 bind 挂进沙箱（`runner.rs:410-416` 的 `read_only: Some(false)`、`bwrap.rs:484-486` 的 `--bind`），容器进程 uid 与宿主机 worker 相同，因此 `/work` 是选手可写的。路径解析发生在宿主机命名空间，且未加 `O_NOFOLLOW`。
- **影响**：选手在第 1 个测试点把 `/work/current.in` 换成 symlink（指向 `~/.ssh/authorized_keys`、其他题目 testdata 等），第 2 个测试点的 `copy` 即跟随链接覆盖宿主文件，`set_private_file_permissions` 还会对目标 `chmod 0600`。悬空 symlink 可创建新文件。等价于 worker 权限的任意文件写。
- **建议**：目标文件先 `remove_file_if_present`，再用 `OpenOptions::new().write(true).create(true).truncate(true).custom_flags(O_NOFOLLOW)` 打开并 `is_file()` 校验；或改为不经过选手可写目录注入输入（如 tmpfs + 只读 bind）。

## P0-02 交互题 interactor 位于选手可写目录，可被替换后白拿 AC ✅

- **位置**：`apps/judge-worker/src/sandbox/runner.rs:131-137`、`apps/judge-worker/src/sandbox/bwrap.rs:290-296`；执行点 `runner.rs:850`（`/work/interactor ...`）
- **问题**：interactor 二进制由 worker 写入 `work_dir/interactor` 并 `chmod 0700`，但该目录在沙箱内**同 uid 可写**。交互 shell 直接执行 `/work/interactor`，AC 判定只依赖其退出码（`runner.rs:856-857` 的 `exit 20`）。
- **影响**：选手在第一个测试点 `unlink("/work/interactor")` 并写入一个立即 `return 0` 的假 interactor，从第二个测试点起恒为「接受」，交互题无需真正作答即 Accepted。代码注释专门论证过 GNU time 不能放 `/work`（同 uid 可写），但 interactor 本身漏了同一威胁模型。
- **建议**：照 `.pb-time` 的做法，把 interactor 以只读 bind 挂到固定路径（如 `/work/.pb-interactor`），Docker 侧增加第二个只读 `Mount`；运行前后校验哈希。

## P0-03 bwrap 后端在宿主机读取选手可写诊断文件，泄露 worker 凭据并可能拖死进程 ✅

- **位置**：`apps/judge-worker/src/sandbox/bwrap.rs:632-642`（`read_interactive_diagnostics`），调用点 `bwrap.rs:358-362`
- **问题**：`tokio::fs::read(work_dir.join("program.err"))` 在**宿主机命名空间**解析路径，无 `O_NOFOLLOW`，且 `read` 无长度上限。`/work/program.err` 由交互 shell 写入，选手可在运行中 `unlink` 后重建为 symlink。
- **影响**：两条利用路径。(1) `program.err → /proc/self/environ`：内核在宿主机解析 `self` 得到 **worker 自身**，读出 `PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY`、AMQP URL 密码等，经 `run.logs` 进入 `stderr_tail` 写入 `JudgeResult`，选手可见。(2) `program.err → /dev/zero`：`tokio::fs::read` 无限读取，worker 内存/线程被拖死。Docker 后端是在容器内 `cat`（`runner.rs:628-634`），symlink 只在容器命名空间解析，无此问题——说明这是 bwrap 独有的宿主逃逸面。
- **建议**：`O_NOFOLLOW` 打开 + `fstat` 校验 regular file + 限长（16/64 KiB）；或改为在沙箱内读取输出流，不在宿主机按路径读选手文件。

## P0-04 计分榜投影的 PostgreSQL 回放查询类型不匹配，重建必然失败 ✅

- **位置**：`apps/api/src/features/scoreboard/redis_state.rs:294-314`（`recompute_cell`）、`:347-362`（`rebuild_contest`）
- **问题**：SQL 返回 `judgements.score_milli`、`contest_problems.max_score_milli`，二者在迁移中均为 `integer`（`migrations/0001_initial.sql:4718`、`:4759`），但 Rust 侧声明为 `i64`。查 sqlx-postgres 0.9.0 源码：`PgTypeInfo::type_compatible` 为严格相等（`sqlx-postgres-0.9.0/src/type_info.rs:1065-1070`），`i64` 只声明 `INT8`（`src/types/int.rs:134-138`），INT4 与 INT8 不兼容。
- **影响**：`recompute_cell` 对任何有判题记录的 cell 都在解码处失败；`rebuild_contest` 只要比赛有已判提交就整体失败。结果：「重判后刷新投影」与「Redis 丢失后重建」两条最关键的恢复路径全部不可用，且由 `tokio::spawn` 触发、错误只进日志。附带：`max_score` 查询用 `.unwrap_or(0)` 吞掉同类错误，POINTS 模式下 `max_score_milli = 0` 会把任意分判为满分并置 `solved`。
- **建议**：解码改为 `i32` 或 SQL 侧 `::bigint`；`max_score` 不用 `unwrap_or(0)`（缺失行应返回 `Ok(None)` 语义）；补一条「含已判提交」的集成测试——现有测试只覆盖空 cell，这是该 bug 长期存活的原因。

## P0-05 优雅关闭与 SSE 长连接互等，SIGTERM 后进程永久挂起 ✅

- **位置**：`apps/api/src/main.rs:74-82`（`with_graceful_shutdown`）、`:308`（`self.shutdown.send(true)`）
- **问题**：`axum::serve(...).with_graceful_shutdown(shutdown_signal()).await` 在收到 SIGTERM 后停止 accept，并等待所有在途连接结束；SSE 处理器只有在 `watch` 收到 `true` 时才返回 `None`（`features/realtime/handlers.rs:179-189`，代码注释也明说这是「graceful shutdown 期间结束流的唯一方式」）；而 `send(true)` 位于 `BackgroundRunners::shutdown_within`，只在 `serve().await` **返回之后**才被调用。
- **影响**：只要有一个浏览器连着 `/api/public/events/...`，`serve` 永不返回 → `send` 永不执行 → 流永不结束。编排器最终 SIGKILL，`database.close()`、runner join、任务排空全部被跳过。比赛现场必然有多个大屏连着 SSE。
- **建议**：把 `send(true)` 提前到等待 `serve` 之前（单独 spawn 一个 signal watcher 负责先 `send` 再触发 graceful），并给 `serve` 外层加硬超时兜底。

## P0-06 比赛结束后公开榜自动解冻，封榜机制在 end_at 失效 ✅

- **位置**：`apps/api/src/features/scoreboard/service.rs:234-247`
- **问题**：`frozen` 的判定是 `!admin && status ∈ {RUNNING, PAUSED} && freeze_at <= now && now < end_at`。`now >= end_at` 时 `frozen = false`，`phase` 落到 `FINAL`，后续 cells 分支只在 `if frozen` 时读 FROZEN 快照，否则直接读 LIVE cells。
- **影响**：与 `docs/architecture/scoreboard-resolver.md`「After contest end, the public board remains frozen and does not automatically reveal hidden submissions」直接冲突；而 `resolver::create` 要求竞赛为 `ENDED`，说明设计意图确实是「结束仍冻结、由 Resolver 揭示」。后果是正式 Resolver 仪式前公众已能看到完整终榜。另有附属竞态：`end_at` 到期到 runner 改状态之间有 60s 宽限，此窗口状态仍为 `RUNNING`，榜单会「先泄露、再回冻」。
- **建议**：公开榜冻结条件只取决于 `freeze_at`，与 `end_at`/状态无关；解冻只能由显式揭示动作（Resolver 完成）触发并留审计。

---

# P1 — 生产中会产生错误行为（42 条）

## 认证与会话（P1-01 ~ P1-09）

### P1-01 三处「撤销会话」只删 PostgreSQL 表，真实会话在 Redis 中继续有效 ✅

- **位置**：`apps/api/src/features/staff_accounts/service.rs:355-365`（调用点 `:217`、`:262`）、`apps/api/src/features/teams/service/password.rs:46`、`apps/api/src/features/teams/service/mod.rs:379`
- **问题**：三处都执行 `DELETE FROM auth_sessions WHERE user_id = $1`。但认证链路早已只读 Redis（`auth/service/sessions.rs:74` 注释、`auth/service/store.rs` 模块文档均声明不再使用该表）。表仍存在于迁移中，因此 DELETE 成功执行、影响 0 行、不报错。
- **影响**：**管理员重置队伍或员工密码后，旧会话依然可用**——`password_hash` 不在 `access_fingerprint` 里（`auth/service/crypto.rs:19-27` 只哈希 `user_type` 与 `permissions`）。改权限/禁用因 fingerprint 或 `enabled` 变化而「碰巧」生效，唯独密码重置是确定性的安全缺口。`staff_accounts/tests.rs:20-88` 只断言表行数变 0，掩盖了问题。
- **建议**：改调 `auth::revoke_user_sessions(redis, user_id, "")` 并在事务提交后执行；清理对 `auth_sessions` 的错觉性写入，或明确标注 legacy 并从迁移中删除表。

### P1-02 `SET ... KEEPTTL` 会把已撤销/已过期的会话「复活」为无 TTL 的永久会话 ✅

- **位置**：`apps/api/src/features/auth/service/store.rs:132-146`（`touch_session`），调用点 `auth/service/sessions.rs:175-183`
- **问题**：`SET key payload KEEPTTL` 在 key **不存在**时会创建新 key，此时「保留现有 TTL」意味着不设过期时间。
- **影响**：请求 A 读到 5 分钟前的会话记录 → 同 token 的登出/改密/管理员禁用删除了该 key → A 执行 `SET ... KEEPTTL` 重建出无 TTL 记录 → 此后 `authenticate` 仍能通过（用户 enabled、fingerprint 均未变）→ 已登出的会话永久有效直到 Redis 重启或人工 DEL。会话自然过期瞬间被 touch 也会产生同样结果。窗口很窄（两次 Redis 往返之间），但确实存在。
- **建议**：改为 `SET key payload XX KEEPTTL`，或用 Lua/MULTI 原子判断 `EXISTS` 后再写。

### P1-03 一次数据库抖动会把工作站会话永久注销 ⚠️

- **位置**：`apps/api/src/features/auth/context.rs:127-137`
- **问题**：`validate_session` 返回的任何 `Err` 都走 `state.auth().logout(...)`，而该函数对「数据库错误」（`AppError::internal`）与「绑定无效」（`not_authenticated`）一视同仁。
- **影响**：比赛中一次 DB 超时即删除工位 Redis 会话，队伍必须用配对码重新登录，而配对码可能已不在手上。
- **建议**：仅当 `error.code() == "NOT_AUTHENTICATED"` 时登出，内部错误原样返回并保留会话。

### P1-04 并发登录时合法的哈希升级路径会把正确密码判为无效 ⚠️

- **位置**：`apps/api/src/features/auth/service/sessions.rs:82-107`
- **问题**：`UPDATE users SET password_hash = COALESCE($1, ...) WHERE id=$2 AND enabled AND password_hash = $3`，`$3` 是登录开始时读到的旧哈希。并发请求中先完成者把 bcrypt 升级为 Argon2，后完成者 `rows_affected = 0`。
- **影响**：后完成者被当作「凭据无效」，还会计入限流失败计数，可能把用户推向 429。
- **建议**：区分「哈希已被并发升级」与真正失效；或改用 `WHERE id=$2 AND enabled` 并对哈希做单独的 CAS。

### P1-05 登录限流只按 IP 计数，赛场 NAT 下整段封锁且无按账号限流 ⚠️

- **位置**：`apps/api/src/features/auth/service/internal.rs:30-32`
- **问题**：限流键为 `(action, request_ip)`，阈值 10 次/5 分钟。默认部署只在显式配置 `trusted_proxy_cidrs` 时才信任 `X-Real-IP`（`lib.rs:62-78`），因此同一 NAT 出口的所有参赛者共享一个计数器。反向也没有按 username 的计数。
- **影响**：任意 10 次失败即让整栋机房 5 分钟内无法登录；同时分布式来源可对单账号无限撞库。
- **建议**：同时维护 `(action, ip)` 与 `(action, username)`，任一超限即拒绝。

### P1-06 自助注册保留用户名大小写，与员工/引导账号的小写规范冲突 ⚠️

- **位置**：`apps/api/src/features/auth/service/account.rs:22` vs `apps/api/src/features/staff_accounts/model.rs:74`
- **问题**：注册只 `trim()`，员工创建强制 `to_ascii_lowercase()`；唯一索引 `users_username_key` 大小写敏感（`migrations:2486-2490`），登录按精确匹配。
- **影响**：`Admin` 与 `admin` 可并存，用于榜单/成员列表的身份混淆；审计日志（`record_failed_login` 里 `to_lowercase()`）与真实账号对应关系变模糊。
- **建议**：注册同样小写化，或建 `lower(username)` 唯一索引。

### P1-07 bootstrap 的「users 表为空」判定可被公开注册抢先打破，且无恢复路径 ⚠️

- **位置**：`apps/api/src/bootstrap.rs:72-77`
- **问题**：判定条件是 `count(*) FROM users == 0`，而 `/api/auth/register` 是公开接口（仅比赛模式下禁用）。
- **影响**：部署后若先对外暴露再执行 `bootstrap-admin`，任意访客注册一个 INDIVIDUAL 账号即让 bootstrap 永久返回 `AlreadyInitialized`，系统再无受支持的超管创建入口。
- **建议**：判定改为「不存在任何 SUPER_ADMIN」；或在无超管时让 register 返回 503 并在启动日志给出恢复步骤。

### P1-08 事务已提交后的二次 I/O 失败会把成功操作报成 500 ⚠️

- **位置**：`apps/api/src/features/auth/service/account.rs:132-135`（改密）、`:179-186`（改资料）
- **问题**：`transaction.commit()` 之后才执行 Redis 撤销 / 回读用户；失败即 `?` 返回 500。
- **影响**：密码已改、审计已写，但客户端收到 500 并重试，撞 `PASSWORD_UNCHANGED`；改资料同理。
- **建议**：提交后的附属操作失败应记录并仍返回成功（或返回明确的 `SESSION_REVOCATION_PENDING` 部分成功语义）；`update_profile` 直接用已知值组装响应，避免二次读库。

### P1-09 登录先建新会话、后注销旧会话，注销失败会留下孤儿会话并返回 500 ⚠️

- **位置**：`apps/api/src/features/auth/handlers.rs:45-47`
- **问题**：`login()` 已在 Redis 创建新会话，随后若 `logout_token(previous)` 失败则返回 500，新 cookie 不下发、旧会话也未撤销。
- **建议**：旧 token 撤销改为 best-effort（记告警不阻断登录）。

## 实时链路（P1-10 ~ P1-12）

### P1-10 租约回收用 `parse_xread` 解析 `XCLAIM` 回复，结果恒为空，重试与死信机制实际失效 ⚠️

- **位置**：`apps/api/src/features/realtime/outbox.rs:226-272`，消费点 `features/realtime/dispatcher.rs:84-107`
- **问题**：`XREADGROUP` 回复是 `[[stream, [entry...]]]`，而 `XCLAIM`（不带 `JUSTID`）回复是 `[[id, [field, value...]], ...]`，没有外层 stream 包裹。`parse_xread` 对每个元素取 `nth(1)` 当字段数组，随后把每个 `BulkString` 当条目，全部走 `_ => continue` 丢弃。
- **影响**：`reclaim_expired` 永远返回空 → 发布失败的条目不重试；超 `max_attempts` 的条目不进死信，`health()` 死信计数恒 0；dispatcher 重启（新 consumer 名，`dispatcher.rs:45`）后旧 consumer 的 pending 条目永久滞留，事件永久丢失。
- **建议**：为 `XCLAIM` 写独立解析器；或用 `XCLAIM ... JUSTID` 取 id 再 `XPENDING`/`XRANGE` 取 payload。另 `dispatcher.rs:93` 的 `delivery_count > max_attempts` 修复后仍会多投一次，建议改 `>=`。

### P1-11 `XADD` 无 `MAXLEN`，全仓库无 `XTRIM`，outbox stream 与 PEL 单调增长 ⚠️

- **位置**：`apps/api/src/features/realtime/outbox.rs:133`
- **问题**：`XACK` 只移出 PEL，不删除 stream 条目；`grep XTRIM|MAXLEN` 在 `apps/api/src` 无命中。
- **影响**：比赛期间提交、判题结果、打印、气球、公告持续入队，`xcpc:realtime:outbox` 随部署时长单调增长，长赛季后可致内存压力甚至 OOM。叠加 P1-10，PEL 同样只增不减。
- **建议**：`XADD ... MAXLEN ~ <bound>`，或定期 `XTRIM MINID`（保留 5 分钟以上窗口）。

### P1-12 consumer group 创建失败时 dispatcher 直接 return，实时链路被静默永久关闭 ⚠️

- **位置**：`apps/api/src/features/realtime/dispatcher.rs:48-52`
- **问题**：`run` 返回 `()`，`tokio::spawn` 后 `join` 只得到 `Ok(())`，无重试、无告警升级。
- **影响**：启动时 Redis 一次抖动 → 所有 SSE 事件（已入队但无人 fanout）永久停止投递，直到进程重启。
- **建议**：改成带退避的重试循环（与 `RedisSubscriber::run` 一致的 `reconnect_delay` 模式），或返回 `Result` 交由守护逻辑重启。

## 提交与判题（P1-13 ~ P1-15）

### P1-13 结果重投递不校验 publisher confirm 却立刻 ACK 原消息，可丢判题结果 ⚠️

- **位置**：`apps/api/src/features/judge_dispatch/result_consumer.rs:272-286`（`defer_result_for_retry`），ACK 点 `:239-240`
- **问题**：只 await confirm 的 `Result`，未检查 `Confirmation::is_ack()`；`BasicPublishOptions` 也未设 `mandatory`。broker 因流控回 nack 时函数返回 `Ok(())`，调用方随即对原消息 ACK。
- **影响**：重试副本不存在、原消息已确认 → 该判题结果彻底消失，提交卡在 `JUDGING`，只能靠 `stuck_reaper` 30 分钟后重投。仓库内 `judge_dispatch/rabbit.rs:127-133` 有正确写法（`is_ack()` + `take_message().is_none()`），说明是遗漏。
- **建议**：检查 `is_ack()`；重投递加 `mandatory: true` 并校验 `take_message().is_none()`；confirm 不通过时 `nack(requeue: true)`。

### P1-14 Redis 实时事件失败被归类为永久错误，把正确判题结果写成 SYSTEM_ERROR ⚠️

- **位置**：`apps/api/src/features/judge_dispatch/result_processor.rs:284-303`，错误分类 `:32-34`、`:92`
- **问题**：`enqueue_optional`（纯 Redis）的错误被 `map_err` 成 `ApplyResultError::Conflict`，而 `Conflict` 的 `is_permanent()` 为真。
- **影响**：链条是——判题结果已在事务内更新 → Redis 超时 → 整个事务回滚 → `result_consumer.rs:216-225` 因 permanent 立即 `reject(requeue:false)` → 进 `judge.dead` → `dead_letter_consumer.rs:242-268` 发现 judgement 未完成，写入 `verdict='SYSTEM_ERROR'`。一次 Redis 抖动把 ACCEPTED 永久写成 SYSTEM_ERROR，runs/score 全丢。这与 `realtime/outbox.rs` 自述的「只是刷新提示，最坏只是少一次提示」直接矛盾。
- **建议**：实时事件失败只 `warn!` 吞掉，或至多返回可重试错误；绝不能映射为 `Conflict`。

### P1-15 commit 结果不确定时仍删除 S3 源码，可能删掉已成功提交的源码 ⚠️

- **位置**：`apps/api/src/features/submissions/service/mod.rs:154-166`、`:323-328`；`apps/api/src/features/submissions/practice.rs:112-124`
- **问题**：补偿删除的条件是 `persisted.is_err()`，其中包含 `transaction.commit()` 失败。
- **影响**：`COMMIT` 发出后连接中断/主备切换，服务端可能已提交而客户端拿到 IO 错误 → 触发删除（或入 `object_storage_cleanup` 无条件删除，`runner.rs:111` 不做引用检查）→ 数据库有 submission 指向的 key 被删 → 详情 500、worker 下载失败 → 该提交变 SYSTEM_ERROR。
- **建议**：commit 失败时不删对象，交给 `object_storage_cleanup/integrity.rs` 的孤儿扫描 + grace 期自动收敛（现成机制）。

## 计分与冻结（P1-16 ~ P1-18）

### P1-16 冻结快照是「发现 freeze 时刻的实时 cells 整体 COPY」，未按 freeze_at 截断 ✅

- **位置**：`apps/api/src/features/scoreboard/redis_state.rs:396-410`（`freeze_snapshot` 执行 `COPY cells → frozen`），触发点 `apps/api/src/features/contests/lifecycle_runner.rs:129-132`
- **问题**：`freeze_snapshot` 只做 `COPY`，完全不看 `submitted_at`；冻结里程碑由 1s 轮询发现，而 `apply_judgement` 对每条判定无条件写 LIVE cells。
- **影响**：若服务在冻结时刻停机/重启，或投影在冻结后才启用，恢复后第一次 tick 会把包含全部封榜后判题结果的 live cells 拷成 frozen，此后公开榜优先读这份快照（`scoreboard/service.rs:287-289`），封榜后的 AC 与 wrong_attempts 全部泄露，直到比赛结束。回退路径 `calculate_frozen_cells` 用的是 `submitted_at < freeze_at`（`service.rs:456-458`），两条路径结论不同。
- **建议**：冻结快照改为调用 `calculate_frozen_cells` 生成后写入 Redis；或让 APPLY 在冻结阶段忽略 `submitted_at >= freeze_at`。

### P1-17 投影缺失时的降级路径会返回并缓存全零榜单，且无 single-flight ⚠️

- **位置**：`apps/api/src/features/scoreboard/service.rs:322-340`、`:416-429`；`redis_state.rs:237-270`
- **问题**：投影模式下 DB 的 `contest_scoreboard_cells` 不再更新（`result_processor.rs:267-273` 在 `projection.is_some()` 时跳过 `rebuild_cell`，`lifecycle_runner` 只在开赛时重建 Redis、结束后才回填 DB）。而 `version == 0` 时直接 `load_live_cells()` 返回这张空表，并按 revision 0 写缓存。
- **影响**：Redis flush / 版本键被淘汰 / 开赛预热失败 → 全零榜单被缓存；若只是 cells 大 hash 被 maxmemory 淘汰而 `ver` 小键还在，`read_cells` 会返回 `version > 0` 的空 cells，走 `repair_dirty` 后装配全零榜并按当前 version 缓存，之后不再触发任何重建。叠加 P0-04，全零榜可长期存在。多个并发请求还会各自 spawn 一次重建。
- **建议**：投影模式下 DB cells 不能当可信回退（要么始终双写，要么显式拒绝降级）；「cells 为空但 roster 非空」应视为 miss 并走单飞重建；`tokio::spawn` 纳入 supervisor。

### P1-18 增量投影不可交换，乱序完成判题会算错 solved_at / penalty / LAST 分数 ⚠️

- **位置**：`apps/api/src/features/scoreboard/redis_state.rs:62-95`（Lua）、`:452-496`（`project_cell`）
- **问题**：Lua 与 `project_cell` 都按「事件到达顺序」更新。判题完成顺序与 `submitted_at` 顺序无关（`result_processor.rs:312-314` 先提交再 apply）。
- **影响**：两个 AC 乱序到达时，`if not cell.solved` 会保留**先到达**的 AC（可能是更晚提交的），`solved_at` 与 penalty 全错；早于 AC 提交的 WA 若在 AC 之后判完，`cell.wa += 1` 但 penalty 已算好、不会补加 20 分钟；POINTS + `LAST` 聚合下最终分数取决于判题完成顺序。DB 的 `rebuild_cell` 按 `(submitted_at, id) < accepted` 计数，与 Redis 结果不一致。
- **建议**：apply 事件携带 `submission_id/submitted_at`，按 key 维护「最早 AC」而非「先到 AC」；对 `t < solved_at` 的乱序事件重算 penalty；最终值必须能由一次 `recompute_cell` 恢复（依赖 P0-04 先修）。

## 题目与赛题管理（P1-19 ~ P1-22）

### P1-19 测试数据上传被拒时临时文件永久残留，可反复利用打满磁盘 ⚠️

- **位置**：`apps/api/src/features/problems/handlers.rs:730`（`create_new` 建文件）、`:750`、`:754`、`:759`（提前返回）、`:763`（守卫构造）
- **问题**：RAII 守卫 `StagedUploadFile` 在 `:763` 才构造，任何提前 `return Err` 都发生在守卫存在之前；future 被取消（客户端断开）同样在守卫构造前丢弃局部变量。函数头注释 `:715-718` 声称「every early-return path leaves no staging debris」，与实现不符。
- **影响**：一次 257 MiB 上传即遗留约 256 MiB；持有 `CONTEST_MANAGE` 的账号可反复触发。
- **建议**：`open` 成功后立即构造守卫（让守卫持有 `File`），或改用 `tempfile::NamedTempFile`；staging 目录设配额并在启动时清扫。

### P1-20 题目响应直接返回对象存储 key ⚠️

- **位置**：`apps/api/src/features/problems/model.rs:253`（`ProblemResponse.interactor_object_key`），出口 `problems/handlers.rs:52/74/124/154`
- **问题**：`interactorObjectKey` 是公开字段并已固化进 OpenAPI 运行时文档；`coding-style.md`「Errors」与 `security.md` 均禁止返回对象键，`problems/service/mod.rs:45-46` 的注释也声称「object key never crosses the handler boundary」。
- **影响**：管理员拿到桶内真实前缀 `problems/{id}/interactors/{uuid}`，可与 P1-22 组合跨题引用。
- **建议**：从响应模型删除该字段（worker 直接读库），只返回「是否已配置」布尔值。

### P1-21 竞赛管理员调用题目列表会拿到全库题目，作用域被放大 ⚠️

- **位置**：`apps/api/src/features/problems/service/mod.rs:78`（鉴权）、`:81-99`（查询）
- **问题**：只用 `contestId` 校验「是否有该竞赛管理权」，随后 `WHERE deleted_at IS NULL` 返回**全库**题目，count 同样是全库计数。
- **影响**：仅被分配到竞赛 A 的管理员可枚举竞赛 B 甚至未分配题目的标题、资源限制、语言、`testdataSha256`、`interactorObjectKey`。与 `docs/api/rust-implementation-notes.md:356-361`「Global catalog access is super-administrator-only」矛盾。现有测试只插入 1 道题，无法暴露。
- **建议**：非超管分支把可见集合作为 `EXISTS` 子查询同时用于 count 与 rows；若语义就是共享题库，需同步修正文档表述。

### P1-22 `create/update` 接受任意 `interactorObjectKey`，绕过上传时的 ELF 校验 ⚠️

- **位置**：`apps/api/src/features/problems/model.rs:466-473`（只校验长度与控制字符）、`:45-57`、`:61-73`
- **问题**：`upload_interactor`（`service/attachments.rs:70`）强制 `\x7fELF` 魔术字节，但 `create/update` 对同一字段只做长度检查，不校验前缀、不校验对象存在、不校验 SHA 匹配内容。
- **影响**：持有 `CONTEST_MANAGE` 且管理该题全部竞赛的账号可把 `interactorObjectKey` 指向 `problems/{其他题}/interactors/{uuid}`（值可从 P1-21 读到），judge-worker 按 key+sha 拉取并执行，等于把「未验证的任意桶内对象」变成可执行交互器。
- **建议**：服务端强制 key 形如 `problems/{problem_id}/interactors/{uuid}`，写入前 `HEAD` 校验存在与 sha256；更彻底的做法是移除请求中的 key/hash 字段。

## 竞赛与运行（P1-23 ~ P1-25）

### P1-23 气球生成用「判定时刻」而非「提交时刻」过滤 freeze，冻结前 AC 会丢气球 ✅

- **位置**：`apps/api/src/features/balloons/service.rs:386-388`
- **问题**：`INSERT ... JOIN contests ... AND contest.freeze_at IS NOT NULL AND now() < contest.freeze_at`，由 `result_processor.rs:274` 在判定事务中调用。
- **影响**：需求 16.1 的生成条件是 `Submission time < freeze time`。判题积压时（「结束后排空判题队列」是标准流程）大量冻结前 AC 会永久丢气球。
- **建议**：条件改为 `submission.submitted_at < contest.freeze_at`；状态限制至少允许 `ENDED`（判题回填阶段）。

### P1-24 CUPS 投递的 stdin 写入无超时，配 10s 租约导致挂死与重复打印 ⚠️

- **位置**：`apps/api/src/features/printing/delivery.rs:94-106`（写入在 `:102` 的 timeout 之前）、`:14`（租约 10s）、`:256-286`（claim）
- **问题**：PDF 可达 5 MB（`printing/service.rs:455`），远超管道缓冲 64 KB；`tokio::time::timeout` 只包住 `child.wait_with_output()`，不含 `stdin.write_all`。租约固定 10s，而 `PROJECT_BALLOON_CUPS_COMMAND_TIMEOUT_MILLISECONDS` 无上限。
- **影响**：`lp` 不读 stdin 时 `write_all` 永久阻塞整个投递循环；租约过期后另一实例重新 claim 并再次提交同一 PDF，产生重复物理打印。
- **建议**：写入与等待放进同一个 timeout；租约取 `max(10s, command_timeout + margin)`；提交成功后立即持久化 job id。

### P1-25 首杀把 STAR 队算进去，与设计文档冲突 ✅

- **位置**：`apps/api/src/features/scoreboard/helpers.rs:172-192`、`apps/api/src/features/resolver/plan.rs:138-160`、`apps/api/src/features/balloons/service.rs:389-391`
- **问题**：只 `filter(|row| row.participation_type != "PRACTICE")`，STAR 队可拿首杀。
- **影响**：文档明确「First Blood is tracked per problem for official non-star teams only」。`scoreboard/tests.rs:264-266` 还把 STAR 拿一血写成断言，偏差已被固化为「预期行为」。
- **建议**：过滤改为 `participation_type == "OFFICIAL"`（或按竞赛配置），三处同步并改测试。

## 基础设施（P1-26 ~ P1-31）

### P1-26 后台 runner panic 在进程生命周期内无人感知 ⚠️

- **位置**：`apps/api/src/main.rs:62-71`（spawn）、`:370-386`（`join_runners_within_deadline`）
- **问题**：`JoinHandle` 存进 `BackgroundRunners` 后，正常运行期没有任何任务 `select!` 它，只在关闭时 join。
- **影响**：lifecycle / resolver auto-play / export / batch-rejudge 等 runner 因 panic 退出后，进程仍返回 200 健康，表现为任务积压，直到关闭时才报 "task failed"。
- **建议**：为每个 runner 加监督任务（`select!` handle + shutdown，捕获 `JoinError` 记录并重启或置 down），或使用 `TaskTracker`。

### P1-27 readiness 不检测「必需的 Redis」 ⚠️

- **位置**：`apps/api/src/health.rs:150`、`:228-241`
- **问题**：`redis_connected` 来自 `RealtimeHub::redis_status()`，仅当 `realtime_redis_enabled = true` 时才返回 `Some(bool)`，而该开关默认 `false`（`config/from_env.rs:355-358`）。但 Redis 同时承载会话、限流、outbox（`main.rs:175-177` 明确 "Redis is required"）。
- **影响**：默认配置下 Redis 挂掉，所有会话请求 500，但 `/api/health` 仍返回 200。
- **建议**：readiness 独立对 Redis 做 `PING`，失败即 503。

### P1-28 可选依赖（对象存储、CUPS）故障会把所有 API Pod 摘出负载均衡 ⚠️

- **位置**：`apps/api/src/health.rs:169-182`、`:214-241`
- **问题**：`object_storage`、`cups` 任一 `Down` 都会把整体状态置 `Down` 并返回 503。
- **影响**：一次打印机或 S3 抖动 → k8s 判定整个 API 不 ready → 全站不可用，而比赛主流程（提交、榜单）本可正常工作。
- **建议**：readiness 只对请求必需依赖（DB、Redis）把关；其余作为报告字段或拆 `/readyz` 与 `/health`。

### P1-29 判题调度仍写 PostgreSQL `realtime_outbox` 表，但该表已无消费者 ⚠️

- **位置**：`apps/api/src/features/judge_dispatch/dispatcher.rs:171-188`、`:224-237`
- **问题**：realtime 已迁到 Redis stream（`features/realtime/outbox.rs:1-20`），生产代码仍 `INSERT INTO realtime_outbox` 写入 `JUDGING` 与终止 `SYSTEM_ERROR` 事件，无任何 dispatcher 读取（仅测试里还在查）。
- **影响**：前端永远收不到这两个状态；表无清理、持续膨胀。
- **建议**：改走 `realtime::outbox::enqueue_optional`；若确认废弃，加迁移删表。

### P1-30 `RABBITMQ_ENABLED=false` 时提交可创建但永不判题，启动与 readiness 均不报错 ⚠️

- **位置**：`apps/api/src/config/from_env.rs:608-619`、`apps/api/src/main.rs:169-173`、`:436-536`
- **问题**：关闭时 `init_judge_publisher` 返回 `None`，判题 dispatcher / reaper / result consumer 全部不 spawn，但提交接口照常写 `submission_outbox(PENDING)`。
- **影响**：比赛模式下可「健康」运行但零判题。
- **建议**：`deployment_mode = competition` 时强制 `rabbitmq_enabled = true`（配置期报错）；readiness 在 `judge_pending` 持续增长且无在线 worker 时置 Down。

### P1-31 S3 流式读取只对建连做超时，body 分块无截止时间 ⚠️

- **位置**：`apps/api/src/object_storage.rs:472-491`
- **问题**：`within_timeout` 只包住 `store.get()`，`into_stream()` 后的每个 chunk 无超时，且流被直接喂给 axum 响应。
- **影响**：对象存储半开连接会让请求任务永久挂起占住连接，并加剧 P0-05 的关闭挂死。
- **建议**：对 `stream.next()` 逐块 `timeout`，或实现带总预算的包装流。

## 展示与大屏（P1-32 ~ P1-34）

### P1-32 公开 metrics 接口在封榜后仍统计全部提交，泄露未公布成绩 ⚠️

- **位置**：`apps/api/src/features/presentation/live.rs:282`（路由 `:271`，放行 `:159-183`），查询 `:301-303`
- **问题**：`/api/public/presentations/{id}/metrics?mode=SCREEN` 的 SCREEN 分支不校验 token，只要求 `visibility='PUBLIC'` + 配置启用；`load_metrics` 对 `submissions` 直接 `count(*)`、`FILTER(verdict='ACCEPTED')`、按语言/小时聚合，**没有** `submitted_at <= freeze_at` 过滤。
- **影响**：封榜期任何匿名观众每秒轮询即可从 `accepted`/`trend`/`languages` 观察封榜后新通过的提交，绕过封榜。
- **建议**：metrics 按 `freeze_at` 截断，或仅 `mode=LIVE`（带 token）返回实时值。

### P1-33 公开展示响应包含 logo 对象存储 key ⚠️

- **位置**：`apps/api/src/features/presentation/model.rs:45`（取值 `service.rs:65`、`live.rs:207`；出口 `live.rs:57`）
- **问题**：`custom_logo_object_key` 无 `#[serde(skip)]`，`published` 把整个 `ConfigResponse` 嵌进公开响应，与 `show_logo` 无关。
- **影响**：任何能访问公开展示页的人（SCREEN 模式无需 token）拿到 RustFS 内部对象键，违反「不返回 object key」。`list_templates` 也返回 `logo_object_key`。
- **建议**：公开投影移除该字段，改为后端代理端点或短期签名 URL。

### P1-34 匿名屏幕注册不校验赛事可见性，私有赛事可被枚举并写入实例 ⚠️

- **位置**：`apps/api/src/features/presentation/service.rs:198`（`require_contest` `:400-413`）、`:211-219`
- **问题**：`POST /api/public/screens/register` 无认证，`require_contest` 只查 `deleted_at IS NULL`；不存在返回 404，存在但未发布返回 409 或直接 201 建行。
- **影响**：与 `live.rs:171-183` 强制 `visibility='PUBLIC'` 的策略不一致，破坏「私有赛事不可枚举」；还允许匿名向私有赛事插入 `screen_instances`。
- **建议**：复用 `require_access` 的 PUBLIC + enabled 判定，两种情况统一 404。

## 前端（P1-35 ~ P1-37）

### P1-35 `crypto.randomUUID` 在非安全上下文未定义，离线 LAN 部署下两个页面直接崩 ✅

- **位置**：`frontend/web/src/views/AdminBulkRejudgeView.vue:173`（调用 `:334` 的 `immediate` watch）、`frontend/web/src/views/AdminTeamImportView.vue:323`（`newId()`，被 `:476`、`:588` 复用）
- **问题**：`crypto.randomUUID` 是 `[SecureContext]` API，现场用 `http://192.168.x.x` 访问时为 `undefined`。批量重判页在 setup 内同步调用 → `TypeError` → 组件 setup 失败、区域白屏；队伍导入在 `parseSource()` 里被 `catch` 当成「JSON 解析失败」，无论怎么改都导入不了。
- **影响**：项目核心卖点是离线 LAN，而开发环境是 `127.0.0.1`（安全上下文），测试与本地都不会复现。
- **建议**：封装 `randomId()`（`crypto.getRandomValues` 生成 hex，`randomUUID` 可用时优先），并补 `app.config.errorHandler`。

### P1-36 `navigator.clipboard` 在 HTTP 下不可用，三个「复制」按钮静默失败 ✅

- **位置**：`frontend/web/src/views/LiveManageView.vue:312`、`frontend/web/src/views/LiveProgramControlView.vue:280`、`frontend/web/src/views/AdminTeamImportView.vue:568`
- **问题**：同为 `[SecureContext]` API，HTTP 下 `navigator.clipboard` 为 `undefined`，未捕获 `TypeError`，无任何提示。
- **影响**：直播控制台生成 OBS 链接后无法复制；导入页「一次性复制账号」失败后密码仍留在页面（安全设计本意是复制后清除）。
- **建议**：封装 `copyText()`，失败时回退隐藏 textarea + `document.execCommand('copy')`，或选中输入框提示手动复制。

### P1-37 公开颁奖展示接口无 token，颁奖前即可拿到完整获奖名单 ⚠️

- **位置**：`frontend/web/src/api/awards.ts:208-210` → `/api/public/contests/{id}/awards/presentation`；后端 `apps/api/src/features/awards/handlers.rs:189`（无任何上下文参数）
- **问题**：与直播不同，该端点不要求广播 Token，只校验 PUBLIC + 已 FROZEN（UI 在 `AwardStage.vue` 的 `WAITING` 状态不渲染名单）。
- **影响**：UI 不泄露，但网络响应里已有 `categories[].recipients[]` 全量名单（含 `teamId`、`seatNo`、`rank`）。枚举 contestId 即可在典礼前拿到结果。
- **建议**：后端按 `status`（PRESENTING/COMPLETED）过滤 recipients，或复用直播的 Token 门禁。

## 判题工作进程沙箱纵深（P1-38 ~ P1-42）

### P1-38 bwrap 降级模式没有任何 rlimit，ADR-003 声称的「rlimit + post-hoc」不存在 ⚠️

- **位置**：`apps/judge-worker/src/sandbox/bwrap.rs:580-611`（`base_args` 无 `--rlimit-*`）、`:459-467`（`cgroup = None` 时直接放行）
- **问题**：`JUDGE_CGROUP_BASE` 缺失或 `JUDGE_CGROUP_REQUIRED=false` 时 `cgroup = None`，代码只在日志里说 "degrades to rlimit-only"，但全仓库 `grep rlimit` **只命中注释**（`bwrap.rs:81/90/122/138/237`、`lib.rs:49`），没有任何 `setrlimit` 或 `--rlimit`。
- **影响**：子进程仅受墙钟 kill 限制。fork 炸弹可无限创建进程耗尽宿主 pids；内存可无限增长触发宿主 OOM（可能先杀掉 worker 自身）；CPU 可占满所有核。ADR-003「Resource Limits」写的是 `memory.max/pids.max/cpu.max` 并在降级时用 rlimit，实现与文档不符。
- **建议**：至少加 `--rlimit-nproc 64 --rlimit-as <bytes> --rlimit-cpu <sec>`；或在 `cgroup_required=false` 时拒绝启动，除非显式确认接受无硬限制。

### P1-39 stderr 只保留前 64 KiB，GNU time 报告被截掉 → 选手可把 AC 变成 SystemError 并触发 8 次重试 ⚠️

- **位置**：`apps/judge-worker/src/sandbox/runner.rs:535-543`、`apps/judge-worker/src/sandbox/bwrap.rs:647-659`、解析器 `apps/judge-worker/src/sandbox/metrics.rs:19-49`
- **问题**：日志只保留**最先**写入的 `MAX_EXEC_LOG_BYTES = 64 KiB`（`sandbox/mod.rs:111`），而 GNU time 的 `__PROJECT_BALLOON_GNU_TIME__` 报告是 stderr 的**最后一次**写。
- **影响**：选手先向 stderr 打超过 64 KiB，报告即被丢弃 → `extract_gnu_time_metrics` 用 `rfind(PREFIX)` 找不到、返回 `None` → 正常结束（exit 0、非 timeout、非 137）的运行命中 `gnu_time.is_none()` 分支（`runner.rs:307-312`）返回 `SandboxError::Api` → `worker.rs:179` 视为可重试，重试 8 次（`rabbit.rs:61-65`）后变成 **SystemError**。选手用几十行 `eprintln!` 就能把判题结果从 AC/WA 变系统错误，并让每个提交占用 8 倍判题时间（放大 DoS）。
- **建议**：保留**尾部**而非头部（环形缓冲），或把指标走独立 fd/只读管道；「无指标」应归 `RuntimeError` 而非可重试的系统错误。

### P1-40 `/work` 无磁盘配额，编译阶段连 `ulimit -f` 都没有 ⚠️

- **位置**：`apps/judge-worker/src/sandbox/runner.rs:410-416`、`bwrap.rs:484-486`；编译执行 `runner.rs:225-232`、`bwrap.rs:303-312`
- **问题**：`/work` 是宿主目录的可写 bind，只有**运行**阶段通过 `standard_shell` 的 `ulimit -f`（`runner.rs:832`）限制单文件；编译命令是 `gcc/g++/javac/go/rustc` 直接 exec（`language.rs:39-90`），不经过 shell，完全无限制；运行阶段选手也能循环创建大量小于限额的文件。
- **影响**：单一提交即可填满宿主磁盘，影响所有 worker 与 testdata 缓存。
- **建议**：`/work` 改用带大小上限的 tmpfs 或项目配额；编译同样套文件大小限制；对目录总量做监控。

### P1-41 output-only 判题整文件读入并做全量行规范化，可打爆 worker 内存 ⚠️

- **位置**：`apps/judge-worker/src/sandbox/runner.rs:735-755`；`apps/judge-worker/src/sandbox/compare.rs:5-33`
- **问题**：`tokio::fs::read(expected)` 与 `read(actual)` 各可到 512 MiB（`MAX_TESTDATA_EXTRACTED_BYTES`，`sandbox/mod.rs:114`），随后 `normalized_lines` 再造一份 `Vec<Vec<u8>>`（每行一次分配）。同一任务峰值接近 2 GiB，而 compose 把 worker 限在 2 GiB（`deploy/compose/rust-app.docker-compose.yml`）。这些计算还都在 async 执行线程上。
- **建议**：流式逐行比较（有界缓冲）+ `spawn_blocking`，并按 `output_limit_kb` 限制单文件与总字节。

### P1-42 panic 型任务经 requeue 回队不增加 `x-death`，形成无限崩溃循环 ⚠️

- **位置**：`apps/judge-worker/src/rabbit.rs:341-349`（panic 结束 session）、`:647-672`（retry 只数 `x-death`）
- **问题**：`process_delivery` panic 被 JoinSet 捕获后 `end_session` → 连接结束 → broker **requeue 未 ack 的消息**，不产生 `x-death` 记录；而 `retry_count` 依赖 `x-death`，因此恒为 0，`MAX_TASK_RETRIES` 永不触发。
- **影响**：一条能触发 panic 的消息（例如 P3-069 的锁中毒路径）会在重连循环中无限重投，永不进死信，任务槽被永久占用。
- **建议**：panic 结果主动 `nack(requeue: false)` 进 DLX，或改用 header 记录重试计数。

---

# P2 — 特定条件下的隐患（104 条）

## 认证 / 实时 / 员工（11）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-01 | `auth/context.rs` + `realtime/handlers.rs` | SSE 连接数无任何上限，公开端点匿名可达，可开数千长连接并把合法观众挤成 `Lagged` | 按 IP + 全局 Semaphore 准入，关闭时归还 |
| P2-02 | `realtime/outbox.rs:322-346` | replay 先 `LIMIT 0 100` 再按 scope/team 过滤，队伍的 TEAM 事件常被 PUBLIC 挤掉 | 按 scope/team 建独立索引，或 `ZSCAN` 分页直到凑满 |
| P2-03 | `realtime/outbox.rs:79-83,327` | replay score 只到毫秒且用 `(` 开区间，同毫秒后续事件被排除 | 改用微秒或 stream id 作游标 |
| P2-04 | `realtime/handlers.rs:54-63,93-101` | 先查 replay 再订阅广播，两步之间发布的事件丢失 | 先 subscribe 再查 replay，按 event id 去重 |
| P2-05 | `realtime/outbox.rs:210-218,258-263` | 无法反序列化的条目既不 ACK 也不计数，永久滞留 PEL | 视为不可重试，ACK + 记死信 |
| P2-06 | `auth/service/sessions.rs:82-107` | 见 P1-04（同源，并发哈希升级） | — |
| P2-07 | `contest_management_scopes/service.rs:56-60` | `replace` 不取 `STAFF_ACCESS_LOCK_ID`，与员工更新交错会留下「幽灵作用域」 | 复用同一 advisory lock |
| P2-08 | `contests/service.rs:590-601` 等 | 实时入队在持有 `FOR UPDATE` 的事务内 await Redis，且失败 500 回滚 | 见「系统性问题 S1」 |
| P2-09 | `staff_accounts/service.rs:51-71` | `count(*)` 与分页查询不在同一快照，`totalElements` 与实际内容不一致 | 放同一 `REPEATABLE READ` 事务或 `count(*) OVER ()` |
| P2-10 | `staff_accounts/model.rs:47-61,105-112` | 缺 `deny_unknown_fields`，与 `RegisterRequest` 不一致，字段拼错被静默忽略 | 统一加上 |
| P2-11 | `auth/service/store.rs:122-131` | 损坏的 session JSON 变成 500 且不清理，直到 TTL 到期 | 解析失败即 `DEL` 并按未认证处理 |

## 提交 / 判题分发（9）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-12 | `judge_dispatch/stuck_reaper.rs:70-82` | 不区分 active judgement，会复活已 superseded 任务，可能把 PENDING 错标成 JUDGING 使其永久卡住 | `JOIN judgements j ON ... j.active_marker IS TRUE` |
| P2-13 | `judge_dispatch/dispatcher.rs:216-238` + `submissions/query/list.rs:67-72` | 终态失败后 outbox 行永久 `FAILED`，`judge_queue_status.drained` 永远 false；若已是 JUDGING 则 SYSTEM_ERROR 兜底不生效 | 终态失败设独立状态；兜底接受 `IN ('PENDING','JUDGING')` |
| P2-14 | `judge_dispatch/dead_letter_consumer.rs:184-193` | 死信处理无重试预算，确定性 DB 错误会无限重投 | 加 `x-retry-count` 预算，超限 ACK + `error!` |
| P2-15 | `judge_dispatch/result_processor.rs:92` + `result_consumer.rs:216-225` | schema 版本不匹配被判永久错误，滚动升级会把在途提交写成 SYSTEM_ERROR | 版本不匹配走延迟重试 + 告警，或支持 N-1 兼容 |
| P2-16 | `submissions/exports/service.rs:73,104` + `exports/helpers.rs:42-73` | 同步导出先 `fetch_all` 全量入内存，之后才判上限，内存峰值已发生 | SQL 加 `LIMIT n+1` 探测，主体改游标流式 |
| P2-17 | `submissions/export_tasks.rs:703-728` | 导出任务无最大尝试次数，永久失败无限重试、`attempts` 无限增长 | 加 `max_attempts`，超限置终态 |
| P2-18 | `submissions/query/similarity.rs:112-146` | 相似对查询是 O(n²) 自连接，`LIMIT 1000` 无法下推，万份提交可打满 CPU | 用 simhash 分桶预过滤，或分批 + statement timeout |
| P2-19 | `submissions/service/mod.rs:94-168` | 提交无幂等键，客户端超时重试会产生第二条提交和双份罚时 | 可选 `Idempotency-Key` + 唯一索引（团队在 `batch_rejudge`/`export_tasks` 已有先例） |
| P2-20 | `submissions/service/mod.rs:285-304` + `service/rejudge.rs:321-343` | 持行锁的事务内 await Redis，延长锁占用；失败还回滚业务 | 见 S1 |

## 计分 / Resolver / 奖项（15）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-21 | `awards/service/recipients.rs:504-531` | RANK_RANGE 用总榜 `rank` 过滤 OFFICIAL-only 类别，STAR 占前位时会漏发官方前几名 | 用 `official_rank` 判区间 |
| P2-22 | `awards/service/recipients.rs:498-513,533-579` | 一血奖项不写 `problem_id`/`award_key`，同队两题一血时 `ON CONFLICT` 覆盖，最终只留一条且无题目信息；PRACTICE 队可能进入奖项 | 以 `(team_id, problem_id)` 为粒度，`award_key='FB:{problem_id}'` |
| P2-23 | `awards/service/recipients.rs:141-192` vs `:194-243` | `manual_add` 与 `manual_remove` 锁顺序相反，可死锁（40P01 → 500） | 统一先 `lock_set` 再操作 recipient |
| P2-24 | `scoreboard/redis_state.rs:224-232` | `mark_dirty` 用 `let _ =` 丢弃错误、无日志、无重试；Redis 抖动后该 cell 永不重放 | 升级为可观测故障 + 独立「投影不可信」键触发重建 |
| P2-25 | `scoreboard/redis_state.rs:292-299` | `max_score` 查询 `unwrap_or(0)`，POINTS 模式 max=0 时任意分判满分（与 P0-04 同源） | 失败返回错误，缺失行返回 `Ok(None)` |
| P2-26 | `scoreboard/redis_state.rs:76-95,475-493` vs `projection.rs:182-191` | POINTS 模式 `wrong_attempts` 在增量投影/回放/DB 三条路径语义不一致，运行中 OI 榜显示 0 次尝试 | 统一语义并三处对齐 |
| P2-27 | `scoreboard/service.rs:253-256` | 投影模式下缓存 key 忽略 `scoreboard_revision`，队伍改名/改组/STAR 变更不及时失效；计分模式变更根本不触发 revision | key 纳入 revision；模式变更显式重建 |
| P2-28 | `scoreboard/service.rs:363-385` | `repair_dirty` 在请求路径串行重算所有脏 cell，无上限无 single-flight | 移到后台补偿任务，读路径最多触发一次限流重建 |
| P2-29 | `scoreboard/redis_state.rs:375-393` | 重建用 `DEL + HSET + INCR` pipeline（非事务），失败会留下空投影 | 写临时 key 后 `RENAME` 原子替换 |
| P2-30 | `scoreboard/redis_state.rs:97` + `service.rs:253-258` | 每次判题都递增 version，缓存命中率趋近 0，每请求全量 `HGETALL` + 排序 | 引入短 TTL 榜缓存或分钟级 epoch |
| P2-31 | `resolver/plan.rs:116-175` | `recompute_board` 只做 ICPC 聚合，`compare_rows` 不接收 `scoring_mode`，OI/IOI 下仪式排名与终榜不一致 | 抽共享排序/聚合函数，补 OI resolver 测试 |
| P2-32 | `resolver/plan.rs:76-114` + `service.rs:85-122` | 计划一次性物化全部状态（步数 × 整榜 JSON），600 队 × 6000 cell 可达数 GB | 流式插入，或只存增量 + 定期 checkpoint |
| P2-33 | `contests/lifecycle_runner.rs:145-171` | 赛后 backfill 在单个大事务内逐 cell 重算，每 cell 触发器都 `UPDATE` 同一行 `contests`，锁持到提交 | 分批提交，revision 批末统一 bump |
| P2-34 | `health.rs` / `main.rs:117-124` | 全站无 `statement_timeout`/`lock_timeout`（已在 backlog 记录为主动延期） | 按角色设置并在 `docs/ops` 说明 |
| P2-35 | `competition/service.rs:224-231,47-80` | `list_workstations` 无 contest 过滤，任一比赛管理员可看所有比赛工作站 IP；赛程重叠校验把历史比赛也算入 | 按当前比赛收窄或要求超管；校验只针对非终态 |

## 题目 / 赛题（9）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-36 | `problems/handlers.rs:446-489` + `service/testdata.rs:29-30` | 先落盘 256 MiB、先解压约 1 GiB，之后才鉴权题目存在性与作用域 | 读 body 前做轻量 preflight |
| P2-37 | `problems/service/testdata.rs:31-48,302-319` | 测试数据版本号在事务外分配，并发上传撞唯一索引时返回 500 而非 409 | 在持有行锁的事务内分配版本，或映射为 409 |
| P2-38 | `problems/service/mod.rs:181-222` | 部分更新语言/判题模式先触发 DB CHECK，返回 500 而非 400 | 更新前组装变更后状态做交叉校验 |
| P2-39 | `problems/service/attachments.rs:94` | 对 OUTPUT_ONLY 题上传 interactor 触发 CHECK → 500，且对象已写入（随后补偿删除） | 上传前读 `judge_mode/languages` 返回 409 |
| P2-40 | `problems/service/mod.rs:243-267` + `contest_problems/service/mod.rs:96-122` | 软删除与竞赛分配存在 TOCTOU，可产生「已删除但仍被竞赛引用」的题目 | 删除前 `FOR UPDATE` 锁题目行；分配时 `FOR SHARE` 复检 |
| P2-41 | `problems/testdata_archive.rs:23-32` + `handlers.rs:725` | 上传无并发/配额上限，`spawn_blocking` 无超时，可放大的 CPU/磁盘 DoS | 加信号量与在途字节配额，解压包 `timeout` |
| P2-42 | `contest_problems/service/helpers.rs:69-79` + `problems/service/mod.rs:486-495` | 任意运营权限（大屏/直播/气球/奖项）可读**任意**竞赛（含 DRAFT）的题目与附件，不需分配 | DRAFT/FROZEN_CONFIG 要求显式 assignment |
| P2-43 | `problems/service/mod.rs:399-417` | `require_problem_manage_pool` 不校验 `problems.deleted_at`，软删除后仍能读题面/附件，与 `get` 的 404 矛盾 | JOIN problems 校验 `deleted_at IS NULL` |
| P2-44 | `problems/service/testdata.rs:31-43` | preflight 后题目被并发软删除，`fetch_one` 抛 `RowNotFound` → 500 | 改 `fetch_optional` 映射 404 |

## 竞赛 / 队伍 / 打印 / 气球 / 澄清（14）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-45 | `printing/service.rs:218-223,240-258` | `RETRY` 不检查在途投递租约，可把提交中的任务租约清零导致重复打印 | `RETRY` 同样拒绝 `delivery_in_progress` |
| P2-46 | `printing/delivery.rs:354-361` | `mark_printing` 失败只 `warn!`，请求仍 QUEUED，下轮重复提交 | 失败时 `cancel` 补偿或先落 job id |
| P2-47 | `printing/delivery.rs:298-331` | 取消时 CUPS `cancel` 持续失败会无限 defer，永不终态；无 job id 时把「取消」改成 `FAILED` | 设取消尝试上限；无 job id 直接 `CANCELLED` |
| P2-48 | `announcements/runner.rs:51-63,64-78` | 归档竞赛下仍有 `SCHEDULED` 公告时，UPDATE 被归档只读触发器拒绝 → 整批回滚 → 每秒重试，所有到期公告永久发不出 | 选择条件加 `contest.status <> 'ARCHIVED'`，按行 `SAVEPOINT` |
| P2-49 | `contests/service.rs:343-348,385-406` + `teams/service/mod.rs:268-289` | ARCHIVED 并非真只读：`contests` 行可改名/改可见性，`teams`/`team_members` 完全不在触发器清单，改名还会 bump 归档榜单 revision | 拒绝 ARCHIVED 下的这些写入，或把表纳入归档守卫 |
| P2-50 | `lifecycle_runner.rs:14,182-186` + `crates/domain/src/lib.rs:30-39,84` | 自动结束在 `end_at+60s` 触发，`ENDED` 在 domain 中无出边，60 秒后结束时间写错不可恢复 | 增加受控 `Ended → Running`（限超管、Resolver 前、强制审计） |
| P2-51 | `printing/model.rs:112-122` | 页数上限按「50 行/页、100 列/行、CJK 按 1 列」估算，实际渲染可超 5 页 | 渲染后用 `pdfinfo` 统计真实页数 |
| P2-52 | `clarifications/service.rs:145-156,424-446` | `reply_visibility=PUBLIC` 的回复没有面向其他队伍的读取接口，实时事件也只发给提问队 | 新增公开澄清列表（只返回 ANSWERED + PUBLIC） |
| P2-53 | `balloons/service.rs:293-350` + `training/*` | 气球批量派发与训练模块全部写操作无审计日志 | 与业务同事务补 `audit_logs` |
| P2-54 | `contests/lifecycle_runner.rs:134-138` | 赛后 DB 回填只尝试一次，失败只 `warn!`，之后 tick 不再选中该竞赛 | 登记为可重试任务，每轮扫描未完成回填 |
| P2-55 | `contests/service.rs:424-521` vs `:584-605` | 人工 start/pause/resume/end/archive 不发实时事件，只有 `extend` 和自动迁移发 | `transition` 内补发 PUBLIC/STAFF 事件 |
| P2-56 | `contests/service.rs:94-124,141-156` | 超管传 `manageableOnly=true` 得到空列表（`contest_manager_id` 为 None 使条件恒假） | 超管视为 `false`，或改条件表达式 |
| P2-57 | `scoreboard/service.rs:265-276,283-310` | 冻结快照不含 roster，冻结后改名/增删队会改变公开冻结榜 | 冻结时一并保存 roster |
| P2-58 | `teams/service/mod.rs:263-267` | `request.school.or(current.1)` 等写法无法把 school/seatNo/groupName 清空为 NULL | 区分「未提供」与「显式 null」 |

## 展示与编排（10）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-59 | `presentation/program.rs:204,262,338` | `validate_scene` 的规范化结果被丢弃，小写场景名通过校验后触发 DB CHECK → 500 | 绑定规范化后的值 |
| P2-60 | `presentation/service.rs:180` + `program.rs:326-335` | 实时事件在 `commit` 前入队，Redis 失败让业务 500 回滚；事件先于提交发出还会让客户端读回旧状态 | 见 S1 |
| P2-61 | `presentation/handlers.rs:206` | 展示模板是全局可变资源，无 `contest_id`、无版本、无审计，赛事 A 的操作员改模板立刻影响赛事 B | 加归属与乐观锁 + 审计 |
| P2-62 | `presentation/program.rs:158,357-358` | 公开节目单在非 RESOLVER 场景也下发 `resolverRunId`，观众可提前调 `/state` 看揭晓 | 仅 `current_scene == "RESOLVER"` 时返回 |
| P2-63 | `scoreboard/cache.rs:81` | FROZEN 相 TTL 放大 20 倍，而 PG 回退路径的冻结榜并非不可变 | 仅命中不可变快照时才放大 TTL |
| P2-64 | `presentation/service.rs:220` | 屏幕注册限流按 NAT 共享 IP 计数（阈值 20），会场第 21 块屏必 429；`last_ip` 无索引，匿名注册触发全表扫描 | 按 contest+凭据限流，补 `(last_ip, created_at)` 索引 |
| P2-65 | `presentation/live.rs:188` | 每个公开轮询都对 `broadcast_tokens` 单行 UPDATE，热行写放大 + 行锁争用，一次刷新打三个端点 | 节流更新或校验与记录分离 |
| P2-66 | `presentation/live.rs:282,301-303` | metrics 每次轮询 5 条无缓存聚合，匿名可达，高峰可打满 PG | 加短 TTL 缓存，或要求 token |
| P2-67 | `scoreboard/cache.rs:36,86-99` | Redis 故障时无 single-flight/负缓存，所有请求同时回源重建整榜，TTL 还无抖动 | 进程内兜底缓存或按 key 去重，TTL 加 ±10% |
| P2-68 | `presentation/program.rs:241` | 首次 INSERT 不校验 `expectedVersion`，行不存在时传 5 也能插入，乐观锁被绕过 | 行不存在时要求 `expected_version == 0` |

## 判题工作进程（12）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-69 | `sandbox/runner.rs:245-268,450-483,496-520` | `update_container`/`create_container`/`start_exec` 等 Docker 调用无超时（只有 kill/inspect/remove 有） | 全部包 `docker_api_timeout` |
| P2-70 | `sandbox/runner.rs:441,417-435` | PID 1 是 `sleep infinity`，不回收僵尸；`pids_limit=64` 会被僵尸耗尽 | `HostConfig.init = true`（tini） |
| P2-71 | `sandbox/runner.rs:873-874` | `exit 137 && !timed_out` 无条件判 MLE，选手 `kill -9 $$` 也会得 MLE | 仅当 `oom_killed` 或 OOM 计数增加时判 MLE |
| P2-72 | `sandbox/runner.rs:628-645` | `fetch_interactor_diagnostics` 内部 `run_exec` 超时会 `kill_container`，选手用 FIFO 即可让后续测试点全失败 | 诊断读取不触发 kill，或捕获后忽略 |
| P2-73 | `sandbox/cgroup.rs:229` | `memory.swap.max` 写入失败被 `let _ =` 忽略，swap 未禁用，`memory.max` 可被绕过 | 失败时告警，严格模式下拒绝启动 |
| P2-74 | `crates/contracts/src/lib.rs:131-145` | 任务字段只有 `>0` 校验，`language_multiplier` 可 1e9，`task_deadline` 可膨胀到数十天 | worker 侧再钳制（时间 ≤10min、内存 ≤4GiB、multiplier ≤5） |
| P2-75 | `rabbit.rs:138-141,163-166` | `InFlightTasks` 以 `judgement_id` 为键，重复投递时 insert 覆盖、guard drop 直接 remove，可提前注销导致 job 目录被误删 | 键改 delivery tag，用引用计数 |
| P2-76 | `artifacts.rs:368-402` | `verify_file_hash` 在 async 任务里对最多 300 MiB 做 SHA-256，占用工作线程数百 ms | `spawn_blocking` |
| P2-77 | `sandbox/cgroup.rs:181-205` | 孤儿 sweep 不看 `keep`，会删掉「已 create 未 attach」的空 cgroup，导致 attach 失败 | 传入 `keep` 或创建时写占位进程 |
| P2-78 | `sandbox/runner.rs:66-82` | preflight 只 `ping` + `inspect_image`，`XCPC_SANDBOX_USER` 与 worker uid 不一致时全量 EACCES 但预检发现不了 | 用实际 user 做读写探针 |
| P2-79 | `artifacts.rs:263-273` + `archive.rs:144-202` | testdata 全量载入内存（300 MiB）后再存一份（≤512 MiB），prefetch 高时内存成倍 | 流式写盘 / 逐条解压 |
| P2-80 | `main.rs:93-96` | 启动 preflight 不验证 RabbitMQ，与 ADR-001「Preflight must fail if ... RabbitMQ unavailable」不符 | 预检加 broker 连通性 |

## 基础设施 / 配置 / 迁移（11）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-81 | `config/mod.rs:70-131` | `AppConfig` 派生 `Debug`，含 DB 密码、Redis/AMQP 凭据、S3 key、CSRF secret、metrics token | 手写 `Debug` 脱敏，或 `secrecy::Secret` |
| P2-82 | `config/from_env.rs:262-296` | `secure_cookies` 默认 `false` 且比赛模式不强制；`ALLOW_DEV_CSRF_SECRET` 在非 Secure 下仍可用开发密钥 | 比赛模式强制 `secure_cookies=true` 并禁止开发密钥 |
| P2-83 | `config/from_env.rs:146-161` + `metrics.rs:44-63` | `/metrics` 默认无鉴权；`METRICS_TOKEN=""` 被静默当作未设置，运维以为配了其实开放 | 空串报错，比赛模式强制要求 |
| P2-84 | `config/from_env.rs:700-716` + `lib.rs:69-76` | `trusted_proxy_cidrs` 接受 `0.0.0.0/0` 且无告警，任意客户端可伪造 `X-Real-Ip` 绕过限流、污染审计 IP | 拒绝或显著告警 `0/0` |
| P2-85 | `object_storage.rs:293-331` | 下载把 HEAD + 完整 body 塞进 5s 的 `request_timeout`，20–32 MiB 附件在慢链路必然超时 | 读路径用独立下载超时 |
| P2-86 | `object_storage.rs:423-456` | multipart 上传超时后未 `abort()`，S3 端遗留 IncompleteMultipartUpload 计费 | 捕获超时后显式 `abort()` |
| P2-87 | `object_storage_cleanup/integrity.rs:126-140` | 引用集合漏 `presentation_templates.logo_object_key`，该列可被写入任意 key，命中前缀则 15 分钟后被永久删除 | 加入引用 SQL 或限制 logo key 前缀 |
| P2-88 | `state.rs:188-220` + `realtime/handlers.rs:182-189` | `AppState::new` 立即丢弃 watch Sender，SSE 回退路径变成忙等自旋 100% CPU（生产走 `with_shutdown` 才幸免，公开构造器会踩） | `changed.is_err()` 时也返回 `None` |
| P2-89 | `health.rs:111-136,183-227` | readiness 每秒 9 个相关子查询（含对大表 `count(*)`）并串行探测 S3/CUPS/AMQP，最坏累计超 k8s probe timeout | 只做 `SELECT 1` + `PING`，统计量移到 `/metrics` |
| P2-90 | `migrations/0001_initial.sql:1590-1601,1447-1459,691-706,4955-4961` | 关键枚举无 CHECK：`users.user_type`、`submissions.language`、`judgements.verdict`、`runs.verdict`；`submissions_scope_shape` 不要求 PRACTICE 的 `team_id IS NULL` | 补 CHECK（新迁移） |
| P2-91 | `crates/domain/src/lib.rs:3-16,97-132` | `SubmissionState` 已架空且与 schema 冲突（把 verdict 当状态、无 COMPLETED），全仓库无引用；真实规则散落在 `features/submissions/model.rs` 与 SQL | 删除或按 status/verdict 拆分重写，并把 `SubmissionStatus` 迁入 domain |

## 前端（13）

| 编号 | 位置 | 问题 | 建议 |
| --- | --- | --- | --- |
| P2-92 | `api/client.ts:100-108,202-223` | 超时定时器在响应头到达后失效，body 卡住时抛出原始 `AbortError` 并显示英文原文 | 统一 `settled` 裁决，区分超时与调用方取消 |
| P2-93 | `views/ContestListView.vue:112-115` | 登出失败时异常中断 `router.push`，页面停在已登出状态且不跳转 | 与 `PracticeView` 一致加 `try/catch` 后无条件跳转 |
| P2-94 | `views/LiveOverlayView.vue:105,106-113` | 组件自带 10s 定时器 + 订阅内 poll 双份轮询，互不知晓 | 去掉组件定时器 |
| P2-95 | `views/LiveProgramView.vue:282-286,312-314` 等 | SSE 每事件触发全量刷新，无合并节流；Resolver 自动播放 500ms/步时形成请求风暴 | 订阅内加 `coalesceMs` |
| P2-96 | `views/SubmissionDetailView.vue:186-204,224-227` | 无 generation 守卫，切题后慢响应覆盖；卸载后 `catch` 仍重排定时器 | `requestGeneration` + `stopped` 标志 |
| P2-97 | `views/SubmissionListView.vue:108-123,146-149` | 卸载后仍可能在 `catch` 中重排轮询 | `stopped` 标志（`AdminBulkRejudgeView` 同） |
| P2-98 | `views/AwardHostScriptView.vue:238-241,270-288` | 草稿期间刷新会用服务端最新 version 覆盖 `script.value.version`，保存时静默覆盖他人修改 | 草稿期间保留 `baseVersion` |
| P2-99 | `views/JudgeClarificationView.vue:281-299,321-329` | 切换比赛后旧比赛在途响应可覆盖列表，导致「跨比赛回复」 | `changeContest` 递增 generation 并清空列表 |
| P2-100 | `components/resolver/ResolverStage.vue:12` + `views/ResolverDisplayView.vue:39-41` | 公屏渲染内部 `teamId`，标题回退显示 `Contest #<id>` | 删除展示，标题用名称占位 |
| P2-101 | `api/presentation.ts:25,51` | 前端类型暴露 `customLogoObjectKey`/`logoObjectKey`（与 P1-33 同源） | 后端删除字段，前端同步 |
| P2-102 | `views/AdminTeamImportView.vue:518-541,549` | `drafts.value[inputIndex]` 未判越界；`importing = false` 不在 `finally`，异常即永久 loading | 判空 + `finally` |
| P2-103 | `main.ts:20-44` | 无全局 `errorHandler`/`router.onError`，任一渲染异常或懒加载 chunk 失败都留空白页 | 注册错误处理与「重新加载」提示 |
| P2-104 | `views/AdminContestListView.vue:205-231,85-101` | 编辑比赛 payload 不带 `expectedVersion`，并发编辑静默覆盖；RUNNING 时赛程字段未禁用 | 带 version，按状态禁用 |

---

# P3 — 可维护性与一致性（106 条，紧凑列出）

## 后端

- **P3-001** `auth/realtime/handlers.rs:78-92` — 内联 SQL 重复实现作用域判定，绕过 contests 的 policy 层 → 抽显式策略函数
- **P3-002** `auth/password.rs:70,76` — 请求路径首次登录触发 `OnceLock` 的 `expect` → 启动时预热
- **P3-003** `auth/service/store.rs:106` — 请求路径 `expect("workstation record carries a binding")` → 改 `ok_or_else`
- **P3-004** `realtime/outbox.rs:185` — 用 `format!("{error:?}").contains("BUSYGROUP")` 判断，依赖 Debug 格式 → 匹配 `ErrorKind`
- **P3-005** `auth/service/account.rs:40` — 任意约束冲突都映射 409 `USERNAME_TAKEN` → 精确匹配 `users_username_key`
- **P3-006** `staff_accounts/service.rs:355` — 上面 P1-01 的配套：legacy 表写入应删除或标注
- **P3-007** `contest_management_scopes/service.rs:150` — 审计 `target_type='USER'` 与其余处 `'user'` 不一致
- **P3-008** `judge_dispatch/dispatcher.rs:87-99` — 判题发布失败完全静默（只在 mark_failed 也失败时 `error!`）→ 加 `warn!` 与指标
- **P3-009** `submissions/service/mod.rs:125` — OUTPUT_ONLY 的 zip 源码用 `text/plain` 上传（practice 侧正确用 `application/zip`）
- **P3-010** `submissions/exports/service.rs:147-151` — async fn 内同步 `OpenOptions::open` → `spawn_blocking`
- **P3-011** `submissions/bulk_rejudge/runner.rs:122-124` — 把 `AppError` 的 `Debug`（含原始 SQL）持久化并回传管理端 → 只存 `code()`
- **P3-012** `submissions/model.rs:386-398` vs `result_processor.rs:248-259` — 状态机文档声明 `PENDING→JUDGING→COMPLETED`，实现允许 `PENDING→COMPLETED` → 文档或代码对齐
- **P3-013** `submissions/query/mod.rs:134-141` — `team_id_for_user` 多行时取不确定第一行（practice 侧有 `ORDER BY`）
- **P3-014** `docs/api/openapi.yaml` vs `openapi.runtime.json` — 旧契约缺 scoring-policy/subtasks/judge-queue/exports 等端点；`ScoringPolicyRequest` 的自由字符串未声明 enum → 对齐
- **P3-015** `scoreboard/redis_state.rs:442` — `#[allow(dead_code)]` 未使用字段
- **P3-016** `features/redis.rs:118` — `fire_and_forget` 死代码（正好可解决 S1）
- **P3-017** `auth/service/store.rs:196` — `clear_workstation_index` 死代码
- **P3-018** `resolver/service.rs:374-380` — `expectedVersion < 0` 返回 interval 字段错误 → 拆校验
- **P3-019** `resolver/service.rs:127-143` — `verdict_at_snapshot` 绑定的是 `submissions.status`，且该表从未被读
- **P3-020** `awards/service/recipients.rs:300-371` — GET 接口里做 DELETE+INSERT 与审计，并发导出撞唯一索引 500 → 移到 freeze 流程
- **P3-021** `scoreboard/helpers.rs:68` vs `redis_state.rs:468` vs `projection.rs:52` — 开赛前提交的罚时钳制三处不一致（`max(0)` / `floor` / `div_euclid`）
- **P3-022** `scoreboard/helpers.rs:205-233` — 并列 tie-break 用 `team_id`，文档要求按队名（大小写不敏感后再敏感）
- **P3-023** `resolver/runner.rs:58-63` — auto-play 的 UPDATE 无 `current_step < total_steps` 守卫，且不写审计
- **P3-024** `competition/service.rs:520-543` — `map_workstation_write_error` 把所有约束错误解释为「已占用」
- **P3-025** `contests/helpers.rs:157` — `serde_json::from_str(payload).unwrap_or(Value::Null)` 静默发 null 载荷
- **P3-026** `contests/service.rs:379-382` — `expected_version` 用 `format!` 拼进 SQL（无注入风险但违风格）
- **P3-027** `teams/service/members.rs:26-40` — 成员新增无数量上限，`team_members` 无对应 CHECK
- **P3-028** `training/sets.rs:106-111` — slug 冲突与未知 problem_id 走 500（应 409/400）
- **P3-029** `printing/service.rs:175-185,279-319` — 列表/transition/pdf 只校验全局 `PRINTING_MANAGE`，无竞赛作用域
- **P3-030** `printing/service.rs:322-327` — 队伍可看到 `cups_job_id`/`printer_id`/`failed_reason`（CUPS stderr 可能暴露内网信息）
- **P3-031** `clarifications/service.rs:188-190` — `reply` 无 `expected_version`，并发回复静默覆盖
- **P3-032** `clarifications/service.rs:158-166` — 队伍读自己单条澄清返回 404，与 `list_mine` 不一致
- **P3-033** `contests/lifecycle_runner.rs:173-208` — runner 直接写状态字符串绕过 domain 状态机，且未校验 `rows_affected`
- **P3-034** `balloons/service.rs:411-429` — 取消最早一队气球后，下次生成会把首杀「漂移」给后来队伍
- **P3-035** — 重判 AC→WA 不回收 `balloon_tasks`，留下错误气球/首杀
- **P3-036** `training/bank.rs:208-229` 等 — 多处列表无分页/上限（题集、收藏、`LIMIT 2000`、`page` 无上限）
- **P3-037** `teams/service/import.rs:49-69` — 幂等重放只按 key 返回历史响应，不校验发起人，猜 key 可读他人批次
- **P3-038** `problems/testdata_archive.rs:88-104` — ZIP 条目大小写冲突未拒绝，不敏感文件系统上会互相覆盖
- **P3-039** `problems/service/testdata.rs:129` — 版本列表 `LIMIT 1000` 静默截断，无 `hasMore`
- **P3-040** `problems/service/attachments.rs:130-141` + `statements.rs:66-77` — 附件/题面列表无上限，题面逐条渲染 HTML
- **P3-041** `problems/handlers.rs:731-742,709-712` — async 中同步 `std::fs::open`/`remove_file`
- **P3-042** `problems/handlers.rs:726-729` — staging 目录默认 0755，多实例共享，崩溃后不清理
- **P3-043** `problems/service/mod.rs:256-270` — 重复删除返回 404 而非幂等
- **P3-044** `problems/service/mod.rs:243-249` — 删除检查把已软删除竞赛也算「已分配」，题目永久不可删
- **P3-045** `problems/service/mod.rs:80-99` — count 与 rows 两条独立语句
- **P3-046** `problems/handlers.rs:464-468,682-697` — `Content-Type` 精确匹配，带 `; charset=utf-8` 被误拒
- **P3-047** `contest_problems/model.rs:233-240` — 气球颜色允许控制字符（`\n`、`\u{202e}`）
- **P3-048** `problems/handlers.rs:492` + `service/attachments.rs:89-106` — 超长单行 utoipa 注解；`upload_interactor` 把锁/查询/UPDATE/审计/提交压进一个闭包
- **P3-049** `problems/model.rs:13-21` + `contest_problems/model.rs:11-20` — 查询结构体无 `deny_unknown_fields`，拼错 `contestId` 变 403 而非 400
- **P3-050** `main.rs:590-621` — 信号安装失败时 async 块立即返回，进程启动后立刻走关闭路径
- **P3-051** `openapi.rs:295-298` — `/api/docs` 与 `/api/openapi.json` 无鉴权，任何模式暴露内部 schema
- **P3-052** `health.rs:18-35` — `/api/health` 匿名返回队列深度、worker 容量等运营数据
- **P3-053** `metrics.rs:95-98` — `unwrap_or((0,0))` 把 Redis 故障伪装成 0
- **P3-054** `object_storage.rs:197-201` — `ensure_buckets` 实际只检查不创建，名字误导
- **P3-055** `object_storage.rs:262-264` — `Path::parse("")` 通过，空 key 可进入 delete/get
- **P3-056** `lib.rs:1-5` — `safe_sql!` 用 `AssertSqlSafe` 绕过 sqlx 注入校验（当前插值均为常量，但为未来开了口子）
- **P3-057** `object_storage_cleanup/runner.rs:131-136` — `practice_platform_settings` 缺行时每次 `RowNotFound`，清理永久停摆
- **P3-058** `config/from_env.rs:36-40` — `redis_pool_size` 无上限
- **P3-059** `migrations:3770-3773` — `judge_workers.capacity/active_tasks` 为 `smallint`，与契约 `u16` 上限冲突
- **P3-060** `migrations/README.md` + `FROZEN.txt` — 单文件迁移不可重复执行，校验和一旦变动已有库拒绝启动
- **P3-061** `config/from_env.rs:9` — `env::var(name).ok()` 把非 UTF-8 当未设置，静默回退默认值（含 CSRF secret）
- **P3-062** `lib.rs:52-58` — 完成日志在 span 之外，`request_id/method/path` 不进日志；`/livez` 探针以 info 记录放大日志
- **P3-063** `lib.rs:80-89` — 日更模式用 `starts_with` 前缀匹配，`/api/practice-offseason` 会被误禁
- **P3-064** `bootstrap.rs:8-13` — `BootstrapAdmin` 派生 `Debug` 含明文密码
- **P3-065** `error.rs:66-74` — `AppError` 派生 `Debug` 会带出 `internal_detail`（可能含 SQL 参数）
- **P3-066** `error.rs:162-168,227-245` — 归档只读等 4xx 场景仍以 `error!` 记录完整内部细节，日志噪声
- **P3-067** `config/from_env.rs:733-746` — `parse_positive` 溢出时报「expected a positive integer」，误导排障
- **P3-068** `config/from_env.rs:152-157` — metrics token 含控制字符时，真实 token 随启动错误打印

## 判题工作进程

- **P3-069** `rabbit.rs:139,146,152,165` + `artifacts.rs:424` + `archive.rs:104` — 判题路径 `expect`（锁中毒即 panic，叠加 P1-05 无限重投）
- **P3-070** `artifacts.rs:266-273` — 缓存 `.tmp` 写入/rename 失败无清理，残留参与 LRU
- **P3-071** `artifacts.rs:282-344` — 淘汰与使用竞态：`prepare` 返回路径后到 `extract_cases` 打开前可能被删
- **P3-072** `bwrap.rs:507-519` — spawn 与写 `cgroup.procs` 之间子进程可能已 fork 出未纳入 cgroup 的后代
- **P3-073** `bwrap.rs:549-552` — reader 任务 2s 超时后未 abort，管道被子孙持有时泄漏
- **P3-074** `bwrap.rs:219-226` vs `580-595` — `resolve_in_path` 用 worker PATH 预检，沙箱内 PATH 固定，可能预检通过但运行找不到工具
- **P3-075** `worker.rs:109` — `error.to_string()` 写入 `compile_log`，含 expected/actual 哈希与宿主路径
- **P3-076** `worker.rs:173-179` — `UnsupportedLanguage` 未特判，永久错误被重试 8 次
- **P3-077** `archive.rs:28-31` — zip 错误靠字符串 `"declared uncompressed size"` 匹配，crate 升级即失效
- **P3-078** `bwrap.rs:49,606-610` — `--ro-bind /etc /etc` 暴露宿主全部 `/etc`
- **P3-079** `language.rs:97-106,108-117` — Go 只在编译设 `GOMAXPROCS=1`，运行未设，易触顶 `pids.max=64`
- **P3-080** `cgroup.rs:216-231` — async 中用同步 `std::fs::write` 写 cgroup 文件
- **P3-081** `bwrap.rs:591-592` — `/tmp` tmpfs 无 size 上限（降级模式下完全无界）
- **P3-082** `sandbox/runner.rs:91-109` + `bwrap.rs:250-268` — 临时目录清理用「先执行后清理」而非 RAII，超时取消/panic 时跳过（依赖 sweep 兜底）

## 前端

- **P3-083** `main.ts:3,9,22-24` — 全量注册 Element Plus 图标 + 全量主题 CSS，违背按需引入（`icon-*.js` 182 KB 进首屏）
- **P3-084** `components/CodeEditor.vue:14,17` — 无条件引入 Monaco JSON 语言与 worker（1.1 MB + 423 KB）
- **P3-085** `api/types.generated.ts` — 15,757 行生成类型零引用，与手写 `types.ts` 靠 diff 脚本人工对齐
- **P3-086** `eslint.config.js:9-19` — 只启用 `flat/essential`，未启 `vue/no-v-html`（4 处 `v-html` 不受约束）；未忽略生成文件
- **P3-087** — 8+ 页面重复「比赛选择 + SSE + 加载 + 抽屉」骨架；`AdminTeamImportView`（666 行）职责混杂 → 抽 `useContestScope`/`useContestRealtime`/`useAsyncGuard`
- **P3-088** — 全仓 10+ 处 `as unknown as T` 关闭类型检查（`AdminAnnouncementsView:199` 等）
- **P3-089** `views/LiveManageView.vue:84-85,202-214` + `LiveView.vue:114` — 自定义模板字体为自由文本，可做 CSS 注入/页面伪装
- **P3-090** `views/LiveView.vue:114` + `AwardHostScriptView.vue:478` — 引用 `Inter`/`Noto Serif SC` 但未 vendor，离线静默回退
- **P3-091** `views/AdminProblemEditorView.vue:118-148` — 仅 `onMounted`，`:problemId` 变化不重载；发布 Tab 对 `CONTEST_MANAGE` 也渲染
- **P3-092** `views/AdminCompetitionView.vue:141-150` — `ElMessageBox.confirm` 在 `run()` 之外，取消成为未处理 rejection
- **P3-093** `views/LiveManageView.vue:53-57` — 多个自定义模板 `ElOption` 值全为 `"CUSTOM"`，选任一项不记录具体模板
- **P3-094** `views/ScreenManageView.vue:204-206` — `targets[item.id] ??= item.currentView` 永不再刷新
- **P3-095** `views/LiveView.vue:156-160` — 公告轮播固定 10s，忽略 `config.announcementIntervalSeconds`
- **P3-096** `views/AdminPermissionsView.vue:131-133` — `findIndex` 为 -1 时写 `admins.value[-1]`
- **P3-097** — 列表固定 page size，静默截断（`PracticeView:285` 100、`AdminContestDetailView:197` 500 等）
- **P3-098** `router/guards.ts:30-31` — `dailyOnly` 在竞赛模式把任意已登录角色重定向到要求 `requiresTeam` 的 `contests`，非队伍 STAFF 落到 `/forbidden`
- **P3-099** `views/LoginView.vue:150-156` — 参赛队登录直接 `router.replace(redirect)`，未做前缀白名单（`AdminLoginView` 做了）
- **P3-100** `components/live/LiveHeroPanel.vue` — `pulseTimer` 卸载时未清理（仅写本地 ref）

---

# 附录 A：系统性问题（跨模块模式，比单点 bug 更值得处理）

## S1 实时事件被当成关键路径

`enqueue_optional` 在业务事务提交前 await，失败即 `?` 回滚，至少出现在 5 个模块：`contests/service.rs:590-601`、`resolver/service.rs:352-362,411-420`、`awards/service/presentation.rs:196-207`、`presentation/service.rs:180` + `program.rs:326-335`、`submissions/service/mod.rs:285-304` + `service/rejudge.rs:321-343`。而 `balloons/service.rs:430` 又用 `let _ =` 吞掉同一错误——同一 API 两种语义。

`realtime/outbox.rs` 开头明确写着「Realtime events are refresh hints… a Redis blip cannot fail the surrounding business operation」，实现与文档矛盾。项目里已有 `features/redis.rs:118` 的 `fire_and_forget` 能表达正确语义，却挂着 `#[allow(dead_code)]`。

**建议统一为**：业务提交后再 best-effort enqueue，失败只记 warn + 指标；删掉 `fire_and_forget` 的 `dead_code` 标注并在此使用。

## S2 时间语义混用

同一件事在 `now()`、`submitted_at`、`generated_at`、DB 时钟与进程时钟之间反复切换，已导致四处错误：封榜自动解冻（P0-06）、冻结快照 COPY（P1-16）、气球冻结判定（P1-23）、播放进度与倒计时漂移（`presentation/service.rs:303`）。**建议**：明确「业务判定一律用提交时刻，展示用 DB 时刻」，在 domain 层封装时间语义函数并补测试。

## S3 文档声明与实现脱节

以下四处在 ADR/设计文档里写得很清楚，代码没做，测试也没测：

| 文档声明 | 实现现状 |
| --- | --- |
| 结束仍冻结（`scoreboard-resolver.md`） | `end_at` 即解冻（P0-06） |
| 首杀仅 official non-star | STAR 可拿，测试还固化了错误行为（P1-25） |
| bwrap 降级到 rlimit（ADR-003） | 全仓库只有注释，无任何 `setrlimit`/`--rlimit`（P1 系列） |
| preflight 必须覆盖 RabbitMQ（ADR-001） | 只验 S3 + 沙箱（P2-80） |

**建议**：把 ADR 中的每条不变量转成集成测试断言，作为「文档-代码一致性」门禁。

## S4 会话撤销只写 PostgreSQL

三处「撤销会话」都只删 `auth_sessions` 表，而真源是 Redis（P1-01）。属同一模式的三次复制。**建议**：把撤销动作收敛到唯一入口 `auth::revoke_user_sessions`，其他模块只能调用它。

## S5 权限作用域校验散落且实现不一

至少五处各自实现「是否有该竞赛管理权」：`realtime/handlers.rs:78-92`（内联 SQL）、`contests/helpers.rs`（`ReadAccess`）、`problems/service/mod.rs:78`（只鉴权不过滤）、`presentation/*`（全局直接权限）、`printing/service.rs`（只校验全局权限）。这正是 P1-21、P2-42、P3-029 的根因。**建议**：统一为单一 policy 函数族，所有 feature 只调用它。

---

# 附录 B：已排除的疑点（避免重复排查）

审读过程中判为「可疑但读上下文后正确」、无需再查的点：

**安全面**
- CSRF 全局 layer 覆盖所有非 GET/HEAD/OPTIONS，HMAC + `subtle::ct_eq`，`reuse_or_issue` 复用是为多标签页，非弱点
- 登录限流脚本在 Redis 内原子执行「裁剪+计数」，是滑动窗口，无 TOCTOU 超发（问题只在键维度）
- 密码哈希为 Argon2id（m=19456, t=2, p=1），并发由 8 permit 信号量限流，有 bcrypt 兼容与 dummy 校验对齐时序
- SQL 全参数绑定；7 处 `format!` 拼接均为编译期常量列名
- ZIP 路径穿越/符号链接/加密/压缩方法/声明大小均校验，且有夹带实际字节预算与 `actual == declared` 复核
- S3 key 走 `object_store::Path::parse`（拒 `.`/`..`/`/`/控制字符），附件名先取 basename 再拼 UUID
- 响应头 `Content-Disposition` 对非安全字节做 `%XX` 编码
- CUPS 用 `.args(&[...])` 数组传参，无 shell，无用户可控 `-o`
- 沙箱 `network_mode=none` + `readonly_rootfs` + `cap_drop ALL` + `no-new-privileges` + 非 root uid + `--unshare-all`
- 判题 ACK 在 confirm 成功后（`rabbit.rs:757-784`），prefetch 与并发双保险不超发
- 前端 4 处 `v-html` 全部来自服务端 `ammonia` 净化内容，无 `innerHTML`/`document.write`
- 前端无 CDN/外链字体，Monaco 本地打包；用户会话走 HttpOnly cookie，CSRF 为 double-submit

**正确性面**
- 竞赛窗口校验左闭右开，且校验与写入用同一事务时间戳，无 TOCTOU
- 判题结果重投递以 `judgement_id` 为幂等键，`runs` 有 `(judgement_id, test_index)` 唯一索引兜底
- 迟到结果不会覆盖新状态（`superseded` 早退在写入前）
- `balloon_tasks` 首杀有部分唯一索引 + advisory lock 双保险
- 快照 version 在事务内 `pg_advisory_xact_lock` 分配，不会重号
- 缓存 key 含 contest/revision/variant/phase/selector，SHA-256 截 64 bit 碰撞概率可忽略
- Resolver 计划可重复执行（无 HashMap 迭代，`plan_sha256` 覆盖全部输入）
- 批量导入用 `pg_advisory_xact_lock(hashtextextended(key))` + 请求哈希，重放与冲突均有测试
- 归档只读触发器覆盖 print/balloon/clarification/announcement/submissions 等业务表
- `contest_problems` 重排用 `SET CONSTRAINTS DEFERRED`，与 `DEFERRABLE INITIALLY IMMEDIATE` 约束匹配
- 对象清理任务 `(bucket,key)` 幂等入队，claim 用 `SKIP LOCKED` + lease fencing
- 指标无高基数标签（未把 user_id/problem_id 放进 label）
- `AppError` 的 HTTP body 只含 code/message/fieldErrors，`internal_detail` 仅进日志

---

# 附录 C：建议修复批次

| 批次 | 内容 | 理由 |
| --- | --- | --- |
| **B0（立即）** | P0-01、P0-02、P0-03 | 唯一可被外部利用的问题，且都在同一目录，可一次改完 |
| **B1** | P0-04 + 补「含已判提交」的投影重建集成测试 | 恢复路径整体失效，且现测试覆盖不到 |
| **B2** | P0-05、P0-06 | 现场部署直接踩到的可用性/公平性问题 |
| **B3** | P1-01 ~ P1-02、S4 | 会话撤销与复活，安全相关且改法明确 |
| **B4** | P1-13 ~ P1-15、S1 | 判题结果丢失链路 + 实时事件语义统一 |
| **B5** | P1-16、P1-23、P1-25、S2 | 封榜/气球/首杀的时间语义一致性 |
| **B6** | P1-19 ~ P1-22 | 上传残留、object key、题目作用域 |
| **B7** | P1-35、P1-36 | 离线 LAN 部署前必须做，否则现场页面崩 |
| **B8** | P1-26 ~ P1-34 | 可观测性与展示面 |
| **B9** | P2 分批 | 按模块各自排期 |
| **B10** | P3 + S3 的「文档-代码一致性门禁」 | 长期治理 |

---

*本清单为只读审计产出，未修改任何代码。P0 与标 ✅ 的 P1 已回读源码核验；其余条目的逻辑链见各条「问题」描述，修复前建议先按「位置」确认一行。*
