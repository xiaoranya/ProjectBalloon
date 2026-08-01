#!/usr/bin/env bash
set -euo pipefail

PACKAGE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PROJECT_BALLOON_PREFIX:-/opt/project-balloon}"
CONFIG_DIR="${PROJECT_BALLOON_CONFIG_DIR:-/etc/project-balloon}"
SYSTEMD_DIR="/etc/systemd/system"
APP_GROUP="project-balloon"
API_USER="project-balloon-api"
WORKER_USER="project-balloon-worker"
CONTAINER_CLI="${PROJECT_BALLOON_CONTAINER_CLI:-}"
CONTAINER_GROUP="${PROJECT_BALLOON_CONTAINER_GROUP:-}"
ROLE=all
NO_START=0
INSTALL_NGINX=1

usage() {
  cat <<'EOF'
Usage: install.sh [options]

Installs ProjectBalloon binaries, Judge Runtime images, systemd units, and
the bundled frontend. PostgreSQL, Redis, RabbitMQ, RustFS, Docker/Podman,
CUPS, and Nginx remain host-managed prerequisites.

Options:
  --role ROLE             Install all, api, or worker components (default: all)
  --no-start              Install and configure without starting services
  --skip-nginx            Do not install the bundled Nginx configuration
  --prefix PATH           Installation prefix (default: /opt/project-balloon)
  --config-dir PATH       Configuration directory (default: /etc/project-balloon)
  --container-cli NAME    Use docker or podman for image import
  --container-group NAME  Socket group for the Judge Worker
  -h, --help              Show this help
EOF
}

die() {
  printf '[install] ERROR: %s\n' "$*" >&2
  exit 1
}

log() {
  printf '[install] %s\n' "$*"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --role)
      [ "$#" -ge 2 ] || die '--role requires all, api, or worker'
      ROLE="$2"
      shift
      ;;
    --no-start) NO_START=1 ;;
    --skip-nginx) INSTALL_NGINX=0 ;;
    --prefix)
      [ "$#" -ge 2 ] || die '--prefix requires a value'
      PREFIX="$2"
      shift
      ;;
    --config-dir)
      [ "$#" -ge 2 ] || die '--config-dir requires a value'
      CONFIG_DIR="$2"
      shift
      ;;
    --container-cli)
      [ "$#" -ge 2 ] || die '--container-cli requires docker or podman'
      CONTAINER_CLI="$2"
      shift
      ;;
    --container-group)
      [ "$#" -ge 2 ] || die '--container-group requires a group name'
      CONTAINER_GROUP="$2"
      shift
      ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
  shift
done

case "$ROLE" in
  all) INSTALL_API=1; INSTALL_WORKER=1 ;;
  api) INSTALL_API=1; INSTALL_WORKER=0 ;;
  worker) INSTALL_API=0; INSTALL_WORKER=1 ;;
  *) die "unsupported role: $ROLE (expected all, api, or worker)" ;;
esac

[ "$(id -u)" -eq 0 ] || die 'run this installer as root'
[ -d "$PACKAGE_ROOT/bin" ] || die "missing binary directory in package: $PACKAGE_ROOT/bin"
[ -d "$PACKAGE_ROOT/judge-images" ] || die "missing judge image directory in package: $PACKAGE_ROOT/judge-images"
case "$PREFIX" in
  /|/etc|/usr|/var|/opt) die "refusing unsafe installation prefix: $PREFIX" ;;
esac
command -v systemctl >/dev/null 2>&1 || die 'systemd is required'
command -v install >/dev/null 2>&1 || die 'install is required'
command -v getent >/dev/null 2>&1 || die 'getent is required'
command -v sha256sum >/dev/null 2>&1 || die 'sha256sum is required'

if [ -f "$PACKAGE_ROOT/PACKAGE-SHA256SUMS" ]; then
  log 'verifying release package checksums'
  (cd "$PACKAGE_ROOT" && sha256sum -c PACKAGE-SHA256SUMS)
fi

ENV_LOADER="$PACKAGE_ROOT/lib/env-file.sh"
[ -f "$ENV_LOADER" ] || ENV_LOADER="$PACKAGE_ROOT/scripts/lib/env-file.sh"
[ -f "$ENV_LOADER" ] || die 'missing env-file.sh in release package'
# shellcheck source=/dev/null
. "$ENV_LOADER"

