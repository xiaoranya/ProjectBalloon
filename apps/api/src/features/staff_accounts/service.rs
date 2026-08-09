use std::net::IpAddr;

use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    error::AppError,
    features::auth::{hash_password, model::UserType},
    pagination::PageResponse,
};

use super::model::{
    PageQuery, StaffAccountResponse, StaffAccountRow, ValidatedCreate, ValidatedUpdate,
};

const ACCOUNT_COLUMNS: &str = r#"
    id,
    username,
    display_name,
    user_type,
    enabled,
    password_reset_required,
    last_login_at,
    created_at,
    updated_at
"#;
const STAFF_ACCESS_LOCK_ID: i64 = 0x0050_4253_5441_4646;

pub struct StaffAccountService {
    database: PgPool,
}

impl StaffAccountService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn list(
        &self,
        query: PageQuery,
    ) -> Result<PageResponse<StaffAccountResponse>, AppError> {
        query.validate()?;
        let offset = query.offset()?;
        let total_elements =
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users WHERE user_type <> 'TEAM'")
                .fetch_one(&self.database)
                .await
                .map_err(|error| AppError::internal("count staff accounts", error))?;
        let sql = format!(
            r#"
            SELECT {ACCOUNT_COLUMNS}
            FROM users
            WHERE user_type <> 'TEAM'
            ORDER BY username ASC, id ASC
            LIMIT $1 OFFSET $2
            "#
        );
        let rows = sqlx::query_as::<_, StaffAccountRow>(sqlx::AssertSqlSafe(sql))
            .bind(i64::from(query.size))
            .bind(offset)
            .fetch_all(&self.database)
            .await
            .map_err(|error| AppError::internal("list staff accounts", error))?;
        let content =
            rows.into_iter().map(StaffAccountRow::response).collect::<Result<Vec<_>, _>>()?;
        Ok(PageResponse::new(content, query.page, query.size, total_elements))
    }

    pub async fn create(
        &self,
        request: ValidatedCreate,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<StaffAccountResponse, AppError> {
        let password_hash = hash_password(request.initial_password)
            .await
            .map_err(|error| AppError::internal("hash initial staff password", error))?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin staff account creation", error))?;
        lock_staff_access_changes(&mut transaction).await?;
        let role_id = require_role(&mut transaction, request.user_type).await?;
        let sql = format!(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ($1, $2, $3, $4, true, $5)
            RETURNING {ACCOUNT_COLUMNS}
            "#
        );
        let row = sqlx::query_as::<_, StaffAccountRow>(sqlx::AssertSqlSafe(sql))
            .bind(request.username)
            .bind(password_hash)
            .bind(request.display_name)
            .bind(request.user_type.as_str())
            .bind(request.require_password_reset)
            .fetch_one(&mut *transaction)
            .await
            .map_err(map_create_error)?;
        sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2)")
            .bind(row.id)
            .bind(role_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("assign staff account role", error))?;
        record_audit(&mut transaction, actor_user_id, "STAFF_ACCOUNT_CREATED", row.id, request_ip)
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit staff account creation", error))?;
        row.response()
    }

    pub async fn update(
        &self,
        user_id: i64,
        request: ValidatedUpdate,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<StaffAccountResponse, AppError> {
        require_positive_id(user_id)?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin staff account update", error))?;
        lock_staff_access_changes(&mut transaction).await?;
        let current = lock_staff_account(&mut transaction, user_id).await?;
        let current_type: UserType = current.user_type.parse()?;
        let next_type = request.user_type.unwrap_or(current_type);
        let next_enabled = request.enabled.unwrap_or(current.enabled);
        let next_display_name =
            request.display_name.unwrap_or_else(|| current.display_name.clone());

        let removes_super_admin_access = current_type == UserType::SuperAdmin
            && current.enabled
            && (next_type != UserType::SuperAdmin || !next_enabled);
        if removes_super_admin_access {
            if user_id == actor_user_id {
                return Err(AppError::conflict(
                    "SELF_ACCESS_CHANGE_FORBIDDEN",
                    "Cannot remove your own super administrator access",
                ));
            }
            let enabled_super_admins = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT id
                FROM users
                WHERE user_type = 'SUPER_ADMIN' AND enabled = true
                ORDER BY id
                FOR UPDATE
                "#,
            )
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("lock enabled super administrators", error))?;
            if enabled_super_admins.len() <= 1 {
                return Err(AppError::conflict(
                    "LAST_SUPER_ADMIN",
                    "At least one enabled super administrator is required",
                ));
            }
        }

        if next_type != current_type {
            let role_id = require_role(&mut transaction, next_type).await?;
            sqlx::query("DELETE FROM user_roles WHERE user_id = $1")
                .bind(user_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("remove previous staff roles", error))?;
            sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2)")
                .bind(user_id)
                .bind(role_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| AppError::internal("assign updated staff role", error))?;
            if current_type == UserType::ContestAdmin {
                sqlx::query("DELETE FROM contest_admin_assignments WHERE user_id = $1")
                    .bind(user_id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| {
                        AppError::internal("remove obsolete contest administrator scopes", error)
                    })?;
            }
        }

        let sql = format!(
            r#"
            UPDATE users
            SET display_name = $1,
                user_type = $2,
                enabled = $3,
                updated_at = now()
            WHERE id = $4
            RETURNING {ACCOUNT_COLUMNS}
            "#
        );
        let updated = sqlx::query_as::<_, StaffAccountRow>(sqlx::AssertSqlSafe(sql))
            .bind(next_display_name)
            .bind(next_type.as_str())
            .bind(next_enabled)
            .bind(user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("update staff account", error))?;

        if next_type != current_type || next_enabled != current.enabled {
            revoke_sessions(&mut transaction, user_id).await?;
        }
        record_audit(&mut transaction, actor_user_id, "STAFF_ACCOUNT_UPDATED", user_id, request_ip)
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit staff account update", error))?;
        updated.response()
    }

    pub async fn reset_password(
        &self,
        user_id: i64,
        new_password: String,
        require_password_reset: bool,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<StaffAccountResponse, AppError> {
        require_positive_id(user_id)?;
        let password_hash = hash_password(new_password)
            .await
            .map_err(|error| AppError::internal("hash reset staff password", error))?;
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin staff password reset", error))?;
        lock_staff_account(&mut transaction, user_id).await?;
        let sql = format!(
            r#"
            UPDATE users
            SET password_hash = $1,
                password_reset_required = $2,
                updated_at = now()
            WHERE id = $3
            RETURNING {ACCOUNT_COLUMNS}
            "#
        );
        let updated = sqlx::query_as::<_, StaffAccountRow>(sqlx::AssertSqlSafe(sql))
            .bind(password_hash)
            .bind(require_password_reset)
            .bind(user_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("reset staff password", error))?;
        revoke_sessions(&mut transaction, user_id).await?;
        record_audit(&mut transaction, actor_user_id, "STAFF_PASSWORD_RESET", user_id, request_ip)
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit staff password reset", error))?;
        updated.response()
    }
}

