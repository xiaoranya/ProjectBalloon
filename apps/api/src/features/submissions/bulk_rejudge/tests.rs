use sqlx::PgPool;

use crate::features::submissions::bulk_rejudge::{
    BatchRejudgeCreateRequest, BatchRejudgeFilter, BatchRejudgeRunner, BatchRejudgeService,
};
use crate::features::{
    auth::model::{AuthUser, UserType},
    submissions::SubmissionService,
};

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn batch_rejudge_is_persistent_pausable_and_item_idempotent(pool: PgPool) {
    let admin_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO users (username, password_hash, display_name, user_type)
        VALUES ('batch-root', 'test-hash', 'Batch Root', 'SUPER_ADMIN') RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert batch administrator");
    let admin = AuthUser {
        id: admin_id,
        username: "batch-root".into(),
        display_name: "Batch Root".into(),
        user_type: UserType::SuperAdmin,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let team_id =
        sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ('Batch Team') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert batch team");
    let contest_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO contests (name, status, visibility, start_at, end_at)
        VALUES ('Batch Contest', 'RUNNING', 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour')
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert batch contest");
    sqlx::query(
        "INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')",
    )
    .bind(contest_id)
    .bind(team_id)
    .execute(&pool)
    .await
    .expect("roster batch team");
    let problem_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO problems
            (slug, title, languages, testdata_version, testdata_object_key, testdata_sha256)
        VALUES ('batch-a', 'Batch A', '["cpp"]', 1, 'problems/batch/v1.zip', $1)
        RETURNING id
        "#,
    )
    .bind("a".repeat(64))
    .fetch_one(&pool)
    .await
    .expect("insert batch problem");
    sqlx::query(
        r#"
        INSERT INTO problem_testdata_versions
            (problem_id, version, object_key, sha256, bytes, case_count)
        VALUES ($1, 1, 'problems/batch/v1.zip', $2, 100, 1)
        "#,
    )
    .bind(problem_id)
    .bind("a".repeat(64))
    .execute(&pool)
    .await
    .expect("insert batch test data");
    sqlx::query(
        "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
    )
    .bind(contest_id)
    .bind(problem_id)
    .execute(&pool)
    .await
    .expect("assign batch problem");
    let submission_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO submissions
            (contest_id, problem_id, team_id, language, source_object_key,
             source_size_bytes, source_sha256, status, verdict, judged_at)
        VALUES ($1, $2, $3, 'cpp', 'sources/batch.cpp', 10, $4, 'COMPLETED', 'ACCEPTED', now())
        RETURNING id
        "#,
    )
    .bind(contest_id)
    .bind(problem_id)
    .bind(team_id)
    .bind("b".repeat(64))
    .fetch_one(&pool)
    .await
    .expect("insert batch submission");
    let old_judgement_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO judgements (id, submission_id, verdict, completed_at) VALUES ($1, $2, 'ACCEPTED', now())",
    )
    .bind(old_judgement_id)
    .bind(submission_id)
    .execute(&pool)
    .await
    .expect("insert batch old judgement");

    let service = BatchRejudgeService::new(pool.clone());
    let filter = || BatchRejudgeFilter {
        problem_id: Some(problem_id),
        team_id: None,
        language: Some("cpp".into()),
        verdict: Some("ACCEPTED".into()),
        submitted_from: None,
        submitted_to: None,
    };
    let preview =
        service.preview(contest_id, &admin, filter()).await.expect("preview batch rejudge");
    assert_eq!(preview.matched_submissions, 1);
    let request = || BatchRejudgeCreateRequest {
        filter: filter(),
        expected_count: 1,
        confirmation_text: "REJUDGE 1".into(),
        idempotency_key: "batch-test-key-0001".into(),
    };
    let task = service.create(contest_id, &admin, request()).await.expect("create batch task");
    assert_eq!((task.status.as_str(), task.total_items, task.items.len()), ("PENDING", 1, 1));
    let duplicate =
        service.create(contest_id, &admin, request()).await.expect("idempotent batch create");
    assert_eq!(duplicate.id, task.id);
    let paused = service.pause(contest_id, task.id, &admin).await.expect("pause batch task");
    assert_eq!(paused.status, "PAUSED");
    let resumed = service.resume(contest_id, task.id, &admin).await.expect("resume batch task");
    assert_eq!(resumed.status, "RUNNING");

    let runner = BatchRejudgeRunner::new(pool.clone());
    let item = runner.claim().await.expect("claim batch item").expect("pending batch item");
    let item_id = item.id;
    runner.process(item).await;
    let completed = service.get(contest_id, task.id, &admin).await.expect("load completed task");
    assert_eq!(completed.status, "COMPLETED");
    assert_eq!(
        (completed.processed_items, completed.succeeded_items, completed.failed_items),
        (1, 1, 0),
        "batch item error: {:?}",
        completed.items[0].error_message
    );
    let new_judgement_id = completed.items[0].new_judgement_id.expect("new batch judgement");
    let recovered = SubmissionService::new(pool.clone())
        .rejudge_batch_item(contest_id, submission_id, old_judgement_id, &admin, item_id)
        .await
        .expect("recover already committed batch item");
    assert_eq!(recovered.judgement_id, new_judgement_id);
    let anchored = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM judgements WHERE batch_rejudge_item_id = $1",
    )
    .bind(item_id)
    .fetch_one(&pool)
    .await
    .expect("count batch judgement anchors");
    assert_eq!(anchored, 1);
}
