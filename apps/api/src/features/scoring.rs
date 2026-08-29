use std::{collections::HashSet, net::SocketAddr};

use axum::{
    Json,
    extract::{ConnectInfo, Path, State, rejection::JsonRejection},
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use utoipa::ToSchema;
use uuid::Uuid;

use project_balloon_contracts::{JudgeRunResult, JudgeVerdict};

use crate::{
    error::AppError,
    features::auth::{ContestManagerContext, model::AuthUser},
    state::AppState,
};
use axum::routing::get;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScoringPolicyRequest {
    scoring_mode: String,
    score_aggregation: String,
    feedback_policy: String,
}

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ScoringPolicyResponse {
    contest_id: i64,
    scoring_mode: String,
    score_aggregation: String,
    feedback_policy: String,
}

struct ValidatedScoringPolicy {
    scoring_mode: &'static str,
    score_aggregation: &'static str,
    feedback_policy: &'static str,
}

impl ScoringPolicyRequest {
    fn validate(self) -> Result<ValidatedScoringPolicy, AppError> {
        let scoring_mode = closed_value("scoringMode", &self.scoring_mode, &["ICPC", "OI", "IOI"])?;
        let score_aggregation =
            closed_value("scoreAggregation", &self.score_aggregation, &["BEST", "LAST"])?;
        let feedback_policy =
            closed_value("feedbackPolicy", &self.feedback_policy, &["FULL", "SCORE_ONLY", "NONE"])?;
        if scoring_mode == "ICPC" && score_aggregation != "BEST" {
            return Err(AppError::validation(
                "scoreAggregation",
                "ICPC contests must use BEST aggregation",
            ));
        }
        Ok(ValidatedScoringPolicy { scoring_mode, score_aggregation, feedback_policy })
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskRequest {
    subtask_key: String,
    name: String,
    display_order: i32,
    score_milli: i32,
    test_indexes: Vec<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskConfigurationRequest {
    max_score_milli: i32,
    subtasks: Vec<SubtaskRequest>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskResponse {
    id: i64,
    subtask_key: String,
    name: String,
    display_order: i32,
    score_milli: i32,
    test_indexes: Vec<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubtaskConfigurationResponse {
    contest_id: i64,
    problem_id: i64,
    max_score_milli: i32,
    subtasks: Vec<SubtaskResponse>,
}

struct ValidatedSubtask {
    key: String,
    name: String,
    display_order: i32,
    score_milli: i32,
    tests: Vec<i32>,
}

impl SubtaskConfigurationRequest {
    fn validate(self) -> Result<(i32, Vec<ValidatedSubtask>), AppError> {
        if !(1..=100_000_000).contains(&self.max_score_milli) {
            return Err(AppError::validation(
                "maxScoreMilli",
                "must contain a value between 1 and 100000000",
            ));
        }
        if self.subtasks.is_empty() || self.subtasks.len() > 100 {
            return Err(AppError::validation("subtasks", "must contain between 1 and 100 items"));
        }
        let mut keys = HashSet::new();
        let mut orders = HashSet::new();
        let mut all_tests = HashSet::new();
        let mut total = 0_i64;
        let mut subtasks = Vec::with_capacity(self.subtasks.len());
        for item in self.subtasks {
            let key = item.subtask_key.trim().to_ascii_uppercase();
            if key.is_empty()
                || key.len() > 32
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
                || !keys.insert(key.clone())
            {
                return Err(AppError::validation(
                    "subtaskKey",
                    "must be a unique 1 to 32 character uppercase identifier",
                ));
            }
            let name = item.name.trim().to_owned();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(AppError::validation(
                    "name",
                    "must contain between 1 and 120 characters",
                ));
            }
            if !(1..=1000).contains(&item.display_order) || !orders.insert(item.display_order) {
                return Err(AppError::validation(
                    "displayOrder",
                    "must be unique and between 1 and 1000",
                ));
            }
            if !(1..=self.max_score_milli).contains(&item.score_milli) {
                return Err(AppError::validation(
                    "scoreMilli",
                    "must be positive and no greater than maxScoreMilli",
                ));
            }
            if item.test_indexes.is_empty() || item.test_indexes.len() > 10_000 {
                return Err(AppError::validation(
                    "testIndexes",
                    "must contain between 1 and 10000 indexes",
                ));
            }
            let mut tests = item.test_indexes;
            tests.sort_unstable();
            tests.dedup();
            if tests.iter().any(|index| !(1..=10_000).contains(index)) {
                return Err(AppError::validation(
                    "testIndexes",
                    "must contain values between 1 and 10000",
                ));
            }
            if tests.iter().any(|index| !all_tests.insert(*index)) {
                return Err(AppError::validation(
                    "testIndexes",
                    "a test may belong to only one subtask",
                ));
            }
            total += i64::from(item.score_milli);
            subtasks.push(ValidatedSubtask {
                key,
                name,
                display_order: item.display_order,
                score_milli: item.score_milli,
                tests,
            });
        }
        if total != i64::from(self.max_score_milli) {
            return Err(AppError::validation(
                "subtasks",
                "subtask scores must sum to maxScoreMilli",
            ));
        }
        subtasks.sort_by_key(|subtask| subtask.display_order);
        Ok((self.max_score_milli, subtasks))
    }
}

#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/scoring-policy", operation_id = "getContestScoringPolicy", tag = "scoring", params(("contest_id" = i64, Path)), responses((status = 200, body = ScoringPolicyResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_policy(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path(contest_id): Path<i64>,
) -> Result<Json<ScoringPolicyResponse>, AppError> {
    require_manage_pool(state.database(), contest_id, context.user()).await?;
    Ok(Json(load_policy(state.database(), contest_id).await?))
}

#[utoipa::path(put, path = "/api/admin/contests/{contest_id}/scoring-policy", operation_id = "updateContestScoringPolicy", tag = "scoring", params(("contest_id" = i64, Path)), request_body = ScoringPolicyRequest, responses((status = 200, body = ScoringPolicyResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn update_policy(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(contest_id): Path<i64>,
    payload: Result<Json<ScoringPolicyRequest>, JsonRejection>,
) -> Result<Json<ScoringPolicyResponse>, AppError> {
    let Json(request) =
        payload.map_err(|_| AppError::validation("request", "must be a valid scoring policy"))?;
    let request = request.validate()?;
    let mut tx = state
        .database()
        .begin()
        .await
        .map_err(|error| AppError::internal("begin scoring policy update", error))?;
    lock_configurable(&mut tx, contest_id, context.user()).await?;
    let response = sqlx::query_as::<_, ScoringPolicyResponse>(
        "UPDATE contests SET scoring_mode=$2,score_aggregation=$3,feedback_policy=$4,updated_at=now(),version=version+1 WHERE id=$1 RETURNING id contest_id,scoring_mode,score_aggregation,feedback_policy",
    )
    .bind(contest_id)
    .bind(request.scoring_mode)
    .bind(request.score_aggregation)
    .bind(request.feedback_policy)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| AppError::internal("update scoring policy", error))?;
    audit(
        &mut tx,
        context.user().id,
        "CONTEST_SCORING_POLICY_UPDATED",
        "CONTEST",
        contest_id,
        peer.ip(),
    )
    .await?;
    tx.commit().await.map_err(|error| AppError::internal("commit scoring policy update", error))?;
    Ok(Json(response))
}

#[utoipa::path(get, path = "/api/admin/contests/{contest_id}/problems/{problem_id}/subtasks", operation_id = "getContestProblemSubtasks", tag = "scoring", params(("contest_id" = i64, Path), ("problem_id" = i64, Path)), responses((status = 200, body = SubtaskConfigurationResponse), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody)), security(("session_cookie" = [])))]
pub async fn get_subtasks(
    context: ContestManagerContext,
    State(state): State<AppState>,
    Path((contest_id, problem_id)): Path<(i64, i64)>,
) -> Result<Json<SubtaskConfigurationResponse>, AppError> {
    require_manage_pool(state.database(), contest_id, context.user()).await?;
    Ok(Json(load_subtasks(state.database(), contest_id, problem_id).await?))
}

#[utoipa::path(put, path = "/api/admin/contests/{contest_id}/problems/{problem_id}/subtasks", operation_id = "replaceContestProblemSubtasks", tag = "scoring", params(("contest_id" = i64, Path), ("problem_id" = i64, Path)), request_body = SubtaskConfigurationRequest, responses((status = 200, body = SubtaskConfigurationResponse), (status = 400, body = crate::error::ApiErrorBody), (status = 401, body = crate::error::ApiErrorBody), (status = 403, body = crate::error::ApiErrorBody), (status = 404, body = crate::error::ApiErrorBody), (status = 409, body = crate::error::ApiErrorBody)), security(("session_cookie" = [], "csrf_cookie" = [], "csrf_header" = [])))]
pub async fn replace_subtasks(
    context: ContestManagerContext,
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((contest_id, problem_id)): Path<(i64, i64)>,
    payload: Result<Json<SubtaskConfigurationRequest>, JsonRejection>,
) -> Result<Json<SubtaskConfigurationResponse>, AppError> {
    let Json(request) = payload
        .map_err(|_| AppError::validation("request", "must be a valid subtask configuration"))?;
    let (max_score, subtasks) = request.validate()?;
    let mut tx = state
        .database()
        .begin()
        .await
        .map_err(|error| AppError::internal("begin subtask replacement", error))?;
    lock_configurable(&mut tx, contest_id, context.user()).await?;
    let changed = sqlx::query(
        "UPDATE contest_problems SET max_score_milli=$3 WHERE contest_id=$1 AND problem_id=$2",
    )
    .bind(contest_id)
    .bind(problem_id)
    .bind(max_score)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::internal("update problem maximum score", error))?
    .rows_affected();
    if changed != 1 {
        return Err(AppError::not_found(
            "CONTEST_PROBLEM_NOT_FOUND",
            "Contest problem assignment was not found",
        ));
    }
    sqlx::query("DELETE FROM contest_problem_subtasks WHERE contest_id=$1 AND problem_id=$2")
        .bind(contest_id)
        .bind(problem_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| AppError::internal("replace problem subtasks", error))?;
    for subtask in subtasks {
        let id = sqlx::query_scalar::<_, i64>("INSERT INTO contest_problem_subtasks(contest_id,problem_id,subtask_key,name,display_order,score_milli) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
            .bind(contest_id).bind(problem_id).bind(&subtask.key).bind(&subtask.name)
            .bind(subtask.display_order).bind(subtask.score_milli).fetch_one(&mut *tx).await
            .map_err(|error| AppError::internal("insert problem subtask", error))?;
        for test_index in subtask.tests {
            sqlx::query(
                "INSERT INTO contest_problem_subtask_tests(subtask_id,test_index) VALUES($1,$2)",
            )
            .bind(id)
            .bind(test_index)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("insert subtask test", error))?;
        }
    }
    audit(
        &mut tx,
        context.user().id,
        "CONTEST_PROBLEM_SUBTASKS_REPLACED",
        "CONTEST_PROBLEM",
        problem_id,
        peer.ip(),
    )
    .await?;
    tx.commit().await.map_err(|error| AppError::internal("commit subtask replacement", error))?;
    Ok(Json(load_subtasks(state.database(), contest_id, problem_id).await?))
}

async fn load_policy(
    database: &PgPool,
    contest_id: i64,
) -> Result<ScoringPolicyResponse, AppError> {
    sqlx::query_as("SELECT id contest_id,scoring_mode,score_aggregation,feedback_policy FROM contests WHERE id=$1 AND deleted_at IS NULL")
        .bind(contest_id).fetch_optional(database).await
        .map_err(|error| AppError::internal("load scoring policy", error))?
        .ok_or_else(contest_not_found)
}

async fn load_subtasks(
    database: &PgPool,
    contest_id: i64,
    problem_id: i64,
) -> Result<SubtaskConfigurationResponse, AppError> {
    let max_score_milli = sqlx::query_scalar::<_, i32>(
        "SELECT max_score_milli FROM contest_problems WHERE contest_id=$1 AND problem_id=$2",
    )
    .bind(contest_id)
    .bind(problem_id)
    .fetch_optional(database)
    .await
    .map_err(|error| AppError::internal("load problem maximum score", error))?
    .ok_or_else(|| {
        AppError::not_found("CONTEST_PROBLEM_NOT_FOUND", "Contest problem assignment was not found")
    })?;
    let rows = sqlx::query_as::<_, (i64, String, String, i32, i32, Vec<i32>)>(
        r#"SELECT subtask.id,subtask.subtask_key,subtask.name,subtask.display_order,subtask.score_milli,
                  array_agg(test.test_index ORDER BY test.test_index)::integer[] test_indexes
           FROM contest_problem_subtasks subtask
           JOIN contest_problem_subtask_tests test ON test.subtask_id=subtask.id
           WHERE subtask.contest_id=$1 AND subtask.problem_id=$2
           GROUP BY subtask.id ORDER BY subtask.display_order"#,
    ).bind(contest_id).bind(problem_id).fetch_all(database).await
      .map_err(|error| AppError::internal("load problem subtasks", error))?;
    Ok(SubtaskConfigurationResponse {
        contest_id,
        problem_id,
        max_score_milli,
        subtasks: rows
            .into_iter()
            .map(|(id, subtask_key, name, display_order, score_milli, test_indexes)| {
                SubtaskResponse { id, subtask_key, name, display_order, score_milli, test_indexes }
            })
            .collect(),
    })
}

async fn lock_configurable(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM contests WHERE id=$1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(contest_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("lock scoring configuration", error))?
    .ok_or_else(contest_not_found)?;
    require_manage_tx(transaction, contest_id, actor).await?;
    if status != "DRAFT" {
        return Err(AppError::conflict(
            "CONTEST_SCORING_CONFIG_FROZEN",
            "Scoring configuration can be changed only in DRAFT",
        ));
    }
    Ok(())
}

async fn require_manage_pool(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.is_super_admin() {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM contests WHERE id=$1 AND deleted_at IS NULL)",
        )
        .bind(contest_id)
        .fetch_one(database)
        .await
        .map_err(|error| AppError::internal("check scoring contest", error))?;
        return if exists { Ok(()) } else { Err(contest_not_found()) };
    }
    let assigned = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM contest_management_assignments assignment JOIN contests contest ON contest.id=assignment.contest_id AND contest.deleted_at IS NULL WHERE assignment.user_id=$1 AND assignment.contest_id=$2)")
        .bind(actor.id).bind(contest_id).fetch_one(database).await
        .map_err(|error| AppError::internal("check scoring management scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

async fn require_manage_tx(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.is_super_admin() {
        return Ok(());
    }
    let assigned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM contest_management_assignments WHERE user_id=$1 AND contest_id=$2)",
    )
    .bind(actor.id)
    .bind(contest_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check scoring mutation scope", error))?;
    if assigned { Ok(()) } else { Err(contest_not_found()) }
}

async fn audit(
    transaction: &mut Transaction<'_, Postgres>,
    actor: i64,
    action: &str,
    target_type: &str,
    target_id: i64,
    ip: std::net::IpAddr,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO audit_logs(actor_user_id,action,target_type,target_id,request_ip,result) VALUES($1,$2,$3,$4,$5,'success')")
        .bind(actor).bind(action).bind(target_type).bind(target_id).bind(ip.to_string()).execute(&mut **transaction).await
        .map(|_| ()).map_err(|error| AppError::internal("record scoring audit", error))
}

fn closed_value(
    field: &'static str,
    value: &str,
    allowed: &[&'static str],
) -> Result<&'static str, AppError> {
    let normalized = value.trim().to_ascii_uppercase();
    allowed
        .iter()
        .copied()
        .find(|candidate| *candidate == normalized)
        .ok_or_else(|| AppError::validation(field, "contains an unsupported value"))
}

fn contest_not_found() -> AppError {
    AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found")
}

pub(crate) async fn score_judgement(
    transaction: &mut Transaction<'_, Postgres>,
    judgement_id: Uuid,
    contest_id: i64,
    problem_id: i64,
    final_verdict: JudgeVerdict,
    runs: &[JudgeRunResult],
) -> Result<i32, sqlx::Error> {
    let (scoring_mode, max_score_milli) = sqlx::query_as::<_, (String, i32)>(
        "SELECT contest.scoring_mode,assignment.max_score_milli FROM contests contest JOIN contest_problems assignment ON assignment.contest_id=contest.id WHERE contest.id=$1 AND assignment.problem_id=$2",
    )
    .bind(contest_id)
    .bind(problem_id)
    .fetch_one(&mut **transaction)
    .await?;

    sqlx::query("DELETE FROM judgement_subtask_scores WHERE judgement_id=$1")
        .bind(judgement_id)
        .execute(&mut **transaction)
        .await?;

    let score = if scoring_mode == "ICPC" {
        if final_verdict == JudgeVerdict::Accepted { max_score_milli } else { 0 }
    } else {
        let subtasks = sqlx::query_as::<_, (i64, i32, Vec<i32>)>(
            r#"SELECT subtask.id,subtask.score_milli,
                      array_agg(test.test_index ORDER BY test.test_index)::integer[]
               FROM contest_problem_subtasks subtask
               JOIN contest_problem_subtask_tests test ON test.subtask_id=subtask.id
               WHERE subtask.contest_id=$1 AND subtask.problem_id=$2
               GROUP BY subtask.id,subtask.display_order
               ORDER BY subtask.display_order"#,
        )
        .bind(contest_id)
        .bind(problem_id)
        .fetch_all(&mut **transaction)
        .await?;
        if subtasks.is_empty() {
            if final_verdict == JudgeVerdict::Accepted { max_score_milli } else { 0 }
        } else {
            let verdicts: std::collections::HashMap<i32, JudgeVerdict> =
                runs.iter().map(|run| (run.test_index, run.verdict)).collect();
            let mut score = 0_i32;
            for (subtask_id, available_score, test_indexes) in subtasks {
                let passed = test_indexes
                    .iter()
                    .filter(|index| verdicts.get(index) == Some(&JudgeVerdict::Accepted))
                    .count();
                let subtask_score = if passed == test_indexes.len() { available_score } else { 0 };
                score = score.saturating_add(subtask_score);
                sqlx::query("INSERT INTO judgement_subtask_scores(judgement_id,subtask_id,score_milli,passed_tests,total_tests) VALUES($1,$2,$3,$4,$5)")
                    .bind(judgement_id)
                    .bind(subtask_id)
                    .bind(subtask_score)
                    .bind(i32::try_from(passed).unwrap_or(i32::MAX))
                    .bind(i32::try_from(test_indexes.len()).unwrap_or(i32::MAX))
                    .execute(&mut **transaction)
                    .await?;
            }
            score.min(max_score_milli)
        }
    };
    sqlx::query("UPDATE judgements SET score_milli=$2 WHERE id=$1")
        .bind(judgement_id)
        .bind(score)
        .execute(&mut **transaction)
        .await?;
    Ok(score)
}

/// Routes owned by this feature, assembled by the root router.
pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route(
            "/api/admin/contests/{contest_id}/scoring-policy",
            get(get_policy).put(update_policy),
        )
        .route(
            "/api/admin/contests/{contest_id}/problems/{problem_id}/subtasks",
            get(get_subtasks).put(replace_subtasks),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_and_subtasks_are_closed_and_exact() {
        assert!(
            ScoringPolicyRequest {
                scoring_mode: "oi".into(),
                score_aggregation: "last".into(),
                feedback_policy: "none".into()
            }
            .validate()
            .is_ok()
        );
        assert!(
            ScoringPolicyRequest {
                scoring_mode: "icpc".into(),
                score_aggregation: "last".into(),
                feedback_policy: "full".into()
            }
            .validate()
            .is_err()
        );
        let request = SubtaskConfigurationRequest {
            max_score_milli: 100_000,
            subtasks: vec![
                SubtaskRequest {
                    subtask_key: "basic".into(),
                    name: "Basic".into(),
                    display_order: 1,
                    score_milli: 40_000,
                    test_indexes: vec![1, 2],
                },
                SubtaskRequest {
                    subtask_key: "full".into(),
                    name: "Full".into(),
                    display_order: 2,
                    score_milli: 60_000,
                    test_indexes: vec![3],
                },
            ],
        };
        let (_, subtasks) = request.validate().expect("valid subtasks");
        assert_eq!(subtasks[0].key, "BASIC");
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn io_score_uses_all_or_nothing_subtasks(pool: PgPool) {
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at,scoring_mode) VALUES('IOI score','RUNNING','PRIVATE',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours','IOI') RETURNING id",
        ).fetch_one(&pool).await.expect("contest");
        let problem_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO problems(slug,title) VALUES('ioi-score','IOI score') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("problem");
        sqlx::query("INSERT INTO contest_problems(contest_id,problem_id,alias,display_order,max_score_milli) VALUES($1,$2,'A',1,100000)")
            .bind(contest_id).bind(problem_id).execute(&pool).await.expect("assignment");
        let subtask_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contest_problem_subtasks(contest_id,problem_id,subtask_key,name,display_order,score_milli) VALUES($1,$2,'BASIC','Basic',1,100000) RETURNING id",
        ).bind(contest_id).bind(problem_id).fetch_one(&pool).await.expect("subtask");
        sqlx::query(
            "INSERT INTO contest_problem_subtask_tests(subtask_id,test_index) VALUES($1,1),($1,2)",
        )
        .bind(subtask_id)
        .execute(&pool)
        .await
        .expect("tests");
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams(name) VALUES('IOI score team') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("team");
        let judgement_id = Uuid::new_v4();
        let submission_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO submissions(contest_id,problem_id,team_id,language,source_object_key,source_size_bytes,source_sha256,status) VALUES($1,$2,$3,'cpp','sources/ioi.cpp',1,$4,'JUDGING') RETURNING id",
        ).bind(contest_id).bind(problem_id).bind(team_id).bind("a".repeat(64)).fetch_one(&pool).await.expect("submission query");
        sqlx::query("INSERT INTO judgements(id,submission_id) VALUES($1,$2)")
            .bind(judgement_id)
            .bind(submission_id)
            .execute(&pool)
            .await
            .expect("judgement");
        let mut tx = pool.begin().await.expect("transaction");
        let score = score_judgement(
            &mut tx,
            judgement_id,
            contest_id,
            problem_id,
            JudgeVerdict::WrongAnswer,
            &[
                JudgeRunResult {
                    test_index: 1,
                    verdict: JudgeVerdict::Accepted,
                    time_ms: 1,
                    memory_kb: 1,
                    exit_code: Some(0),
                    stderr_tail: None,
                },
                JudgeRunResult {
                    test_index: 2,
                    verdict: JudgeVerdict::WrongAnswer,
                    time_ms: 1,
                    memory_kb: 1,
                    exit_code: Some(1),
                    stderr_tail: None,
                },
            ],
        )
        .await
        .expect("score");
        assert_eq!(score, 0);
        tx.rollback().await.expect("rollback");
    }
}
