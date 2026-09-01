---
title: 健壮性整改待办清单
description: 2026-09 健壮性审计的记录——三轮整改已关闭的问题，以及尚未解决、留待后续安排的缺口，每项均附原因与修复思路。
---

# 健壮性整改待办清单

本页记录 2026-08-31 至 2026-09-01 对四个领域（API、评测 Worker、前端、部署/CI）完成的健壮性审计结果。发现项分三轮完成整改：第一轮在 `fix/robustness-remediation` 分支上关闭了 1 个 P0、14 个 P1 以及部分 P2/P3 项；第二轮关闭了评测 Worker 的两个 P2 项；第三轮关闭了其余的 API、评测 Worker、前端与部署/CI 发现。**所有未修复**的发现均记录在此，并附原因与修复思路，使缺口保持可见、可以被主动排期，而不是在事故中重新被发现。

## 状态说明

- **已关闭** — 已在其中一轮整改中修复。
- **延后** — 因有记录的明确决策（附日期）而推迟；阻塞依赖变化后重新审视。
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

## 第二轮整改已关闭项（2026-09-01）

| 领域 | 严重度 | 内容 |
| --- | --- | --- |
| 评测 Worker | P2 | 交互题 GNU-time 报告直接写在 exec stderr 流上——选手进程无法持有该描述符；选手可写的诊断文件（`program.err`、`interactor.err`）改由独立 exec 回读，并在标记解析完成后再追加，伪造标记不可能成为最后一个。附 docker 集成层的伪造攻击回归测试 |
| 评测 Worker | P2 | 测试数据缓存以大小上限收敛为 LRU（`JUDGE_TESTDATA_CACHE_MAX_BYTES`，默认 8 GiB，`0` 关闭）：缓存命中刷新 mtime，插入时优先淘汰最旧条目并记录淘汰日志，刚存入的条目不会被自身插入触发淘汰 |

## 第三轮整改已关闭项（2026-09-01）

| 领域 | 严重度 | 内容 |
| --- | --- | --- |
| API | P2 | 测试数据上传改为流式写入 `0600` 临时文件、增量计算 SHA-256、流式 256 MiB 上限与流式 S3 PUT——并发管理员上传不再造成数百 MiB 的 RSS 尖峰 |
| API | P3 | `/metrics` 可选 bearer token 鉴权（`PROJECT_BALLOON_METRICS_TOKEN`，恒时比较）；compose 透传、env 模板与配置参考均已记录 |
| API | P3 | 结果消费者瞬时失败经 `judge.results.retry` TTL 延迟重投（以 `x-retry-count` 计数上限 20 次，之后进 `judge.dead`），PostgreSQL 降级时不再立即循环重投 |
| 评测 Worker | P2 | 拓扑声明校验改为非被动全参数声明（死信参数、TTL、绑定——与 API 的 `topology::declare` 保持一致）；不匹配时以运维提示使运行失败，而非静默丢弃重试 nack |
| 评测 Worker | P3 | 文件系统错误经 `with_path_context` 携带路径上下文，管理员可见的编译/运行日志不再出现裸 `os error 2` |
| 评测 Worker | P3 | 兜底 CPU 指标改为每次 exec 的 one-shot stats 快照差值，而非容器累计值，超时/OOM 运行不再虚增 `time_ms` |
| 评测 Worker | P3 | Docker 超时改由配置推导（`JUDGE_DOCKER_CONNECT_TIMEOUT_SECONDS`、`JUDGE_DOCKER_API_TIMEOUT_MILLISECONDS`），不再硬编码 |
| 前端 | P2 | 大文件下载带 10 分钟上限、流式进度与显式取消按钮（TestdataTab），替代无上限的 `timeoutMs: 0` 豁免 |
| 前端 | P3 | 奖项展示轮换时钟锚定 `performance.now()`，SSE 中断期间不再漂移 |
| 前端 | P3 | 重复的 `?contestId` 查询参数经 `numericQueryId` 在各展示视图归一化，不再得到 NaN |
| 部署 | P2 | Compose 服务声明 `mem_limit`/`cpus`（默认 postgres 1g、rabbitmq 1g、judge-worker 2g；全部上限可用环境变量覆盖），覆盖 data、app 与 observability 三个栈 |
| 部署 | P2 | 备份自动化：安装程序渲染并启用 `project-balloon-backup.service`/`.timer`（每天 03:15 加抖动、停机补跑、以 `ExecStartPost` 断言时效），`scripts/backup/check-freshness.sh` 超 26 小时告警，ops 文档附 crontab 方案 |
| 部署 | P2 | Compose 安装脚本创建评测缓存目录并断言归属与 `XCPC_SANDBOX_USER` 一致（否则给出确切的 `chown` 命令后失败），root 属主的 bind mount 不再破坏 worker 缓存写入 |
| 部署 | P3 | `install.sh` 以带 deadline 的 `curl` 轮询 `/livez`（默认 120 秒）作为 API 与 Judge Worker 的完成门禁，替代单次 `systemctl is-active` |
| 部署 | P3 | 六个 observability 服务与 compose `web` 服务均带 healthcheck（端点与探测工具已对照实际镜像核实） |
| 部署 | P3 | 安装程序前缀防护经 `readlink -m` 解析并拒绝全部系统根目录；旧前端包改移至带时间戳的 `web.old-*` 目录而非 `rm -rf` |
| 部署 | P3 | 备份归档内含脱敏 env 快照（承载凭据的变量替换为 `CHANGE_ME_redacted_from_backup`）与恢复清单 |
| CI | P3 | 覆盖率汇总强制 llvm-cov 行覆盖阈值，不再仅展示；docs 工作流同时构建 docs-only PR（Pages 部署仍仅限 main） |

