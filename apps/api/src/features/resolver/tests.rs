use std::net::{IpAddr, Ipv4Addr};

use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::features::auth::model::{AuthUser, UserType};
use crate::features::resolver::plan::build_states;
use crate::features::resolver::{
    AutoPlayRequest, CreateRequest, ResolverAutoRunner, ResolverService,
};
use crate::features::scoreboard::{
    ScoreboardCell, ScoreboardProblem, ScoreboardResponse, ScoreboardRow,
};
use time::OffsetDateTime;

fn board(solved: bool) -> ScoreboardResponse {
    let solved_at = solved.then(OffsetDateTime::now_utc);
    ScoreboardResponse {
        contest_id: 1,
        variant: "PUBLIC".into(),
        frozen: true,
        scoring_mode: "ICPC".into(),
        score_aggregation: "BEST".into(),
        generated_at: OffsetDateTime::now_utc(),
        problems: vec![ScoreboardProblem {
            problem_id: 1,
            alias: "A".into(),
            display_order: 1,
            first_blood_team_id: None,
            first_blood_at: None,
        }],
        rows: vec![ScoreboardRow {
            rank: 1,
            official_rank: Some(1),
            team_id: 1,
            team_name: "Team".into(),
            school: None,
            participation_type: "OFFICIAL".into(),
            group_name: None,
            is_star: false,
            solved_count: i32::from(solved),
            penalty_minutes: if solved { 60 } else { 0 },
            total_score_milli: if solved { 100_000 } else { 0 },
            last_solved_at: solved_at,
            problems: vec![ScoreboardCell {
                problem_id: 1,
                wrong_attempts: 0,
                solved,
                solved_at,
                penalty_minutes: if solved { 60 } else { 0 },
                score_milli: if solved { 100_000 } else { 0 },
                first_blood: solved,
            }],
        }],
    }
}

#[test]
fn plan_is_deterministic_and_reaches_final_cell() {
    let states = build_states(board(false), board(true)).expect("build plan");
    assert_eq!(states.len(), 2);
    assert_eq!(states[1].step_index, 1);
    assert!(states[1].board.rows[0].problems[0].solved);
    assert_eq!(states[1].board.rows[0].solved_count, 1);
}

