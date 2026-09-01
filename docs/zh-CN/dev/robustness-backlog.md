---
title: 健壮性整改待办清单
description: 2026-09 健壮性审计的记录——第一轮整改已关闭的问题，以及尚未解决、留待后续安排的缺口，每项均附原因与修复思路。
---

# 健壮性整改待办清单

本页记录 2026-08-31 至 2026-09-01 对四个领域（API、评测 Worker、前端、部署/CI）完成的健壮性审计结果。第一轮整改在 `fix/robustness-remediation` 分支上关闭了 1 个 P0、14 个 P1 以及部分 P2/P3 项。**所有未修复**的发现均记录在此，并附原因与修复思路，使缺口保持可见、可以被主动排期，而不是在事故中重新被发现。

## 状态说明

- **已关闭** — 已在第一轮整改中修复。
- **未解决** — 已对照代码核实，属有意延后；下方列出原因与修复思路。请在首个 beta 发布前以及每次线下比赛前重新审视各项。

## 第一轮整改已关闭项

| 领域 | 严重度 | 内容 |
| --- | --- | --- |
| 评测 Worker | P0 | OutputOnly 压缩包谎报声明尺寸导致 zip 炸弹，并放大为毒消息崩溃循环（`sandbox/archive.rs` 改为实际字节预算） |
| 评测 Worker | P1 | OutputOnly 非法压缩包现在判选手 WrongAnswer，而非 SystemError |
| 评测 Worker | P1 | 依据任务契约推导单任务墙钟 deadline；超时走既有重试预算 |
| 评测 Worker | P1 | 孤儿容器/作业目录 GC（启动 + 周期清扫）、409 创建冲突重试、drain 上限由在飞 deadline 推导 |
| 评测 Worker | P1 | 单投递错误隔离；仅协议级失败才终结 AMQP 会话 |
| 评测 Worker | P2 | 无 swap 记账主机上 `memory_swap` 更新失败改为非致命（仅内存限制重试） |
| 评测 Worker | P3 | 判定成功后的清理失败不再丢弃已完成的评判结果 |
| API | P1 | SSE 流在 shutdown 时终止；有客户端在线时优雅关闭可以完成 |
| API | P1 | 对象存储上传超时与 5 秒请求超时拆分（`PROJECT_BALLOON_OBJECT_STORAGE_UPLOAD_TIMEOUT_MILLISECONDS`，默认 300000） |
| API | P2 | RabbitMQ 全部 channel 等待加界；publish 重连期间不再持有 channel 互斥锁；消费者建连循环走重连路径 |
| API | P2 | `BackgroundRunners::shutdown` 在 30 秒总 deadline 内 join；超时任务具名告警并中止；连接池关闭必达 |
| API | P2 | 数据库连接改由 `PgConnectOptions` 构建，启动建连限时 10 秒 |
| API | P3 | `OptionalAuthContext` 对过期/非法 Cookie 降级为匿名而非 401 |
| 前端 | P1 | API client 请求超时（30 秒）、浏览器级失败统一映射 `NETWORK_ERROR`、非 JSON 错误体截断 |
| 前端 | P1 | CSRF 透明恢复：`CSRF_INVALID` 时清缓存、重取并重放一次 |
| 前端 | P1 | CodeEditor 立即 emit；消除提交旧代码的时间窗 |
| 前端 | P1 | 会话初始化失败不再缓存为已初始化；每次导航重新探测 |
| 前端 | P1 | 大屏客户端在启动失败或 token 失效后按有上限退避自动重新注册 |
| 前端 | P2 | LiveView 过期响应代数守卫；首屏加载不再阻塞轮询启动 |
| 前端 | P2 | 计分板静默轮询失败时显示可见的"数据可能过期"提示 |
| 前端 | P2 | 登出失败不再留下半清理状态（全部角色布局） |
| 部署 | P1 | 发布工作流以 tagged SHA 的全部必需 CI check-run 作为门禁（fail-closed） |
| 部署 | P1 | 生产依赖 npm 审计：PR 时步骤 + 夜间定时任务 |
| 部署 | P1 | Compose 透传 `PROJECT_BALLOON_JUDGE_STUCK_REQUEUE_INTERVAL_SECONDS`、`JUDGE_HEALTH_PORT`、`JUDGE_HEALTH_SESSION_ERROR_WINDOW_SECONDS` |
| 部署 | P2 | `.env.example` 补齐代码读取的 5 个变量，并增加 judge-worker 校验的警示注释 |
| 部署 | P2 | 工具链在 `rust-toolchain.toml` 钉死 1.94.1；CI、发布与 docker-integration 工作流的版本由其派生（于 0.1.0-alpha.4 发布准备轮关闭） |

