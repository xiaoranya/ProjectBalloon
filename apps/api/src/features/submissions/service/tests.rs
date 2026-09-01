use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use bytes::Bytes;
use project_balloon_contracts::JudgeTask;
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::features::submissions::service::{SubmissionService, language_multiplier};
use crate::{
    features::{
        auth::model::{AuthUser, UserType},
        scoreboard,
        submissions::model::{
            RejudgeRequest, ValidatedSubmission, ValidatedSubmissionListQuery, source_fingerprint,
        },
        submissions::query::SimilarityPairQuery,
    },
    object_storage::{ObjectStorage, ObjectStorageError, ObjectStorageHandle},
};

#[derive(Default)]
struct MemoryStorage {
    objects: Mutex<HashMap<(String, String), Bytes>>,
}

#[test]
fn p0_language_time_multipliers_are_explicit() {
    assert_eq!(language_multiplier("c"), 1.0);
    assert_eq!(language_multiplier("cpp"), 1.0);
    assert_eq!(language_multiplier("java"), 2.0);
    assert_eq!(language_multiplier("python"), 3.0);
    assert_eq!(language_multiplier("go"), 1.0);
    assert_eq!(language_multiplier("rust"), 1.0);
}

#[async_trait]
impl ObjectStorage for MemoryStorage {
    async fn check_bucket(&self, _bucket: &str) -> Result<(), ObjectStorageError> {
        Ok(())
    }

    async fn put(
        &self,
        bucket: &str,
        key: &str,
        _content_type: Option<&str>,
        content: Bytes,
    ) -> Result<(), ObjectStorageError> {
        self.objects
            .lock()
            .expect("memory storage lock")
            .insert((bucket.to_owned(), key.to_owned()), content);
        Ok(())
    }

    async fn get(&self, bucket: &str, key: &str) -> Result<Bytes, ObjectStorageError> {
        self.objects
            .lock()
            .expect("memory storage lock")
            .get(&(bucket.to_owned(), key.to_owned()))
            .cloned()
            .ok_or_else(|| ObjectStorageError::Request("not found".into()))
    }

