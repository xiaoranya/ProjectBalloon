---
title: 配置参考
description: 二进制部署中 API 与 Judge Worker 的环境变量：数据库、对象存储、实时、榜单缓存、打印、判题 Worker 与沙箱。
---

# 配置参考

本文档是二进制部署的配置参考。安装程序创建 `/etc/project-balloon/project-balloon.env`；API 与 Judge Worker 通过其 systemd 单元读取它。本地开发使用从 `.env.example` 复制的根目录 `.env` 文件（见 [本地开发](../dev/local-development.md)）。

## 配置规则

- 只提交 `.env.example` 与非机密模板。真实 `.env` 文件是部署特定的，绝不能提交。
- 正式比赛前替换每个开发默认值。
- 为 PostgreSQL、Redis、RabbitMQ、RustFS 与应用使用不同的凭据。
- 不记录原始会话 token、CSRF token、密码或 live token。
- 更改环境文件后，重启受影响的服务：

```text
sudo systemctl restart project-balloon-api project-balloon-judge-worker
```

## API 与部署

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_API_BIND` | `127.0.0.1:8080` | API 监听 socket；反向代理连接到这里 |
| `PROJECT_BALLOON_DEPLOYMENT_MODE` | `standard` | `competition` 启用非重叠日程与 IP 绑定工作站配对，并禁用日常功能 |
| `PROJECT_BALLOON_TRUSTED_PROXY_CIDRS` | `127.0.0.1/32,::1/128` | API 信任其 `X-Forwarded-*` 请求头的 CIDR |
| `RUST_LOG` | `info` | API 与 Worker 的结构化日志级别 |
| `XCPC_API_PROXY_TARGET` | `http://127.0.0.1:18080` | 前端开发服务器代理目标（仅 Vite） |

## 数据库

| 变量 | 默认值 | 用途 |
|---|---|---|
| `DATABASE_URL` | 未设置 | 带部署凭据的 PostgreSQL 连接 URL；没有带凭据的默认值 |
| `PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS` | `20` | PostgreSQL 连接池上限 |
| `PROJECT_BALLOON_DATABASE_ACQUIRE_TIMEOUT_SECONDS` | `5` | 连接池背压超时 |
| `PROJECT_BALLOON_READINESS_TIMEOUT_MILLISECONDS` | `1000` | 就绪探针超时 |
| `PROJECT_BALLOON_RUN_MIGRATIONS` | `true` | 启动时运行内嵌 SQLx 迁移；仅当迁移由单独的已评审步骤管理时禁用 |
| `PROJECT_BALLOON_BOOTSTRAP_ADMIN_*` | 开发值 | `bootstrap-admin` 使用的首个管理员凭据 |

## 对象存储（RustFS）

对象存储对文件操作（附件、测试数据、提交源码、导出、打印 PDF）是强制的。

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_OBJECT_STORAGE_ENABLED` | `false` | 启用 S3 兼容适配器 |
| `PROJECT_BALLOON_OBJECT_STORAGE_ENDPOINT` | `http://127.0.0.1:9000` | RustFS/S3 端点 |
| `PROJECT_BALLOON_OBJECT_STORAGE_REGION` | `us-east-1` | S3 区域 |
| `PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY` / `SECRET_KEY` | 开发值 | 静态部署凭据 |
| `PROJECT_BALLOON_OBJECT_STORAGE_PROBLEM_BUCKET` | `xcpc-problems` | 题目附件与测试数据 |
| `PROJECT_BALLOON_OBJECT_STORAGE_SOURCE_BUCKET` | `xcpc-sources` | 提交源文件 |
| `PROJECT_BALLOON_OBJECT_STORAGE_FORCE_PATH_STYLE` | `true` | 面向 RustFS 的 path-style 桶寻址 |
| `PROJECT_BALLOON_OBJECT_STORAGE_REQUEST_TIMEOUT_MILLISECONDS` | `5000` | 每次 S3 请求超时 |
| `PROJECT_BALLOON_OBJECT_STORAGE_UPLOAD_TIMEOUT_MILLISECONDS` | `300000` | 上传（PUT）的 S3 请求超时；上传体积大，与 5 秒请求超时分开设置 |

