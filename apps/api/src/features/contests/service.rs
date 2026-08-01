use std::net::IpAddr;

use project_balloon_domain::{
    ContestExtensionError, ContestSchedule as DomainContestSchedule, ContestTransitionError,
    validate_contest_end_extension, validate_contest_transition,
};
use sqlx::{PgPool, Postgres, Transaction};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::auth::model::{AuthUser, UserType},
    pagination::PageResponse,
};

use super::model::{
    ContestCloneResponse, ContestExtensionRequest, ContestExtensionResponse, ContestResponse,
    ContestRow, ContestSchedule, ContestStatus, LifecycleTransitionResponse, ValidatedContestClone,
    ValidatedContestListQuery, ValidatedCreateContest, ValidatedUpdateContest,
};

const CONTEST_COLUMNS: &str = r#"
    id,
    name,
    status,
    visibility,
    start_at,
    end_at,
    freeze_at,
    version,
    created_at,
    updated_at,
    deleted_at
"#;

pub struct ContestService {
    database: PgPool,
}

impl ContestService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn require_team_id(&self, contest_id: i64, user_id: i64) -> Result<i64, AppError> {
        if contest_id <= 0 {
            return Err(contest_not_found());
        }
        sqlx::query_scalar(
            r#"
            SELECT account.team_id
            FROM contest_teams contest_team
            JOIN team_accounts account ON account.team_id = contest_team.team_id
            JOIN teams team ON team.id = account.team_id
            JOIN contests contest ON contest.id = contest_team.contest_id
            WHERE contest_team.contest_id = $1
              AND account.user_id = $2
              AND team.deleted_at IS NULL
              AND contest.deleted_at IS NULL
            "#,
        )
        .bind(contest_id)
        .bind(user_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("resolve authenticated contest team", error))?
        .ok_or_else(contest_not_found)
    }

    pub async fn list(
        &self,
        query: ValidatedContestListQuery,
        user: Option<&AuthUser>,
    ) -> Result<PageResponse<ContestResponse>, AppError> {
        let access = ReadAccess::for_user(user);
        if query.manageable_only && !access.super_admin && access.contest_admin_id.is_none() {
            return Err(AppError::forbidden(
                "FORBIDDEN",
                "Only contest managers may list manageable contests",
            ));
        }
        if query.include_deleted && !access.super_admin {
            return Err(AppError::forbidden(
                "FORBIDDEN",
                "Only super administrators may include deleted contests",
            ));
        }
        let include_deleted = query.include_deleted && access.super_admin;
        let total_elements = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT count(*)
            FROM contests c
            WHERE ($1 OR c.deleted_at IS NULL)
              AND (
                    $2
                    OR ($3::bigint IS NOT NULL AND EXISTS (
                        SELECT 1
                        FROM contest_admin_assignments caa
                        WHERE caa.contest_id = c.id AND caa.user_id = $3
                    ))
                    OR (NOT $5 AND c.visibility = 'PUBLIC' AND c.status <> 'DRAFT')
                    OR ($4::bigint IS NOT NULL AND EXISTS (
                        SELECT 1
                        FROM contest_teams ct
                        JOIN team_accounts ta ON ta.team_id = ct.team_id
                        WHERE ct.contest_id = c.id
                          AND ta.user_id = $4
                    ))
              )
            "#,
        )
        .bind(include_deleted)
        .bind(access.read_all)
        .bind(access.contest_admin_id)
        .bind(access.team_user_id)
        .bind(query.manageable_only)
        .fetch_one(&self.database)
        .await
        .map_err(|error| AppError::internal("count readable contests", error))?;
        let sql = format!(
            r#"
            SELECT
                c.id,
                c.name,
                c.status,
                c.visibility,
                c.start_at,
                c.end_at,
                c.freeze_at,
                c.version,
                c.created_at,
                c.updated_at,
                c.deleted_at
            FROM contests c
            WHERE ($1 OR c.deleted_at IS NULL)
              AND (
                    $2
                    OR ($3::bigint IS NOT NULL AND EXISTS (
                        SELECT 1
                        FROM contest_admin_assignments caa
                        WHERE caa.contest_id = c.id AND caa.user_id = $3
                    ))
                    OR (NOT $5 AND c.visibility = 'PUBLIC' AND c.status <> 'DRAFT')
                    OR ($4::bigint IS NOT NULL AND EXISTS (
                        SELECT 1
                        FROM contest_teams ct
                        JOIN team_accounts ta ON ta.team_id = ct.team_id
                        WHERE ct.contest_id = c.id
                          AND ta.user_id = $4
                    ))
              )
            ORDER BY {}
            LIMIT $6 OFFSET $7
            "#,
            query.order_by
        );
        let rows = sqlx::query_as::<_, ContestRow>(&sql)
            .bind(include_deleted)
            .bind(access.read_all)
            .bind(access.contest_admin_id)
            .bind(access.team_user_id)
            .bind(query.manageable_only)
            .bind(i64::from(query.size))
            .bind(query.offset)
            .fetch_all(&self.database)
            .await
            .map_err(|error| AppError::internal("list readable contests", error))?;
        let content = rows.into_iter().map(ContestRow::response).collect::<Result<Vec<_>, _>>()?;
        Ok(PageResponse::new(content, query.page, query.size, total_elements))
    }

    pub async fn get(
        &self,
        contest_id: i64,
        user: Option<&AuthUser>,
    ) -> Result<ContestResponse, AppError> {
        if contest_id <= 0 {
            return Err(contest_not_found());
        }
        let access = ReadAccess::for_user(user);
        let sql = format!(
            r#"
            SELECT {CONTEST_COLUMNS}
            FROM contests c
            WHERE c.id = $1
              AND c.deleted_at IS NULL
              AND (
                    $2
                    OR ($3::bigint IS NOT NULL AND EXISTS (
                        SELECT 1
                        FROM contest_admin_assignments caa
                        WHERE caa.contest_id = c.id AND caa.user_id = $3
                    ))
                    OR (c.visibility = 'PUBLIC' AND c.status <> 'DRAFT')
                    OR ($4::bigint IS NOT NULL AND EXISTS (
                        SELECT 1
                        FROM contest_teams ct
                        JOIN team_accounts ta ON ta.team_id = ct.team_id
                        WHERE ct.contest_id = c.id
                          AND ta.user_id = $4
                    ))
              )
            "#
        );
        sqlx::query_as::<_, ContestRow>(&sql)
            .bind(contest_id)
            .bind(access.read_all)
            .bind(access.contest_admin_id)
            .bind(access.team_user_id)
            .fetch_optional(&self.database)
            .await
            .map_err(|error| AppError::internal("load readable contest", error))?
            .ok_or_else(contest_not_found)?
            .response()
    }

    pub async fn create(
        &self,
        request: ValidatedCreateContest,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<ContestResponse, AppError> {
        let (start_at, freeze_at, end_at) = schedule_values(request.schedule);
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest creation", error))?;
        let sql = format!(
            r#"
            INSERT INTO contests
                (name, status, visibility, start_at, freeze_at, end_at)
            VALUES
                ($1, 'DRAFT', $2, $3, $4, $5)
            RETURNING {CONTEST_COLUMNS}
            "#
        );
        let created = sqlx::query_as::<_, ContestRow>(&sql)
            .bind(request.name)
            .bind(request.visibility.as_str())
            .bind(start_at)
            .bind(freeze_at)
            .bind(end_at)
            .fetch_one(&mut *transaction)
            .await
            .map_err(map_contest_write_error)?;
        record_audit(&mut transaction, actor_user_id, "CONTEST_CREATED", created.id, request_ip)
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest creation", error))?;
        created.response()
    }

    pub async fn clone_contest(
        &self,
        source_contest_id: i64,
        request: ValidatedContestClone,
        actor_user_id: i64,
        request_ip: IpAddr,
    ) -> Result<ContestCloneResponse, AppError> {
        let (start_at, freeze_at, end_at) = schedule_values(request.schedule);
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest clone", error))?;
        let source_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM contests WHERE id=$1 AND deleted_at IS NULL)",
        )
        .bind(source_contest_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("load clone source contest", error))?;
        if !source_exists {
            return Err(contest_not_found());
        }
        let sql = format!(
            "INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES($1,'DRAFT',$2,$3,$4,$5) RETURNING {CONTEST_COLUMNS}"
        );
        let target = sqlx::query_as::<_, ContestRow>(&sql)
            .bind(request.name)
            .bind(request.visibility.as_str())
            .bind(start_at)
            .bind(freeze_at)
            .bind(end_at)
            .fetch_one(&mut *transaction)
            .await
            .map_err(map_contest_write_error)?;
        let problems_copied = sqlx::query("INSERT INTO contest_problems(contest_id,problem_id,alias,display_order,color) SELECT $1,problem_id,alias,display_order,color FROM contest_problems WHERE contest_id=$2 ORDER BY display_order").bind(target.id).bind(source_contest_id).execute(&mut *transaction).await.map_err(|error| AppError::internal("clone contest problems", error))?.rows_affected() as i64;
        let teams_copied = if request.copy_teams {
            sqlx::query("INSERT INTO contest_teams(contest_id,team_id,participation_type,group_name) SELECT $1,ct.team_id,ct.participation_type,ct.group_name FROM contest_teams ct JOIN teams t ON t.id=ct.team_id AND t.deleted_at IS NULL WHERE ct.contest_id=$2 ORDER BY ct.id").bind(target.id).bind(source_contest_id).execute(&mut *transaction).await.map_err(|error| AppError::internal("clone contest teams", error))?.rows_affected() as i64
        } else {
            0
        };
        record_audit_result(
            &mut transaction,
            actor_user_id,
            "CONTEST_CLONED",
            target.id,
            request_ip,
            &format!("source:{source_contest_id}"),
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest clone", error))?;
        Ok(ContestCloneResponse {
            source_contest_id,
            contest: target.response()?,
            problems_copied,
            teams_copied,
        })
    }

    pub async fn update(
        &self,
        contest_id: i64,
        request: ValidatedUpdateContest,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ContestResponse, AppError> {
        if contest_id <= 0 {
            return Err(contest_not_found());
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest update", error))?;
        require_manage(&mut transaction, contest_id, actor).await?;
        let current = lock_active_contest(&mut transaction, contest_id).await?;
        let status: ContestStatus = current.status.parse()?;
        if request.changes_schedule() && !status.can_reschedule() {
            return Err(AppError::conflict(
                "CONTEST_SCHEDULE_LOCKED",
                "Contest schedule cannot be changed after the contest starts",
            ));
        }
        let schedule = if request.changes_schedule() {
            Some(
                ContestSchedule {
                    start_at: request.start_at.or(current.start_at).ok_or_else(|| {
                        AppError::validation("startAt", "is required to configure the schedule")
                    })?,
                    freeze_at: request.freeze_at.or(current.freeze_at).ok_or_else(|| {
                        AppError::validation("freezeAt", "is required to configure the schedule")
                    })?,
                    end_at: request.end_at.or(current.end_at).ok_or_else(|| {
                        AppError::validation("endAt", "is required to configure the schedule")
                    })?,
                }
                .validate()?,
            )
        } else {
            match (current.start_at, current.freeze_at, current.end_at) {
                (Some(start_at), Some(freeze_at), Some(end_at)) => {
                    Some(ContestSchedule { start_at, freeze_at, end_at })
                }
                _ => None,
            }
        };
        let (start_at, freeze_at, end_at) = schedule_values(schedule);
        let name = request.name.unwrap_or(current.name);
        let visibility = request
            .visibility
            .map_or(current.visibility, |visibility| visibility.as_str().to_owned());
        let version_guard = request
            .expected_version
            .map(|version| format!(" AND version = {version}"))
            .unwrap_or_default();
        let sql = format!(
            r#"
            UPDATE contests
            SET name = $1,
                visibility = $2,
                start_at = $3,
                freeze_at = $4,
                end_at = $5,
                version = version + 1,
                updated_at = now()
            WHERE id = $6 AND deleted_at IS NULL{version_guard}
            RETURNING {CONTEST_COLUMNS}
            "#
        );
        let updated = sqlx::query_as::<_, ContestRow>(&sql)
            .bind(name)
            .bind(visibility)
            .bind(start_at)
            .bind(freeze_at)
            .bind(end_at)
            .bind(contest_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(map_contest_write_error)?;
        let updated = match updated {
            Some(row) => row,
            None => {
                return Err(AppError::conflict(
                    "CONTEST_UPDATE_STALE",
                    "Contest was modified by another administrator; reload and retry",
                ));
            }
        };
        record_audit(&mut transaction, actor.id, "CONTEST_UPDATED", contest_id, request_ip).await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest update", error))?;
        updated.response()
    }

    pub async fn transition(
        &self,
        contest_id: i64,
        to: ContestStatus,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<LifecycleTransitionResponse, AppError> {
        if contest_id <= 0 {
            return Err(contest_not_found());
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest transition", error))?;
        require_manage(&mut transaction, contest_id, actor).await?;
        let current = lock_active_contest(&mut transaction, contest_id).await?;
        let from: ContestStatus = current.status.parse()?;
        let schedule = match (current.start_at, current.freeze_at, current.end_at) {
            (Some(start_at), Some(freeze_at), Some(end_at)) => {
                Some(DomainContestSchedule { start_at, freeze_at, end_at })
            }
            _ => None,
        };
        validate_contest_transition(from.domain(), to.domain(), schedule)
            .map_err(map_transition_error)?;
        if to == ContestStatus::FrozenConfig {
            let (problem_count, missing_colors) = sqlx::query_as::<_, (i64, i64)>(
                r#"
                SELECT count(*), count(*) FILTER (WHERE color IS NULL OR btrim(color) = '')
                FROM contest_problems WHERE contest_id = $1
                "#,
            )
            .bind(contest_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("validate frozen contest problems", error))?;
            if problem_count == 0 {
                return Err(AppError::validation(
                    "problems",
                    "must contain at least one assigned problem before configuration freeze",
                ));
            }
            if missing_colors > 0 {
                return Err(AppError::validation(
                    "problemColors",
                    "every assigned problem must have a balloon color before configuration freeze",
                ));
            }
        }
        if to == ContestStatus::Archived {
            let busy = sqlx::query_scalar::<_, bool>(r#"
                SELECT
                    EXISTS(SELECT 1 FROM submissions WHERE contest_id=$1 AND status IN('PENDING','JUDGING'))
                    OR EXISTS(SELECT 1 FROM batch_rejudge_tasks WHERE contest_id=$1 AND status IN('PENDING','RUNNING','PAUSED'))
                    OR EXISTS(SELECT 1 FROM print_requests WHERE contest_id=$1 AND status IN('REQUESTED','QUEUED','PRINTING'))
                    OR EXISTS(SELECT 1 FROM balloon_tasks WHERE contest_id=$1 AND upper(status) IN('PENDING','CLAIMED'))
                    OR EXISTS(SELECT 1 FROM resolver_runs WHERE contest_id=$1 AND status IN('READY','RUNNING','PAUSED'))
                    OR EXISTS(SELECT 1 FROM screen_groups WHERE contest_id=$1 AND playback_status IN('PLAYING','PAUSED'))
            "#).bind(contest_id).fetch_one(&mut *transaction).await.map_err(|error| AppError::internal("validate contest archive readiness", error))?;
            if busy {
                return Err(AppError::conflict(
                    "CONTEST_ARCHIVE_BUSY",
                    "Contest still has active judging, delivery, presentation, or ceremony work",
                ));
            }
        }
        let (version, transitioned_at) = sqlx::query_as::<_, (i64, OffsetDateTime)>(
            r#"
            UPDATE contests
            SET status = $1,
                version = version + 1,
                updated_at = now()
            WHERE id = $2 AND deleted_at IS NULL
            RETURNING version, updated_at
            "#,
        )
        .bind(to.as_str())
        .bind(contest_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("persist contest transition", error))?;
        let result = format!("{}->{}", from.as_str(), to.as_str());
        record_audit_result(
            &mut transaction,
            actor.id,
            "CONTEST_TRANSITIONED",
            contest_id,
            request_ip,
            &result,
        )
        .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest transition", error))?;
        Ok(LifecycleTransitionResponse { contest_id, from, to, version, transitioned_at })
    }

    pub async fn extend(
        &self,
        contest_id: i64,
        request: ContestExtensionRequest,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ContestExtensionResponse, AppError> {
        if contest_id <= 0 {
            return Err(contest_not_found());
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest extension", error))?;
        require_manage(&mut transaction, contest_id, actor).await?;
        let current = lock_active_contest(&mut transaction, contest_id).await?;
        let status: ContestStatus = current.status.parse()?;
        let previous_end_at = validate_contest_end_extension(
            status.domain(),
            current.end_at,
            request.expected_end_at,
            request.new_end_at,
        )
        .map_err(map_extension_error)?;
        let (version, updated_at) = sqlx::query_as::<_, (i64, OffsetDateTime)>(
            r#"
            UPDATE contests
            SET end_at = $1,
                version = version + 1,
                updated_at = now()
            WHERE id = $2 AND deleted_at IS NULL
            RETURNING version, updated_at
            "#,
        )
        .bind(request.new_end_at)
        .bind(contest_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("persist contest extension", error))?;
        let audit_result = format!(
            "{}->{}",
            previous_end_at.unix_timestamp(),
            request.new_end_at.unix_timestamp()
        );
        record_audit_result(
            &mut transaction,
            actor.id,
            "CONTEST_EXTENDED",
            contest_id,
            request_ip,
            &audit_result,
        )
        .await?;
        let payload = serde_json::json!({
            "previousEndAt": format_rfc3339(previous_end_at)?,
            "endAt": format_rfc3339(request.new_end_at)?,
            "updatedAt": format_rfc3339(updated_at)?,
        })
        .to_string();
        insert_realtime_outbox(
            &mut transaction,
            contest_id,
            "CONTEST_EXTENDED",
            "PUBLIC",
            &payload,
        )
        .await?;
        insert_realtime_outbox(&mut transaction, contest_id, "CONTEST_EXTENDED", "STAFF", &payload)
            .await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest extension", error))?;
        Ok(ContestExtensionResponse {
            contest_id,
            previous_end_at,
            end_at: request.new_end_at,
            version,
            updated_at,
        })
    }

    pub async fn delete(
        &self,
        contest_id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        if contest_id <= 0 {
            return Err(contest_not_found());
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin contest deletion", error))?;
        require_manage(&mut transaction, contest_id, actor).await?;
        lock_active_contest(&mut transaction, contest_id).await?;
        let has_teams = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM contest_teams WHERE contest_id = $1)",
        )
        .bind(contest_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("check contest teams before deletion", error))?;
        if has_teams {
            return Err(AppError::conflict(
                "CONTEST_HAS_ACTIVE_TEAMS",
                "Contest with assigned teams cannot be deleted",
            ));
        }
        sqlx::query(
            r#"
            UPDATE contests
            SET deleted_at = now(),
                updated_at = now(),
                version = version + 1
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(contest_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("soft delete contest", error))?;
        record_audit(&mut transaction, actor.id, "CONTEST_DELETED", contest_id, request_ip).await?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit contest deletion", error))
    }
}

struct ReadAccess {
    super_admin: bool,
    read_all: bool,
    contest_admin_id: Option<i64>,
    team_user_id: Option<i64>,
}

impl ReadAccess {
    fn for_user(user: Option<&AuthUser>) -> Self {
        let super_admin = user.is_some_and(|user| user.has_role("SUPER_ADMIN"));
        let read_all = user.is_some_and(can_read_all);
        let contest_admin_id =
            user.filter(|user| user.user_type == UserType::ContestAdmin).map(|user| user.id);
        let team_user_id = user.filter(|user| user.user_type == UserType::Team).map(|user| user.id);
        Self { super_admin, read_all, contest_admin_id, team_user_id }
    }
}

fn can_read_all(user: &AuthUser) -> bool {
    const READ_ALL_ROLES: [&str; 7] = [
        "SUPER_ADMIN",
        "JUDGE",
        "PRINTER",
        "BALLOON_STAFF",
        "RESOLVER_OPERATOR",
        "SCREEN_OPERATOR",
        "LIVE_OPERATOR",
    ];
    READ_ALL_ROLES.iter().any(|role| user.has_role(role))
}

async fn require_manage(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.has_role("SUPER_ADMIN") {
        return Ok(());
    }
    let valid_actor = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT enabled AND user_type = 'CONTEST_ADMIN'
        FROM users
        WHERE id = $1
        FOR SHARE
        "#,
    )
    .bind(actor.id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("lock contest administrator access", error))?
    .unwrap_or(false);
    if !valid_actor {
        return Err(contest_not_found());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM contest_admin_assignments
            WHERE user_id = $1 AND contest_id = $2
        )
        "#,
    )
    .bind(actor.id)
    .bind(contest_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check contest administrator scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

async fn lock_active_contest(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
) -> Result<ContestRow, AppError> {
    let sql = format!(
        r#"
        SELECT {CONTEST_COLUMNS}
        FROM contests
        WHERE id = $1 AND deleted_at IS NULL
        FOR UPDATE
        "#
    );
    sqlx::query_as::<_, ContestRow>(&sql)
        .bind(contest_id)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| AppError::internal("lock active contest", error))?
        .ok_or_else(contest_not_found)
}

async fn record_audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    action: &'static str,
    contest_id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    record_audit_result(transaction, actor_user_id, action, contest_id, request_ip, "success").await
}

async fn record_audit_result(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: i64,
    action: &'static str,
    contest_id: i64,
    request_ip: IpAddr,
    result: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO audit_logs
            (actor_user_id, action, target_type, target_id, request_ip, result)
        VALUES
            ($1, $2, 'CONTEST', $3, $4, $5)
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(contest_id.to_string())
    .bind(request_ip.to_string())
    .bind(result)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("record contest audit", error))
}

async fn insert_realtime_outbox(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    event_type: &'static str,
    scope: &'static str,
    payload_json: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO realtime_outbox
            (event_id, contest_id, event_type, scope, payload_json)
        VALUES
            ($1, $2, $3, $4, $5::jsonb)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(contest_id)
    .bind(event_type)
    .bind(scope)
    .bind(payload_json)
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(|error| AppError::internal("persist realtime outbox event", error))
}

fn schedule_values(
    schedule: Option<ContestSchedule>,
) -> (Option<time::OffsetDateTime>, Option<time::OffsetDateTime>, Option<time::OffsetDateTime>) {
    schedule.map_or((None, None, None), |schedule| {
        (Some(schedule.start_at), Some(schedule.freeze_at), Some(schedule.end_at))
    })
}

fn map_contest_write_error(error: sqlx::Error) -> AppError {
    if error.as_database_error().and_then(sqlx::error::DatabaseError::constraint)
        == Some("idx_contests_active_name_unique")
    {
        AppError::conflict("CONTEST_NAME_TAKEN", "Contest name is already in use")
    } else {
        AppError::internal("write contest", error)
    }
}

fn map_transition_error(error: ContestTransitionError) -> AppError {
    match error {
        ContestTransitionError::Invalid { .. } => AppError::conflict(
            "CONTEST_TRANSITION_INVALID",
            "The requested contest lifecycle transition is not allowed",
        ),
        ContestTransitionError::ScheduleRequired => AppError::validation(
            "schedule",
            "must be configured before freezing contest configuration",
        ),
        ContestTransitionError::InvalidSchedule(_) => {
            AppError::validation("schedule", "must be ordered before freezing configuration")
        }
    }
}

fn map_extension_error(error: ContestExtensionError) -> AppError {
    match error {
        ContestExtensionError::InvalidState(_) => AppError::conflict(
            "CONTEST_EXTENSION_STATUS_INVALID",
            "Contest can be extended only while running or paused",
        ),
        ContestExtensionError::EndTimeNotSet => {
            AppError::conflict("CONTEST_END_TIME_NOT_SET", "Contest end time is not configured")
        }
        ContestExtensionError::Stale => AppError::conflict(
            "CONTEST_EXTENSION_STALE",
            "Contest end time changed; reload before extending",
        ),
        ContestExtensionError::NotLater => AppError::conflict(
            "CONTEST_EXTENSION_NOT_LATER",
            "New contest end time must be later than the current end time",
        ),
    }
}

fn format_rfc3339(value: OffsetDateTime) -> Result<String, AppError> {
    value
        .format(&Rfc3339)
        .map_err(|error| AppError::internal("format realtime event timestamp", error))
}

fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::auth::model::{AuthUser, UserType};
    use crate::features::contests::model::{ContestVisibility, ValidatedContestClone};
    use sqlx::PgPool;
    use std::net::{IpAddr, Ipv4Addr};
    use time::{Duration, OffsetDateTime};

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn clone_copies_configuration_and_optionally_active_teams(pool: PgPool) {
        let actor=sqlx::query_scalar::<_,i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('clone-root','hash','Clone Root','SUPER_ADMIN') RETURNING id").fetch_one(&pool).await.expect("actor");
        let source=sqlx::query_scalar::<_,i64>("INSERT INTO contests(name,status,visibility) VALUES('Clone Source','ENDED','PRIVATE') RETURNING id").fetch_one(&pool).await.expect("source");
        let problem = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems(slug,title) VALUES('clone-a','Clone A') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("problem");
        sqlx::query("INSERT INTO contest_problems(contest_id,problem_id,alias,display_order,color) VALUES($1,$2,'A',1,'#ff0000')").bind(source).bind(problem).execute(&pool).await.expect("assignment");
        let team = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams(name,school) VALUES('Clone Team','School') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("team");
        sqlx::query("INSERT INTO contest_teams(contest_id,team_id,participation_type,group_name) VALUES($1,$2,'OFFICIAL','North')").bind(source).bind(team).execute(&pool).await.expect("roster");
        let start = OffsetDateTime::now_utc() + Duration::hours(1);
        let response = ContestService::new(pool.clone())
            .clone_contest(
                source,
                ValidatedContestClone {
                    name: "Clone Target".into(),
                    visibility: ContestVisibility::Public,
                    schedule: Some(ContestSchedule {
                        start_at: start,
                        freeze_at: start + Duration::hours(4),
                        end_at: start + Duration::hours(5),
                    }),
                    copy_teams: true,
                },
                actor,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .expect("clone");
        assert_eq!(response.source_contest_id, source);
        assert_eq!(response.contest.status, ContestStatus::Draft);
        assert_eq!(response.problems_copied, 1);
        assert_eq!(response.teams_copied, 1);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT color FROM contest_problems WHERE contest_id=$1"
            )
            .bind(response.contest.id)
            .fetch_one(&pool)
            .await
            .expect("color"),
            "#ff0000"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT group_name FROM contest_teams WHERE contest_id=$1"
            )
            .bind(response.contest.id)
            .fetch_one(&pool)
            .await
            .expect("group"),
            "North"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn archive_requires_quiescence_and_database_enforces_read_only(pool: PgPool) {
        let actor_id=sqlx::query_scalar::<_,i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('archive-root','hash','Archive Root','SUPER_ADMIN') RETURNING id").fetch_one(&pool).await.expect("actor");
        let contest=sqlx::query_scalar::<_,i64>("INSERT INTO contests(name,status,visibility) VALUES('Archive Contest','ENDED','PRIVATE') RETURNING id").fetch_one(&pool).await.expect("contest");
        let announcement=sqlx::query_scalar::<_,i64>("INSERT INTO announcements(contest_id,title,body,created_by) VALUES($1,'Final','Body',$2) RETURNING id").bind(contest).bind(actor_id).fetch_one(&pool).await.expect("announcement");
        let award_category=sqlx::query_scalar::<_,i64>("INSERT INTO award_categories(contest_id,code,name,display_order) VALUES($1,'CHAMPION','Champion',1) RETURNING id").bind(contest).fetch_one(&pool).await.expect("award category");
        let award_rule=sqlx::query_scalar::<_,i64>("INSERT INTO award_rules(category_id,rule_type,fixed_count) VALUES($1,'FIXED_COUNT',1) RETURNING id").bind(award_category).fetch_one(&pool).await.expect("award rule");
        let run = sqlx::query_scalar::<_, i64>(
            "INSERT INTO resolver_runs(contest_id,official) VALUES($1,true) RETURNING id",
        )
        .bind(contest)
        .fetch_one(&pool)
        .await
        .expect("resolver run");
        let actor = AuthUser {
            id: actor_id,
            username: "archive-root".into(),
            display_name: "Archive Root".into(),
            user_type: UserType::SuperAdmin,
            roles: vec![],
            password_reset_required: false,
        };
        let service = ContestService::new(pool.clone());
        assert!(
            service
                .transition(
                    contest,
                    ContestStatus::Archived,
                    &actor,
                    IpAddr::V4(Ipv4Addr::LOCALHOST)
                )
                .await
                .is_err()
        );
        sqlx::query("UPDATE resolver_runs SET status='COMPLETED',started_at=now(),completed_at=now() WHERE id=$1")
            .bind(run)
            .execute(&pool)
            .await
            .expect("complete resolver");
        let archived = service
            .transition(contest, ContestStatus::Archived, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .expect("archive");
        assert_eq!(archived.to, ContestStatus::Archived);
        assert!(
            sqlx::query("UPDATE announcements SET title='Changed' WHERE id=$1")
                .bind(announcement)
                .execute(&pool)
                .await
                .is_err()
        );
        assert!(sqlx::query("INSERT INTO announcements(contest_id,title,body,created_by) VALUES($1,'Late','Body',$2)").bind(contest).bind(actor_id).execute(&pool).await.is_err());
        assert!(
            sqlx::query("UPDATE award_rules SET fixed_count=2 WHERE id=$1")
                .bind(award_rule)
                .execute(&pool)
                .await
                .is_err()
        );
    }
}