    async fn delete(&self, bucket: &str, key: &str) -> Result<(), ObjectStorageError> {
        self.objects
            .lock()
            .expect("memory storage lock")
            .remove(&(bucket.to_owned(), key.to_owned()));
        Ok(())
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn submission_persists_authoritative_task_and_compensates_rejection(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO users
                (username, password_hash, display_name, user_type, enabled,
                 password_reset_required)
            VALUES ('submit-team', 'test-hash', 'Submit Team', 'TEAM', true, false)
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert team user");
    let team_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO teams (name) VALUES ('Submit Team') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert team");
    sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
        .bind(user_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("link team account");
    let contest_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO contests (name, status, visibility, start_at, end_at)
            VALUES (
                'Submission Contest', 'RUNNING', 'PRIVATE',
                now() - interval '1 hour', now() + interval '1 hour'
            )
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert running contest");
    sqlx::query(
            "INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')",
        )
        .bind(contest_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("insert roster");
    let problem_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO problems
                (slug, title, languages, testdata_version, testdata_object_key, testdata_sha256)
            VALUES ('submit-problem', 'Submit Problem', '["cpp"]', 1,
                    'problems/1/testdata/v1/fixture.zip', $1)
            RETURNING id
            "#,
    )
    .bind("a".repeat(64))
    .fetch_one(&pool)
    .await
    .expect("insert problem");
    sqlx::query(
        r#"
            INSERT INTO problem_testdata_versions
                (problem_id, version, object_key, sha256, bytes, case_count)
            VALUES ($1, 1, 'problems/1/testdata/v1/fixture.zip', $2, 100, 1)
            "#,
    )
    .bind(problem_id)
    .bind("a".repeat(64))
    .execute(&pool)
    .await
    .expect("insert test-data version");
    sqlx::query(
            "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
        )
        .bind(contest_id)
        .bind(problem_id)
        .execute(&pool)
        .await
        .expect("assign problem");
    let actor = AuthUser {
        id: user_id,
        username: "submit-team".into(),
        display_name: "Submit Team".into(),
        user_type: UserType::Team,
        permissions: vec![],
        password_reset_required: false,
    };
    let memory = Arc::new(MemoryStorage::default());
    let storage = ObjectStorageHandle::with_buckets(
        memory.clone(),
        "problems-test".into(),
        "sources-test".into(),
    );
    let service = SubmissionService::new(pool.clone());
    let response = service
        .submit(
            contest_id,
            ValidatedSubmission {
                problem_id,
                language: "cpp".into(),
                extension: ".cpp",
                source: Bytes::from_static(b"int main() { return 0; }"),
            },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("create submission");
    assert_eq!(response.status, "PENDING");
    let (source_key, source_hash, fingerprint, payload) =
        sqlx::query_as::<_, (String, String, String, String)>(
            r#"
            SELECT submission.source_object_key, submission.source_sha256,
                   submission.source_fingerprint, outbox.payload
            FROM submissions submission
            JOIN submission_outbox outbox ON outbox.submission_id = submission.id
            WHERE submission.id = $1
            "#,
        )
        .bind(response.submission_id)
        .fetch_one(&pool)
        .await
        .expect("load submission and outbox");
    let task: JudgeTask = serde_json::from_str(&payload).expect("deserialize judge task");
    task.validate().expect("valid judge task");
    assert_eq!(task.judgement_id, response.judgement_id);
    assert_eq!(task.source_object_key, source_key);
    assert_eq!(task.source_sha256, source_hash);
    assert_eq!(fingerprint, source_fingerprint(b"int main() { return 0; }"));
    assert_eq!(task.testdata_version, 1);
    assert_eq!(task.testdata_sha256, "a".repeat(64));
    assert!(
        memory
            .get("sources-test", &source_key)
            .await
            .expect("stored source")
            .starts_with(b"int main")
    );
    let second_user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('submit-team-2', 'test-hash', 'Submit Team 2', 'TEAM', true, false) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert second team user");
    let second_team_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO teams (name) VALUES ('Submit Team 2') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert second team");
    sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
        .bind(second_user_id)
        .bind(second_team_id)
        .execute(&pool)
        .await
        .expect("link second team account");
    sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
            .bind(contest_id)
            .bind(second_team_id)
            .execute(&pool)
            .await
            .expect("roster second team");
    let second_actor = AuthUser {
        id: second_user_id,
        username: "submit-team-2".into(),
        display_name: "Submit Team 2".into(),
        user_type: UserType::Team,
        permissions: vec![],
        password_reset_required: false,
    };
    let second_response = service
        .submit(
            contest_id,
            ValidatedSubmission {
                problem_id,
                language: "cpp".into(),
                extension: ".cpp",
                source: Bytes::from_static(b"int main(){ return 1; }"),
            },
            &second_actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await
        .expect("create similar second-team submission");
    let team_event = sqlx::query_scalar::<_, bool>(
        r#"
            SELECT EXISTS (
                SELECT 1 FROM realtime_outbox
                WHERE contest_id = $1 AND team_id = $2
                  AND event_type = 'SUBMISSION_STATUS_CHANGED' AND scope = 'TEAM'
            )
            "#,
    )
    .bind(contest_id)
    .bind(team_id)
    .fetch_one(&pool)
    .await
    .expect("check team realtime event");
    assert!(team_event);

    sqlx::query(
        r#"
            UPDATE judgements
            SET verdict = 'ACCEPTED', completed_at = now(), total_time_ms = 12,
                peak_memory_kb = 1024, compile_log = $2
            WHERE id = $1
            "#,
    )
    .bind(response.judgement_id)
    .bind("compiled\u{7}success")
    .execute(&pool)
    .await
    .expect("complete original judgement");
    sqlx::query(
        r#"
            INSERT INTO runs
                (judgement_id, test_index, verdict, time_ms, memory_kb, exit_code, stderr_tail)
            VALUES ($1, 1, 'ACCEPTED', 12, 1024, 0, $2)
            "#,
    )
    .bind(response.judgement_id)
    .bind("run\u{1}ok")
    .execute(&pool)
    .await
    .expect("insert original judgement run");
    sqlx::query("UPDATE submissions SET status = 'COMPLETED', verdict = 'ACCEPTED', judged_at = now() WHERE id = $1")
        .bind(response.submission_id)
        .execute(&pool)
        .await
        .expect("complete original submission");
    let mut projection = pool.begin().await.expect("begin accepted projection");
    scoreboard::rebuild_cell(&mut projection, contest_id, team_id, problem_id)
        .await
        .expect("project accepted submission");
    projection.commit().await.expect("commit accepted projection");
    let admin_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO users (username, password_hash, display_name, user_type)
            VALUES ('rejudge-root', 'test-hash', 'Rejudge Root', 'SUPER_ADMIN')
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert rejudge administrator");
    let admin = AuthUser {
        id: admin_id,
        username: "rejudge-root".into(),
        display_name: "Rejudge Root".into(),
        user_type: UserType::SuperAdmin,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    sqlx::query(
            "UPDATE submissions SET source_fingerprint = NULL, source_simhash = NULL, source_token_count = NULL WHERE id = $1",
        )
        .bind(response.submission_id)
        .execute(&pool)
        .await
        .expect("simulate a pre-similarity submission");
    let backfill = service
        .backfill_similarity(contest_id, &admin, &storage)
        .await
        .expect("backfill historical source similarity");
    assert_eq!((backfill.scanned, backfill.updated, backfill.failed), (1, 1, 0));
    assert!(
            sqlx::query_scalar::<_, bool>(
                "SELECT source_simhash IS NOT NULL AND source_token_count > 0 FROM submissions WHERE id = $1",
            )
            .bind(response.submission_id)
            .fetch_one(&pool)
            .await
            .expect("load backfilled similarity")
        );
    let similar_pairs = service
        .list_similarity_pairs(
            contest_id,
            &admin,
            SimilarityPairQuery {
                problem_id: Some(problem_id),
                language: Some("cpp".into()),
                min_similarity_percent: 85,
            },
        )
        .await
        .expect("list approximate submission pairs");
    assert!(similar_pairs.iter().any(|pair| {
        pair.submission_id == response.submission_id
            && pair.other_submission_id == second_response.submission_id
    }));
    sqlx::query("UPDATE submissions SET status = 'COMPLETED', verdict = 'CANCELLED' WHERE id = $1")
        .bind(second_response.submission_id)
        .execute(&pool)
        .await
        .expect("remove similarity fixture from pending assertions");
    sqlx::query("UPDATE submission_outbox SET status = 'CANCELLED' WHERE submission_id = $1")
        .bind(second_response.submission_id)
        .execute(&pool)
        .await
        .expect("cancel similarity fixture task");
    let expected_judgement_id = response.judgement_id;
    let (first_rejudge, concurrent_rejudge) = tokio::join!(
        service.rejudge(
            contest_id,
            response.submission_id,
            RejudgeRequest { expected_judgement_id },
            &admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        ),
        service.rejudge(
            contest_id,
            response.submission_id,
            RejudgeRequest { expected_judgement_id },
            &admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
    );
    let rejudged = match (first_rejudge, concurrent_rejudge) {
        (Ok(response), Err(_)) | (Err(_), Ok(response)) => response,
        (Ok(_), Ok(_)) => panic!("concurrent rejudge must create exactly one task"),
        (Err(first), Err(second)) => {
            panic!("one concurrent rejudge must succeed: {first:?}; {second:?}")
        }
    };
    assert_eq!(rejudged.previous_judgement_id, expected_judgement_id);
    assert_eq!(rejudged.status, "PENDING");
    let rejudge_state =
        sqlx::query_as::<_, (bool, Option<bool>, String, Option<OffsetDateTime>, String, String)>(
            r#"
            SELECT old_judgement.superseded,
                   old_judgement.active_marker,
                   old_outbox.status,
                   submission.judged_at,
                   submission.status,
                   new_outbox.payload
            FROM submissions submission
            JOIN judgements old_judgement ON old_judgement.id = $2
            JOIN submission_outbox old_outbox ON old_outbox.judgement_id = old_judgement.id
            JOIN submission_outbox new_outbox ON new_outbox.judgement_id = $3
            WHERE submission.id = $1
            "#,
        )
        .bind(response.submission_id)
        .bind(expected_judgement_id)
        .bind(rejudged.judgement_id)
        .fetch_one(&pool)
        .await
        .expect("load committed rejudge state");
    assert!(rejudge_state.0);
    assert_eq!(rejudge_state.1, None);
    assert_eq!(rejudge_state.2, "CANCELLED");
    assert_eq!(rejudge_state.3, None);
    assert_eq!(rejudge_state.4, "PENDING");
    let rejudge_task: JudgeTask =
        serde_json::from_str(&rejudge_state.5).expect("decode rejudge task");
    rejudge_task.validate().expect("valid rejudge task");
    assert_eq!(rejudge_task.judgement_id, rejudged.judgement_id);
    assert_eq!(rejudge_task.source_object_key, source_key);
    assert_eq!(rejudge_task.source_sha256, source_hash);
    let rejudge_effects = sqlx::query_as::<_, (bool, i64, i64)>(
        r#"
            SELECT cell.solved,
                   (SELECT count(*) FROM realtime_outbox
                    WHERE contest_id = $1 AND event_type = 'SUBMISSION_REJUDGED'
                      AND payload_json ->> 'submissionId' = $2::text),
                   (SELECT count(*) FROM audit_logs
                    WHERE actor_user_id = $3 AND action = 'SUBMISSION_REJUDGED'
                      AND target_id = $2::text)
            FROM contest_scoreboard_cells cell
            WHERE cell.contest_id = $1 AND cell.team_id = $4 AND cell.problem_id = $5
            "#,
    )
    .bind(contest_id)
    .bind(response.submission_id)
    .bind(admin_id)
    .bind(team_id)
    .bind(problem_id)
    .fetch_one(&pool)
    .await
    .expect("load rejudge side effects");
    assert_eq!(rejudge_effects, (false, 2, 1));

    let all_submissions = || ValidatedSubmissionListQuery {
        team_id: None,
        problem_id: None,
        status: None,
        verdict: None,
        language: None,
        page: 0,
        size: 25,
        offset: 0,
    };
    let own_list = service
        .list_own(contest_id, &actor, all_submissions())
        .await
        .expect("team lists own submissions");
    assert_eq!(own_list.total_elements, 1);
    assert_eq!(own_list.content[0].active_judgement_id, Some(rejudged.judgement_id));
    let admin_list = service
        .list_admin(
            contest_id,
            &admin,
            ValidatedSubmissionListQuery { status: Some("PENDING".into()), ..all_submissions() },
        )
        .await
        .expect("administrator filters contest submissions");
    assert_eq!(admin_list.total_elements, 1);
    let own_detail = service
        .detail_own(contest_id, response.submission_id, &actor, &storage)
        .await
        .expect("team loads own submission detail");
    assert!(own_detail.source.starts_with("int main"));
    assert_eq!(own_detail.source_sha256.as_deref(), Some(source_hash.as_str()));
    assert_eq!(own_detail.judgements.len(), 2);
    let previous = own_detail
        .judgements
        .iter()
        .find(|judgement| judgement.id == expected_judgement_id)
        .expect("detail retains previous judgement");
    assert!(previous.superseded);
    assert_eq!(previous.compile_log.as_deref(), Some("compiledsuccess"));
    assert_eq!(previous.runs.len(), 1);
    assert_eq!(previous.runs[0].stderr_tail.as_deref(), Some("runok"));
    let admin_detail = service
        .detail_admin(contest_id, response.submission_id, &admin, &storage)
        .await
        .expect("administrator loads contest submission detail");
    assert_eq!(admin_detail.summary.id, response.submission_id);

    let other_user_id = sqlx::query_scalar::<_, i64>(
        r#"
            INSERT INTO users (username, password_hash, display_name, user_type)
            VALUES ('other-team', 'test-hash', 'Other Team', 'TEAM')
            RETURNING id
            "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert unrelated team user");
    let other_team_id =
        sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ('Other Team') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert unrelated team");
    sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
        .bind(other_user_id)
        .bind(other_team_id)
        .execute(&pool)
        .await
        .expect("link unrelated team account");
    let other_actor = AuthUser {
        id: other_user_id,
        username: "other-team".into(),
        display_name: "Other Team".into(),
        user_type: UserType::Team,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    assert!(
        service
            .detail_own(contest_id, response.submission_id, &other_actor, &storage)
            .await
            .is_err(),
        "another team must not enumerate submission details"
    );

    let queued =
        service.judge_queue_status(contest_id, &admin).await.expect("load queued judge status");
    assert!(!queued.drained);
    assert_eq!(queued.pending_submissions, 1);
    assert_eq!(queued.judging_submissions, 0);
    assert_eq!(queued.outbox_pending, 1);
    assert_eq!(queued.outbox_failed, 0);

    sqlx::query("UPDATE submission_outbox SET status='SENT',sent_at=now() WHERE judgement_id=$1")
        .bind(rejudged.judgement_id)
        .execute(&pool)
        .await
        .expect("mark rejudge task sent");
    sqlx::query("UPDATE submissions SET status='JUDGING' WHERE id=$1")
        .bind(response.submission_id)
        .execute(&pool)
        .await
        .expect("mark rejudged submission in flight");
    let judging =
        service.judge_queue_status(contest_id, &admin).await.expect("load in-flight judge status");
    assert!(!judging.drained);
    assert_eq!(judging.pending_submissions, 0);
    assert_eq!(judging.judging_submissions, 1);
    assert_eq!(judging.outbox_pending, 0);

    sqlx::query("UPDATE judgements SET verdict='ACCEPTED',completed_at=now() WHERE id=$1")
        .bind(rejudged.judgement_id)
        .execute(&pool)
        .await
        .expect("complete rejudgement");
    sqlx::query(
        "UPDATE submissions SET status='COMPLETED',verdict='ACCEPTED',judged_at=now() WHERE id=$1",
    )
    .bind(response.submission_id)
    .execute(&pool)
    .await
    .expect("complete rejudged submission");
    assert!(
        service
            .judge_queue_status(contest_id, &admin)
            .await
            .expect("load drained judge status")
            .drained
    );

    sqlx::query(
        r#"
            INSERT INTO submissions
                (contest_id, problem_id, team_id, language, source_object_key,
                 source_size_bytes, status)
            SELECT $1, $2, $3, 'cpp', 'fixture/rate-' || value, 1, 'PENDING'
            FROM generate_series(1, 19) value
            "#,
    )
    .bind(contest_id)
    .bind(problem_id)
    .bind(team_id)
    .execute(&pool)
    .await
    .expect("fill exact rolling rate limit");
    let before = memory.objects.lock().expect("memory storage lock").len();
    let rejected = service
        .submit(
            contest_id,
            ValidatedSubmission {
                problem_id,
                language: "cpp".into(),
                extension: ".cpp",
                source: Bytes::from_static(b"int main() { return 1; }"),
            },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            &storage,
        )
        .await;
    assert!(rejected.is_err());
    assert_eq!(memory.objects.lock().expect("memory storage lock").len(), before);
}