ensure_group() {
  local group="$1"
  if ! getent group "$group" >/dev/null; then
    groupadd --system "$group"
  fi
}

ensure_user() {
  local user="$1"
  if ! id "$user" >/dev/null 2>&1; then
    useradd --system --no-create-home --home-dir "$PREFIX" \
      --shell /usr/sbin/nologin --gid "$APP_GROUP" "$user"
  fi
  usermod --append --groups "$APP_GROUP" "$user"
}

add_group_if_present() {
  local user="$1" group="$2"
  if getent group "$group" >/dev/null; then
    usermod --append --groups "$group" "$user"
  fi
}

if [ "$INSTALL_WORKER" -eq 1 ]; then
  if [ -z "$CONTAINER_CLI" ]; then
    if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
      CONTAINER_CLI=docker
    elif command -v podman >/dev/null 2>&1; then
      CONTAINER_CLI=podman
    else
      die 'a working Docker or Podman installation is required for the worker role'
    fi
  fi
  case "$CONTAINER_CLI" in
    docker|podman) command -v "$CONTAINER_CLI" >/dev/null 2>&1 || die "$CONTAINER_CLI is not installed" ;;
    *) die "unsupported container CLI: $CONTAINER_CLI" ;;
  esac

  if [ -z "$CONTAINER_GROUP" ]; then
    if getent group "$CONTAINER_CLI" >/dev/null; then
      CONTAINER_GROUP="$CONTAINER_CLI"
    fi
  fi
  [ -n "$CONTAINER_GROUP" ] || die 'could not determine the container socket group; use --container-group'
  getent group "$CONTAINER_GROUP" >/dev/null || die "container group does not exist: $CONTAINER_GROUP"
fi

ensure_group "$APP_GROUP"
if [ "$INSTALL_API" -eq 1 ]; then
  ensure_user "$API_USER"
  add_group_if_present "$API_USER" lp
fi
if [ "$INSTALL_WORKER" -eq 1 ]; then
  ensure_user "$WORKER_USER"
  usermod --append --groups "$CONTAINER_GROUP" "$WORKER_USER"
fi

install -d -o root -g root -m 0755 "$PREFIX"
install -d -o root -g root -m 0755 "$PREFIX/bin"
install -d -o root -g root -m 0755 "$PREFIX/scripts/backup" "$PREFIX/scripts/lib" "$PREFIX/docs"
rm -rf "$PREFIX/web"
install -d -o root -g root -m 0755 "$PREFIX/web"

if [ "$INSTALL_API" -eq 1 ]; then
  binaries=(project-balloon-api bootstrap-admin)
else
  binaries=(project-balloon-judge-worker)
fi
for binary in "${binaries[@]}"; do
  [ -f "$PACKAGE_ROOT/bin/$binary" ] || die "missing release binary: $PACKAGE_ROOT/bin/$binary"
  install -o root -g root -m 0755 "$PACKAGE_ROOT/bin/$binary" "$PREFIX/bin/$binary"
done
cp -a "$PACKAGE_ROOT/web/." "$PREFIX/web/"
chown -R root:root "$PREFIX/web"
cp -a "$PACKAGE_ROOT/scripts/backup/." "$PREFIX/scripts/backup/"
cp -a "$PACKAGE_ROOT/scripts/lib/." "$PREFIX/scripts/lib/"
cp -a "$PACKAGE_ROOT/docs/." "$PREFIX/docs/"
chown -R root:root "$PREFIX/scripts" "$PREFIX/docs"
find "$PREFIX/scripts" -type f -name '*.sh' -exec chmod 0755 {} +

install -d -o root -g root -m 0755 "$CONFIG_DIR"
ENV_FILE="$CONFIG_DIR/project-balloon.env"
BOOTSTRAP_ENV_FILE="$CONFIG_DIR/bootstrap-admin.env"
CONFIG_CREATED=0
if [ ! -f "$ENV_FILE" ]; then
  [ -f "$PACKAGE_ROOT/config/project-balloon.env.example" ] \
    || die 'missing project-balloon.env.example in release package'
  install -o root -g "$APP_GROUP" -m 0640 \
    "$PACKAGE_ROOT/config/project-balloon.env.example" "$ENV_FILE"
  CONFIG_CREATED=1
