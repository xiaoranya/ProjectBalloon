//! Service-level integration tests for the contest-scoped bulk rejudge
//! workbench: preview counts, idempotent creation, and pause/resume guards.

use project_balloon_api::features::auth::model::{AuthUser, UserType};
use project_balloon_api::features::submissions::{
    BatchRejudgeCreateRequest, BatchRejudgeFilter, BatchRejudgeService,
};
use sqlx::PgPool;

async fn insert_user(pool: &PgPool, username: &str, user_type: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ($1, 'hash', $1, $2) RETURNING id",
    )
    .bind(username)
    .bind(user_type)
    .fetch_one(pool)
    .await
    .expect("insert user")
}

fn actor(id: i64, username: &str, user_type: UserType, permissions: &[&str]) -> AuthUser {
    AuthUser {
        id,
        username: username.to_owned(),
        display_name: username.to_owned(),
        user_type,
        permissions: permissions.iter().map(|code| (*code).to_owned()).collect(),
        password_reset_required: false,
    }
}

async fn seed_contest(pool: &PgPool, name: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests (name, status, visibility) VALUES ($1, 'RUNNING', 'PRIVATE') RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert contest")
}

/// Seeds a problem assignment plus one submission carrying a completed active
/// judgement — exactly what the batch filter matches against.
async fn seed_submission(
    pool: &PgPool,
    contest_id: i64,
    problem_alias: &str,
    display_order: i32,
    language: &str,
    verdict: &str,
) -> i64 {
    let problem_id =
        sqlx::query_scalar::<_, i64>("INSERT INTO problems(slug,title) VALUES($1,$2) RETURNING id")
            .bind(format!("bulk-{problem_alias}"))
            .bind("Bulk problem")
            .fetch_one(pool)
            .await
            .expect("insert problem");
    sqlx::query("INSERT INTO contest_problems(contest_id,problem_id,alias,display_order,max_score_milli) VALUES($1,$2,$3,$4,100000)")
        .bind(contest_id).bind(problem_id).bind(problem_alias)
        .bind(display_order)
        .execute(pool)
        .await
        .expect("assign problem");
    let team_id = sqlx::query_scalar::<_, i64>("INSERT INTO teams(name) VALUES($1) RETURNING id")
        .bind(format!("bulk team {problem_alias}"))
        .fetch_one(pool)
        .await
        .expect("insert team");
    let submission_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO submissions(contest_id,problem_id,team_id,language,source_object_key,source_size_bytes,source_sha256,status,verdict,judged_at) VALUES($1,$2,$3,$4,$5,10,$6,'COMPLETED',$7,now()) RETURNING id",
    )
    .bind(contest_id)
    .bind(problem_id)
    .bind(team_id)
    .bind(language)
    .bind(format!("sources/bulk/{problem_alias}.cpp"))
    .bind("b".repeat(64))
    .bind(verdict)
    .fetch_one(pool)
    .await
    .expect("insert submission");
    let judgement_id = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO judgements(id,submission_id) VALUES($1,$2)")
        .bind(judgement_id)
        .bind(submission_id)
        .execute(pool)
        .await
        .expect("insert judgement");
    sqlx::query("UPDATE judgements SET verdict=$2, completed_at=now(), total_time_ms=1, peak_memory_kb=8 WHERE id=$1")
        .bind(judgement_id)
        .bind(verdict)
        .execute(pool)
        .await
        .expect("complete judgement");
    submission_id
}

fn unfiltered() -> BatchRejudgeFilter {
    BatchRejudgeFilter {
        problem_id: None,
        team_id: None,
        language: None,
        verdict: None,
        submitted_from: None,
        submitted_to: None,
    }
}

