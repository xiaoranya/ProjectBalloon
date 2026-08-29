use std::time::Duration as StdDuration;

use redis::AsyncCommands;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::features::{
    auth::model::{AuthUser, UserType},
    scoreboard::{ScoreboardCache, projection::rebuild_cell},
};

use crate::features::scoreboard::helpers::{assemble, csv_field, score_submissions, to_csv};
use crate::features::scoreboard::model::{
    CellRow, RosterRow, ScoreboardProblem, SubmissionScoreRow, ValidatedScoreboardQuery,
    ValidatedSnapshotSelector,
};
use crate::features::scoreboard::service::ScoreboardService;

#[test]
fn scoreboard_csv_blocks_spreadsheet_formulas() {
    assert_eq!(csv_field("=cmd()"), "'=cmd()");
    assert_eq!(csv_field("+1"), "'+1");
    assert_eq!(csv_field("A,B"), "\"A,B\"");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn public_freeze_hides_late_acceptance_while_admin_sees_true_board(pool: PgPool) {
    let now = OffsetDateTime::now_utc();
    let start_at = now - Duration::hours(2);
    let freeze_at = start_at + Duration::hours(1);
    let end_at = now + Duration::hours(1);
    let contest_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO contests
            (name, status, visibility, start_at, freeze_at, end_at)
        VALUES ('Frozen Board', 'RUNNING', 'PUBLIC', $1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(start_at)
    .bind(freeze_at)
    .bind(end_at)
    .fetch_one(&pool)
    .await
    .expect("insert contest");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('board-a', 'Board A') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert problem");
    sqlx::query(
        "INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)",
    )
    .bind(contest_id)
    .bind(problem_id)
    .execute(&pool)
    .await
    .expect("assign problem");
    let mut teams = Vec::new();
    for (name, participation_type) in
        [("Official, Team", "OFFICIAL"), ("Star Team", "STAR"), ("Practice Team", "PRACTICE")]
    {
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name, school) VALUES ($1, 'XCPC University') RETURNING id",
        )
        .bind(name)
        .fetch_one(&pool)
        .await
        .expect("insert team");
        sqlx::query(
            r#"
            INSERT INTO contest_teams
                (contest_id, team_id, participation_type, group_name)
            VALUES ($1, $2, $3, 'East')
            "#,
        )
        .bind(contest_id)
        .bind(team_id)
        .bind(participation_type)
        .execute(&pool)
        .await
        .expect("roster team");
        teams.push(team_id);
    }
    insert_scored_submission(
        &pool,
        contest_id,
        problem_id,
        teams[0],
        start_at + Duration::minutes(10),
        "WRONG_ANSWER",
    )
    .await;
    insert_scored_submission(
        &pool,
        contest_id,
        problem_id,
        teams[0],
        start_at + Duration::minutes(50),
        "ACCEPTED",
    )
    .await;
    insert_scored_submission(
        &pool,
        contest_id,
        problem_id,
        teams[1],
        start_at + Duration::minutes(70),
        "ACCEPTED",
    )
    .await;
    let mut transaction = pool.begin().await.expect("begin projection transaction");
    rebuild_cell(&mut transaction, contest_id, teams[0], problem_id)
        .await
        .expect("project official team");
    rebuild_cell(&mut transaction, contest_id, teams[1], problem_id)
        .await
        .expect("project star team");
    transaction.commit().await.expect("commit projections");

    let service = ScoreboardService::new(pool.clone());
    let query = || ValidatedScoreboardQuery { group_name: None, participation_type: None };
    let public = service.public(contest_id, query()).await.expect("load public board");
    assert!(public.frozen);
    assert_eq!(public.rows.len(), 3);
    assert_eq!(public.rows[0].team_id, teams[0]);
    assert_eq!((public.rows[0].solved_count, public.rows[0].penalty_minutes), (1, 70));
    assert_eq!(public.rows[0].official_rank, Some(1));
    assert!(public.rows[0].problems[0].first_blood);
    assert_eq!(public.problems[0].first_blood_team_id, Some(teams[0]));
    assert_eq!(public.rows[1].solved_count, 0);
    assert_eq!(public.rows[2].solved_count, 0);

    let admin_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO users (username, password_hash, display_name, user_type)
        VALUES ('snapshot-root', 'test-only-hash', 'Snapshot Root', 'SUPER_ADMIN')
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert snapshot creator");
    let admin = AuthUser {
        id: admin_id,
        username: "root".to_owned(),
        display_name: "Root".to_owned(),
        user_type: UserType::SuperAdmin,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let true_board = service.admin(contest_id, &admin, query()).await.expect("load admin board");
    assert!(!true_board.frozen);
    assert_eq!(true_board.rows[0].team_id, teams[0]);
    assert_eq!(true_board.rows[1].team_id, teams[1]);
    assert_eq!(true_board.rows[1].solved_count, 1);
    assert_eq!(true_board.rows[1].official_rank, None);
    assert_eq!(true_board.rows[2].team_id, teams[2]);
    let csv = to_csv(&true_board);
    assert!(csv.contains("\"Official, Team\""));
    assert!(csv.contains("+1@50"));
    let snapshot_selector = || ValidatedSnapshotSelector {
        variant: "PUBLIC",
        query: ValidatedScoreboardQuery {
            group_name: Some("East".to_owned()),
            participation_type: None,
        },
    };
    let first_snapshot = service
        .create_snapshot(
            contest_id,
            &admin,
            "127.0.0.1".parse().expect("loopback IP"),
            snapshot_selector(),
        )
        .await
        .expect("create first immutable snapshot");
    let second_snapshot = service
        .create_snapshot(
            contest_id,
            &admin,
            "127.0.0.1".parse().expect("loopback IP"),
            snapshot_selector(),
        )
        .await
        .expect("create second immutable snapshot");
    assert_eq!((first_snapshot.version, second_snapshot.version), (1, 2));
    assert_eq!(first_snapshot.payload_sha256.len(), 64);
    assert_eq!(first_snapshot.payload["frozen"], true);
    let latest = service
        .latest_snapshot(contest_id, &admin, snapshot_selector())
        .await
        .expect("load latest immutable snapshot");
    assert_eq!(latest.id, second_snapshot.id);
    let snapshot_audits = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM audit_logs WHERE action = 'SCOREBOARD_SNAPSHOT_CREATED' AND actor_user_id = $1",
    )
    .bind(admin.id)
    .fetch_one(&pool)
    .await
    .expect("count snapshot audits");
    assert_eq!(snapshot_audits, 2);
    let mutation = sqlx::query("UPDATE scoreboard_snapshots SET frozen = false WHERE id = $1")
        .bind(first_snapshot.id)
        .execute(&pool)
        .await;
    assert!(mutation.is_err(), "database must reject snapshot mutation");
    let accepted_submission_id = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT submission.id
        FROM submissions submission
        JOIN judgements judgement ON judgement.submission_id = submission.id
        WHERE submission.contest_id = $1
          AND submission.team_id = $2
          AND submission.problem_id = $3
          AND judgement.active_marker IS TRUE
          AND judgement.verdict = 'ACCEPTED'
        "#,
    )
    .bind(contest_id)
    .bind(teams[0])
    .bind(problem_id)
    .fetch_one(&pool)
    .await
    .expect("find accepted submission for rejudge");
    sqlx::query(
        r#"
        UPDATE judgements
        SET superseded = true, active_marker = NULL
        WHERE submission_id = $1 AND active_marker IS TRUE
        "#,
    )
    .bind(accepted_submission_id)
    .execute(&pool)
    .await
    .expect("supersede accepted judgement");
    sqlx::query(
        r#"
        INSERT INTO judgements (id, submission_id, verdict, completed_at)
        VALUES ($1, $2, 'WRONG_ANSWER', now())
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(accepted_submission_id)
    .execute(&pool)
    .await
    .expect("insert replacement judgement");
    let mut transaction = pool.begin().await.expect("begin rejudge projection");
    rebuild_cell(&mut transaction, contest_id, teams[0], problem_id)
        .await
        .expect("rebuild rejudged scoreboard cell");
    transaction.commit().await.expect("commit rejudge projection");
    let rolled_back = service
        .admin(contest_id, &admin, query())
        .await
        .expect("load scoreboard after AC rollback");
    assert_eq!(rolled_back.rows[0].team_id, teams[1]);
    assert_eq!(rolled_back.rows[0].solved_count, 1);
    assert!(rolled_back.rows[0].problems[0].first_blood);
    let official = rolled_back
        .rows
        .iter()
        .find(|row| row.team_id == teams[0])
        .expect("official team remains on board");
    assert_eq!((official.solved_count, official.penalty_minutes), (0, 0));
    assert_eq!(official.problems[0].wrong_attempts, 2);
    let persisted_snapshot = service
        .latest_snapshot(contest_id, &admin, snapshot_selector())
        .await
        .expect("snapshot survives live rejudge");
    assert_eq!(persisted_snapshot.payload["rows"][0]["teamId"], teams[0]);
    assert_eq!(persisted_snapshot.payload["rows"][0]["solvedCount"], 1);

    let stars = service
        .public(
            contest_id,
            ValidatedScoreboardQuery {
                group_name: Some("East".to_owned()),
                participation_type: Some("STAR".to_owned()),
            },
        )
        .await
        .expect("filter star board");
    assert_eq!(stars.rows.len(), 1);
    assert_eq!(stars.rows[0].team_id, teams[1]);
    assert_eq!(stars.rows[0].solved_count, 0);

    if let Ok(redis_url) = std::env::var("PROJECT_BALLOON_TEST_REDIS_URL") {
        let cache = ScoreboardCache::connect(
            &redis_url,
            StdDuration::from_secs(300),
            StdDuration::from_millis(500),
        )
        .await
        .expect("connect scoreboard test Redis");
        let cached_service = ScoreboardService::new(pool.clone()).with_cache(cache);
        let first = cached_service
            .admin(contest_id, &admin, query())
            .await
            .expect("populate scoreboard cache");
        let hit = cached_service
            .admin(contest_id, &admin, query())
            .await
            .expect("read scoreboard cache hit");
        assert_eq!(hit.generated_at, first.generated_at);

        let client = redis::Client::open(redis_url).expect("open scoreboard test Redis");
        let mut redis =
            client.get_multiplexed_async_connection().await.expect("connect Redis recovery probe");
        let _: () = redis.flushdb().await.expect("clear scoreboard test Redis");
        let rebuilt_after_clear = cached_service
            .admin(contest_id, &admin, query())
            .await
            .expect("rebuild cleared scoreboard cache from PostgreSQL");
        assert_ne!(rebuilt_after_clear.generated_at, first.generated_at);
        assert_eq!(rebuilt_after_clear.rows.len(), first.rows.len());

        sqlx::query("UPDATE teams SET name = 'Renamed Official Team' WHERE id = $1")
            .bind(teams[0])
            .execute(&pool)
            .await
            .expect("rename team and bump scoreboard revision");
        let rebuilt = cached_service
            .admin(contest_id, &admin, query())
            .await
            .expect("bypass stale revision cache");
        assert_ne!(rebuilt.generated_at, first.generated_at);
        assert_eq!(
            rebuilt
                .rows
                .iter()
                .find(|row| row.team_id == teams[0])
                .expect("renamed team remains visible")
                .team_name,
            "Renamed Official Team"
        );
    }
}

