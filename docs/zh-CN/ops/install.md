# 二进制安装

默认发布物是二进制包。它包含 API、Judge Worker、bootstrap CLI 与 Vue 静态文件。四个 Judge Runtime 镜像作为独立归档发布，并通过 `install.sh --judge-images` 导入。PostgreSQL、Redis、RabbitMQ、RustFS、Docker 或 Podman，以及可选的 CUPS/Nginx 服务是主机管理的先决条件。

## 发布平台状态

GitHub Actions 为 Linux amd64、Linux arm64、macOS Intel、macOS arm64 与 Windows x64 生成包。Linux 包是部署包；macOS 与 Windows 包是只包含二进制与前端内容的可移植构建包；两者都不包含 Judge Runtime 镜像归档。Judge Runtime 镜像归档分别针对 Linux amd64 与 Linux arm64 发布，必须在 Judge 主机上下载并导入。

| 包 | Rust 目标 | GitHub runner |
|---|---|---|
| `linux-amd64` | `x86_64-unknown-linux-gnu` | `ubuntu-24.04` |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` |
| `macos-x86_64` | `x86_64-apple-darwin` | `macos-15-intel` |
| `macos-arm64` | `aarch64-apple-darwin` | `macos-15` |
| `windows-x86_64` | `x86_64-pc-windows-msvc` | `windows-2022` |

只有 Linux x86_64 经过端到端测试。其他平台产物目前仅限于 runner 构建与包完整性检查。在目标主机完成测试之前，不要将它们视为运行时兼容或安装已验证的发布。

## 主机先决条件

安装前从可信介质或主机发行版安装：

- systemd；
- PostgreSQL、Redis、RabbitMQ 与 RustFS 或其他 S3 兼容服务；
- `tar`、`gzip`、`sha256sum` 与 GNU coreutils；
- 用于直接二进制模式备份与恢复的 `postgresql-client`（`pg_dump` 与 `psql`）；
- 用于 RustFS/S3 备份与恢复的 AWS CLI v2；
- 用于捆绑前端配置的 Nginx；
- 启用打印时需要的 `cups-client`、`cups-filters` 与已配置的 CUPS 打印机。

`api` 角色不需要 Docker 或 Podman。`worker` 与 `all` 角色需要 Docker Engine 或预配置的 rootful Podman 服务，且其 socket 对 Judge Worker 可访问。Rootless Podman 设置、其用户服务与 rootless 镜像存储仍是由沙箱 ADR 描述的主机准备步骤；安装程序不会自动创建该服务。

安装程序不创建数据库、队列、对象存储凭据或生产机密。它从 `--judge-images` 给定的目录（或捆绑时的 `judge-images/`）导入四个 Judge Runtime 镜像，并创建应用用户、目录、环境文件、systemd 单元与 Nginx 配置。

## 安装发布

解压已发布的二进制归档与匹配的 Judge Runtime 镜像归档，然后以 root 运行安装程序：

```text
tar -xzf project-balloon-<version>-<target>.tar.gz
tar -xzf project-balloon-<version>-<target>-judge-images.tar.gz
cd project-balloon-<version>-<target>
sudo ./install.sh --no-start --judge-images ../judge-images
```

对于分离拓扑，在每台主机上只安装相关角色：

```text
# app/gateway 主机
sudo ./install.sh --role api --no-start

# judge 主机
sudo ./install.sh --role worker --skip-nginx --no-start \
  --container-group docker --judge-images ../judge-images
```

两台主机都必须接收相同的外部服务配置，而每台 Judge 主机另外需要其本地沙箱 socket 与导入的运行时镜像。默认的 `all` 角色对于单主机彩排仍然方便。

首次运行会创建 `/etc/project-balloon/project-balloon.env` 并退出，以便复核外部服务 URL 与机密。编辑该文件，然后再次运行安装程序：

```text
sudoedit /etc/project-balloon/project-balloon.env
sudo ./install.sh
```

第二次运行在所选角色包含 Worker 时从 `--judge-images` 导入 Judge Runtime 镜像，安装或刷新相关 systemd 单元，在 API 角色启用 CUPS 时验证 CUPS，在可用时重新加载 Nginx，并启动所选服务。当 `PROJECT_BALLOON_RUN_MIGRATIONS=true` 时，API 运行内嵌的 SQLx 迁移。

应用安装在 `/opt/project-balloon` 下。服务用户为 `project-balloon-api` 与 `project-balloon-worker`；后者必须能访问 Docker/Podman socket。当主机布局需要时，可用安装程序选项覆盖前缀、配置目录或 socket 组。

一旦 API 能访问 PostgreSQL，引导第一个管理员：

```text
sudoedit /etc/project-balloon/bootstrap-admin.env
sudo sh -c 'set -a; . /etc/project-balloon/bootstrap-admin.env; set +a; exec /opt/project-balloon/bin/bootstrap-admin'
```

命令成功后移除或轮换 bootstrap 密码。

## 服务操作

```text
sudo systemctl status project-balloon-api project-balloon-judge-worker
sudo systemctl restart project-balloon-api project-balloon-judge-worker
sudo journalctl -u project-balloon-api -u project-balloon-judge-worker -f
curl --fail http://127.0.0.1:8080/livez
```

安装程序将前端配置写入使用 `conf.d` 发行版的 `/etc/nginx/conf.d/project-balloon.conf`，或写入 `sites-available`/`sites-enabled` 布局。在此配置之前放置 TLS 终止，并为生产环境保持 `PROJECT_BALLOON_SECURE_COOKIES=true`。

## 备份

安装程序将 `scripts/backup` 放在 `/opt/project-balloon/scripts/backup` 下。使用默认的 `PROJECT_BALLOON_DATABASE_MODE=direct` 时，脚本使用主机 PostgreSQL 客户端工具与 `DATABASE_URL`；它们不需要 Docker。当 RustFS 在默认端点不可达时设置 `BACKUP_OBJECT_STORAGE_ENDPOINT`，然后运行：

```text
sudo /opt/project-balloon/scripts/backup/backup.sh /var/backups/project-balloon
PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA \
  sudo -E /opt/project-balloon/scripts/backup/restore.sh \
  /var/backups/project-balloon/project-balloon-<timestamp>
```

Redis 可重建，RabbitMQ 应在最终比赛备份前排空。脚本为遗留的单主机部署保留 `compose` 模式；在该环境中设置 `PROJECT_BALLOON_DATABASE_MODE=compose`。

## 兼容 Compose 模式

仓库仍包含 `deploy/compose/`，用于开发与单主机彩排。它构建 API、Worker 与 Web 镜像，并可以启动数据与监控栈，但它不是默认的二进制发布路径。仅当主机有意将完整栈作为容器管理时，才使用 Compose 脚本。
