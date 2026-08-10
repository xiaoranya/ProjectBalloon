# 本地开发

本文档定义 Rust 工作区的本地开发。除非另有说明，命令从仓库根目录运行。生产安装使用 `docs/ops/install.md` 中描述的二进制包；本节中的 Compose 文件只是开发与彩排便利设施。

## 必需工具链

- `rust-toolchain.toml` 选择的稳定 Rust 工具链，版本不低于工作区 `rust-version`。
- Cargo 与仓库选择的 `cargo-nextest`、`cargo-sqlx`、审计与许可证检查工具。
- 前端需要 Node.js LTS 与 npm。
- 本地基础设施与 Judge 沙箱开发需要带 Docker Compose 的 Docker Engine。
- 通过 Compose 提供 PostgreSQL 16、Redis 7、RabbitMQ 3 与 RustFS。

不要为 Rust 后端要求全局安装 Java 或 Gradle 工具链。Java 仅保留安装在参赛者 Java 运行时镜像内。

## 本地服务模型

直接运行应用代码以获得快速重建，并通过 Docker Compose 运行基础设施。

```text
frontend/web          Vite 开发服务器
apps/api              Cargo 二进制；包含 scheduler/outbox dispatcher
apps/judge-worker     Cargo 二进制或开发容器
data services         本地 Compose 项目
```

本地默认值绝不能复用为正式比赛机密。生产 PostgreSQL、Redis、RabbitMQ、对象存储、沙箱、代理、打印与可观测性服务由部署者提供与维护。

## Docker 集成套件

运行完整的隔离 Docker 后端套件：

```text
scripts/test/docker-integration.sh
```

该脚本启动唯一命名的 PostgreSQL、Redis、RabbitMQ 与 RustFS 容器，创建隔离桶，声明已评审的 Judge 拓扑，按队列安全顺序运行所有被忽略的 API/Worker 测试，并在成功或失败时移除其容器。固定的 Judge 运行时镜像必须已存在。

同一脚本由计划 / 手动 `.github/workflows/docker-integration.yml` 工作流执行。常规 PR 工作流有意只运行无依赖测试；它不会静默声称覆盖这些环境后端场景。

## 开发数据

种子数据应包含：

- 管理员用户。
- 比赛管理员用户。
- 服务认证需要的 Judge 用户（如需要）。
- 示例比赛。
- 示例队伍。
- 示例题目。
- 小型测试数据包。

种子数据绝不能包含真实比赛凭据。

## 本地工作流

引导后推荐的工作流：

```text
启动本地数据服务
  -> 运行数据库迁移
  -> 启动后端 API
  -> 启动 judge worker
  -> 启动前端开发服务器
  -> 提交示例解答
  -> 验证记分板更新
```

创建被忽略的本地 Compose 环境，替换每个 `CHANGE_ME` 值，并只启动开发数据服务：

```bash
cp deploy/compose/.env.rust.example deploy/compose/.env.local
$EDITOR deploy/compose/.env.local
docker compose --env-file deploy/compose/.env.local \
  -f deploy/compose/data.docker-compose.yml up -d
```

将应用环境模板复制到被忽略的根目录 `.env`，将服务 URL 改为其主机发布的 `127.0.0.1` 端口，然后将其导出到 shell。API 在启动时创建其配置的对象存储桶。直接运行 Rust 进程：

```bash
cp .env.example .env
$EDITOR .env
set -a
. ./.env
set +a
PROJECT_BALLOON_API_BIND=127.0.0.1:18080 cargo run -p project-balloon-api

# 在另一个 shell 中，启动 Worker 前按上述方式导出 .env：
cargo run -p project-balloon-judge-worker
```

Worker 命令在消费前执行严格的 RabbitMQ、RustFS、沙箱 socket 与运行时镜像预检。对于文档化的本地 Docker profile，将 `JUDGE_CACHE_DIR` 设置为可写的绝对主机路径，并使用 uid/gid `1000:1000`。正式部署必须改用 rootless Podman socket、uid/gid `10001:10001`，以及 ADR-001 要求的 `XCPC_SANDBOX_RUNTIME=runsc`。

