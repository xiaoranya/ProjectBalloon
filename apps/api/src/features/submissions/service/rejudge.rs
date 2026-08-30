use super::*;

use crate::features::submissions::SubmissionStatus;

impl SubmissionService {
    pub async fn rejudge(
        &self,
        contest_id: i64,
        submission_id: i64,
        request: RejudgeRequest,
        actor: &AuthUser,
        request_ip: IpAddr,
    ) -> Result<RejudgeResponse, AppError> {
        self.rejudge_internal(contest_id, submission_id, request, actor, request_ip, None).await
    }

    pub(crate) async fn rejudge_batch_item(
        &self,
        contest_id: i64,
        submission_id: i64,
        expected_judgement_id: Uuid,
        actor: &AuthUser,
        batch_item_id: i64,
    ) -> Result<RejudgeResponse, AppError> {
        self.rejudge_internal(
            contest_id,
            submission_id,
            RejudgeRequest { expected_judgement_id },
            actor,
            "0.0.0.0".parse().map_err(|error| AppError::internal("parse batch audit IP", error))?,
            Some(batch_item_id),
        )
        .await
    }

    async fn rejudge_internal(
        &self,
        contest_id: i64,
        submission_id: i64,
        request: RejudgeRequest,
        actor: &AuthUser,
        request_ip: IpAddr,
        batch_item_id: Option<i64>,
    ) -> Result<RejudgeResponse, AppError> {
        if contest_id <= 0 || submission_id <= 0 {
            return Err(rejudge_submission_not_found());
        }
        let mut transaction = self.database.begin().await.map_err(|error| {
            AppError::internal("begin submission rejudge", error)
                .with_contest_id(contest_id)
                .with_submission_id(submission_id)
                .with_user_id(actor.id)
        })?;
        if !actor.is_super_admin() {
            let assigned = sqlx::query_scalar::<_, bool>(
                r#"
                    SELECT EXISTS (
                        SELECT 1 FROM contest_management_assignments
                        WHERE contest_id = $1 AND user_id = $2
                    )
                    "#,
            )
            .bind(contest_id)
            .bind(actor.id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| {
                AppError::internal("check rejudge administrator scope", error)
                    .with_contest_id(contest_id)
                    .with_submission_id(submission_id)
                    .with_user_id(actor.id)
            })?;
            if !assigned {
                return Err(rejudge_submission_not_found());
            }
        }
        if let Some(batch_item_id) = batch_item_id {
            let existing = sqlx::query_as::<_, (Uuid, OffsetDateTime, Uuid)>(
                r#"
                    SELECT judgement.id, judgement.created_at, item.old_judgement_id
                    FROM judgements judgement
                    JOIN batch_rejudge_items item ON item.id = judgement.batch_rejudge_item_id
                    WHERE judgement.batch_rejudge_item_id = $1
                      AND judgement.submission_id = $2
                    "#,
            )
            .bind(batch_item_id)
            .bind(submission_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| {
                AppError::internal("recover completed batch rejudge item", error)
                    .with_contest_id(contest_id)
                    .with_submission_id(submission_id)
                    .with_user_id(actor.id)
            })?;
            if let Some((judgement_id, queued_at, previous_judgement_id)) = existing {
                transaction.commit().await.map_err(|error| {
                    AppError::internal("commit recovered batch rejudge", error)
                        .with_contest_id(contest_id)
                        .with_submission_id(submission_id)
                        .with_user_id(actor.id)
                })?;
                return Ok(RejudgeResponse {
                    submission_id,
                    previous_judgement_id,
                    judgement_id,
                    status: SubmissionStatus::Pending.as_str(),
                    queued_at,
                });
            }
        }
        let context = sqlx::query_as::<_, RejudgeContext>(
            r#"
                SELECT contest.status AS contest_status,
                       submission.team_id,
                       submission.problem_id,
                       submission.language,
                       submission.source_object_key,
                       submission.source_sha256,
                       problem.time_limit_ms,
                       problem.memory_limit_mb,
                       problem.output_limit_kb,
                       problem.languages,
                       version.version AS testdata_version,
                       version.object_key AS testdata_object_key,
                       version.sha256 AS testdata_sha256,
                       problem.judge_mode,
                       problem.interactor_object_key,
                       problem.interactor_sha256,
                       judgement.id AS active_judgement_id,
                       judgement.completed_at AS active_completed_at
                FROM submissions submission
                JOIN contests contest
                  ON contest.id = submission.contest_id AND contest.deleted_at IS NULL
                JOIN contest_problems assignment
                  ON assignment.contest_id = contest.id
                 AND assignment.problem_id = submission.problem_id
                JOIN problems problem
                  ON problem.id = submission.problem_id AND problem.deleted_at IS NULL
                JOIN problem_testdata_versions version
                  ON version.problem_id = problem.id
                 AND version.version = problem.testdata_version
                 AND version.object_key = problem.testdata_object_key
                 AND version.sha256 = problem.testdata_sha256
                JOIN judgements judgement
                  ON judgement.submission_id = submission.id
                 AND judgement.active_marker IS TRUE
                WHERE submission.id = $1 AND submission.contest_id = $2
                FOR UPDATE OF submission, judgement
                "#,
        )
        .bind(submission_id)
        .bind(contest_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| {
            AppError::internal("lock submission for rejudge", error)
                .with_contest_id(contest_id)
                .with_submission_id(submission_id)
                .with_user_id(actor.id)
        })?
        .ok_or_else(rejudge_submission_not_found)?;
        if context.contest_status == "ARCHIVED" {
            return Err(AppError::conflict(
                "CONTEST_ARCHIVED",
                "Archived contest submissions cannot be rejudged",
            ));
        }
        if context.active_judgement_id != request.expected_judgement_id {
            return Err(AppError::conflict(
                "JUDGEMENT_VERSION_STALE",
                "The active judgement changed; reload the submission before rejudging",
            ));
        }
        if context.active_completed_at.is_none() {
            return Err(AppError::conflict(
                "JUDGEMENT_NOT_FINAL",
                "Only a completed judgement can be rejudged",
            ));
        }
        require_language(&context.languages, &context.language)?;
        let source_sha256 = context.source_sha256.as_deref().ok_or_else(|| {
            AppError::conflict(
                "SUBMISSION_SOURCE_UNAVAILABLE",
                "The submission has no verified source hash and cannot be rejudged",
            )
        })?;

        sqlx::query(
            r#"
                UPDATE judgements
                SET superseded = true, active_marker = NULL, version = version + 1
                WHERE id = $1 AND active_marker IS TRUE
                "#,
        )
        .bind(context.active_judgement_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            AppError::internal("supersede previous judgement", error)
                .with_contest_id(contest_id)
                .with_submission_id(submission_id)
                .with_judgement_id(context.active_judgement_id)
                .with_user_id(actor.id)
        })?;
        sqlx::query(
            r#"
                UPDATE submission_outbox
                SET status = 'CANCELLED', last_error = 'superseded by rejudge',
                    lease_owner = NULL, lease_until = NULL, version = version + 1
                WHERE judgement_id = $1 AND status <> 'SENT'
                "#,
        )
        .bind(context.active_judgement_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            AppError::internal("cancel previous judge task", error)
                .with_contest_id(contest_id)
                .with_submission_id(submission_id)
                .with_judgement_id(context.active_judgement_id)
                .with_user_id(actor.id)
        })?;
        let judgement_id = Uuid::new_v4();
        let queued_at = sqlx::query_scalar::<_, OffsetDateTime>(
                "INSERT INTO judgements (id, submission_id, batch_rejudge_item_id) VALUES ($1, $2, $3) RETURNING created_at",
            )
            .bind(judgement_id)
            .bind(submission_id)
            .bind(batch_item_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| {
                AppError::internal("insert rejudge judgement", error)
                    .with_contest_id(contest_id)
                    .with_submission_id(submission_id)
                    .with_judgement_id(judgement_id)
                    .with_user_id(actor.id)
            })?;
        // Rejudging is the documented administrative exemption to the
        // submission state machine: any state resets to Pending while the
        // previous judgement is superseded.
        sqlx::query(
            "UPDATE submissions SET status = $2, verdict = NULL, judged_at = NULL WHERE id = $1",
        )
        .bind(submission_id)
        .bind(SubmissionStatus::Pending.as_str())
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            AppError::internal("reset submission for rejudge", error)
                .with_contest_id(contest_id)
                .with_submission_id(submission_id)
                .with_judgement_id(judgement_id)
                .with_user_id(actor.id)
        })?;
        scoreboard::rebuild_cell(&mut transaction, contest_id, context.team_id, context.problem_id)
            .await
            .map_err(|error| {
                AppError::internal("rollback scoreboard for rejudge", error)
                    .with_contest_id(contest_id)
                    .with_submission_id(submission_id)
                    .with_judgement_id(judgement_id)
                    .with_user_id(actor.id)
            })?;
        let task = JudgeTask {
            schema_version: JUDGE_TASK_SCHEMA_VERSION,
            judgement_id,
            submission_id,
            problem_id: context.problem_id,
            testdata_version: context.testdata_version,
            testdata_object_key: context.testdata_object_key,
            testdata_sha256: context.testdata_sha256,
            source_object_key: context.source_object_key,
            source_sha256: source_sha256.to_owned(),
            language: context.language.clone(),
            time_limit_ms: context.time_limit_ms,
            memory_limit_mb: context.memory_limit_mb,
            output_limit_kb: context.output_limit_kb,
            language_multiplier: language_multiplier(&context.language),
            judge_mode: parse_judge_mode(&context.judge_mode)?,
            interactor_object_key: context.interactor_object_key.clone(),
            interactor_sha256: context.interactor_sha256.clone(),
        };
        task.validate().map_err(|error| {
            AppError::internal("validate rejudge task", error)
                .with_submission_id(submission_id)
                .with_judgement_id(judgement_id)
        })?;
        let payload = serde_json::to_string(&task).map_err(|error| {
            AppError::internal("serialize rejudge task", error)
                .with_submission_id(submission_id)
                .with_judgement_id(judgement_id)
        })?;
        sqlx::query(
                "INSERT INTO submission_outbox (judgement_id, submission_id, payload) VALUES ($1, $2, $3)",
            )
            .bind(judgement_id)
            .bind(submission_id)
            .bind(payload)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                AppError::internal("enqueue rejudge task", error)
                    .with_contest_id(contest_id)
                    .with_submission_id(submission_id)
                    .with_judgement_id(judgement_id)
                    .with_user_id(actor.id)
            })?;
        for (scope, recipient) in [("TEAM", Some(context.team_id)), ("STAFF", None)] {
            sqlx::query(
                r#"
                    INSERT INTO realtime_outbox
                        (event_id, contest_id, event_type, scope, team_id, payload_json)
                    VALUES ($1, $2, 'SUBMISSION_REJUDGED', $3, $4, $5)
                    "#,
            )
            .bind(Uuid::new_v4())
            .bind(contest_id)
            .bind(scope)
            .bind(recipient)
            .bind(json!({
                "submissionId": submission_id,
                "previousJudgementId": context.active_judgement_id,
                "judgementId": judgement_id,
                "status": SubmissionStatus::Pending.as_str()
            }))
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                AppError::internal("enqueue rejudge realtime event", error)
                    .with_contest_id(contest_id)
                    .with_submission_id(submission_id)
                    .with_judgement_id(judgement_id)
                    .with_user_id(actor.id)
            })?;
        }
        sqlx::query(
            r#"
                INSERT INTO audit_logs
                    (actor_user_id, action, target_type, target_id, request_ip, result)
                VALUES ($1, 'SUBMISSION_REJUDGED', 'SUBMISSION', $2, $3, 'success')
                "#,
        )
        .bind(actor.id)
        .bind(submission_id.to_string())
        .bind(request_ip.to_string())
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            AppError::internal("record rejudge audit", error)
                .with_contest_id(contest_id)
                .with_submission_id(submission_id)
                .with_judgement_id(judgement_id)
                .with_user_id(actor.id)
        })?;
        transaction.commit().await.map_err(|error| {
            AppError::internal("commit submission rejudge", error)
                .with_contest_id(contest_id)
                .with_submission_id(submission_id)
                .with_judgement_id(judgement_id)
                .with_user_id(actor.id)
        })?;
        Ok(RejudgeResponse {
            submission_id,
            previous_judgement_id: context.active_judgement_id,
            judgement_id,
            status: SubmissionStatus::Pending.as_str(),
            queued_at,
        })
    }
}