API 在启动时创建其配置的桶。桶名属于部署配置，绝不通过公共 API 返回。

## RabbitMQ 与 Judge 分发

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_RABBITMQ_ENABLED` | `false` | 启用通过 RabbitMQ 的持久 Judge 任务分发 |
| `PROJECT_BALLOON_RABBITMQ_URL` | 未设置 | 带凭据的 AMQP/AMQPS URL |
| `PROJECT_BALLOON_RABBITMQ_REQUEST_TIMEOUT_MILLISECONDS` | `5000` | broker 操作超时 |
| `PROJECT_BALLOON_JUDGE_DISPATCH_POLL_MILLISECONDS` | `500` | 提交分发器的 Outbox 轮询间隔 |
| `PROJECT_BALLOON_JUDGE_DISPATCH_LEASE_SECONDS` | `30` | Outbox 认领租约 |
| `PROJECT_BALLOON_JUDGE_DISPATCH_RETRY_BASE_MILLISECONDS` | `1000` | 初始重试退避 |
| `PROJECT_BALLOON_JUDGE_DISPATCH_BATCH_SIZE` | `50` | 每次轮询最多认领的行数 |
| `PROJECT_BALLOON_JUDGE_DISPATCH_MAX_ATTEMPTS` | `8` | 操作员介入前的尝试次数 |
| `PROJECT_BALLOON_JUDGE_RESULT_PREFETCH` | `32` | 结果消费者 prefetch |
| `PROJECT_BALLOON_JUDGE_RESULT_RECONNECT_MILLISECONDS` | `1000` | 消费者重连延迟 |

## 浏览器会话与 CSRF

生产要求在 API 前终止 TLS、使用 `Secure` Cookie 与独立生成的 CSRF secret。

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_SESSION_TTL_SECONDS` | `43200` | 浏览器会话生命周期 |
| `PROJECT_BALLOON_SECURE_COOKIES` | `false` | 添加 `Secure` Cookie 属性；生产必须为 `true` |
| `PROJECT_BALLOON_CSRF_SECRET` | 仅开发值 | CSRF token 的 HMAC secret；除非 `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET=true`，公开开发值被拒绝，且与安全 Cookie 一起无条件拒绝 |
| `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET` | `false` | 仅本地开发的逃生舱 |

## Realtime Outbox 与 SSE

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED` | `true` | 认领并发布持久 outbox 行 |
| `PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY` | `1024` | 每进程 SSE 广播缓冲区 |
| `PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS` | `250` | Outbox 轮询间隔 |
| `PROJECT_BALLOON_REALTIME_LEASE_SECONDS` | `30` | 放弃认领的恢复租约 |
| `PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS` | `1000` | 初始投递失败退避 |
| `PROJECT_BALLOON_REALTIME_BATCH_SIZE` | `100` | 每次轮询最多认领的行数 |
| `PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS` | `8` | 操作员介入前的投递尝试次数 |
| `PROJECT_BALLOON_REALTIME_REDIS_ENABLED` | `false` | 通过 Redis 发布 / 订阅 SSE 扇出；多实例部署在每个 API 副本上启用 |
| `REDIS_URL` | 未设置 | 带 ACL 凭据的 Redis 连接 URL |
| `PROJECT_BALLOON_REALTIME_REDIS_CHANNEL` | `xcpc:realtime:events` | 版本 1 实时 Pub/Sub 通道 |
| `PROJECT_BALLOON_REALTIME_REDIS_RECONNECT_MILLISECONDS` | `1000` | 订阅者重连延迟（指数，封顶 30 秒） |

## 记分板缓存

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED` | `false` | 在 Redis 中缓存渲染的记分板变体，同时保持 PostgreSQL 权威 |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS` | `30` | 修订版本范围缓存条目的过期时间 |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS` | `200` | PostgreSQL 回退前的最大 Redis 等待 |