PostgreSQL 集成测试使用 SQLx 临时测试数据库，并被无依赖的默认测试运行忽略。将 `DATABASE_URL` 指向一个其配置用户可创建数据库的 PostgreSQL 服务器，然后运行：

```text
cargo test -p project-balloon-api --test bootstrap_postgres -- --ignored
```

运行每个被忽略的 PostgreSQL 场景（包括题目描述与比赛配置冻结）：

```text
cargo test -p project-balloon-api --lib -- --ignored
```

SQLx 创建隔离数据库、应用 `migrations/`、运行测试并在之后移除数据库。绝不要将此命令指向无法安全创建和删除临时测试数据库的凭据。

API 默认运行内嵌的 SQLx 迁移。仅当迁移由单独的已评审部署步骤管理时，才设置 `PROJECT_BALLOON_RUN_MIGRATIONS=false`。

重要的 API 环境变量：

| 变量 | 开发默认值 | 用途 |
|---|---|---|
| `DATABASE_URL` | 未设置 | PostgreSQL 连接；设置带本地凭据的显式 URL |
| `PROJECT_BALLOON_API_BIND` | `127.0.0.1:8080` | API 监听 socket |
| `PROJECT_BALLOON_DEPLOYMENT_MODE` | `standard` | 设为 `competition` 以启用非重叠日程与 IP 绑定工作站配对，同时禁用日常功能 |
| `PROJECT_BALLOON_DATABASE_MAX_CONNECTIONS` | `20` | PostgreSQL 连接池上限 |
| `PROJECT_BALLOON_DATABASE_ACQUIRE_TIMEOUT_SECONDS` | `5` | 连接池背压超时 |
| `PROJECT_BALLOON_READINESS_TIMEOUT_MILLISECONDS` | `1000` | 就绪探针超时 |
| `PROJECT_BALLOON_RUN_MIGRATIONS` | `true` | 启动时运行内嵌 SQLx 迁移 |
| `PROJECT_BALLOON_SESSION_TTL_SECONDS` | `43200` | 浏览器会话生命周期 |
| `PROJECT_BALLOON_SECURE_COOKIES` | `false` | 添加 Cookie `Secure` 属性；生产环境必需 |
| `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET` | `false` | 允许已检入的开发 CSRF secret；仅本地开发设为 `true` |
| `PROJECT_BALLOON_CSRF_SECRET` | 仅开发值 | CSRF token 的 HMAC secret；除非设置上述标志，否则启动拒绝开发值；与安全 Cookie 一起使用时无条件拒绝 |
| `PROJECT_BALLOON_REALTIME_DISPATCHER_ENABLED` | `true` | 认领并发布持久的实时 outbox 行 |
| `PROJECT_BALLOON_REALTIME_CHANNEL_CAPACITY` | `1024` | 每进程 SSE 广播缓冲区 |
| `PROJECT_BALLOON_REALTIME_POLL_MILLISECONDS` | `250` | Outbox 轮询间隔 |
| `PROJECT_BALLOON_REALTIME_LEASE_SECONDS` | `30` | 放弃认领的恢复租约 |
| `PROJECT_BALLOON_REALTIME_RETRY_BASE_MILLISECONDS` | `1000` | 初始投递失败退避 |
| `PROJECT_BALLOON_REALTIME_BATCH_SIZE` | `100` | 每次轮询最多认领的行数 |
| `PROJECT_BALLOON_REALTIME_MAX_ATTEMPTS` | `8` | 操作员介入前的投递尝试次数 |
| `PROJECT_BALLOON_REALTIME_REDIS_ENABLED` | `false` | 通过 Redis 发布与订阅 SSE 扇出 |
| `REDIS_URL` | 未设置 | 启用时带 ACL 凭据的 Redis 连接 URL |
| `PROJECT_BALLOON_REALTIME_REDIS_CHANNEL` | `xcpc:realtime:events` | 版本 1 实时 Pub/Sub 通道 |
| `PROJECT_BALLOON_REALTIME_REDIS_RECONNECT_MILLISECONDS` | `1000` | 订阅者初始重连延迟；指数封顶 30 秒 |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_ENABLED` | `false` | 在 Redis 中缓存渲染的记分板变体，同时保持 PostgreSQL 权威 |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_TTL_SECONDS` | `30` | 修订版本范围记分板缓存条目的过期时间 |
| `PROJECT_BALLOON_SCOREBOARD_CACHE_TIMEOUT_MILLISECONDS` | `200` | PostgreSQL 回退前的最大 Redis 连接 / 读 / 写等待 |

