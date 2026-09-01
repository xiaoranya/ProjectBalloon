#!/usr/bin/env bash
set -euo pipefail
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$ROOT/scripts/lib/deploy-common.sh"
. "$ROOT/scripts/lib/env-file.sh"

pb_load_env_file "$PB_ENV_FILE" DATABASE_URL PROJECT_BALLOON_DATABASE_MODE \
  PROJECT_BALLOON_DATABASE_NAME POSTGRES_DB POSTGRES_USER \
  PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY \
  BACKUP_OBJECT_STORAGE_ENDPOINT POSTGRES_PORT RUSTFS_PORT

OUTPUT="${1:-$ROOT/backups}"
DB="${PROJECT_BALLOON_DATABASE_NAME:-${POSTGRES_DB:-project_balloon}}"
DB_USER="${POSTGRES_USER:-project_balloon}"
DATABASE_MODE="${PROJECT_BALLOON_DATABASE_MODE:-auto}"
DATABASE_URL="${DATABASE_URL:-}"
S3_ENDPOINT="${BACKUP_OBJECT_STORAGE_ENDPOINT:-http://127.0.0.1:${RUSTFS_PORT:-9000}}"
ACCESS_KEY="${PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY:?object storage access key is required}"
SECRET_KEY="${PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY:?object storage secret key is required}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
FINAL="$OUTPUT/project-balloon-$STAMP"
TMP="$FINAL.tmp"

command -v aws >/dev/null 2>&1 || pb_die 'aws CLI is required for RustFS backup'
command -v gzip >/dev/null 2>&1 || pb_die 'gzip is required for database backup'
[ "$DATABASE_MODE" = direct ] || [ "$DATABASE_MODE" = compose ] || [ "$DATABASE_MODE" = auto ] \
  || pb_die 'PROJECT_BALLOON_DATABASE_MODE must be direct, compose, or auto'
if [ "$DATABASE_MODE" = auto ]; then
  if [ -n "$DATABASE_URL" ] && command -v pg_dump >/dev/null 2>&1; then
    DATABASE_MODE=direct
  else
    DATABASE_MODE=compose
  fi
fi
if [ "$DATABASE_MODE" = direct ]; then
  [ -n "$DATABASE_URL" ] || pb_die 'DATABASE_URL is required for direct database backup'
  command -v pg_dump >/dev/null 2>&1 || pb_die 'pg_dump is required for direct database backup'
else
  pb_require_docker
  pb_require_env
fi
[ ! -e "$FINAL" ] && [ ! -e "$TMP" ] || pb_die "backup path already exists: $FINAL"
mkdir -p "$TMP/postgres" "$TMP/objects"
cleanup() {
  local status="$?"
  if [ "$status" -ne 0 ]; then
    rm -rf "$TMP" "$TMP.SHA256SUMS"
  fi
}
trap cleanup EXIT

pb_log "dumping PostgreSQL database=$DB"
if [ "$DATABASE_MODE" = direct ]; then
  pg_dump "$DATABASE_URL" --clean --if-exists --no-owner --no-privileges \
    | gzip -c > "$TMP/postgres/database.sql.gz"
else
  pb_compose data exec -T postgres pg_dump -U "$DB_USER" -d "$DB" \
    --clean --if-exists --no-owner --no-privileges | gzip -c > "$TMP/postgres/database.sql.gz"
fi

export AWS_ACCESS_KEY_ID="$ACCESS_KEY" AWS_SECRET_ACCESS_KEY="$SECRET_KEY"
pb_log "copying RustFS buckets from $S3_ENDPOINT"
buckets="$(aws --endpoint-url "$S3_ENDPOINT" s3api list-buckets --query 'Buckets[].Name' --output text | tr '\t' '\n')"
printf '%s\n' "$buckets" | sed '/^$/d' > "$TMP/buckets.txt"
while IFS= read -r bucket; do
  [ -n "$bucket" ] || continue
  aws --endpoint-url "$S3_ENDPOINT" s3 sync "s3://$bucket" "$TMP/objects/$bucket" --no-progress
