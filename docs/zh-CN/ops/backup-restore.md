---
title: 备份与恢复
description: 介绍如何创建备份、恢复部署、强制备份点与恢复后验证。
---

# 备份与恢复

PostgreSQL 与 RustFS 是权威备份目标。Redis 可重建；计划中的最终备份前应排空 RabbitMQ。

## 创建备份

```text
sudo /opt/project-balloon/scripts/backup/backup.sh /var/backups/project-balloon
```

它包含：

```text
postgres/database.sql.gz
objects/<bucket>/...
config/project-balloon.env.masked
RESTORE-CHECKLIST.md
deploy-config.tar.gz
manifest.txt
SHA256SUMS
```

脚本加载配置的环境而不将其作为 shell 代码求值。在二进制安装中，`PROJECT_BALLOON_DATABASE_MODE=direct` 使用 `DATABASE_URL` 与主机 `pg_dump` 命令，因此不需要 Docker。PostgreSQL 使用 `--clean --if-exists --no-owner` 转储；S3 API 返回的每个 RustFS 桶都会被复制。运行时机密从配置归档中排除。

`config/project-balloon.env.masked` 是部署环境的脱敏副本：变量名以 `KEY`、`SECRET`、`PASSWORD`、`TOKEN`、`URL`、`DSN` 结尾的值会被替换为 `CHANGE_ME_redacted_from_backup`，使归档可以离机保存而不泄露在线凭据，同时仍记录部署设置的全部变量。恢复时用该副本加运维保密库重建 env 文件。`RESTORE-CHECKLIST.md` 记录归档的逐步恢复顺序。

当对象存储无法从主机在 `http://127.0.0.1:9000` 访问时，在 `/etc/project-balloon/project-balloon.env` 中设置 `BACKUP_OBJECT_STORAGE_ENDPOINT`。

必需工具为 gzip、sha256sum、tar、PostgreSQL 客户端工具（`pg_dump` 与 `psql`）与 AWS CLI v2。遗留 Compose 模式额外需要 Docker Compose，并读取 `deploy/compose/.env.rust`。

## 备份自动化

二进制安装程序在每次 API 安装（含 `--role api`）时渲染并启用 `project-balloon-backup.timer`。定时器在每天本地时间 03:15 触发 `project-balloon-backup.service` 一次性任务，带 15 分钟抖动；`Persistent=true` 在停机后补跑错过的任务。该服务运行 `backup.sh /var/backups/project-balloon`，随后以 `ExecStartPost` 运行 `check-freshness.sh`，输出路径漂移或备份时间戳异常会使单元失败而非静默通过。安装时用 `--backup-dir PATH`（或 `PROJECT_BALLOON_BACKUP_DIR` 环境变量）覆盖输出目录；修改后需重跑 `install.sh` 重新渲染单元。

用以下命令管理计划：

```text
systemctl list-timers project-balloon-backup.timer
systemctl status project-balloon-backup.service   # 最近一次运行结果
```

要把服务失败接入告警，添加 drop-in：

```text
mkdir -p /etc/systemd/system/project-balloon-backup.service.d
printf '[Unit]\nOnFailure=notify-admin@%%n.service\n' \
  > /etc/systemd/system/project-balloon-backup.service.d/on-failure.conf
systemctl daemon-reload
```

在没有 systemd 定时器的主机上——或机构标准是 cron 时——使用等价的 crontab 条目（`/etc/cron.d/project-balloon-backup`）：

```cron
SHELL=/bin/bash
# 每天 03:15 带抖动备份，09:00 做时效告警。
11 3 * * * root /opt/project-balloon/scripts/backup/backup.sh /var/backups/project-balloon >>/var/log/project-balloon-backup.log 2>&1
7 9 * * * root /opt/project-balloon/scripts/backup/check-freshness.sh /var/backups/project-balloon || logger -p daemon.alert -t project-balloon-backup "ProjectBalloon backup is stale"
```

定时器与 crontab 两者只启用其一。`check-freshness.sh` 在目录下最新备份的年龄超过 `BACKUP_MAX_AGE_HOURS`（默认 26——一次错过的日备加余量）或不存在已完成的备份时以非零码退出；把监控指向其退出码或读取 `daemon.alert` syslog 行。

## 恢复备份

恢复是刻意的破坏性操作。它替换配置的 PostgreSQL 对象，并用 `aws s3 sync --delete` 镜像每个备份的桶。

```text
PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA \
  sudo -E /opt/project-balloon/scripts/backup/restore.sh \
  /var/backups/project-balloon/project-balloon-<timestamp>
```

在修改状态之前，脚本验证每个校验和、备份格式与配置的数据库名。二进制模式停止 API 与 Judge Worker，通过 `psql` 恢复 PostgreSQL，然后恢复 RustFS。它故意不自动重启应用服务。遗留 Compose 模式停止 `monitor` 与 `app`、保持 `data` 运行，并通过 PostgreSQL 容器恢复 PostgreSQL。

## 必需备份点

- 账户与队伍导入后。
- 题目 / 测试数据冻结后。
- 正式比赛开始前立即。
- 比赛期间定期。
- Judge 队列排空后。
- Resolver、颁奖与最终导出后。

## 恢复后验证

启动二进制服务并验证应用健康：

```text
sudo systemctl start project-balloon-api project-balloon-judge-worker
curl --fail http://127.0.0.1:8080/livez
curl --fail http://127.0.0.1:8080/api/health
```

使用部署者自己的流程验证 PostgreSQL、Redis、RabbitMQ、对象存储、沙箱、代理、打印、备份与可观测性服务。然后验证比赛生命周期、账户、题目与测试数据哈希、提交 / 判定计数、公共 / 管理员记分板、Resolver 快照、颁奖、打印、气球任务与清理 / 导出积压。在任何实时比赛恢复之前，单独保留失败状态数据与操作员时间线。

## 另见

- [运维](ops.md) — 常规现场运维与监控。
- [灾难恢复](disaster-recovery.md) — 故障响应流程。