#[test]
fn icpc_frozen_score_uses_the_assigned_problem_maximum() {
    let start = OffsetDateTime::UNIX_EPOCH;
    let cells = score_submissions(
        start,
        "ICPC",
        "BEST",
        vec![SubmissionScoreRow {
            submission_id: 1,
            team_id: 7,
            problem_id: 10,
            submitted_at: start + Duration::minutes(5),
            verdict: "ACCEPTED".to_owned(),
            score_milli: 100_000,
            max_score_milli: 250_000,
        }],
    );
    assert_eq!(cells[0].score_milli, 250_000);
}

#[test]
fn same_second_ties_use_team_id_for_rank_and_first_blood() {
    let solved_at =
        OffsetDateTime::from_unix_timestamp(1_750_000_000).expect("fixed scoreboard timestamp");
    let board = assemble(
        1,
        "ADMIN",
        false,
        solved_at,
        "ICPC".to_owned(),
        "BEST".to_owned(),
        vec![ScoreboardProblem {
            problem_id: 10,
            alias: "A".to_owned(),
            display_order: 1,
            first_blood_team_id: None,
            first_blood_at: None,
        }],
        vec![
            RosterRow {
                team_id: 20,
                team_name: "Higher ID".to_owned(),
                school: None,
                participation_type: "OFFICIAL".to_owned(),
                group_name: None,
                team_star: false,
            },
            RosterRow {
                team_id: 10,
                team_name: "Lower ID".to_owned(),
                school: None,
                participation_type: "OFFICIAL".to_owned(),
                group_name: None,
                team_star: false,
            },
        ],
        vec![
            CellRow {
                team_id: 20,
                problem_id: 10,
                wrong_attempts: 0,
                solved: true,
                solved_at: Some(solved_at),
                penalty_minutes: 60,
                score_milli: 100_000,
            },
            CellRow {
                team_id: 10,
                problem_id: 10,
                wrong_attempts: 0,
                solved: true,
                solved_at: Some(solved_at),
                penalty_minutes: 60,
                score_milli: 100_000,
            },
        ],
    );
    assert_eq!(board.rows[0].team_id, 10);
    assert_eq!(board.rows[0].rank, 1);
    assert!(board.rows[0].problems[0].first_blood);
    assert_eq!(board.problems[0].first_blood_team_id, Some(10));
}

async fn insert_scored_submission(
    pool: &PgPool,
    contest_id: i64,
    problem_id: i64,
    team_id: i64,
    submitted_at: OffsetDateTime,
    verdict: &str,
) {
    let submission_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO submissions
            (contest_id, problem_id, team_id, language, source_object_key,
             source_size_bytes, source_sha256, status, submitted_at, judged_at)
        VALUES ($1, $2, $3, 'cpp', $4, 10, $5, $6, $7, now())
        RETURNING id
        "#,
    )
    .bind(contest_id)
    .bind(problem_id)
    .bind(team_id)
    .bind(format!("scoreboard/{team_id}/{}.cpp", Uuid::new_v4()))
    .bind("c".repeat(64))
    .bind(verdict)
    .bind(submitted_at)
    .fetch_one(pool)
    .await
    .expect("insert scored submission");
    sqlx::query(
        r#"
        INSERT INTO judgements (id, submission_id, verdict, completed_at)
        VALUES ($1, $2, $3, now())
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(submission_id)
    .bind(verdict)
    .execute(pool)
    .await
    .expect("insert scored judgement");
}
