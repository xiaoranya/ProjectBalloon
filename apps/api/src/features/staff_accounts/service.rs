use std::net::IpAddr;

use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    error::AppError,
    features::auth::{hash_password, model::UserType},
    pagination::PageResponse,
};

use crate::features::staff_accounts::model::{
    PageQuery, StaffAccountResponse, StaffAccountRow, ValidatedCreate, ValidatedUpdate,
};

const ACCOUNT_COLUMNS: &str = r#"
    u.id,
    u.username,
    u.display_name,
    u.user_type,
    COALESCE(
        (SELECT array_agg(p.code ORDER BY p.code)
         FROM user_permissions up
         JOIN permissions p ON p.id = up.permission_id
         WHERE up.user_id = u.id),
        ARRAY[]::varchar[]
    ) AS permissions,
    u.enabled,
    u.password_reset_required,
    u.last_login_at,
    u.created_at,
    u.updated_at
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
        let total_elements = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM users WHERE user_type IN ('STAFF', 'SUPER_ADMIN')",
        )
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("count staff accounts", error))?;
        let sql = format!(
            r#"
            SELECT {ACCOUNT_COLUMNS}
            FROM users u
            WHERE u.user_type IN ('STAFF', 'SUPER_ADMIN')
            ORDER BY u.username ASC, u.id ASC
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
        let user_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ($1, $2, $3, $4, true, $5)
            RETURNING id
            "#,
        )
        .bind(request.username)
        .bind(password_hash)
        .bind(request.display_name)
        .bind(request.user_type.as_str())
        .bind(request.require_password_reset)
        .fetch_one(&mut *transaction)
        .await
        .map_err(map_create_error)?;
        replace_permissions(&mut transaction, user_id, &request.permissions).await?;
        record_audit(&mut transaction, actor_user_id, "STAFF_ACCOUNT_CREATED", user_id, request_ip)
            .await?;
        let row = load_staff_account(&mut transaction, user_id).await?;
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
        let next_type = request
            .is_super_admin
            .map(|value| if value { UserType::SuperAdmin } else { UserType::Staff })
            .unwrap_or(current_type);
        let next_permissions = request.permissions.unwrap_or_else(|| current.permissions.clone());
        if next_type == UserType::SuperAdmin && !next_permissions.is_empty() {
            return Err(AppError::validation(
                "permissions",
                "super administrators must not have explicit permissions",
            ));
        }
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

        let permissions_changed = next_permissions != current.permissions;
        if permissions_changed || next_type != current_type {
            replace_permissions(&mut transaction, user_id, &next_permissions).await?;
        }
        if !next_permissions
            .iter()
            .any(|code| code == crate::features::auth::permissions::CONTEST_MANAGE)
        {
            sqlx::query("DELETE FROM contest_management_assignments WHERE user_id = $1")
                .bind(user_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    AppError::internal("remove obsolete contest management scopes", error)
                })?;
        }

        sqlx::query(
            r#"
            UPDATE users
            SET display_name = $1,
                user_type = $2,
                enabled = $3,
                updated_at = now()
            WHERE id = $4
            "#,
        )
        .bind(next_display_name)
        .bind(next_type.as_str())
        .bind(next_enabled)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("update staff account", error))?;

        if next_type != current_type || next_enabled != current.enabled || permissions_changed {
            revoke_sessions(&mut transaction, user_id).await?;
        }
        record_audit(&mut transaction, actor_user_id, "STAFF_ACCOUNT_UPDATED", user_id, request_ip)
            .await?;
        let updated = load_staff_account(&mut transaction, user_id).await?;
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
        sqlx::query(
            r#"
            UPDATE users
            SET password_hash = $1,
                password_reset_required = $2,
                updated_at = now()
            WHERE id = $3
            "#,
        )
        .bind(password_hash)
        .bind(require_password_reset)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("reset staff password", error))?;
        revoke_sessions(&mut transaction, user_id).await?;
        record_audit(&mut transaction, actor_user_id, "STAFF_PASSWORD_RESET", user_id, request_ip)
            .await?;
        let updated = load_staff_account(&mut transaction, user_id).await?;
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
        FROM users u
        WHERE u.id = $1 AND u.user_type IN ('STAFF', 'SUPER_ADMIN')
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

