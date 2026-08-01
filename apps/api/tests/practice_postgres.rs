use project_balloon_api::features::judge_dispatch::{ApplyResultOutcome, JudgeResultProcessor};
use project_balloon_contracts::{
    JUDGE_RESULT_SCHEMA_VERSION, JudgeResult, JudgeRunResult, JudgeVerdict,
};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn practice_result_updates_personal_progress(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('practice-user','hash','Practice User','INDIVIDUAL') RETURNING id")
        .fetch_one(&pool).await.expect("user");
    let problem_id = sqlx::query_scalar::<_, i64>("INSERT INTO problems(slug,title,testdata_version,testdata_object_key,testdata_sha256) VALUES('practice-result','Practice Result',1,'problems/practice.zip',$1) RETURNING id")
        .bind("a".repeat(64)).fetch_one(&pool).await.expect("problem");
    sqlx::query("INSERT INTO problem_bank_entries(problem_id,visibility,tags,published_at) VALUES($1,'PUBLIC','[]',now())").bind(problem_id).execute(&pool).await.expect("public problem");
    sqlx::query("INSERT INTO problem_testdata_versions(problem_id,version,object_key,sha256) VALUES($1,1,'problems/practice.zip',$2)").bind(problem_id).bind("a".repeat(64)).execute(&pool).await.expect("testdata");
    let submission_id = sqlx::query_scalar::<_, i64>("INSERT INTO submissions(contest_id,problem_id,team_id,language,source_object_key,source_size_bytes,source_sha256,status,submission_scope,participant_user_id) VALUES(NULL,$1,NULL,'cpp','practice-source',10,$2,'PENDING','PRACTICE',$3) RETURNING id")
        .bind(problem_id).bind("b".repeat(64)).bind(user_id).fetch_one(&pool).await.expect("submission");
    let judgement_id = Uuid::new_v4();
    sqlx::query("INSERT INTO judgements(id,submission_id) VALUES($1,$2)")
        .bind(judgement_id)
        .bind(submission_id)
        .execute(&pool)
        .await
        .expect("judgement");
    let now = OffsetDateTime::now_utc();
    let result = JudgeResult {
        schema_version: JUDGE_RESULT_SCHEMA_VERSION,
        message_id: judgement_id,
        judgement_id,
        submission_id,
        worker_id: "practice-worker".into(),
        verdict: JudgeVerdict::Accepted,
        total_time_ms: 5,
        peak_memory_kb: 100,
        compile_log: None,
        started_at: now - Duration::SECOND,
        completed_at: now,
        runs: vec![JudgeRunResult {
            test_index: 1,
            verdict: JudgeVerdict::Accepted,
            time_ms: 5,
            memory_kb: 100,
            exit_code: Some(0),
            stderr_tail: None,
        }],
    };
    assert_eq!(
        JudgeResultProcessor::new(pool.clone()).apply(&result).await.expect("apply"),
        ApplyResultOutcome::Applied
    );
    let progress=sqlx::query_as::<_,(i32,i32,bool,i64)>("SELECT attempts,best_score,solved,last_submission_id FROM practice_problem_progress WHERE user_id=$1 AND problem_id=$2").bind(user_id).bind(problem_id).fetch_one(&pool).await.expect("progress");
    assert_eq!(progress, (1, 100, true, submission_id));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn accepted_result_completes_required_training_enrollment(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users(username,password_hash,display_name,user_type) VALUES('training-result-user','hash','Training Result User','INDIVIDUAL') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("user");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems(slug,title,testdata_version,testdata_object_key,testdata_sha256) VALUES('training-result','Training Result',1,'problems/training.zip',$1) RETURNING id",
    )
    .bind("a".repeat(64))
    .fetch_one(&pool)
    .await
    .expect("problem");
    sqlx::query("INSERT INTO problem_bank_entries(problem_id,visibility,tags,published_at) VALUES($1,'PUBLIC','[]',now())")
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("public problem");
    sqlx::query("INSERT INTO problem_testdata_versions(problem_id,version,object_key,sha256) VALUES($1,1,'problems/training.zip',$2)")
        .bind(problem_id)
        .bind("a".repeat(64))
        .execute(&pool)
        .await
        .expect("testdata");
    let set_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO training_sets(slug,title,visibility,created_by_user_id) VALUES('training-result-set','Training Result Set','PUBLIC',$1) RETURNING id",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("training set");
    sqlx::query(
        "INSERT INTO training_set_items(set_id,problem_id,position,required) VALUES($1,$2,1,true)",
    )
    .bind(set_id)
    .bind(problem_id)
    .execute(&pool)
    .await
    .expect("training item");
    let enrollment_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO training_enrollments(set_id,user_id,status) VALUES($1,$2,'ACTIVE') RETURNING id",
    )
    .bind(set_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("training enrollment");
    let submission_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO submissions(contest_id,problem_id,team_id,language,source_object_key,source_size_bytes,source_sha256,status,submission_scope,participant_user_id,training_enrollment_id) VALUES(NULL,$1,NULL,'cpp','practice-training-source',10,$2,'PENDING','PRACTICE',$3,$4) RETURNING id",
    )
    .bind(problem_id)
    .bind("b".repeat(64))
    .bind(user_id)
    .bind(enrollment_id)
    .fetch_one(&pool)
    .await
    .expect("submission");
    let judgement_id = Uuid::new_v4();
    sqlx::query("INSERT INTO judgements(id,submission_id) VALUES($1,$2)")
        .bind(judgement_id)
        .bind(submission_id)
        .execute(&pool)
        .await
        .expect("judgement");
    let now = OffsetDateTime::now_utc();
    let result = JudgeResult {
        schema_version: JUDGE_RESULT_SCHEMA_VERSION,
        message_id: judgement_id,
        judgement_id,
        submission_id,
        worker_id: "training-result-worker".into(),
        verdict: JudgeVerdict::Accepted,
        total_time_ms: 5,
        peak_memory_kb: 100,
        compile_log: None,
        started_at: now - Duration::SECOND,
        completed_at: now,
        runs: vec![JudgeRunResult {
            test_index: 1,
            verdict: JudgeVerdict::Accepted,
            time_ms: 5,
            memory_kb: 100,
            exit_code: Some(0),
            stderr_tail: None,
        }],
    };

    assert_eq!(
        JudgeResultProcessor::new(pool.clone()).apply(&result).await.expect("apply"),
        ApplyResultOutcome::Applied
    );
    let progress = sqlx::query_as::<_, (String, i32, i32, bool)>(
        "SELECT status, attempts, best_score, solved_at IS NOT NULL FROM training_progress WHERE enrollment_id=$1 AND problem_id=$2",
    )
    .bind(enrollment_id)
    .bind(problem_id)
    .fetch_one(&pool)
    .await
    .expect("training progress");
    assert_eq!(progress, ("SOLVED".into(), 1, 100, true));
    let enrollment_status =
        sqlx::query_scalar::<_, String>("SELECT status FROM training_enrollments WHERE id=$1")
            .bind(enrollment_id)
            .fetch_one(&pool)
            .await
            .expect("enrollment status");
    assert_eq!(enrollment_status, "COMPLETED");
}