## 未解决项

### API

- **[P2] 测试数据上传在单请求内将 256 MiB 整体读入内存。** multipart 处理器先用 `field.bytes().await` 收拢整个字段再做哈希与 S3 PUT，而下载路径已特意改为流式。两三个并发的管理员上传就会造成数百 MiB 的 RSS 尖峰。
  *延后原因：* 改动会触及上传校验管线与哈希计算，值得独立成变更并配套存储集成测试。*修复思路：* 将字段流式写入 S3 multipart 上传（或临时文件），增量计算 SHA-256，并在流式读取侧强制 256 MiB 上限。

- **[P2] PostgreSQL 缺少服务端 statement / idle-in-transaction 超时。** 故障切换时黑洞 TCP 连接上的查询可能挂到 OS 级 TCP 超时；池 `acquire_timeout` 只约束获取（现已有启动建连限时叠加）。
  *延后原因：* 全局 `statement_timeout` 可能误杀合法长查询（导出构建）；合理取值是按角色的部署决策。*修复思路：* 通过启动 SQL 或 `ALTER ROLE` 为 API 数据库角色设置 `statement_timeout` 与 `idle_in_transaction_session_timeout`，并写入 `docs/ops`。

- **[P2] 数据库连接无 TCP keepalive。** `sqlx` 0.9.0 的 `PgConnectOptions` 未暴露 `tcp_keepalive`（已对照锁定版本核实），开启 keepalive 需要升级 sqlx 或做套接字层变通。建连现在已有限时。

- **[P3] `/metrics` 无鉴权。** 队列深度、worker 容量、提交量对所有可达端口者可见。加固前需运维决策（绑定期隔离、可信代理 CIDR 或 token）。

- **[P3] 结果消费者瞬时失败无延迟重投。** PostgreSQL 降级时立即重投循环（受约 5 秒获取超时限制）。修复思路：为结果增加 TTL 重试交换器，对齐任务路径。

### 评测 Worker

- **[P2] 交互题：选手可通过 `/work/time.err` 伪造 GNU-time 指标。** 选手进程与 interactor 同 UID，可追加伪造标记字节，`rfind` 会取到假值，按 0 ms/0 KB 计时绕过 TLE/PLE（墙钟强杀仍然生效）。
  *延后原因：* 修复需重定向计时报告 fd 或与容器统计交叉校验——属于沙箱行为变更，需要独立的 docker 集成测试覆盖。*修复思路：* 将报告写入选手进程无法打开的 fd，或校验标记必须位于文件末尾且与容器 CPU 统计一致。

- **[P2] 测试数据缓存无限增长。** 每个题目版本的 zip 永久留在缓存目录；大赛期间可能中途耗尽磁盘。缺失条目可安全重新拉取并哈希校验。*修复思路：* 带大小上限的 LRU 淘汰 + 淘汰日志与压测。

- **[P2] 拓扑声明校验是被动的。** 被动声明不比较死信参数与绑定关系；`judge.tasks` 一旦被错误重建，重试 nack 会静默丢消息。修复思路：以相同参数做非被动声明（不匹配即失败）或启动期金丝雀任务。