else
  chown root:"$APP_GROUP" "$ENV_FILE"
  chmod 0640 "$ENV_FILE"
fi
if [ "$INSTALL_API" -eq 1 ] && [ ! -f "$BOOTSTRAP_ENV_FILE" ]; then
  [ -f "$PACKAGE_ROOT/config/bootstrap-admin.env.example" ] \
    || die 'missing bootstrap-admin.env.example in release package'
  install -o root -g root -m 0600 \
    "$PACKAGE_ROOT/config/bootstrap-admin.env.example" "$BOOTSTRAP_ENV_FILE"
fi

if grep -Eq 'CHANGE_ME|^[[:space:]]*DATABASE_URL[[:space:]]*=[[:space:]]*$' "$ENV_FILE"; then
  log "created $ENV_FILE; fill in external services and secrets, then rerun install.sh"
  CONFIG_CREATED=1
fi

unset JUDGE_CACHE_DIR XCPC_SANDBOX_SOCKET XCPC_SANDBOX_RUNTIME \
  PROJECT_BALLOON_CUPS_ENABLED PROJECT_BALLOON_CUPS_PRINTER
pb_load_env_file "$ENV_FILE" JUDGE_CACHE_DIR XCPC_SANDBOX_SOCKET XCPC_SANDBOX_RUNTIME \
  PROJECT_BALLOON_CUPS_ENABLED PROJECT_BALLOON_CUPS_PRINTER
JUDGE_CACHE_DIR="${JUDGE_CACHE_DIR:-/var/cache/project-balloon/judge}"
XCPC_SANDBOX_SOCKET="${XCPC_SANDBOX_SOCKET:-/var/run/docker.sock}"
PROJECT_BALLOON_CUPS_ENABLED="${PROJECT_BALLOON_CUPS_ENABLED:-false}"

if [ "$INSTALL_WORKER" -eq 1 ]; then
  install -d -o "$WORKER_USER" -g "$APP_GROUP" -m 0700 "$JUDGE_CACHE_DIR"
fi
STATE_OWNER="$WORKER_USER"
[ "$INSTALL_API" -eq 1 ] && STATE_OWNER="$API_USER"
install -d -o "$STATE_OWNER" -g "$APP_GROUP" -m 0750 /var/lib/project-balloon

if [ "$INSTALL_API" -eq 1 ] && [ "$PROJECT_BALLOON_CUPS_ENABLED" = true ]; then
  command -v cupsfilter >/dev/null 2>&1 || die 'CUPS is enabled but cupsfilter is not installed'
  command -v lpstat >/dev/null 2>&1 || die 'CUPS is enabled but lpstat is not installed'
  [ -f /usr/share/ppd/cupsfilters/Generic-PDF_Printer-PDF.ppd ] \
    || die 'CUPS is enabled but the Generic PDF PPD is not installed'
fi

if [ "$INSTALL_WORKER" -eq 1 ] && [ ! -S "$XCPC_SANDBOX_SOCKET" ]; then
  [ "$NO_START" -eq 1 ] || die "sandbox socket is not available: $XCPC_SANDBOX_SOCKET"
  log "warning: sandbox socket is not available yet: $XCPC_SANDBOX_SOCKET"
fi

if [ "$INSTALL_WORKER" -eq 1 ] && [ -f "$PACKAGE_ROOT/judge-images/SHA256SUMS" ]; then
  (cd "$PACKAGE_ROOT/judge-images" && sha256sum -c SHA256SUMS)