async fn load_staff_account(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: i64,
) -> Result<StaffAccountRow, AppError> {
    let sql = format!(
        r#"
        SELECT {ACCOUNT_COLUMNS}
        FROM users u
        WHERE u.id = $1 AND u.user_type IN ('STAFF', 'SUPER_ADMIN')
        "#,
    );
    sqlx::query_as::<_, StaffAccountRow>(sqlx::AssertSqlSafe(sql))
        .bind(user_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("load staff account", error))
}

async fn replace_permissions(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: i64,
    permissions: &[String],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM user_permissions WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("clear staff account permissions", error))?;
    if permissions.is_empty() {
        return Ok(());
    }
    let inserted = sqlx::query(
        r#"
        INSERT INTO user_permissions (user_id, permission_id)
        SELECT $1, id FROM permissions WHERE code = ANY($2)
        "#,
    )
    .bind(user_id)
    .bind(permissions)
    .execute(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("assign staff account permissions", error))?;
    if inserted.rows_affected() != permissions.len() as u64 {
        return Err(AppError::validation("permissions", "contains an unknown permission"));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::PgPool;

    use crate::features::staff_accounts::model::{
        CreateStaffAccountRequest, UpdateStaffAccountRequest,
    };
    use crate::features::staff_accounts::service::StaffAccountService;

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
    async fn permissions_are_direct_composable_and_replaceable(pool: PgPool) {
        let actor_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users(username,password_hash,display_name,user_type) VALUES('root','hash','Root','SUPER_ADMIN') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert actor");
        let service = StaffAccountService::new(pool.clone());
        let created = service
            .create(
                CreateStaffAccountRequest {
                    username: "multi.operator".into(),
                    display_name: "Multi Operator".into(),
                    is_super_admin: false,
                    permissions: vec!["PRINTING_MANAGE".into(), "CLARIFICATION_MANAGE".into()],
                    initial_password: "temporary-password".into(),
                    require_password_reset: true,
                }
                .validate()
                .expect("valid create"),
                actor_id,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("create staff account");
        assert_eq!(created.user_type.as_str(), "STAFF");
        assert_eq!(
            created.permissions,
            vec!["CLARIFICATION_MANAGE".to_owned(), "PRINTING_MANAGE".to_owned()]
        );
        sqlx::query(
            "INSERT INTO auth_sessions(token_hash,user_id,access_fingerprint,expires_at) VALUES($1,$2,$3,now() + interval '1 hour')",
        )
        .bind("a".repeat(64))
        .bind(created.id)
        .bind("b".repeat(64))
        .execute(&pool)
        .await
        .expect("insert active session");

        let updated = service
            .update(
                created.id,
                UpdateStaffAccountRequest {
                    display_name: None,
                    is_super_admin: None,
                    permissions: Some(vec!["BALLOON_MANAGE".into()]),
                    enabled: None,
                }
                .validate()
                .expect("valid update"),
                actor_id,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("replace permissions");
        assert_eq!(updated.permissions, vec!["BALLOON_MANAGE"]);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM auth_sessions WHERE user_id = $1")
                .bind(created.id)
                .fetch_one(&pool)
                .await
                .expect("count revoked sessions"),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM user_permissions WHERE user_id = $1",
            )
            .bind(created.id)
            .fetch_one(&pool)
            .await
            .expect("count permissions"),
            1
        );
        assert!(
            sqlx::query_scalar::<_, Option<String>>("SELECT to_regclass('public.roles')::text")
                .fetch_one(&pool)
                .await
                .expect("inspect removed role table")
                .is_none()
        );
    }
}
