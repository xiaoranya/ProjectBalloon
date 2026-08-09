# 故障排查

本文档将常见症状映射到检查项与恢复操作。针对特定故障的响应流程，参见 `docs/ops/disaster-recovery.md`（中文镜像：`docs/zh-CN/ops/disaster-recovery.md`）。

## API 健康检查 DOWN

症状：`GET /api/health` 返回 HTTP 503 且 `status: down`，或某个组件报告 `DOWN`。

检查项：

- `curl --fail http://127.0.0.1:8080/livez` — 如果失败，说明 API 进程本身未提供服务；检查 `systemctl status project-balloon-api` 与 `journalctl -u project-balloon-api`。
- PostgreSQL 的可达性与凭据（`DATABASE_URL`）。
- 当 `PROJECT_BALLOON_REALTIME_REDIS_ENABLED=true` 时，检查 Redis 可达性。
- 当 `PROJECT_BALLOON_RABBITMQ_ENABLED=true` 时，检查 RabbitMQ 可达性。
- 当 `PROJECT_BALLOON_OBJECT_STORAGE_ENABLED=true` 时，检查对象存储可达性。
- 当 `PROJECT_BALLOON_CUPS_ENABLED=true` 时，检查 `lpstat -h <host>:631 -p <queue>`。

`cups` 组件 DOWN 意味着新的打印请求会进入可重试的 `FAILED` 状态，而不是被报告为已打印。

## 判题 Worker 离线

症状：Worker 在线数下降、判题队列深度增长、提交停留在 `pending`/`judging`。

检查项：

- `systemctl status project-balloon-judge-worker` 及其日志。
- Worker 的 RabbitMQ 连接与凭据。
- 沙箱 socket 权限（`XCPC_SANDBOX_SOCKET`）。
- 运行时镜像已导入且标签与环境文件一致。
- `JUDGE_CACHE_DIR` 对 Worker 用户可写。
- 主机 CPU、内存与磁盘。

Worker 在最近一次心跳后的 15 秒内被视为在线。重启后，确认 RabbitMQ 未确认的投递已被重新入队或完成，并审查任何 `system_error` 提交。

## 判题队列无法排空

症状：`GET /api/admin/contests/{contestId}/judge-queue/status` 报告非零计数。

检查项：

- `PENDING`/`JUDGING` 提交 — 审查卡住的提交，必要时重判。
- `outboxPending` 中包含 `PUBLISHING` 租约；没有 Publisher 确认的任务不能安全排空。
- `outboxFailed` 行 — 检查 RabbitMQ 连接与死信队列。
- Worker 在线数与预取容量。

不要手动删除 outbox 行；请使用已评审的恢复路径。

## 登录或 CSRF 失败

症状：登录返回 403/CSRF 错误，变更类请求因 CSRF 不匹配被拒绝。

检查项：

- 前端在登录前调用 `GET /api/auth/csrf`，并在每个状态变更请求的 `X-XSRF-TOKEN` 中发送返回的令牌。
- `XSRF-TOKEN` cookie 与请求头一致。
- 生产环境中 `PROJECT_BALLOON_CSRF_SECRET` 是部署专属密钥；`PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET` 不得在生产启用。
- `PROJECT_BALLOON_SECURE_COOKIES=true` 且 API 前存在 TLS 终结；浏览器不会通过明文 HTTP 发送 `Secure` cookie。
- 被标记为需要重置密码的用户在修改密码前只能访问认证流程。

## 记分板过期或不一致

症状：公共榜显示过时结果、缓存未命中，或公共榜与管理榜不一致。

检查项：

- 封榜语义：封榜期间公共榜隐藏 `submitted_at > freeze_at` 的提交；管理榜显示真实状态。
- `PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED` 设置：Redis 故障时回退到 PostgreSQL；确认缓存 TTL 与超时合理。
- 从管理控制台重建记分板缓存，并对比公共、管理、分组与 First Blood 视图。
- 判题结果应用正确；确认提交/判定计数。

## 打印失败

症状：打印请求卡在 `QUEUED`/`PRINTING`、CUPS 任务失败、打印机离线。

检查项：

- 打印机电源、纸张、网络与 CUPS 状态（`lpstat -h <host>:631 -p <queue>`）。
- `PROJECT_BALLOON_CUPS_ENABLED` 与 `PROJECT_BALLOON_CUPS_PRINTER` 与配置的队列一致。
- 打印机恢复后重试失败任务，或对紧急请求使用手动下载兜底。
- 审计状态保持准确。

## 大屏或直播页面问题

症状：大屏心跳停止、OBS 浏览器源无法加载直播页面、展示内容过期。

检查项：

- 网关与页面路由可用性。
- 直播页面令牌有效性（正式比赛前轮换排练时共享的令牌）。
- 刷新浏览器源或重新连接大屏客户端。
- 动态展示无法快速恢复时，使用静态兜底页面。

## 备份或恢复问题

症状：备份失败、恢复验证失败。

检查项：

- 备份主机可访问 `BACKUP_OBJECT_STORAGE_ENDPOINT`。
- 所需工具存在（`pg_dump`、`psql`、`sha256sum`、AWS CLI v2）。
- 恢复需要 `PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA`。
- 恢复后，在正式比赛前按照 `docs/ops/backup-restore.md`（中文镜像：`docs/zh-CN/ops/backup-restore.md`）执行恢复后验证。

## 对象清理积压

症状：`objectCleanup` 健康组件显示待处理/失败任务，`object_storage_cleanup_failed` 增长。

检查项：

- 失败的附件、测试数据、提交源码与打印 PDF 补偿会自动带退避重试。
- 比赛期间不要手动删除清理行；S3 兼容删除是幂等的，任务可以安全重试。
- 检查清理 runner 日志中的存储连接错误。

## 指标缺失

症状：`GET /metrics` 不可达或为空。

检查项：

- 该端点无需认证；确认反向代理或防火墙将其限制在监控网段。
- Prometheus 抓取目标与 `api:8080`（或配置的绑定地址）一致。
- 所需仪表盘面板与标签名与导出的指标一致。

## 通用事故记录

每次事故都应记录：时间、受影响服务、症状、操作者、采取的操作、验证结果与后续跟进。在重启或恢复前保留日志与当前状态。
