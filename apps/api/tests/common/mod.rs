//! Shared fixtures for the PostgreSQL-backed integration tests. Each helper
//! wraps one insert so the test binaries do not repeat the same SQL literals.
//! Different binaries import different subsets, so unused helpers are allowed.

#![allow(dead_code)]

use project_balloon_contracts::{
    JUDGE_RESULT_SCHEMA_VERSION, JudgeResult, JudgeRunResult, JudgeVerdict,
};
use sqlx::PgPool;
use time::Duration;
use uuid::Uuid;

pub async fn insert_user(
    pool: &PgPool,
    username: &str,
    display_name: &str,
    user_type: &str,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO users(username, password_hash, display_name, user_type) VALUES($1, 'hash', $2, $3) RETURNING id",
    )
    .bind(username)
    .bind(display_name)
    .bind(user_type)
    .fetch_one(pool)
    .await
    .expect("insert user")
}

pub async fn insert_contest(pool: &PgPool, name: &str, status: &str, visibility: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests(name, status, visibility) VALUES($1, $2, $3) RETURNING id",
    )
    .bind(name)
    .bind(status)
    .bind(visibility)
    .fetch_one(pool)
    .await
    .expect("insert contest")
}

pub async fn insert_team(pool: &PgPool, name: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("INSERT INTO teams(name) VALUES($1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("insert team")
}

/// Inserts a problem with one testdata version and, when `bank_visibility` is
/// set, a matching problem-bank entry.
pub async fn insert_problem(
    pool: &PgPool,
    slug: &str,
    title: &str,
    testdata_object_key: &str,
    bank_visibility: Option<&str>,
) -> i64 {
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems(slug, title, testdata_version, testdata_object_key, testdata_sha256) VALUES($1, $2, 1, $3, $4) RETURNING id",
    )
    .bind(slug)
    .bind(title)
    .bind(testdata_object_key)
    .bind("a".repeat(64))
    .fetch_one(pool)
    .await
    .expect("insert problem");
    sqlx::query("INSERT INTO problem_testdata_versions(problem_id, version, object_key, sha256) VALUES($1, 1, $2, $3)")
        .bind(problem_id)
        .bind(testdata_object_key)
        .bind("a".repeat(64))
        .execute(pool)
        .await
        .expect("insert testdata version");
    if let Some(visibility) = bank_visibility {
        sqlx::query("INSERT INTO problem_bank_entries(problem_id, visibility, tags, published_at) VALUES($1, $2, '[]', now())")
            .bind(problem_id)
            .bind(visibility)
            .execute(pool)
            .await
            .expect("insert bank entry");
    }
    problem_id
}

pub async fn insert_practice_submission(
    pool: &PgPool,
    problem_id: i64,
    participant_user_id: i64,
    source_object_key: &str,
    training_enrollment_id: Option<i64>,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO submissions
            (contest_id, problem_id, team_id, language, source_object_key, source_size_bytes,
             source_sha256, status, submission_scope, participant_user_id, training_enrollment_id)
        VALUES (NULL, $1, NULL, 'cpp', $2, 10, $3, 'PENDING', 'PRACTICE', $4, $5)
        RETURNING id
        "#,
    )
    .bind(problem_id)
    .bind(source_object_key)
    .bind("b".repeat(64))
    .bind(participant_user_id)
    .bind(training_enrollment_id)
    .fetch_one(pool)
    .await
    .expect("insert practice submission")
}

pub async fn insert_judgement(pool: &PgPool, submission_id: i64) -> Uuid {
    let judgement_id = Uuid::new_v4();
    sqlx::query("INSERT INTO judgements(id, submission_id) VALUES($1, $2)")
        .bind(judgement_id)
        .bind(submission_id)
        .execute(pool)
        .await
        .expect("insert judgement");
    judgement_id
}

pub fn accepted_judge_result(
    judgement_id: Uuid,
    submission_id: i64,
    worker_id: &str,
) -> JudgeResult {
    let now = time::OffsetDateTime::now_utc();
    JudgeResult {
        schema_version: JUDGE_RESULT_SCHEMA_VERSION,
        message_id: judgement_id,
        judgement_id,
        submission_id,
        worker_id: worker_id.to_owned(),
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
    }
}
