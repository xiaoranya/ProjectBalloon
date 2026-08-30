use std::net::{IpAddr, Ipv4Addr};

use sqlx::PgPool;

use crate::features::auth::model::{AuthUser, UserType};
use crate::features::balloons::model::NoteRequest;
use crate::features::balloons::service::{
    BalloonService, validate_note, validate_reason, validate_status,
};

#[test]
fn balloon_input_domains_are_closed() {
    assert_eq!(
        validate_status(Some(" pending ".to_owned())).expect("status"),
        Some("PENDING".to_owned())
    );
    assert!(validate_status(Some("lost".to_owned())).is_err());
    assert!(validate_reason(Some("\n".to_owned())).is_err());
    assert_eq!(validate_note(Some("  note  ".to_owned())).expect("note"), Some("note".to_owned()));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL"]
async fn balloon_workbench_enforces_claim_ownership_and_recovery(pool: PgPool) {
    let first_user = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('balloon-one', 'hash', 'Balloon One', 'STAFF') RETURNING id")
        .fetch_one(&pool).await.expect("insert first balloon operator");
    let second_user = sqlx::query_scalar::<_, i64>("INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('balloon-two', 'hash', 'Balloon Two', 'STAFF') RETURNING id")
        .fetch_one(&pool).await.expect("insert second balloon operator");
    let contest_id = sqlx::query_scalar::<_, i64>("INSERT INTO contests (name, status, visibility, start_at, freeze_at, end_at) VALUES ('Balloon Workbench', 'RUNNING', 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour', now() + interval '2 hours') RETURNING id")
        .fetch_one(&pool).await.expect("insert balloon contest");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('balloon-a', 'Balloon A') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert balloon problem");
    let mut task_ids = Vec::new();
    for index in 1..=2 {
        let team_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO teams (name, seat_no) VALUES ($1, $2) RETURNING id",
        )
        .bind(format!("Balloon Team {index}"))
        .bind(format!("A0{index}"))
        .fetch_one(&pool)
        .await
        .expect("insert balloon team");
        let submission_id = sqlx::query_scalar::<_, i64>("INSERT INTO submissions (contest_id, problem_id, team_id, language, source_object_key, source_size_bytes, source_sha256, status, verdict) VALUES ($1, $2, $3, 'cpp', $4, 10, $5, 'COMPLETED', 'ACCEPTED') RETURNING id")
            .bind(contest_id).bind(problem_id).bind(team_id)
            .bind(format!("sources/balloon-{index}.cpp")).bind(format!("{index}").repeat(64))
            .fetch_one(&pool).await.expect("insert balloon submission");
        let task_id = sqlx::query_scalar::<_, i64>("INSERT INTO balloon_tasks (contest_id, team_id, problem_id, submission_id, color, status, seat_no, team_name, problem_alias) VALUES ($1, $2, $3, $4, '#ff0000', 'PENDING', $5, $6, 'A') RETURNING id")
            .bind(contest_id).bind(team_id).bind(problem_id).bind(submission_id)
            .bind(format!("A0{index}")).bind(format!("Balloon Team {index}"))
            .fetch_one(&pool).await.expect("insert balloon task");
        task_ids.push(task_id);
    }
    let actor = |id, username: &str| AuthUser {
        id,
        username: username.to_owned(),
        display_name: username.to_owned(),
        user_type: UserType::Staff,
        permissions: vec!["BALLOON_MANAGE".to_owned()],
        password_reset_required: false,
    };
    let first = actor(first_user, "balloon-one");
    let second = actor(second_user, "balloon-two");
    let service = BalloonService::new(pool.clone());
    let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

    let claimed =
        service.transition(task_ids[0], "CLAIM", 0, None, &first, ip).await.expect("claim balloon");
    assert_eq!(claimed.status, "CLAIMED");
    let noted = service
        .note(
            task_ids[0],
            NoteRequest { expected_version: 1, note: Some("gate west".to_owned()) },
            &first,
            ip,
        )
        .await
        .expect("note balloon");
    // Another operator can take over and deliver a claimed task; the task
    // is re-assigned to whoever actually delivered it.
    let delivered = service
        .transition(task_ids[0], "DELIVER", noted.version, None, &second, ip)
        .await
        .expect("another operator delivers a claimed balloon");
    assert_eq!(delivered.status, "DELIVERED");
    assert_eq!(delivered.claimed_by_user_id, Some(second.id));

    let cancelled = service
        .transition(task_ids[1], "CANCEL", 0, Some("team absent".to_owned()), &second, ip)
        .await
        .expect("cancel balloon");
    let reopened = service
        .transition(task_ids[1], "REOPEN", cancelled.version, None, &second, ip)
        .await
        .expect("reopen balloon");
    assert_eq!(reopened.status, "PENDING");
    assert_eq!(reopened.reopened_count, 1);
    let stats = service.stats(contest_id, &first).await.expect("balloon stats");
    assert_eq!((stats.total, stats.pending, stats.delivered), (2, 1, 1));
    assert_eq!(service.list(contest_id, None, &first).await.expect("list balloons").len(), 2);
    let (audits, events) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT (SELECT count(*) FROM audit_logs WHERE target_type = 'BALLOON_TASK'), (SELECT count(*) FROM realtime_outbox WHERE event_type = 'BALLOON_TASK_UPDATED')",
    )
    .fetch_one(&pool)
    .await
    .expect("count balloon side effects");
    assert_eq!((audits, events), (5, 5));
}
