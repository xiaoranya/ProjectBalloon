---
title: 运维
description: 彩排与正式比赛的常规现场运维：健康检查、监控、日志、常见操作与事故记录。
---

# 运维

本节介绍彩排与正式比赛的常规现场运维。

## 操作员原则

- 优先使用已评审的服务流程，而不是临时进程或容器命令。
- 在重启或恢复前保留日志。
- 正式比赛期间，除非遵循已批准的恢复流程，否则不清除 Redis、RabbitMQ、数据库、RustFS 或卷。
- 在比赛操作日志中记录所有手动变更。
- 在任何 Judge、缓存或数据恢复操作后，确认公共记分板与管理员记分板。

## 日常命令

二进制包将应用服务安装在 systemd 下：

```text
sudo systemctl status project-balloon-api project-balloon-judge-worker
sudo systemctl start project-balloon-api project-balloon-judge-worker
sudo systemctl stop project-balloon-api project-balloon-judge-worker
sudo systemctl restart project-balloon-api project-balloon-judge-worker
sudo journalctl -u project-balloon-api -u project-balloon-judge-worker -f
curl --fail http://127.0.0.1:8080/livez
curl --fail http://127.0.0.1:8080/api/health
```

PostgreSQL、Redis、RabbitMQ、对象存储、沙箱服务、Nginx、CUPS 与可观测性都是用户管理的外部组件。使用主机准备期间选择的命令与服务名检查和操作它们；ProjectBalloon 包不假定拥有其生命周期。正式比赛期间，记录操作后只重启受影响的服务。

## 健康检查清单

比赛开始前检查：

- API 健康端点（`GET /api/health` —— 汇总数据库、Redis、RabbitMQ、RustFS、队列与 Worker；200 up / 503 down）与 `/livez` 的进程存活。
- PostgreSQL 读写检查。
- Redis 读写检查。
- RabbitMQ 队列发布 / 消费检查。
- RustFS 桶读写检查。
- Judge worker 在线数。
- Judge 队列长度。
- 记分板缓存新鲜度。
- CUPS 打印机状态。
- Screen 实例心跳。
- Live 页面加载且不暴露敏感信息。
- 备份任务成功。

启用 CUPS 投递时，API 健康端点通过 `lpstat` 检查配置的队列。`cups` 组件 DOWN 意味着新请求将失败进入可重试的 `FAILED` 状态，而不是报告为已打印。直接验证队列：

```text
lpstat -h <host>:631 -p <queue-name>
```

`objectCleanup` 健康组件暴露待处理与失败的 RustFS 清理任务。失败的附件、测试数据、提交源码与打印 PDF 补偿会自动重试。默认值可通过以下配置调整：

```text
PROJECT_BALLOON_OBJECT_CLEANUP_POLL_MILLISECONDS=5000
PROJECT_BALLOON_OBJECT_CLEANUP_LEASE_SECONDS=30
PROJECT_BALLOON_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS=1000
PROJECT_BALLOON_OBJECT_CLEANUP_BATCH_SIZE=50
```

比赛期间不要手动删除清理行。任务可以安全重试，因为 S3 兼容的对象删除是幂等的。

## 监控指标

Rust API 在 `GET /metrics` 暴露 Prometheus 文本格式。该端点出于 scraper 兼容性无需认证，必须由反向代理或防火墙限制到监控网络。它目前导出实时与 Judge outbox 积压 / 失败、对象清理积压 / 失败、异步导出状态、在线 Judge 容量 / 活动槽位，以及每日 practice 提交 / 判题计数。

示例抓取任务：

```yaml
- job_name: project-balloon-api
  metrics_path: /metrics
  static_configs:
    - targets: ["api:8080"]
```

关键仪表板面板：

- API QPS。
- API p95 延迟。
- HTTP 5xx 率。
- PostgreSQL 连接、锁、复制或备份状态（如适用）。
- Redis 内存使用与命令延迟。
- RabbitMQ 队列深度与未确认消息。
- RustFS 磁盘使用与请求错误。
- Judge worker 在线数与占用槽位。
- Judge 任务等待时间。
- 提交率。
- 今日 practice 提交与当前判中的 practice 任务。将每日数量与 `practice_platform_settings.daily_submission_limit` 比较，并在用户达到并发限制之前调查持续存在的判题积压。
- 记分板更新延迟。
- 主机 CPU、内存、负载与磁盘使用。
- 打印机状态与待处理打印任务。

## 日志

日志应由用户选择的日志栈收集。仓库在 `deploy/observability/` 下包含可选的 Promtail/Loki 示例。

重要日志流：

- Nginx access/error 日志。
- API 应用日志。
- Judge scheduler 日志。
- Judge worker 日志。
- RabbitMQ 日志。
- PostgreSQL 日志。
- RustFS 日志。
- CUPS 日志。

日志绝不能包含密码、超出显式管理员视图的源码，或 live token。

## 常见操作

重建记分板缓存：

```text
管理员操作：为比赛重建记分板缓存
验证：公共记分板、管理员记分板、分组记分板、first blood 状态
```

比赛后排空 Judge 队列：

```text
等待 pending 与 judging 提交归零
复核 system_error 提交
运行所需的重判操作
生成最终记分板
```

备份与恢复：

```text
sudo /opt/project-balloon/scripts/backup/backup.sh /var/backups/project-balloon
PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA \
  sudo -E /opt/project-balloon/scripts/backup/restore.sh <backup-run-dir>
```

参见 [备份与恢复](backup-restore.md) 了解强制备份点、内容、保留与恢复后验证。在彩排前、数据冻结后、比赛开始前立即以及 Judge 队列排空后各做一次备份。

每日 practice 操作：

- 在正常负载下查看 `practice_submissions_today` 与 `practice_judging` Prometheus 仪表。
- 从管理控制台的 `日常练习` 配置限制；源码保留设置由 API 清理 runner 强制执行。
- 超过保留窗口的源码会在提交终态后标记为过期。不要将过期源码视为清理失败；检查 `objectCleanup.failed` 与 `object_storage_cleanup_failed` 寻找真正的重试问题。

准备 Resolver：

```text
生成 freeze 与最终快照
运行预览
复核预期最终排名
冻结正式 Resolver run
```

准备颁奖：

```text
选择最终记分板来源
生成获奖者
复核冲突与明星队伍设置
冻结获奖名单
导出 CSV 或 Excel
```

## 事件记录

每次事件记录：

- 时间。
- 受影响的服务。
- 症状。
- 操作员。
- 采取的行动。
- 验证结果。
- 需要的后续跟进。

## 另见

- [快速开始](quickstart.md) — 端到端比赛部署。
- [备份与恢复](backup-restore.md) — 强制备份点。
- [故障排查](troubleshooting.md) — 症状检查。