- **[P3] I/O 错误信息不带路径上下文**（如编译日志只出现 `os error 2`）。面广但机械：包装各处 `tokio::fs` 调用点。

- **[P3] 兜底 CPU 指标是容器累计值**，超时/OOM 时上报的 `time_ms` 偏大。判定不受影响（由标志位决定）。修复思路：按 exec 前后快照取差值。

- **[P3] Docker 超时硬编码**（bollard 客户端 10 秒、`DOCKER_API_TIMEOUT` 5 秒），未接入 worker 配置。

### 前端

- **[P2] 大文件下载无超时**（testdata ZIP 与附件豁免，`timeoutMs: 0`）。传输中途停滞不设限；传输层失败仍会以 `NETWORK_ERROR` 呈现。修复思路：进度 UI + 显式取消，可选长上限。

- **[P3] 奖项展示轮换时钟在 SSE 中断期间漂移**：本地时钟按每次 `+= 1000` 累加。每个事件都会重新对时，影响有限。修复思路：锚定 `performance.now()`。

- **[P3] 展示路由对重复 `?contestId` 参数得到 NaN**（LiveView、AwardDisplayView、ScreenManageView），畸形链接给出误导性的"缺少 contestId"。修复思路：按 ResolverDisplayView 的方式归一化数组参数。

- **[接受] 启动探测失败期间用户可能短暂落在登录页**，直到下一次导航重试成功；未增加专门的"探测中"界面（不存在重定向循环风险）。若网络事故中用户反馈困惑再行处理。

### 部署与 CI

- **[P2] Compose 服务未定义资源限制。** 无上限的 PostgreSQL、RabbitMQ 或 judge-worker 可能 OOM 单机演练宿主机。*延后原因：* 合理上限取决于主机规格，应结合压测数据一起设定而非拍脑袋。*修复思路：* 至少为 judge-worker、postgres、rabbitmq 设置 `mem_limit`/`cpus`。

- **[P2] 备份完全依赖人工。** 未随包提供 systemd timer/cron 示例，也没有备份时效告警；数据保护全凭运维记忆。*修复思路：* 二进制包内置 `project-balloon-backup.timer`，同时给出 crontab 方案与 ops 文档中的时效检查。

- **[P2] Compose 安装脚本不创建评测缓存目录**，归属不对的自动创建 bind mount 会让 worker 缓存写入以难懂的方式失败。修复思路：在 `scripts/deploy/install.sh` 中创建并校验归属。

- **[P3] install.sh 在服务被证实存活前就报告成功**；应以带 deadline 的 `/livez`（和 `/api/health`）轮询替代单次 `systemctl is-active`。

- **[P3] 可观测性栈与 compose `web` 服务缺少 healthcheck。**

- **[P3] install.sh 的前缀防护窄于其 `rm -rf "$PREFIX/web"`。** 应解析真实前缀并拒绝紧邻系统目录的位置，或将旧目录改名移走。

- **[P3] 二进制模式备份缺少部署配置骨架**（归档清单中应包含脱敏 env 文件副本与恢复清单）。

- **[P3] CI 硬化遗留：** 覆盖率汇总仅展示无阈值、无前端浏览器 e2e、docs-only PR 不触发文档构建。

## 已核实可靠（无需行动）

审计同时确认以下方面是健全的，除非设计变更否则无需复审：outbox 派发模式（`SKIP LOCKED`、租户所有权、尝试上限、退避）、提交路径事务一致性（含 S3 补偿）、判定结果的幂等应用、SQL 注入面（全部插值为编译期常量或白名单）、登录限流与恒时比较、沙箱加固（禁网、cap-drop-all、noexec、`O_NOFOLLOW` 输出读取、防路径穿越的 zip 条目名）、前端 XSS 面（所有 `v-html` 内容均经服务端 ammonia 净化）、定时器清理与路由守卫、迁移事务性与冻结校验和（CI 真实强制）、以及密钥卫生（无真实凭据入库）。
