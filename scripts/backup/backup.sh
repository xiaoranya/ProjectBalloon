#!/usr/bin/env bash
set -euo pipefail
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$ROOT/scripts/lib/deploy-common.sh"
. "$ROOT/scripts/lib/env-file.sh"

pb_require_docker
pb_require_env
pb_load_env_file "$PB_ENV_FILE" POSTGRES_DB POSTGRES_USER \
  PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY \
  BACKUP_OBJECT_STORAGE_ENDPOINT POSTGRES_PORT RUSTFS_PORT

OUTPUT="${1:-$ROOT/backups}"
DB="${POSTGRES_DB:-project_balloon}"
DB_USER="${POSTGRES_USER:-project_balloon}"
S3_ENDPOINT="${BACKUP_OBJECT_STORAGE_ENDPOINT:-http://127.0.0.1:${RUSTFS_PORT:-9000}}"
ACCESS_KEY="${PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY:?object storage access key is required}"
SECRET_KEY="${PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY:?object storage secret key is required}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
FINAL="$OUTPUT/project-balloon-$STAMP"
TMP="$FINAL.tmp"

command -v aws >/dev/null 2>&1 || pb_die 'aws CLI is required for RustFS backup'
command -v gzip >/dev/null 2>&1 || pb_die 'gzip is required for database backup'
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
pb_compose data exec -T postgres pg_dump -U "$DB_USER" -d "$DB" \
  --clean --if-exists --no-owner --no-privileges | gzip -c > "$TMP/postgres/database.sql.gz"

export AWS_ACCESS_KEY_ID="$ACCESS_KEY" AWS_SECRET_ACCESS_KEY="$SECRET_KEY"
pb_log "copying RustFS buckets from $S3_ENDPOINT"
buckets="$(aws --endpoint-url "$S3_ENDPOINT" s3api list-buckets --query 'Buckets[].Name' --output text | tr '\t' '\n')"
printf '%s\n' "$buckets" | sed '/^$/d' > "$TMP/buckets.txt"
while IFS= read -r bucket; do
  [ -n "$bucket" ] || continue
  aws --endpoint-url "$S3_ENDPOINT" s3 sync "s3://$bucket" "$TMP/objects/$bucket" --no-progress
done <<< "$buckets"

tar -C "$ROOT" --exclude='deploy/compose/.env.rust' --exclude='deploy/compose/.env.local' \
  -czf "$TMP/deploy-config.tar.gz" deploy docs/ops
{
  printf 'format=project-balloon-backup-v1\n'
  printf 'created_at=%s\n' "$STAMP"
  printf 'database=%s\n' "$DB"
  printf 'database_user=%s\n' "$DB_USER"
  printf 'object_endpoint=%s\n' "$S3_ENDPOINT"
} > "$TMP/manifest.txt"
(cd "$TMP" && find . -type f -print0 | sort -z | xargs -0 sha256sum) > "$TMP.SHA256SUMS"
mv "$TMP.SHA256SUMS" "$TMP/SHA256SUMS"
mv "$TMP" "$FINAL"
pb_log "backup complete: $FINAL"