async fn lock_staff_account(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: i64,
) -> Result<StaffAccountRow, AppError> {
    let sql = format!(
        r#"
        SELECT {ACCOUNT_COLUMNS}
        FROM users
        WHERE id = $1 AND user_type <> 'TEAM'
        FOR UPDATE
        "#
    );
    sqlx::query_as::<_, StaffAccountRow>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("lock staff account", error))?
        .ok_or_else(|| {
            AppError::not_found("STAFF_ACCOUNT_NOT_FOUND", "Staff account was not found")
        })
}

async fn lock_staff_access_changes(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), AppError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(STAFF_ACCESS_LOCK_ID)
        .execute(&mut **transaction)
        .await
        .map(|_| ())
        .map_err(|error| AppError::internal("serialize staff access changes", error))
}

async fn require_role(
    transaction: &mut Transaction<'_, Postgres>,
    user_type: UserType,
) -> Result<i64, AppError> {
    sqlx::query_scalar("SELECT id FROM roles WHERE code = $1")
        .bind(user_type.as_str())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("load configured staff role", error))?
        .ok_or_else(|| {
            AppError::conflict(
                "STAFF_ROLE_NOT_CONFIGURED",
                "The selected staff role is not configured",
            )
        })
}

async fn revoke_sessions(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: i64,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM auth_sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut **transaction)
        .await
        .map(|_| ())
        .map_err(|error| AppError::internal("revoke staff sessions", error))
}

async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    action: &'static str,
    target_user_id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES
            ($1, $2, 'user', $3, $4, 'success')
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(target_user_id.to_string())
    .bind(request_ip.to_string())
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record staff account audit", error))
}

fn require_positive_id(user_id: i64) -> Result<(), AppError> {
    if user_id > 0 {
        Ok(())
    } else {
        Err(AppError::not_found("STAFF_ACCOUNT_NOT_FOUND", "Staff account was not found"))
    }
}

fn map_create_error(error: sqlx::Error) -> AppError {
    if error.as_database_error().and_then(sqlx::error::DatabaseError::constraint)
        == Some("users_username_key")
    {
        AppError::conflict("USERNAME_TAKEN", "Username is already in use")
    } else {
        AppError::internal("create staff account", error)
    }
}
