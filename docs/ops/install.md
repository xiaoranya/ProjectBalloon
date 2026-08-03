# Binary Install

The default release is a binary package. It contains the API, Judge Worker,
bootstrap CLI, and Vue static files. The four Judge Runtime images are
published as a separate archive and imported with `install.sh --judge-images`.
PostgreSQL, Redis, RabbitMQ, RustFS, Docker or Podman, and optional CUPS/Nginx
services are host-managed prerequisites.

## Release platform status

GitHub Actions produces packages for Linux amd64, Linux arm64, macOS Intel,
macOS arm64, and Windows x64. Linux packages are deployment packages; macOS
and Windows packages are portable build packages containing the binaries and
frontend only; neither includes the Judge Runtime image archives. Judge
Runtime image archives are published separately for Linux amd64 and Linux
arm64 and must be downloaded and imported on Judge hosts.

| Package | Rust target | GitHub runner |
|---|---|---|
| `linux-amd64` | `x86_64-unknown-linux-gnu` | `ubuntu-24.04` |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` |
| `macos-x86_64` | `x86_64-apple-darwin` | `macos-15-intel` |
| `macos-arm64` | `aarch64-apple-darwin` | `macos-15` |
| `windows-x86_64` | `x86_64-pc-windows-msvc` | `windows-2022` |

Only Linux x86_64 has been tested end to end. The other platform artifacts are
currently limited to runner build and package-integrity checks. Do not treat
them as runtime-compatible or installation-validated releases until testing on
the target host has been completed.

## Host prerequisites

Install these from trusted media or the host distribution before installation:

- systemd;
- PostgreSQL, Redis, RabbitMQ, and RustFS or another S3-compatible service;
- `tar`, `gzip`, `sha256sum`, and GNU coreutils;
- `postgresql-client` (`pg_dump` and `psql`) for direct binary-mode backups and restores;
- AWS CLI v2 for RustFS/S3 backup and restore;
- Nginx for the bundled frontend configuration;
- `cups-client`, `cups-filters`, and a configured CUPS printer when printing is enabled.

The `api` role does not require Docker or Podman. The `worker` and `all` roles
require Docker Engine or a preconfigured rootful Podman service with a socket
accessible to the Judge Worker. Rootless Podman setup, its user service, and
the rootless image store remain host preparation steps described by the sandbox
ADR; the installer does not create that service automatically.

The installer does not create databases, queues, object-storage credentials, or
production secrets. It imports the four Judge Runtime images from the directory
given by `--judge-images` (or from `judge-images/` when bundled) and creates the
application users, directories, environment file, systemd units, and Nginx
configuration.

## Install a release

Extract the published binary archive and the matching Judge Runtime image
archive, then run the installer as root:

```text
tar -xzf project-balloon-<version>-<target>.tar.gz
tar -xzf project-balloon-<version>-<target>-judge-images.tar.gz
cd project-balloon-<version>-<target>
sudo ./install.sh --no-start --judge-images ../judge-images
```

For the separated topology, install only the relevant role on each host:

```text
# app/gateway host
sudo ./install.sh --role api --no-start

# judge host
sudo ./install.sh --role worker --skip-nginx --no-start \
  --container-group docker --judge-images ../judge-images
```

Both hosts must receive the same external-service configuration, while each
Judge host additionally needs its local sandbox socket and the imported runtime
images. The default `all` role remains convenient for a single-host rehearsal.

The first run creates `/etc/project-balloon/project-balloon.env` and exits so
that external service URLs and secrets can be reviewed. Edit that file, then
run the installer again:

```text
sudoedit /etc/project-balloon/project-balloon.env
sudo ./install.sh
```

The second run imports the Judge Runtime images from `--judge-images` when the
selected role includes the Worker, installs or refreshes the relevant systemd
units, validates CUPS when the API role enables it, reloads Nginx when
available, and starts the selected services. The API runs embedded SQLx
migrations when `PROJECT_BALLOON_RUN_MIGRATIONS=true`.

The application is installed under `/opt/project-balloon`. The service users
are `project-balloon-api` and `project-balloon-worker`; the latter must be able
to access the Docker/Podman socket. Override the prefix, config directory, or
socket group with installer options when the host layout requires it.

Bootstrap the first administrator once the API can reach PostgreSQL:

```text
sudoedit /etc/project-balloon/bootstrap-admin.env
sudo sh -c 'set -a; . /etc/project-balloon/bootstrap-admin.env; set +a; exec /opt/project-balloon/bin/bootstrap-admin'
```

Remove or rotate the bootstrap password after the command succeeds.

## Service operations

```text
sudo systemctl status project-balloon-api project-balloon-judge-worker
sudo systemctl restart project-balloon-api project-balloon-judge-worker
sudo journalctl -u project-balloon-api -u project-balloon-judge-worker -f
curl --fail http://127.0.0.1:8080/livez
```

The installer writes a frontend configuration to
`/etc/nginx/conf.d/project-balloon.conf` on distributions using `conf.d`, or
to the `sites-available`/`sites-enabled` layout. Put TLS termination in front
of this configuration and keep `PROJECT_BALLOON_SECURE_COOKIES=true` for the
production environment.

## Backups

The installer places `scripts/backup` under `/opt/project-balloon/scripts/backup`.
With the default `PROJECT_BALLOON_DATABASE_MODE=direct`, the scripts use the
host PostgreSQL client tools and `DATABASE_URL`; they do not require Docker.
Set `BACKUP_OBJECT_STORAGE_ENDPOINT` when RustFS is not reachable at the
default endpoint, then run:

```text
sudo /opt/project-balloon/scripts/backup/backup.sh /var/backups/project-balloon
PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA \
  sudo -E /opt/project-balloon/scripts/backup/restore.sh \
  /var/backups/project-balloon/project-balloon-<timestamp>
```

Redis is rebuildable and RabbitMQ should be drained before a final contest
backup. The scripts retain `compose` mode for legacy single-host deployments;
set `PROJECT_BALLOON_DATABASE_MODE=compose` in that environment.

## Compatibility Compose mode

The repository still contains `deploy/compose/` for development and single-host
rehearsal. It builds the API, Worker, and Web images and can start the data and
monitoring stacks, but it is not the default binary release path. Use the
Compose scripts only when the host intentionally manages the complete stack as
containers.
