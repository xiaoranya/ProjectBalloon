# ADR-001：生产判题沙箱运行时

## 状态

已接受，用于生产二进制部署。

## 决策

生产环境 Judge Worker 以 uid/gid `10001` 运行，并使用由同一专用主机账户拥有的 rootless Podman 服务的 Docker 兼容 API。Podman 必须使用 `runsc`（gVisor）OCI 运行时启动提交容器。Worker 绝不挂载 `/var/run/docker.sock`，绝不以特权模式运行，也不具有任何 root 等价的容器引擎访问权限。

Worker 缓存是挂载在 Worker 容器内完全相同绝对路径的主机目录。这是因为 OCI 守护进程在主机命名空间中解析 `/work` 绑定源。命名卷不适用于该架构。

`deploy/compose/rust-app.docker-compose.yml` 中的 sibling Docker 仍作为开发和单机预演例外保留。它不是正式比赛部署配置。

## 主机准备

1. 创建 uid/gid 为 `10001`、无交互登录的专用系统账户。
2. 从离线、带校验和的软件包安装 Podman 与 gVisor（`runsc`）。
3. 为该账户配置 rootless Podman，并确认 `runsc` 出现在可用 OCI 运行时列表中。
4. 将 `JUDGE_CACHE_DIR` 设置为绝对主机路径，以属主 `10001:10001` 创建，权限为 `0700`。
5. 启动专用的 rootless Podman API 服务，并设置 `XCPC_SANDBOX_SOCKET=/run/xcpc-judge/podman.sock`。
6. 将四个运行时镜像（C、C++、Java、Python）加载到 rootless Podman 镜像存储中。

若 socket、运行时镜像、缓存目录、S3 桶或 RabbitMQ 不可用，预检必须失败。在 Docker 标签的沙箱测试针对该确切配置通过之前，发布候选版本不具备比赛就绪条件。

## 安全属性

- 提交容器无网络、根文件系统只读、`no-new-privileges`、固定非 root 用户，并具有 CPU/内存/PID/输出限制。
- gVisor 在不可信提交周围提供用户态内核边界。
- 即使 Worker 或 Podman API 被攻破，影响也仅限于专用 rootless 账户和隔离的 Judge 主机，而不是主机 root 或数据区。
- 正式比赛中 Judge 主机与 PostgreSQL、Redis、RabbitMQ 和 RustFS 主机保持分离。