Practice 默认值存储在 PostgreSQL 中，可由超级管理员在 `/admin/practice` 修改：每日提交 `200`、并发判题 `3`、源码保留 `365` 天。API 通过 Prometheus 指标暴露活动中的 practice 工作负载；源码删除对 pending 或 judging 提交禁用，并通过对象清理 runner 重试。

探针行为：

- `GET /livez` 验证 Rust 进程与 HTTP 运行时存活，不访问依赖。
- `GET /api/health` 验证 PostgreSQL 就绪，并在启用实时 Redis 扇出时验证 Redis 连通性。必需依赖不可用时返回 HTTP 503 且 `status: down`。

浏览器认证工作流：

1. 启用凭据调用 `GET /api/auth/csrf`。
2. 将响应 token 复制到 `X-XSRF-TOKEN`。
3. 将该请求头、`XSRF-TOKEN` Cookie 与 JSON 凭据发送到 `POST /api/auth/login`。
4. 对 `POST`、`PUT`、`PATCH` 与 `DELETE` 请求继续发送 CSRF 请求头。会话本身保存在 `HttpOnly` 的 `PB_SESSION` Cookie 中。

开发 CSRF secret 是公开值，因此除非设置 `PROJECT_BALLOON_ALLOW_DEV_CSRF_SECRET=true`，启动会拒绝它，并且它与安全 Cookie 一起仍不可用。在启用 `PROJECT_BALLOON_SECURE_COOKIES=true` 之前设置部署特定 secret。

本地开发可以保持 Redis 扇出禁用，并使用进程内 Tokio 广播通道。多实例部署在每个 API 副本上启用 Redis。分发器也可以运行在多个副本上：PostgreSQL `SKIP LOCKED` 防止重复认领，而 Redis 将事件投递到每个副本的本地 SSE hub。轮询仍是浏览器的状态恢复路径；SSE 事件是失效提示，而非权威业务记录。

单独启动前端：

```text
cd frontend/web
npm ci
npm run dev
```

Vite 默认监听 `http://127.0.0.1:5173` 并将 `/api` 代理到 `http://127.0.0.1:18080`。在另一端口直接运行 API 时覆盖后端目标：

```text
XCPC_API_PROXY_TARGET=http://127.0.0.1:8080 npm run dev
```

停止本地依赖：

```bash
docker compose --env-file deploy/compose/.env.local \
  -f deploy/compose/data.docker-compose.yml down
```

要故意重置所有本地依赖数据，移除 Compose 卷：

```bash
docker compose --env-file deploy/compose/.env.local \
  -f deploy/compose/data.docker-compose.yml down --volumes
```

## 配置规则

- 只提交 `.env.example` 文件。
- 将真实 `.env` 文件保留在 Git 之外。
- 在本地 Compose 中使用固定服务名，使应用配置稳定。
- 通过 Vite `/api` 代理保持浏览器请求同源；不要添加第二个仅前端 API base URL。
- 保持本地测试数据小而确定。

## 有用检查

在打开 pull request 或合并功能工作之前，运行：

```text
cargo fmt --all --check
cargo check-all
cargo lint
cargo test-all
cargo deny check

cd frontend/web
npm run typecheck
npm test
```

引入编译期 SQLx 查询宏时，还要运行 `cargo sqlx prepare --check --workspace` 并提交生成的 `.sqlx` 元数据。当前工作区使用运行时 SQLx 查询，因此暂不需要元数据目录。

需要 PostgreSQL、RabbitMQ、Redis、RustFS 或开发沙箱的集成测试必须声明该需求，并在依赖不可用时以可操作的消息失败。
