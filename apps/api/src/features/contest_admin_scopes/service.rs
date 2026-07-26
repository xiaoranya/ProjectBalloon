use std::net::IpAddr;

use sqlx::{PgPool, Postgres, Transaction};

use crate::error::AppError;

use super::model::ContestAdminScopeResponse;

pub struct ContestAdminScopeService {
    database: PgPool,
}

impl ContestAdminScopeService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn list(&self) -> Result<Vec<ContestAdminScopeResponse>, AppError> {
        sqlx::query_as(
            r#"
            SELECT
                u.id AS user_id,
                u.username,
                u.display_name,
                u.enabled,
                COALESCE(
                    array_agg(caa.contest_id ORDER BY caa.contest_id)
                        FILTER (WHERE caa.contest_id IS NOT NULL),
                    ARRAY[]::bigint[]
                ) AS contest_ids
            FROM users u
            LEFT JOIN contest_admin_assignments caa ON caa.user_id = u.id
            WHERE u.user_type = 'CONTEST_ADMIN'
            GROUP BY u.id
            ORDER BY u.username ASC, u.id ASC
            "#,
        )
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list contest administrator scopes", error))
    }

    pub async fn replace(
        &self,
        user_id: i64,
        contest_ids: Vec<i64>,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<ContestAdminScopeResponse, AppError> {
        if user_id <= 0 {
            return Err(contest_admin_not_found());
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest admin scope update", error))?;
        let admin = sqlx::query_as::<_, (String, String, bool)>(
            r#"
            SELECT username, display_name, enabled
            FROM users
            WHERE id = $1 AND user_type = 'CONTEST_ADMIN'
            FOR UPDATE
            "#,
        )
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("lock contest administrator", error))?
        .ok_or_else(contest_admin_not_found)?;

        if !contest_ids.is_empty() {
            let found = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT id
                FROM contests
                WHERE id = ANY($1) AND deleted_at IS NULL
                ORDER BY id
                "#,
            )
            .bind(&contest_ids)
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("validate assigned contests", error))?;
            if found != contest_ids {
                return Err(AppError::not_found(
                    "CONTEST_NOT_FOUND",
                    "One or more contests were not found",
                ));
            }
        }

        sqlx::query("DELETE FROM contest_admin_assignments WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("clear contest administrator scopes", error))?;
        if !contest_ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO contest_admin_assignments
                    (user_id, contest_id, assigned_by_user_id)
                SELECT $1, contest_id, $3
                FROM unnest($2::bigint[]) AS contest_id
                "#,
            )
            .bind(user_id)
            .bind(&contest_ids)
            .bind(actor_user_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("assign contest administrator scopes", error))?;
        }
        record_audit(&mut transaction, actor_user_id, user_id, request_ip).await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest admin scope update", error))?;

        Ok(ContestAdminScopeResponse {
            user_id,
            username: admin.0,
            display_name: admin.1,
            enabled: admin.2,
            contest_ids,
        })
    }
}

async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    target_user_id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES
            ($1, 'CONTEST_ADMIN_SCOPE_UPDATED', 'USER', $2, $3, 'success')
        "#,
    )
    .bind(actor_user_id)
    .bind(target_user_id.to_string())
    .bind(request_ip.to_string())
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record contest admin scope audit", error))
}

fn contest_admin_not_found() -> AppError {
    AppError::not_found("CONTEST_ADMIN_NOT_FOUND", "Contest administrator was not found")
}