done <<< "$buckets"

if [ -d "$ROOT/deploy" ]; then
  tar -C "$ROOT" --exclude='deploy/compose/.env.rust' --exclude='deploy/compose/.env.local' \
    -czf "$TMP/deploy-config.tar.gz" deploy docs/ops
else
  tar -C "$ROOT" -czf "$TMP/deploy-config.tar.gz" docs
fi

# Snapshot the deployment environment with every credential-bearing value
# replaced, so a restored host can rebuild its env file from the archive plus
# the operator's secret store instead of reconstructing variable names from
# documentation. Values of *_KEY/*_SECRET/*_PASSWORD/*_TOKEN and credential
# URLs (DATABASE_URL, *_URL, *_DSN) are masked.
[ -f "$PB_ENV_FILE" ] || pb_die "environment file not found: $PB_ENV_FILE"
mkdir -p "$TMP/config"
while IFS= read -r line || [ -n "$line" ]; do
  line="${line%$'\r'}"
  if [[ "$line" =~ ^[[:space:]]*(#|$) ]] || \
     [[ ! "$line" =~ ^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*=(.*)$ ]]; then
    printf '%s\n' "$line"
  else
    key="${BASH_REMATCH[1]}"
    if [[ "$key" =~ .*(KEY|SECRET|PASSWORD|TOKEN|URL|DSN)$ ]]; then
      printf '%s=CHANGE_ME_redacted_from_backup\n' "$key"
    else
      printf '%s\n' "$line"
    fi
  fi
done < "$PB_ENV_FILE" > "$TMP/config/project-balloon.env.masked"

{
  printf '%s\n' '# ProjectBalloon restore checklist'
  printf '%s\n' ''
  printf '%s\n' 'This archive was produced by scripts/backup/backup.sh. Restore order:'
  printf '%s\n' ''
  printf '%s\n' '1. Verify integrity: `sha256sum -c SHA256SUMS` inside the backup directory.'
  printf '%s\n' '2. Read manifest.txt; confirm database= matches the target deployment.'
  printf '%s\n' '3. Rebuild /etc/project-balloon/project-balloon.env (or'
  printf '%s\n' '   deploy/compose/.env.rust) from config/project-balloon.env.masked,'
  printf '%s\n' '   filling every CHANGE_ME_redacted_from_backup value from the'
  printf '%s\n' '   operator secret store.'
  printf '%s\n' '4. Stop the application services (restore.sh stops them in direct mode;'
  printf '%s\n' '   stop monitor/app first in legacy Compose mode).'
  printf '%s\n' '5. Run restore.sh with'
  printf '%s\n' '   PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA.'
  printf '%s\n' '6. Restore is not a service restart: start project-balloon-api and'
  printf '%s\n' '   project-balloon-judge-worker (or compose app) explicitly.'
  printf '%s\n' '7. Verify http://127.0.0.1:8080/livez and /api/health, then follow'
  printf '%s\n' '   docs/ops/backup-restore.md "Post-restore verification".'
  printf '%s\n' '8. Only after verification, delete the superseded pre-restore data.'
} > "$TMP/RESTORE-CHECKLIST.md"

{
  printf 'format=project-balloon-backup-v1\n'
  printf 'created_at=%s\n' "$STAMP"
  printf 'database=%s\n' "$DB"
  printf 'database_user=%s\n' "$DB_USER"
  printf 'database_mode=%s\n' "$DATABASE_MODE"
  printf 'object_endpoint=%s\n' "$S3_ENDPOINT"
} > "$TMP/manifest.txt"
(cd "$TMP" && find . -type f -print0 | sort -z | xargs -0 sha256sum) > "$TMP.SHA256SUMS"
mv "$TMP.SHA256SUMS" "$TMP/SHA256SUMS"
mv "$TMP" "$FINAL"
pb_log "backup complete: $FINAL"
