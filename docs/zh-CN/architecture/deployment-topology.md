# 部署拓扑

本文档定义标准离线局域网部署形态。

## 主机角色

| 主机 | 服务 | 备注 |
|---|---|---|
| `gateway-01` | Nginx | 单一局域网入口；初期可由手动故障切换备份 |
| `app-01` | Web 静态文件、API 后端 | 主 API 节点 |
| `app-02` | API 后端、Judge Scheduler | 第二 API 节点与调度主机 |
| `data-01` | PostgreSQL、Redis、RabbitMQ、RustFS | 有状态服务；与 Judge Worker 分离 |
| `judge-01` 至 `judge-N` | Judge Worker | CPU 密集且隔离的执行主机 |
| `backup-01` | 备份存储、镜像备份、备用服务 | 存储备份与二进制包副本 |

## 网络分区

建议分区：

- 公共局域网访问：参赛者、管理员、Screen、Live 制作机。
- 应用区：网关与应用主机。
- 数据区：PostgreSQL、Redis、RabbitMQ、RustFS。
- Judge 区：Judge Worker 与沙箱运行时。
- 运维区：备份与监控访问。

分区之间只应暴露必要端口。数据库与队列端口不应被参赛者机器访问。

## 服务放置

生产目标应避免将 Judge Worker 与数据库同置。Judge 工作负载对 CPU、内存、进程与磁盘要求高，并且安全风险最高。生产应用进程来自二进制包；每个外部组件都由部署者提供与运维。仓库中的 Compose 应用栈是单主机彩排拓扑，并且有意挂载 Docker socket。

可接受的开发或彩排部署可以使用更少的机器，但生产文档与脚本应保持角色分离清晰。

## 二进制发布包映射

源码仓库目录与发布包的映射如下：

| 源码 | 二进制包输出 |
|---|---|
| Rust 发布二进制 | `bin/` |
| Vue 生产构建 | `web/` |
| systemd 单元与环境模板 | `systemd/`、`config/` |
| Nginx 模板 | `nginx/` |
| 备份脚本 | `scripts/backup/` |
| 部署脚本库 | `scripts/lib/` |

Judge Runtime 镜像不再捆绑。它们作为独立归档 `project-balloon-<version>-<target>-judge-images.tar.gz` 发布，并通过 `install.sh --judge-images` 在 Judge 主机上导入。

生成的发布包形态：

```text
project-balloon-vX.Y.Z-linux-amd64/
  bin/
  web/
  systemd/
  config/
  nginx/
  scripts/backup/
  docs/
  install.sh
  PACKAGE-SHA256SUMS
```

单独的 Judge Runtime 镜像归档形态：

```text
project-balloon-vX.Y.Z-linux-amd64-judge-images.tar.gz
  judge-images/
    judge-runtime-*.tar
    SHA256SUMS
```

发布工作流还构建 `linux-arm64`、`macos-x86_64`、`macos-arm64` 与 `windows-x86_64` 二进制归档。Linux arm64 采用相同的部署包形态，Judge Runtime 镜像归档同时为 Linux amd64 与 Linux arm64 构建。macOS 与 Windows 获得可移植二进制包，不包含 Linux 安装程序或 Judge Runtime 归档。目前仅 Linux x86_64 经过端到端测试；在目标主机上的运行时与安装流程得到验证之前，其他目标只是构建 / 打包输出。

## 开发与彩排 Compose

仓库保留三个可选的 Compose 项目：

- `rust-app.docker-compose.yml`（API、Worker 与 web）
- `data.docker-compose.yml`
- `../observability/compose.yml`

它们仅用于本地开发、集成测试与单主机彩排。它们不安装或管理正式部署。用于彩排时，镜像标签必须保持固定；不允许使用 `latest`。

## 配置规则

- 只提交 `.env.example` 文件与非机密模板。
- 真实 `.env` 文件在部署期间生成或复制，不得提交。
- 服务密码、token、RustFS 密钥、数据库凭据与 live token 都是机密。
- 二进制服务与 Nginx 模板位于 `deploy/binary/` 下。
- 仓库可选的可观测性示例位于 `deploy/observability/` 下；生产服务配置仍由部署者负责。

## 二进制部署流程

```text
将二进制发布归档与匹配的 judge-images 归档复制到目标主机
  -> 安装外部 PostgreSQL、Redis、RabbitMQ 与 RustFS
  -> 在 app/gateway 主机运行 install.sh --role api --no-start
  -> 在 Judge 主机安装 Docker/Podman
  -> 在 Judge 主机运行 install.sh --role worker --skip-nginx --no-start --judge-images ../judge-images
  -> 填写 /etc/project-balloon/project-balloon.env
  -> 再次运行相应角色以导入镜像并启动服务
  -> 引导第一个管理员
  -> 运行健康检查并验证备份
```

二进制模型将所有外部与有状态服务置于应用包之外。API 与 Worker 进程可以在不同主机上运行，而 Judge Worker 保留对其用户提供的本地 Docker/Podman 沙箱 socket 与导入的运行时镜像的访问。

该流程必须可重复。脚本应快速失败，并清晰打印失败的步骤。
