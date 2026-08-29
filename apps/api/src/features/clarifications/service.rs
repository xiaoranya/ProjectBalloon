use std::net::IpAddr;

use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    error::AppError,
    features::{
        announcements::{
            AnnouncementResponse, audit_tx as announcement_audit, ensure_open_tx,
            load as load_announcement, public_event_tx, validate_text,
        },
        auth::model::{AuthUser, UserType},
    },
};

use super::model::{ClarificationResponse, ConvertRequest, ValidatedAsk, ValidatedReply};

const RATE_LIMIT_MINUTES: i64 = 5;

pub struct ClarificationService {
    database: PgPool,
}

impl ClarificationService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub(super) async fn ask(
        &self,
        contest_id: i64,
        command: ValidatedAsk,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ClarificationResponse, AppError> {
        if actor.user_type != UserType::Team {
            return Err(AppError::forbidden(
                "TEAM_ACCOUNT_REQUIRED",
                "Only a team can ask a clarification",
            ));
        }
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin clarification ask", error))?;
        let (team_id, team_name, contest_status) = sqlx::query_as::<_, (i64, String, String)>(
            r#"
            SELECT team.id, team.name, contest.status FROM team_accounts account
            JOIN teams team ON team.id = account.team_id AND team.deleted_at IS NULL
            JOIN contest_teams roster ON roster.team_id = team.id AND roster.contest_id = $2
            JOIN contests contest ON contest.id = roster.contest_id AND contest.deleted_at IS NULL
            WHERE account.user_id = $1
        "#,
        )
        .bind(actor.id)
        .bind(contest_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("resolve clarification team", error))?
        .ok_or_else(clarification_not_found)?;
        if !matches!(contest_status.as_str(), "RUNNING" | "PAUSED") {
            return Err(AppError::conflict(
                "CLARIFICATION_CONTEST_NOT_OPEN",
                "Clarifications are only open while a contest is running or paused",
            ));
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("clarification:{contest_id}:{team_id}"))
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("lock clarification rate limit", error))?;
        let recent = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM clarifications WHERE contest_id = $1 AND team_id = $2 AND created_at > now() - make_interval(mins => $3::integer))",
        ).bind(contest_id).bind(team_id).bind(RATE_LIMIT_MINUTES).fetch_one(&mut *tx).await
            .map_err(|error| AppError::internal("check clarification rate limit", error))?;
        if recent {
            return Err(AppError::too_many_requests(
                "CLARIFICATION_RATE_LIMITED",
                "A team may ask one clarification every five minutes",
            ));
        }
        let problem_alias = if let Some(problem_id) = command.problem_id {
            Some(
                sqlx::query_scalar::<_, String>(
                    "SELECT alias FROM contest_problems WHERE contest_id = $1 AND problem_id = $2",
                )
                .bind(contest_id)
                .bind(problem_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|error| AppError::internal("resolve clarification problem", error))?
                .ok_or_else(|| {
                    AppError::not_found("PROBLEM_NOT_FOUND", "Contest problem was not found")
                })?,
            )
        } else {
            None
        };
        let id = sqlx::query_scalar::<_, i64>(r#"
            INSERT INTO clarifications
                (contest_id, team_id, team_name, scope, problem_id, problem_alias, question, status, asked_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'PENDING', $8) RETURNING id
        "#).bind(contest_id).bind(team_id).bind(team_name).bind(command.scope)
            .bind(command.problem_id).bind(problem_alias).bind(command.question).bind(actor.id)
            .fetch_one(&mut *tx).await
            .map_err(|error| AppError::internal("insert clarification", error))?;
        audit(&mut tx, actor.id, "CLARIFICATION_ASKED", id, request_ip).await?;
        realtime(&mut tx, contest_id, team_id, id, "ASKED").await?;
        tx.commit().await.map_err(|error| AppError::internal("commit clarification ask", error))?;
        load(&self.database, id).await
    }

    pub(super) async fn list_mine(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<ClarificationResponse>, AppError> {
        if actor.user_type != UserType::Team {
            return Err(AppError::forbidden(
                "TEAM_ACCOUNT_REQUIRED",
                "Only a team can view team clarifications",
            ));
        }
        sqlx::query_as::<_, ClarificationResponse>(safe_sql!(
            "{CLARIFICATION_SQL} JOIN team_accounts account ON account.team_id = clarification.team_id JOIN contest_teams roster ON roster.team_id = clarification.team_id AND roster.contest_id = clarification.contest_id WHERE clarification.contest_id = $1 AND account.user_id = $2 ORDER BY clarification.created_at DESC LIMIT 1000"
        )).bind(contest_id).bind(actor.id).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list team clarifications", error))
    }

    pub(super) async fn list_all(
        &self,
        contest_id: i64,
        status: Option<String>,
        actor: &AuthUser,
    ) -> Result<Vec<ClarificationResponse>, AppError> {
        require_staff_access_pool(&self.database, contest_id, actor).await?;
        sqlx::query_as::<_, ClarificationResponse>(safe_sql!(
            "{CLARIFICATION_SQL} WHERE clarification.contest_id = $1 AND ($2::text IS NULL OR clarification.status = $2) ORDER BY clarification.created_at DESC LIMIT 1000"
        )).bind(contest_id).bind(status.as_deref()).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list contest clarifications", error))
    }

    pub(super) async fn get(
        &self,
        id: i64,
        actor: &AuthUser,
    ) -> Result<ClarificationResponse, AppError> {
        let row = load(&self.database, id).await?;
        require_staff_access_pool(&self.database, row.contest_id, actor).await?;
        Ok(row)
    }

    pub(super) async fn reply(
        &self,
        id: i64,
        command: ValidatedReply,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<ClarificationResponse, AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin clarification reply", error))?;
        let (contest_id, team_id, status) = lock_context(&mut tx, id).await?;
        require_staff_access(&mut tx, contest_id, actor).await?;
        if status == "CLOSED" {
            return Err(AppError::conflict(
                "CLARIFICATION_CLOSED",
                "Closed clarification cannot be replied to",
            ));
        }
        sqlx::query("UPDATE clarifications SET reply = $2, reply_visibility = $3, replied_by = $4, replied_at = now(), status = 'ANSWERED', updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(command.reply).bind(command.visibility).bind(actor.id)
            .execute(&mut *tx).await.map_err(|error| AppError::internal("reply to clarification", error))?;
        audit(&mut tx, actor.id, "CLARIFICATION_REPLIED", id, request_ip).await?;
        realtime(&mut tx, contest_id, team_id, id, "REPLIED").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit clarification reply", error))?;
        load(&self.database, id).await
    }

    pub(super) async fn close(
        &self,
        id: i64,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<(), AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin clarification close", error))?;
        let (contest_id, team_id, status) = lock_context(&mut tx, id).await?;
        require_staff_access(&mut tx, contest_id, actor).await?;
        if status == "CLOSED" {
            return Err(AppError::conflict(
                "CLARIFICATION_CLOSED",
                "Clarification is already closed",
            ));
        }
        sqlx::query("UPDATE clarifications SET status = 'CLOSED', closed_by = $2, closed_at = now(), updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(actor.id).execute(&mut *tx).await
            .map_err(|error| AppError::internal("close clarification", error))?;
        audit(&mut tx, actor.id, "CLARIFICATION_CLOSED", id, request_ip).await?;
        realtime(&mut tx, contest_id, team_id, id, "CLOSED").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit clarification close", error))?;
        Ok(())
    }

    pub(super) async fn convert(
        &self,
        id: i64,
        request: ConvertRequest,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<AnnouncementResponse, AppError> {
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin clarification conversion", error))?;
        let row = sqlx::query_as::<
            _,
            (i64, i64, String, String, Option<String>, Option<String>, Option<i64>),
        >(
            r#"
            SELECT contest_id, team_id, status, question, reply, reply_visibility,
                   converted_announcement_id
            FROM clarifications WHERE id = $1 FOR UPDATE
        "#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock clarification conversion", error))?
        .ok_or_else(clarification_not_found)?;
        let (contest_id, team_id, status, question, reply, visibility, converted_id) = row;
        require_staff_access(&mut tx, contest_id, actor).await?;
        if converted_id.is_some() {
            return Err(AppError::conflict(
                "CLARIFICATION_ALREADY_CONVERTED",
                "Clarification has already been converted into an announcement",
            ));
        }
        if status != "ANSWERED" || visibility.as_deref() != Some("PUBLIC") {
            return Err(AppError::conflict(
                "CLARIFICATION_REPLY_NOT_PUBLIC",
                "Only an answered clarification with a PUBLIC reply can be converted",
            ));
        }
        ensure_open_tx(&mut tx, contest_id).await?;
        let default_title = question.chars().take(80).collect::<String>();
        let title = request.title.filter(|value| !value.trim().is_empty()).unwrap_or(default_title);
        let body =
            request.body.filter(|value| !value.trim().is_empty()).or(reply).ok_or_else(|| {
                AppError::conflict("CLARIFICATION_NOT_ANSWERED", "Clarification has no reply")
            })?;
        let (title, body) = validate_text(title, body)?;
        let announcement_id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO announcements
                (contest_id, title, body, pinned, status, created_by,
                 source_clarification_id, published_at)
            VALUES ($1, $2, $3, false, 'PUBLISHED', $4, $5, now()) RETURNING id
        "#,
        )
        .bind(contest_id)
        .bind(title)
        .bind(body)
        .bind(actor.id)
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("insert clarification announcement", error))?;
        sqlx::query("UPDATE clarifications SET converted_announcement_id = $2, updated_at = now(), version = version + 1 WHERE id = $1")
            .bind(id).bind(announcement_id).execute(&mut *tx).await
            .map_err(|error| AppError::internal("link clarification announcement", error))?;
        audit(&mut tx, actor.id, "CLARIFICATION_CONVERTED", id, request_ip).await?;
        announcement_audit(
            &mut tx,
            actor.id,
            "ANNOUNCEMENT_PUBLISHED",
            announcement_id,
            request_ip,
        )
        .await?;
        realtime(&mut tx, contest_id, team_id, id, "CONVERTED").await?;
        public_event_tx(&mut tx, contest_id, announcement_id, "PUBLISHED").await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit clarification conversion", error))?;
        load_announcement(&self.database, announcement_id).await
    }
}