## 未解决项

### API

- **[按用户决定延后（2026-09-01）] PostgreSQL 缺少服务端 statement / idle-in-transaction 超时。** 故障切换时黑洞 TCP 连接上的查询可能挂到 OS 级 TCP 超时；池 `acquire_timeout` 只约束获取（现已有启动建连限时叠加）。用户决定延后此项：全局 `statement_timeout` 可能误杀合法长查询（导出构建）；合理取值是按角色的部署决策。重启此项时，通过启动 SQL 或 `ALTER ROLE` 为 API 数据库角色设置 `statement_timeout` 与 `idle_in_transaction_session_timeout`，并写入 `docs/ops`。

- **[按用户决定延后（2026-09-01）] 数据库连接无 TCP keepalive。** `sqlx` 0.9.0 的 `PgConnectOptions` 未暴露 `tcp_keepalive`（已对照锁定版本核实），开启 keepalive 需要升级 sqlx 或做套接字层变通。用户决定待依赖面变化后再处理；建连现在已有限时。

### 前端

- **[接受] 启动探测失败期间用户可能短暂落在登录页**，直到下一次导航重试成功；未增加专门的"探测中"界面（不存在重定向循环风险）。若网络事故中用户反馈困惑再行处理。

### 部署与 CI

- **[P3] CI 无前端浏览器 e2e。** 前端已有单测、lint/format 门禁与 OpenAPI 漂移检查，但没有无头浏览器运行真实 UI 的端到端验证。修复思路：针对本地构建的 API + web 组合的 Playwright 任务；在测得运行成本前先不进每次 PR 的 CI（可作夜间定时任务的候选）。

## 已核实可靠（无需行动）

审计同时确认以下方面是健全的，除非设计变更否则无需复审：outbox 派发模式（`SKIP LOCKED`、租户所有权、尝试上限、退避）、提交路径事务一致性（含 S3 补偿）、判定结果的幂等应用、SQL 注入面（全部插值为编译期常量或白名单）、登录限流与恒时比较、沙箱加固（禁网、cap-drop-all、noexec、`O_NOFOLLOW` 输出读取、防路径穿越的 zip 条目名）、前端 XSS 面（所有 `v-html` 内容均经服务端 ammonia 净化）、定时器清理与路由守卫、迁移事务性与冻结校验和（CI 真实强制）、以及密钥卫生（无真实凭据入库）。
