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

输出包含 `project-balloon-<UTC 时间戳>` 目录。一次运行在临时目录中构建，并且仅在所有步骤成功后才重命名。它包含：

```text
postgres/database.sql.gz
objects/<bucket>/...
deploy-config.tar.gz
manifest.txt
SHA256SUMS
```

脚本加载配置的环境而不将其作为 shell 代码求值。在二进制安装中，`PROJECT_BALLOON_DATABASE_MODE=direct` 使用 `DATABASE_URL` 与主机 `pg_dump` 命令，因此不需要 Docker。PostgreSQL 使用 `--clean --if-exists --no-owner` 转储；S3 API 返回的每个 RustFS 桶都会被复制。运行时机密从配置归档中排除。

当对象存储无法从主机在 `http://127.0.0.1:9000` 访问时，在 `/etc/project-balloon/project-balloon.env` 中设置 `BACKUP_OBJECT_STORAGE_ENDPOINT`。

必需工具为 gzip、sha256sum、tar、PostgreSQL 客户端工具（`pg_dump` 与 `psql`）与 AWS CLI v2。遗留 Compose 模式额外需要 Docker Compose，并读取 `deploy/compose/.env.rust`。

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
