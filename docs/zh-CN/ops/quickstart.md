# 快速开始

本指南让全新的 ProjectBalloon 部署为正式比赛做好准备。它假设 `docs/architecture/deployment-topology.md` 中描述的标准拓扑；单主机彩排可以使用 `all` 角色。详细安装、运维与恢复流程见 `docs/ops/install.md`、`docs/ops/ops.md` 与 `docs/ops/disaster-recovery.md`。

## 1. 准备主机

开始前从可信介质安装主机先决条件：

- systemd、GNU coreutils（`tar`、`gzip`、`sha256sum`）。
- PostgreSQL、Redis、RabbitMQ 与 RustFS（或其他 S3 兼容服务）。
- 用于捆绑前端配置的 Nginx。
- 运行备份的主机需要 `postgresql-client` 与 AWS CLI v2。
- 启用打印时需要 CUPS 包与已配置的打印机。
- Judge 主机需要 Docker Engine 或 Podman（生产使用 rootless Podman 与 `runsc`，见 `docs/architecture/ADR-001-production-judge-sandbox.md`）。

提供外部服务，并创建数据库、队列、对象存储桶与凭据。安装程序不会创建它们。

## 2. 安装发布

将发布归档与匹配的 Judge Runtime 镜像归档复制到每台目标主机，解压并安装相关角色：

```text
# app/gateway 主机
sudo ./install.sh --role api --no-start

# judge 主机
sudo ./install.sh --role worker --skip-nginx --no-start \
  --container-group docker --judge-images ../judge-images
```

首次运行创建 `/etc/project-balloon/project-balloon.env` 并退出。填写外部服务 URL 与机密（见 `docs/ops/configuration.md`），然后再次运行安装程序以导入镜像并启动服务：

```text
sudoedit /etc/project-balloon/project-balloon.env
sudo ./install.sh
```

## 3. 引导第一个管理员

一旦 API 能访问 PostgreSQL，引导第一个超级管理员：

```text
sudoedit /etc/project-balloon/bootstrap-admin.env
sudo sh -c 'set -a; . /etc/project-balloon/bootstrap-admin.env; set +a; exec /opt/project-balloon/bin/bootstrap-admin'
```

成功后立即移除或轮换 bootstrap 密码。

## 4. 验证健康

从网关主机检查服务状态与健康：

```text
sudo systemctl status project-balloon-api project-balloon-judge-worker
curl --fail http://127.0.0.1:8080/livez
curl --fail http://127.0.0.1:8080/api/health
```

仅当 PostgreSQL（以及启用实时扇出时的 Redis）就绪时，`/api/health` 返回 `200` 且 `status: up`。完整的赛前健康检查清单见 `docs/ops/ops.md`。

## 5. 准备比赛

- 导入队伍并生成账户。
- 创建比赛并配置日程（开始、封榜、结束）。
- 创建题目，上传题面、附件与测试数据。
- 配置气球颜色与打印机。
- 运行试机赛与压测套件（`docs/ops/pressure-test.md`）。
- 做一次备份，然后冻结比赛配置。

## 6. 运行比赛

- 监控健康、Judge 队列深度、Worker 数量与记分板新鲜度。
- 从工作台处理 Clarification、气球与打印请求。
- 比赛结束后，排空 Judge 队列、生成最终榜与 Resolver 快照，然后运行正式 Resolver 与颁奖。

## 7. 备份与归档

在每个强制备份点做备份（`docs/ops/backup-restore.md`），导出成绩与提交，并归档比赛。