fn create_request(expected_count: i32, idempotency_key: &str) -> BatchRejudgeCreateRequest {
    BatchRejudgeCreateRequest {
        filter: unfiltered(),
        expected_count,
        confirmation_text: format!("REJUDGE {expected_count}"),
        idempotency_key: idempotency_key.to_owned(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn bulk_rejudge_service_previews_counts_and_creates_idempotently(pool: PgPool) {
    let contest_id = seed_contest(&pool, "Bulk Service Contest").await;
    seed_submission(&pool, contest_id, "A", 1, "cpp", "WRONG_ANSWER").await;
    seed_submission(&pool, contest_id, "B", 2, "java", "ACCEPTED").await;

    let admin_id = insert_user(&pool, "bulk-admin", "SUPER_ADMIN").await;
    let admin = actor(admin_id, "bulk-admin", UserType::SuperAdmin, &[]);
    let service = BatchRejudgeService::new(pool.clone());

    let preview = service.preview(contest_id, &admin, unfiltered()).await.expect("preview");
    assert_eq!(preview.matched_submissions, 2);

    let scoped = service
        .preview(
            contest_id,
            &admin,
            BatchRejudgeFilter { language: Some("Java".to_owned()), ..unfiltered() },
        )
        .await
        .expect("scoped preview");
    assert_eq!(scoped.matched_submissions, 1, "language filter must normalize and match");

    let created = service
        .create(contest_id, &admin, create_request(2, "bulk-key-0001"))
        .await
        .expect("create batch rejudge");
    assert_eq!(created.status, "PENDING");
    assert_eq!(created.total_items, 2);

    let replayed = service
        .create(contest_id, &admin, create_request(2, "bulk-key-0001"))
        .await
        .expect("idempotent replay returns the existing task");
    assert_eq!(replayed.id, created.id);

    let listed = service.list(contest_id, &admin).await.expect("list tasks");
    assert_eq!(listed.len(), 1);

    let fetched = service.get(contest_id, created.id, &admin).await.expect("fetch task");
    assert_eq!(fetched.items.len(), 2);
    assert!(!fetched.items_truncated);

    // A conflicting idempotency key (same key, different contest) is rejected.
    let other_contest = seed_contest(&pool, "Bulk Other Contest").await;
    let conflict = service
        .create(other_contest, &admin, create_request(1, "bulk-key-0001"))
        .await
        .expect_err("reused key across contests");
    assert_eq!(conflict.code(), "IDEMPOTENCY_KEY_REUSED");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn bulk_rejudge_service_guards_scope_counts_and_state(pool: PgPool) {
    let contest_id = seed_contest(&pool, "Bulk Guard Contest").await;
    let submission_id = seed_submission(&pool, contest_id, "A", 1, "cpp", "ACCEPTED").await;
    let admin_id = insert_user(&pool, "bulk-guard-admin", "SUPER_ADMIN").await;
    let admin = actor(admin_id, "bulk-guard-admin", UserType::SuperAdmin, &[]);
    let service = BatchRejudgeService::new(pool.clone());

    assert_eq!(
        service
            .preview(contest_id + 50, &admin, unfiltered())
            .await
            .expect_err("unknown contest")
            .code(),
        "BATCH_REJUDGE_NOT_FOUND"
    );
    assert_eq!(
        service.preview(0, &admin, unfiltered()).await.expect_err("non-positive contest").code(),
        "BATCH_REJUDGE_NOT_FOUND"
    );

    let manager_id = insert_user(&pool, "bulk-guard-manager", "STAFF").await;
    sqlx::query("INSERT INTO contest_management_assignments(user_id,contest_id) VALUES($1,$2)")
        .bind(manager_id)
        .bind(contest_id)
        .execute(&pool)
        .await
        .expect("assign manager");
    let manager = actor(manager_id, "bulk-guard-manager", UserType::Staff, &["CONTEST_MANAGE"]);
    assert!(service.preview(contest_id, &manager, unfiltered()).await.is_ok());

    let outsider_id = insert_user(&pool, "bulk-guard-outsider", "STAFF").await;
    let outsider = actor(outsider_id, "bulk-guard-outsider", UserType::Staff, &["CONTEST_MANAGE"]);
    assert_eq!(
        service
            .preview(contest_id, &outsider, unfiltered())
            .await
            .expect_err("unassigned staff")
            .code(),
        "BATCH_REJUDGE_NOT_FOUND"
    );

    // A stale expected count is rejected without creating anything.
    let stale = service
        .create(contest_id, &admin, create_request(3, "bulk-key-stale"))
        .await
        .expect_err("stale count");
    assert_eq!(stale.code(), "BATCH_REJUDGE_COUNT_CHANGED");

    // A bad confirmation text or key length is a validation failure.
    let mut bad_confirmation = create_request(1, "bulk-key-badconf");
    bad_confirmation.confirmation_text = "REJUDGE 2".to_owned();
    assert_eq!(
        service
            .create(contest_id, &admin, bad_confirmation)
            .await
            .expect_err("wrong confirmation text")
            .code(),
        "VALIDATION_FAILED"
    );
    assert_eq!(
        service
            .create(contest_id, &admin, create_request(1, "short"))
            .await
            .expect_err("short idempotency key")
            .code(),
        "VALIDATION_FAILED"
    );

    let created = service
        .create(contest_id, &admin, create_request(1, "bulk-key-guard"))
        .await
        .expect("create batch rejudge");

    // Pause only applies to PENDING/RUNNING tasks; resume only to PAUSED ones.
    let resumed =
        service.resume(contest_id, created.id, &admin).await.expect_err("resume a non-paused task");
    assert_eq!(resumed.code(), "BATCH_REJUDGE_STATE_CHANGED");
    let paused = service.pause(contest_id, created.id, &admin).await.expect("pause pending task");
    assert_eq!(paused.status, "PAUSED");
    assert!(paused.cancel_requested);
    let paused_again =
        service.pause(contest_id, created.id, &admin).await.expect_err("pause a paused task");
    assert_eq!(paused_again.code(), "BATCH_REJUDGE_STATE_CHANGED");
    let resumed = service.resume(contest_id, created.id, &admin).await.expect("resume paused task");
    assert_eq!(resumed.status, "RUNNING");
    assert!(!resumed.cancel_requested);

    let foreign = service
        .get(contest_id + 1, created.id, &admin)
        .await
        .expect_err("task belongs to another contest");
    assert_eq!(foreign.code(), "BATCH_REJUDGE_NOT_FOUND");

    let _unused = submission_id;
}
