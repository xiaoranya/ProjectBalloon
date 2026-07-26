#!/usr/bin/env bash
set -euo pipefail
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
. "$ROOT/scripts/lib/deploy-common.sh"
. "$ROOT/scripts/lib/env-file.sh"

[ "$#" -eq 1 ] || pb_die 'usage: scripts/backup/restore.sh <backup-directory>'
BACKUP="$(cd "$1" && pwd)"
[ "${PROJECT_BALLOON_RESTORE_ACK:-}" = I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA ] \
  || pb_die 'set PROJECT_BALLOON_RESTORE_ACK=I_UNDERSTAND_THIS_REPLACES_CURRENT_DATA'

pb_require_docker
pb_require_env
pb_load_env_file "$PB_ENV_FILE" POSTGRES_DB POSTGRES_USER \
  PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY \
  BACKUP_OBJECT_STORAGE_ENDPOINT RUSTFS_PORT

(cd "$BACKUP" && sha256sum -c SHA256SUMS)
grep -qx 'format=project-balloon-backup-v1' "$BACKUP/manifest.txt" || pb_die 'unsupported backup format'
DB="${POSTGRES_DB:-project_balloon}"
DB_USER="${POSTGRES_USER:-project_balloon}"
BACKUP_DB="$(sed -n 's/^database=//p' "$BACKUP/manifest.txt")"
[ "$DB" = "$BACKUP_DB" ] || pb_die "backup database '$BACKUP_DB' does not match configured '$DB'"
S3_ENDPOINT="${BACKUP_OBJECT_STORAGE_ENDPOINT:-http://127.0.0.1:${RUSTFS_PORT:-9000}}"
export AWS_ACCESS_KEY_ID="${PROJECT_BALLOON_OBJECT_STORAGE_ACCESS_KEY:?object storage access key is required}"
export AWS_SECRET_ACCESS_KEY="${PROJECT_BALLOON_OBJECT_STORAGE_SECRET_KEY:?object storage secret key is required}"
command -v aws >/dev/null 2>&1 || pb_die 'aws CLI is required for RustFS restore'

pb_log 'stopping application and monitoring services'
"$ROOT/scripts/deploy/stop.sh" monitor || true
"$ROOT/scripts/deploy/stop.sh" app || true
pb_log "restoring PostgreSQL database=$DB"
gzip -dc "$BACKUP/postgres/database.sql.gz" | pb_compose data exec -T postgres psql -v ON_ERROR_STOP=1 -U "$DB_USER" -d "$DB"

for bucket_dir in "$BACKUP"/objects/*; do
  [ -d "$bucket_dir" ] || continue
  bucket="$(basename "$bucket_dir")"
  aws --endpoint-url "$S3_ENDPOINT" s3 sync "$bucket_dir" "s3://$bucket" --delete --no-progress
done
pb_log 'restore complete; start and health-check the application explicitly'
