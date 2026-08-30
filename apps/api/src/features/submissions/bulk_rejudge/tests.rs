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

// ---------------------------------------------------------------------------
// Runner claim/finish mechanics
// ---------------------------------------------------------------------------

struct RejudgeWorld {
    creator_id: i64,
    contest_id: i64,
    team_id: i64,
    problem_id: i64,
}

/// Seeds the minimal FK world every batch item needs: an enabled creator, a
/// contest, and one judged-looking submission (claim and finish only touch the
/// batch tables, so the submission itself can stay inert).
async fn seed_rejudge_world(pool: &PgPool) -> RejudgeWorld {
    let creator_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type)
         VALUES ('runner-root', 'test-hash', 'Runner Root', 'SUPER_ADMIN') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert runner creator");
    let contest_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests (name, status, visibility, start_at, end_at)
         VALUES ('Runner Contest', 'RUNNING', 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour')
         RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert runner contest");
    let team_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO teams (name) VALUES ('Runner Team') RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert runner team");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title, languages, testdata_version, testdata_object_key, testdata_sha256)
         VALUES ('runner-a', 'Runner A', '[\"cpp\"]', 1, 'problems/runner/v1.zip', $1) RETURNING id",
    )
    .bind("c".repeat(64))
    .fetch_one(pool)
    .await
    .expect("insert runner problem");
    RejudgeWorld { creator_id, contest_id, team_id, problem_id }
}

/// Each item of a task must reference a distinct submission (unique per task),
/// so every seeded item gets its own inert submission row.
async fn seed_submission(pool: &PgPool, world: &RejudgeWorld, suffix: usize) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO submissions (contest_id, problem_id, team_id, language, source_object_key,
                                   source_size_bytes, source_sha256, status, verdict, judged_at)
         VALUES ($1, $2, $3, 'cpp', $4, 10, $5, 'COMPLETED', 'ACCEPTED', now())
         RETURNING id",
    )
    .bind(world.contest_id)
    .bind(world.problem_id)
    .bind(world.team_id)
    .bind(format!("sources/runner-{suffix}.cpp"))
    .bind(format!("{:0<64}", format!("d{suffix}")))
    .fetch_one(pool)
    .await
    .expect("insert runner submission")
}

struct SeededTask {
    id: i64,
    item_ids: Vec<i64>,
}

/// Inserts a batch task with `item_count` items in one of three shapes:
/// `Fresh` (PENDING task, PENDING items), `Stalled` (RUNNING task with one
/// PROCESSING item on an expired or live lease), or an arbitrary task status.
async fn seed_batch_task(
    pool: &PgPool,
    world: &RejudgeWorld,
    status: &str,
    created_hours_ago: i64,
    item_count: usize,
) -> SeededTask {
    let task_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO batch_rejudge_tasks
            (contest_id, status, idempotency_key, filter_data, total_items,
             created_by_user_id, created_at)
        VALUES ($1, $2, $3, '{}', $4, $5, now() - make_interval(hours => $6))
        RETURNING id
        "#,
    )
    .bind(world.contest_id)
    .bind(status)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(item_count as i32)
    .bind(world.creator_id)
    .bind(created_hours_ago as i32)
    .fetch_one(pool)
    .await
    .expect("insert runner batch task");
    let mut item_ids = Vec::new();
    for index in 0..item_count {
        let submission_id = seed_submission(pool, world, index).await;
        let item_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO batch_rejudge_items (task_id, submission_id, status, old_judgement_id)
             VALUES ($1, $2, 'PENDING', $3) RETURNING id",
        )
        .bind(task_id)
        .bind(submission_id)
        .bind(uuid::Uuid::new_v4())
        .fetch_one(pool)
        .await
        .expect("insert runner batch item");
        item_ids.push(item_id);
    }
    SeededTask { id: task_id, item_ids }
}

/// Rewrites one item into a PROCESSING row held on the given lease, modelling
/// a claim left behind by another runner instance.
async fn seed_leased_item(pool: &PgPool, item_id: i64, owner: uuid::Uuid, expired: bool) {
    sqlx::query(
        "UPDATE batch_rejudge_items
         SET status = 'PROCESSING', attempts = 1, lease_owner = $2,
             lease_until = CASE WHEN $3 THEN now() - interval '5 seconds'
                                ELSE now() + interval '30 seconds' END
         WHERE id = $1",
    )
    .bind(item_id)
    .bind(owner)
    .bind(expired)
    .execute(pool)
    .await
    .expect("lease runner batch item");
}

