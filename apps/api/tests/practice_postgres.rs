mod common;

use project_balloon_api::features::judge_dispatch::{ApplyResultOutcome, JudgeResultProcessor};

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn practice_result_updates_personal_progress(pool: sqlx::PgPool) {
    let user_id = common::insert_user(&pool, "practice-user", "Practice User", "INDIVIDUAL").await;
    let problem_id = common::insert_problem(
        &pool,
        "practice-result",
        "Practice Result",
        "problems/practice.zip",
        Some("PUBLIC"),
    )
    .await;
    let submission_id =
        common::insert_practice_submission(&pool, problem_id, user_id, "practice-source", None)
            .await;
    let judgement_id = common::insert_judgement(&pool, submission_id).await;
    let result = common::accepted_judge_result(judgement_id, submission_id, "practice-worker");

    assert_eq!(
        JudgeResultProcessor::new(pool.clone()).apply(&result).await.expect("apply"),
        ApplyResultOutcome::Applied
    );
    let progress = sqlx::query_as::<_, (i32, i32, bool, i64)>(
        "SELECT attempts,best_score,solved,last_submission_id FROM practice_problem_progress WHERE user_id=$1 AND problem_id=$2",
    )
    .bind(user_id)
    .bind(problem_id)
    .fetch_one(&pool)
    .await
    .expect("progress");
    assert_eq!(progress, (1, 100, true, submission_id));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn accepted_result_completes_required_training_enrollment(pool: sqlx::PgPool) {
    let user_id =
        common::insert_user(&pool, "training-result-user", "Training Result User", "INDIVIDUAL")
            .await;
    let problem_id = common::insert_problem(
        &pool,
        "training-result",
        "Training Result",
        "problems/training.zip",
        Some("PUBLIC"),
    )
    .await;
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
    let submission_id = common::insert_practice_submission(
        &pool,
        problem_id,
        user_id,
        "practice-training-source",
        Some(enrollment_id),
    )
    .await;
    let judgement_id = common::insert_judgement(&pool, submission_id).await;
    let result =
        common::accepted_judge_result(judgement_id, submission_id, "training-result-worker");

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