const CLARIFICATION_SQL: &str = r#"
    SELECT clarification.id, clarification.contest_id, clarification.team_id,
           clarification.team_name, clarification.scope, clarification.problem_id,
           clarification.problem_alias, clarification.question, clarification.status,
           clarification.reply, clarification.reply_visibility,
           clarification.asked_by AS asked_by_user_id,
           clarification.replied_by AS replied_by_user_id, clarification.replied_at,
           clarification.converted_announcement_id, clarification.created_at,
           clarification.updated_at, clarification.version FROM clarifications clarification
           JOIN contests contest ON contest.id = clarification.contest_id
                                AND contest.deleted_at IS NULL
"#;

async fn load(database: &PgPool, id: i64) -> Result<ClarificationResponse, AppError> {
    if id <= 0 {
        return Err(clarification_not_found());
    }
    sqlx::query_as::<_, ClarificationResponse>(safe_sql!(
        "{CLARIFICATION_SQL} WHERE clarification.id = $1"
    ))
    .bind(id)
    .fetch_optional(database)
    .await
    .map_err(|error| AppError::internal("load clarification", error))?
    .ok_or_else(clarification_not_found)
}

async fn lock_context(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<(i64, i64, String), AppError> {
    sqlx::query_as(
        r#"
        SELECT clarification.contest_id, clarification.team_id, clarification.status
        FROM clarifications clarification
        JOIN contests contest
            ON contest.id = clarification.contest_id AND contest.deleted_at IS NULL
        WHERE clarification.id = $1
        FOR UPDATE OF clarification
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| AppError::internal("lock clarification", error))?
    .ok_or_else(clarification_not_found)
}

async fn require_staff_access_pool(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let mut tx = database
        .begin()
        .await
        .map_err(|error| AppError::internal("begin clarification access check", error))?;
    require_staff_access(&mut tx, contest_id, actor).await?;
    tx.commit()
        .await
        .map_err(|error| AppError::internal("commit clarification access check", error))
}

async fn require_staff_access(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let active = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contests WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(contest_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| AppError::internal("check clarification contest", error))?;
    if !active {
        return Err(clarification_not_found());
    }
    if actor.is_super_admin() {
        return Ok(());
    }
    // Judge workers are global staff operators; unlike contest managers
    // they are intentionally not present in contest_management_assignments.
    if actor.has_permission(crate::features::auth::permissions::CLARIFICATION_MANAGE) {
        return Ok(());
    }
    if !actor.has_permission(crate::features::auth::permissions::CONTEST_MANAGE) {
        return Err(clarification_not_found());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM contest_management_assignments WHERE contest_id = $1 AND user_id = $2)",
    ).bind(contest_id).bind(actor.id).fetch_one(&mut **tx).await
        .map_err(|error| AppError::internal("check clarification staff scope", error))?;
    if assigned { Ok(()) } else { Err(clarification_not_found()) }
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: i64,
    action: &str,
    id: i64,
    request_ip: IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs (actor_user_id, action, target_type, target_id, request_ip, result) VALUES ($1, $2, 'CLARIFICATION', $3, $4, 'success')")
        .bind(actor_id).bind(action).bind(id.to_string()).bind(request_ip.to_string())
        .execute(&mut **tx).await.map(|_| ())
        .map_err(|error| AppError::internal("record clarification audit", error))
}

async fn realtime(
    tx: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    team_id: i64,
    id: i64,
    action: &str,
) -> Result<(), AppError> {
    for (scope, recipient) in [("STAFF", None), ("TEAM", Some(team_id))] {
        sqlx::query("INSERT INTO realtime_outbox (event_id, contest_id, event_type, scope, team_id, payload_json) VALUES ($1, $2, 'CLARIFICATION_UPDATED', $3, $4, $5)")
            .bind(Uuid::new_v4()).bind(contest_id).bind(scope).bind(recipient)
            .bind(json!({"clarificationId": id, "action": action}))
            .execute(&mut **tx).await
            .map_err(|error| AppError::internal("enqueue clarification event", error))?;
    }
    Ok(())
}

fn clarification_not_found() -> AppError {
    AppError::not_found("CLARIFICATION_NOT_FOUND", "Clarification was not found")
}