## 对象清理

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_OBJECT_CLEANUP_POLL_MILLISECONDS` | `5000` | 清理 runner 轮询间隔 |
| `PROJECT_BALLOON_OBJECT_CLEANUP_LEASE_SECONDS` | `30` | 清理任务租约 |
| `PROJECT_BALLOON_OBJECT_CLEANUP_RETRY_BASE_MILLISECONDS` | `1000` | 初始重试退避 |
| `PROJECT_BALLOON_OBJECT_CLEANUP_BATCH_SIZE` | `50` | 每次迭代认领的任务数 |

## CUPS 打印

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_CUPS_ENABLED` | `false` | 启用 PDF 生成与 CUPS 投递 |
| `PROJECT_BALLOON_CUPS_PRINTER` | `xcpc` | 健康检查使用且 `lp` 使用的 CUPS 队列名 |
| `PROJECT_BALLOON_CUPS_COMMAND_TIMEOUT_MILLISECONDS` | `5000` | CUPS 命令超时 |

## Judge Worker

| 变量 | 默认值 | 用途 |
|---|---|---|
| `WORKER_ID` | `worker-local` | 心跳使用的稳定 Worker 实例身份 |
| `JUDGE_CACHE_DIR` | `/var/cache/judge` | 本地测试数据缓存目录 |
| `JUDGE_TASK_QUEUE` | `judge.tasks` | 消费的任务队列 |
| `JUDGE_TASK_PREFETCH` | `1` | 并行执行容量；优雅关闭排空进行中的工作 |
| `JUDGE_RECONNECT_MILLISECONDS` | `1000` | RabbitMQ 重连延迟 |
| `JUDGE_HEARTBEAT_INTERVAL_SECONDS` | `5` | 心跳发布间隔 |
| `JUDGE_REQUEST_TIMEOUT_MILLISECONDS` | `10000` | 存储 / 沙箱请求超时 |
| `JUDGE_MAX_ARTIFACT_BYTES` | `314572800` | 每个任务接受的最大产物大小 |
| `JUDGE_TESTDATA_CACHE_MAX_BYTES` | `8589934592` | 本地测试数据 zip 缓存的大小上限；超过上限时按最近最少使用（以 mtime 为准，缓存命中会刷新）淘汰并记录日志。`0` 表示不设上限 |

## 沙箱

| 变量 | 默认值 | 用途 |
|---|---|---|
| `XCPC_SANDBOX_SOCKET` | `/var/run/docker.sock` | Docker 或 rootless Podman socket 路径 |
| `XCPC_SANDBOX_RUNTIME` | 未设置 | 生产使用 `runsc`（ADR-001）；仅在文档化的本地 Docker profile 留空 |
| `XCPC_SANDBOX_USER` | `1000:1000` | 沙箱容器内的非 root UID/GID；生产使用 `10001:10001` |
| `JUDGE_C_IMAGE` | `judge-runtime-c:12.2.0` | C 运行时镜像标签 |
| `JUDGE_CPP_IMAGE` | `judge-runtime-cpp:12.2.0` | C++ 运行时镜像标签 |
| `JUDGE_JAVA_IMAGE` | `judge-runtime-java:21` | Java 21 运行时镜像标签 |
| `JUDGE_PYTHON_IMAGE` | `judge-runtime-python:3.12.13` | Python 3.12 运行时镜像标签 |

运行时镜像标签必须固定；绝不允许 `latest`。生产必须使用 rootless Podman socket、uid/gid `10001:10001` 与 `runsc`。

## 备份

| 变量 | 默认值 | 用途 |
|---|---|---|
| `PROJECT_BALLOON_DATABASE_MODE` | `direct` | `direct` 使用主机 PostgreSQL 客户端工具；`compose` 用于遗留单主机部署 |
| `BACKUP_OBJECT_STORAGE_ENDPOINT` | `http://127.0.0.1:9000` | 当备份主机无法在默认端点访问 RustFS 时覆盖 |

## 另见

- [安装](install.md) — 环境文件的创建位置。
- [故障排查](troubleshooting.md) — 引用这些变量的检查项。
- [本地开发](../dev/local-development.md) — 开发环境 `.env` 配置。