fi
if [ "$INSTALL_WORKER" -eq 1 ]; then
  shopt -s nullglob
  judge_archives=("$PACKAGE_ROOT"/judge-images/*.tar)
  [ "${#judge_archives[@]}" -gt 0 ] || die 'no Judge Runtime image archives found'
  for archive in "${judge_archives[@]}"; do
    log "loading $(basename "$archive") with $CONTAINER_CLI"
    "$CONTAINER_CLI" load --input "$archive" >/dev/null
  done
  shopt -u nullglob
fi

render_unit() {
  local name="$1" api_groups="$2" container_group="$3"
  sed \
    -e "s|@PREFIX@|$PREFIX|g" \
    -e "s|@ENV_DIR@|$CONFIG_DIR|g" \
    -e "s|@APP_GROUP@|$APP_GROUP|g" \
    -e "s|@API_USER@|$API_USER|g" \
    -e "s|@WORKER_USER@|$WORKER_USER|g" \
    -e "s|@API_SUPPLEMENTARY_GROUPS@|$api_groups|g" \
    -e "s|@CONTAINER_GROUP@|$container_group|g" \
    -e "s|@JUDGE_CACHE_DIR@|$JUDGE_CACHE_DIR|g" \
    "$PACKAGE_ROOT/systemd/$name" > "$SYSTEMD_DIR/$name"
  chmod 0644 "$SYSTEMD_DIR/$name"
}

API_SUPPLEMENTARY_GROUPS="$APP_GROUP"
if getent group lp >/dev/null; then
  API_SUPPLEMENTARY_GROUPS="$API_SUPPLEMENTARY_GROUPS lp"
fi
if [ "$INSTALL_API" -eq 1 ]; then
  render_unit project-balloon-api.service "$API_SUPPLEMENTARY_GROUPS" "${CONTAINER_GROUP:-$APP_GROUP}"
fi
if [ "$INSTALL_WORKER" -eq 1 ]; then
  render_unit project-balloon-judge-worker.service "$API_SUPPLEMENTARY_GROUPS" "$CONTAINER_GROUP"
fi
systemctl daemon-reload
if [ "$INSTALL_API" -eq 1 ] && [ "$INSTALL_WORKER" -eq 1 ]; then
  systemctl enable project-balloon-api.service project-balloon-judge-worker.service >/dev/null
elif [ "$INSTALL_API" -eq 1 ]; then
  systemctl enable project-balloon-api.service >/dev/null
else
  systemctl enable project-balloon-judge-worker.service >/dev/null
fi

if [ "$INSTALL_API" -eq 1 ] && [ "$INSTALL_NGINX" -eq 1 ] && command -v nginx >/dev/null 2>&1; then
  NGINX_CONF="$CONFIG_DIR/project-balloon.nginx.conf"
  sed -e "s|@PREFIX@|$PREFIX|g" "$PACKAGE_ROOT/nginx/project-balloon.nginx.conf" \
    > "$NGINX_CONF"
  if [ -d /etc/nginx/conf.d ]; then
    install -o root -g root -m 0644 "$NGINX_CONF" /etc/nginx/conf.d/project-balloon.conf
  elif [ -d /etc/nginx/sites-available ] && [ -d /etc/nginx/sites-enabled ]; then
    install -o root -g root -m 0644 "$NGINX_CONF" /etc/nginx/sites-available/project-balloon.conf
    ln -sfn /etc/nginx/sites-available/project-balloon.conf /etc/nginx/sites-enabled/project-balloon.conf
  fi
  nginx -t
  systemctl enable nginx >/dev/null
  if [ "$NO_START" -eq 0 ]; then
    systemctl reload nginx 2>/dev/null || systemctl start nginx
  fi
elif [ "$INSTALL_API" -eq 1 ] && [ "$INSTALL_NGINX" -eq 1 ]; then
  log 'Nginx is not installed; frontend files are ready under the installation prefix'
fi

if [ "$CONFIG_CREATED" -ne 0 ]; then
  log 'installation files and systemd units are ready; edit configuration before starting services'
  exit 2
fi

if [ "$NO_START" -eq 0 ] && [ "$INSTALL_API" -eq 1 ]; then
  systemctl restart project-balloon-api.service
  systemctl is-active --quiet project-balloon-api.service || {
    journalctl -u project-balloon-api.service -n 80 --no-pager >&2 || true
    die 'API service did not become active'
  }
fi
if [ "$NO_START" -eq 0 ] && [ "$INSTALL_WORKER" -eq 1 ]; then
  systemctl restart project-balloon-judge-worker.service
  systemctl is-active --quiet project-balloon-judge-worker.service || {
    journalctl -u project-balloon-judge-worker.service -n 80 --no-pager >&2 || true
    die 'Judge Worker service did not become active'
  }
fi
if [ "$NO_START" -ne 0 ]; then
  log 'services installed but not started (--no-start)'
else
  log "${ROLE} services are active"
fi

log "installation complete under $PREFIX"