async fn item_state(pool: &PgPool, item_id: i64) -> (String, i32, Option<uuid::Uuid>) {
    sqlx::query_as::<_, (String, i32, Option<uuid::Uuid>)>(
        "SELECT status, attempts, lease_owner FROM batch_rejudge_items WHERE id = $1",
    )
    .bind(item_id)
    .fetch_one(pool)
    .await
    .expect("load runner batch item state")
}

async fn task_state(
    pool: &PgPool,
    task_id: i64,
) -> (String, i32, i32, i32, Option<time::OffsetDateTime>, Option<time::OffsetDateTime>) {
    sqlx::query_as::<
        _,
        (String, i32, i32, i32, Option<time::OffsetDateTime>, Option<time::OffsetDateTime>),
    >(
        "SELECT status, processed_items, succeeded_items, failed_items, started_at, completed_at
         FROM batch_rejudge_tasks WHERE id = $1",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
    .expect("load runner batch task state")
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn claim_walks_items_in_task_then_item_order_and_promotes_each_task(pool: PgPool) {
    let world = seed_rejudge_world(&pool).await;
    let older = seed_batch_task(&pool, &world, "PENDING", 2, 2).await;
    let newer = seed_batch_task(&pool, &world, "PENDING", 1, 1).await;
    let runner = BatchRejudgeRunner::new(pool.clone());

    let first = runner.claim().await.expect("claim first").expect("pending item first");
    assert_eq!(first.task_id, older.id, "older task must be claimed first");
    let (_, attempts, owner) = item_state(&pool, first.id).await;
    assert_eq!(attempts, 1);
    assert!(owner.is_some(), "claim must take a lease");
    let (older_status, .., older_started_at, _) = task_state(&pool, older.id).await;
    assert_eq!(older_status, "RUNNING");
    assert!(older_started_at.is_some(), "claim must stamp started_at");
    let (newer_status, ..) = task_state(&pool, newer.id).await;
    assert_eq!(newer_status, "PENDING", "untouched task must stay pending");

    let second = runner.claim().await.expect("claim second").expect("pending item second");
    assert_eq!((second.task_id, second.id), (older.id, older.item_ids[1]));
    let third = runner.claim().await.expect("claim third").expect("pending item third");
    assert_eq!(third.task_id, newer.id);
    assert!(runner.claim().await.expect("claim drained").is_none(), "queue must drain");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn concurrent_claims_never_share_an_item(pool: PgPool) {
    let world = seed_rejudge_world(&pool).await;
    let task = seed_batch_task(&pool, &world, "PENDING", 1, 3).await;

    let mut handles = Vec::new();
    for _ in 0..6 {
        let runner = BatchRejudgeRunner::new(pool.clone());
        handles.push(tokio::spawn(async move { runner.claim().await.expect("concurrent claim") }));
    }
    let mut outcomes = Vec::new();
    for handle in handles {
        outcomes.push(handle.await.expect("claim task join"));
    }

    let mut claimed: Vec<i64> = outcomes.iter().flatten().map(|item| item.id).collect();
    claimed.sort_unstable();
    claimed.dedup();
    assert_eq!(claimed.len(), 3, "every claim either takes a unique item or finds none");
    assert_eq!(claimed, task.item_ids, "all three items must be claimed exactly once");
    assert_eq!(outcomes.iter().filter(|item| item.is_none()).count(), 3);

    for item_id in &task.item_ids {
        let (status, attempts, owner) = item_state(&pool, *item_id).await;
        assert_eq!((status.as_str(), attempts), ("PROCESSING", 1));
        assert!(owner.is_some());
    }
    let (status, ..) = task_state(&pool, task.id).await;
    assert_eq!(status, "RUNNING");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn expired_leases_are_reclaimed_with_another_attempt(pool: PgPool) {
    let world = seed_rejudge_world(&pool).await;
    let task = seed_batch_task(&pool, &world, "RUNNING", 1, 1).await;
    let stranded_owner = uuid::Uuid::new_v4();
    seed_leased_item(&pool, task.item_ids[0], stranded_owner, true).await;
    let runner = BatchRejudgeRunner::new(pool.clone());

    let reclaimed = runner
        .claim()
        .await
        .expect("claim expired lease")
        .expect("expired lease must be reclaimed");
    assert_eq!(reclaimed.id, task.item_ids[0]);
    let (status, attempts, owner) = item_state(&pool, reclaimed.id).await;
    assert_eq!((status.as_str(), attempts), ("PROCESSING", 2), "reclaim must count an attempt");
    assert_ne!(owner, Some(stranded_owner), "reclaim must move the lease to this runner");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn claim_ignores_cancelled_completed_and_leased_work(pool: PgPool) {
    let world = seed_rejudge_world(&pool).await;
    let cancelled = seed_batch_task(&pool, &world, "CANCELLED", 4, 1).await;
    let completed = seed_batch_task(&pool, &world, "COMPLETED", 3, 1).await;
    let leased = seed_batch_task(&pool, &world, "RUNNING", 2, 1).await;
    seed_leased_item(&pool, leased.item_ids[0], uuid::Uuid::new_v4(), false).await;
    let fresh = seed_batch_task(&pool, &world, "PENDING", 1, 1).await;
    let runner = BatchRejudgeRunner::new(pool.clone());

    let only = runner.claim().await.expect("claim fresh").expect("only the fresh item");
    assert_eq!(only.task_id, fresh.id);
    assert!(runner.claim().await.expect("claim drained").is_none());

    assert_eq!(item_state(&pool, cancelled.item_ids[0]).await.0, "PENDING");
    assert_eq!(item_state(&pool, completed.item_ids[0]).await.0, "PENDING");
    let (status, attempts, _) = item_state(&pool, leased.item_ids[0]).await;
    assert_eq!((status.as_str(), attempts), ("PROCESSING", 1), "live leases must be left alone");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn finishing_rolls_up_counters_and_completes_the_task_last(pool: PgPool) {
    let world = seed_rejudge_world(&pool).await;
    let task = seed_batch_task(&pool, &world, "PENDING", 1, 2).await;
    let runner = BatchRejudgeRunner::new(pool.clone());
    let first = runner.claim().await.expect("claim one").expect("first item");
    let second = runner.claim().await.expect("claim two").expect("second item");

    runner
        .finish_item(&first, "SUCCEEDED", Some(uuid::Uuid::new_v4()), None)
        .await
        .expect("finish first item");
    let (status, processed, succeeded, failed, _, completed_at) = task_state(&pool, task.id).await;
    assert_eq!(
        (status.as_str(), processed, succeeded, failed),
        ("RUNNING", 1, 1, 0),
        "a half-finished task must stay running"
    );
    assert!(completed_at.is_none());

    runner
        .finish_item(&second, "FAILED", None, Some("boom".into()))
        .await
        .expect("finish second item");
    let (status, processed, succeeded, failed, _, completed_at) = task_state(&pool, task.id).await;
    assert_eq!((status.as_str(), processed, succeeded, failed), ("COMPLETED", 2, 1, 1));
    assert!(completed_at.is_some(), "last item must complete the task");
    let (status, ..) = item_state(&pool, first.id).await;
    assert_eq!(status, "SUCCEEDED");
    let (status, _, owner) = item_state(&pool, second.id).await;
    assert_eq!((status.as_str(), owner.is_none()), ("FAILED", true));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn stale_lease_owners_cannot_finish_someone_elses_item(pool: PgPool) {
    let world = seed_rejudge_world(&pool).await;
    let task = seed_batch_task(&pool, &world, "PENDING", 1, 1).await;
    let current = BatchRejudgeRunner::new(pool.clone());
    let impostor = BatchRejudgeRunner::new(pool.clone());
    let item = current.claim().await.expect("claim item").expect("pending item");

    impostor
        .finish_item(&item, "SUCCEEDED", Some(uuid::Uuid::new_v4()), None)
        .await
        .expect("impostor finish must not error");
    let (status, processed, ..) = task_state(&pool, task.id).await;
    assert_eq!(
        (status.as_str(), processed),
        ("RUNNING", 0),
        "impostor outcome must not be counted"
    );
    assert_eq!(item_state(&pool, item.id).await.0, "PROCESSING");

    current
        .finish_item(&item, "SUCCEEDED", Some(uuid::Uuid::new_v4()), None)
        .await
        .expect("lease owner finish");
    let (status, processed, ..) = task_state(&pool, task.id).await;
    assert_eq!((status.as_str(), processed), ("COMPLETED", 1));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn failure_reasons_are_bounded_before_persistence(pool: PgPool) {
    let world = seed_rejudge_world(&pool).await;
    let seeded = seed_batch_task(&pool, &world, "PENDING", 1, 1).await;
    let runner = BatchRejudgeRunner::new(pool.clone());
    let item = runner.claim().await.expect("claim item").expect("pending item");
    let verbose = "x".repeat(2000);

    runner.finish_item(&item, "FAILED", None, Some(verbose)).await.expect("finish failed item");
    assert_eq!(seeded.item_ids[0], item.id);

    let length = sqlx::query_scalar::<_, i32>(
        "SELECT length(error_message) FROM batch_rejudge_items WHERE id = $1",
    )
    .bind(item.id)
    .fetch_one(&pool)
    .await
    .expect("load error message length");
    assert_eq!(length, 1000, "persisted failure reasons must be truncated");
}