#[test]
fn public_state_does_not_expose_internal_run_metadata() {
    let response = super::ResolverPublicStateResponse {
        id: 9,
        contest_id: 7,
        status: "RUNNING".to_owned(),
        current_step: 1,
        total_steps: 2,
        updated_at: OffsetDateTime::now_utc(),
        state: serde_json::json!({"stepIndex": 1}),
    };
    let value = serde_json::to_value(response).expect("serialize public Resolver state");
    assert!(value.get("state").is_some());
    assert!(value.get("createdByUserId").is_none());
    assert!(value.get("sourcePublicSnapshotId").is_none());
    assert!(value.get("planSha256").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL"]
async fn official_run_is_immutable_reversible_and_restart_safe(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('resolver-op', 'hash', 'Resolver Operator', 'STAFF') RETURNING id")
            .fetch_one(&pool).await.expect("insert Resolver operator");
    let contest_id = sqlx::query_scalar::<_, i64>("INSERT INTO contests (name, status, visibility, start_at, freeze_at, end_at) VALUES ('Resolver Contest', 'ENDED', 'PUBLIC', now() - interval '3 hours', now() - interval '2 hours', now() - interval '1 hour') RETURNING id")
            .fetch_one(&pool).await.expect("insert Resolver contest");
    let public_board = board(false);
    let public_payload = serde_json::to_string(&public_board).expect("encode public board");
    let public_sha = hex::encode(Sha256::digest(public_payload.as_bytes()));
    let public_id = sqlx::query_scalar::<_, i64>("INSERT INTO scoreboard_snapshots (contest_id, variant, version, frozen, generated_at, payload_json, payload_sha256, created_by, created_by_user_id) VALUES ($1, 'PUBLIC', 1, true, now(), $2, $3, 'resolver-op', $4) RETURNING id")
            .bind(contest_id).bind(public_payload).bind(public_sha).bind(user_id)
            .fetch_one(&pool).await.expect("insert public source snapshot");
    let mut final_board = board(true);
    final_board.contest_id = contest_id;
    final_board.variant = "ADMIN".to_owned();
    final_board.frozen = false;
    let final_payload = serde_json::to_string(&final_board).expect("encode final board");
    let final_sha = hex::encode(Sha256::digest(final_payload.as_bytes()));
    let final_id = sqlx::query_scalar::<_, i64>("INSERT INTO scoreboard_snapshots (contest_id, variant, version, frozen, generated_at, payload_json, payload_sha256, created_by, created_by_user_id) VALUES ($1, 'ADMIN', 1, false, now(), $2, $3, 'resolver-op', $4) RETURNING id")
            .bind(contest_id).bind(final_payload).bind(final_sha).bind(user_id)
            .fetch_one(&pool).await.expect("insert final source snapshot");
    sqlx::query("UPDATE scoreboard_snapshots SET payload_json = replace(payload_json, '\"contestId\":1', $2) WHERE id = $1")
            .bind(public_id).bind(format!("\"contestId\":{contest_id}"))
            .execute(&pool).await.expect_err("source snapshots are immutable");
    // Build the public payload with the actual contest before inserting a replacement snapshot.
    let mut actual_public = board(false);
    actual_public.contest_id = contest_id;
    let payload = serde_json::to_string(&actual_public).expect("encode actual public board");
    let sha = hex::encode(Sha256::digest(payload.as_bytes()));
    let actual_public_id = sqlx::query_scalar::<_, i64>("INSERT INTO scoreboard_snapshots (contest_id, variant, version, frozen, generated_at, payload_json, payload_sha256, created_by, created_by_user_id) VALUES ($1, 'PUBLIC', 2, true, now(), $2, $3, 'resolver-op', $4) RETURNING id")
            .bind(contest_id).bind(payload).bind(sha).bind(user_id)
            .fetch_one(&pool).await.expect("insert actual public source snapshot");
    let actor = AuthUser {
        id: user_id,
        username: "resolver-op".to_owned(),
        display_name: "Resolver Operator".to_owned(),
        user_type: UserType::Staff,
        permissions: vec!["RESOLVER_MANAGE".to_owned()],
        password_reset_required: false,
    };
    let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let service = ResolverService::new(pool.clone());
    let sources = service.sources(contest_id, &actor).await.expect("load Resolver sources");
    assert_eq!(
        (sources.public_snapshot.id, sources.final_snapshot.id),
        (actual_public_id, final_id)
    );
    let created = service
        .create(
            contest_id,
            CreateRequest {
                public_snapshot_id: actual_public_id,
                final_snapshot_id: final_id,
                official: true,
            },
            &actor,
            ip,
        )
        .await
        .expect("create official Resolver run");
    assert_eq!(
        (created.status.as_str(), created.current_step, created.total_steps),
        ("READY", 0, 1)
    );
    assert_eq!(service.list(contest_id, &actor).await.expect("list runs").len(), 1);
    assert!(service.public_state(created.id).await.is_err());

    let started = service.command(created.id, "START", 0, &actor, ip).await.expect("start");
    assert_eq!(started.status, "RUNNING");
    assert!(service.public_state(created.id).await.is_ok());
    let advanced = service.command(created.id, "NEXT", 1, &actor, ip).await.expect("next");
    assert_eq!(advanced.state["board"]["rows"][0]["solvedCount"], 1);
    let backed = service.command(created.id, "PREVIOUS", 2, &actor, ip).await.expect("previous");
    assert_eq!(backed.current_step, 0);
    let advanced = service.command(created.id, "NEXT", 3, &actor, ip).await.expect("next again");
    let paused = service.command(created.id, "PAUSE", 4, &actor, ip).await.expect("pause");
    let resumed = service.command(created.id, "RESUME", 5, &actor, ip).await.expect("resume");
    let completed = service.command(created.id, "COMPLETE", 6, &actor, ip).await.expect("complete");
    assert_eq!(
        (
            advanced.current_step,
            paused.status.as_str(),
            resumed.status.as_str(),
            completed.status.as_str()
        ),
        (1, "PAUSED", "RUNNING", "COMPLETED")
    );
    assert!(completed.completed_at.is_some());

    let recovered = ResolverService::new(pool.clone())
        .get(created.id, &actor)
        .await
        .expect("recover after restart");
    assert_eq!((recovered.status.as_str(), recovered.current_step), ("COMPLETED", 1));
    assert_eq!(service.events(created.id, &actor).await.expect("events").len(), 8);
    let preview = service
        .create(
            contest_id,
            CreateRequest {
                public_snapshot_id: actual_public_id,
                final_snapshot_id: final_id,
                official: false,
            },
            &actor,
            ip,
        )
        .await
        .expect("create preview Resolver run");
    let preview = service.command(preview.id, "START", 0, &actor, ip).await.expect("start preview");
    let preview = service
        .configure_auto_play(
            preview.id,
            AutoPlayRequest {
                expected_version: preview.version,
                enabled: true,
                interval_milliseconds: 500,
            },
            &actor,
            ip,
        )
        .await
        .expect("enable auto-play");
    sqlx::query("UPDATE resolver_runs SET next_auto_at = now() WHERE id = $1")
        .bind(preview.id)
        .execute(&pool)
        .await
        .expect("make auto-play due");
    assert!(ResolverAutoRunner::new(pool.clone()).advance_due().await.expect("auto advance"));
    let auto_advanced = service.get(preview.id, &actor).await.expect("load auto advance");
    assert_eq!(auto_advanced.current_step, 1);
    assert!(!auto_advanced.auto_play_enabled);
    assert!(
        sqlx::query(
            "UPDATE resolver_snapshots SET state_data = '{}' WHERE run_id = $1 AND step_index = 0"
        )
        .bind(created.id)
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(sqlx::query("UPDATE resolver_runs SET source_final_snapshot_id = source_public_snapshot_id WHERE id = $1")
            .bind(created.id).execute(&pool).await.is_err());
}

fn resolver_cell(
    problem_id: i64,
    solved: bool,
    penalty_minutes: i64,
    solved_at: Option<OffsetDateTime>,
) -> ScoreboardCell {
    ScoreboardCell {
        problem_id,
        wrong_attempts: 0,
        solved,
        solved_at,
        penalty_minutes,
        score_milli: 0,
        first_blood: false,
    }
}

#[test]
fn recompute_board_rewrites_aggregates_ranks_and_first_blood() {
    use crate::features::resolver::plan::recompute_board;

    let earlier = OffsetDateTime::from_unix_timestamp(300).expect("timestamp");
    let later = OffsetDateTime::from_unix_timestamp(540).expect("timestamp");
    let mut board = ScoreboardResponse {
        contest_id: 1,
        variant: "PUBLIC".into(),
        frozen: true,
        scoring_mode: "ICPC".into(),
        score_aggregation: "BEST".into(),
        generated_at: earlier,
        problems: vec![ScoreboardProblem {
            problem_id: 1,
            alias: "A".into(),
            display_order: 1,
            first_blood_team_id: None,
            first_blood_at: None,
        }],
        rows: vec![
            ScoreboardRow {
                team_id: 2,
                rank: 1,
                official_rank: Some(1),
                team_name: "Team 2".into(),
                school: None,
                participation_type: "OFFICIAL".into(),
                group_name: None,
                is_star: false,
                solved_count: 1,
                penalty_minutes: 30,
                total_score_milli: 0,
                last_solved_at: Some(later),
                problems: vec![resolver_cell(1, true, 30, Some(later))],
            },
            ScoreboardRow {
                team_id: 1,
                rank: 2,
                official_rank: Some(2),
                team_name: "Team 1".into(),
                school: None,
                participation_type: "OFFICIAL".into(),
                group_name: None,
                is_star: false,
                solved_count: 1,
                penalty_minutes: 30,
                total_score_milli: 0,
                last_solved_at: None,
                problems: vec![resolver_cell(1, true, 5, Some(earlier))],
            },
        ],
    };

    recompute_board(&mut board);
    assert_eq!(board.rows[0].team_id, 1, "smaller penalty must overtake after the reveal");
    assert_eq!(board.rows[0].rank, 1);
    assert_eq!(board.rows[0].penalty_minutes, 5);
    assert!(board.rows[0].problems[0].first_blood);
    assert_eq!(board.problems[0].first_blood_team_id, Some(1));
    assert_eq!(board.problems[0].first_blood_at, Some(earlier));
    assert_eq!(board.rows[1].team_id, 2);
    assert_eq!(board.rows[1].rank, 2);
    assert_eq!(board.rows[1].official_rank, Some(2));
    assert!(!board.rows[1].problems[0].first_blood);
}

fn scored_cell(
    problem_id: i64,
    score_milli: i32,
    solved: bool,
    solved_at: Option<OffsetDateTime>,
) -> ScoreboardCell {
    ScoreboardCell {
        problem_id,
        wrong_attempts: 0,
        solved,
        solved_at,
        penalty_minutes: 0,
        score_milli,
        first_blood: false,
    }
}

fn scored_problem(problem_id: i64, alias: &str) -> ScoreboardProblem {
    ScoreboardProblem {
        problem_id,
        alias: alias.into(),
        display_order: i32::try_from(problem_id).expect("problem id fits"),
        first_blood_team_id: None,
        first_blood_at: None,
    }
}

fn scored_row(team_id: i64, name: &str, problems: Vec<ScoreboardCell>) -> ScoreboardRow {
    ScoreboardRow {
        rank: 0,
        official_rank: None,
        team_id,
        team_name: name.into(),
        school: None,
        participation_type: "OFFICIAL".into(),
        group_name: None,
        is_star: false,
        solved_count: 0,
        penalty_minutes: 0,
        total_score_milli: 0,
        last_solved_at: None,
        problems,
    }
}

/// The ceremony used to carry its own ICPC-only comparator, so an OI/IOI board
/// ranked a team with two 10-point solves above a team with one 100-point solve
/// — the live board did the opposite. Both now share `scoreboard::helpers`.
#[test]
fn recompute_board_ranks_oi_teams_by_score_not_by_solved_count() {
    use crate::features::resolver::plan::recompute_board;

    let early = OffsetDateTime::from_unix_timestamp(300).expect("timestamp");
    let late = OffsetDateTime::from_unix_timestamp(540).expect("timestamp");
    let mut board = ScoreboardResponse {
        contest_id: 1,
        variant: "PUBLIC".into(),
        frozen: false,
        scoring_mode: "OI".into(),
        score_aggregation: "LAST".into(),
        generated_at: early,
        problems: vec![scored_problem(1, "A"), scored_problem(2, "B"), scored_problem(3, "C")],
        rows: vec![
            scored_row(
                1,
                "Two small solves",
                vec![
                    scored_cell(1, 10_000, true, Some(early)),
                    scored_cell(2, 10_000, true, Some(late)),
                    scored_cell(3, 0, false, None),
                ],
            ),
            scored_row(
                2,
                "One big solve",
                vec![
                    scored_cell(1, 0, false, None),
                    scored_cell(2, 0, false, None),
                    scored_cell(3, 100_000, true, Some(late)),
                ],
            ),
        ],
    };

    recompute_board(&mut board);
    assert_eq!(board.rows[0].team_id, 2, "the 100-point solve must lead the ceremony");
    assert_eq!(board.rows[0].total_score_milli, 100_000);
    assert_eq!(board.rows[0].official_rank, Some(1));
    assert_eq!(board.rows[1].team_id, 1);
    assert_eq!(board.rows[1].total_score_milli, 20_000);
    assert_eq!(board.rows[1].solved_count, 2, "solved count is still reported, just not ranked on");
}

/// Guard against a future regression that reintroduces a private comparator:
/// the same two reveals must produce the ICPC order under ICPC and the score
/// order under OI/IOI.
#[test]
fn recompute_board_ordering_follows_the_contest_scoring_mode() {
    use crate::features::resolver::plan::recompute_board;

    let early = OffsetDateTime::from_unix_timestamp(300).expect("timestamp");
    let late = OffsetDateTime::from_unix_timestamp(540).expect("timestamp");
    for (scoring_mode, expected) in [("ICPC", vec![1, 2]), ("OI", vec![2, 1]), ("IOI", vec![2, 1])]
    {
        let mut board = ScoreboardResponse {
            contest_id: 1,
            variant: "PUBLIC".into(),
            frozen: false,
            scoring_mode: scoring_mode.into(),
            score_aggregation: "BEST".into(),
            generated_at: early,
            problems: vec![scored_problem(1, "A"), scored_problem(2, "B")],
            rows: vec![
                scored_row(
                    1,
                    "Small early solve",
                    vec![scored_cell(1, 0, false, None), scored_cell(2, 10_000, true, Some(early))],
                ),
                scored_row(
                    2,
                    "Big late solve",
                    vec![scored_cell(1, 100_000, true, Some(late)), scored_cell(2, 0, false, None)],
                ),
            ],
        };

        recompute_board(&mut board);
        assert_eq!(
            board.rows.iter().map(|row| row.team_id).collect::<Vec<_>>(),
            expected,
            "{scoring_mode}: the ceremony must rank like the live board"
        );
        assert_eq!(
            board.rows.iter().map(|row| row.rank).collect::<Vec<_>>(),
            vec![1, 2],
            "{scoring_mode}: ranks must be dense after sorting"
        );
    }
}
