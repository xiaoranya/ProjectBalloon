use std::net::{IpAddr, Ipv4Addr};

use sqlx::PgPool;
use time::OffsetDateTime;

use crate::features::announcements::{
    AnnouncementScheduleRunner, AnnouncementService, CreateRequest, UpdateRequest, validate_text,
};
use crate::features::auth::model::{AuthUser, UserType};

#[test]
fn announcement_text_is_trimmed_and_bounded() {
    assert_eq!(
        validate_text(" Title ".into(), " Body ".into()).expect("valid text"),
        ("Title".into(), "Body".into())
    );
    assert!(validate_text(" ".into(), "Body".into()).is_err());
    assert!(validate_text("Title".into(), "x".repeat(16_001)).is_err());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn published_announcement_is_editable_pinnable_and_irreversibly_withdrawn(pool: PgPool) {
    let admin_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('ann-root', 'test-hash', 'Ann Root', 'SUPER_ADMIN') RETURNING id",
    ).fetch_one(&pool).await.expect("insert announcement administrator");
    let user_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('ann-team', 'test-hash', 'Ann Team', 'TEAM') RETURNING id",
    ).fetch_one(&pool).await.expect("insert announcement team user");
    let team_id =
        sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ('Ann Team') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert announcement team");
    sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
        .bind(user_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("link announcement team");
    let contest_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO contests (name, status, visibility, start_at, end_at)
        VALUES ('Announcement Contest', 'RUNNING', 'PRIVATE',
                now() - interval '1 hour', now() + interval '1 hour') RETURNING id
    "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert announcement contest");
    sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
        .bind(contest_id).bind(team_id).execute(&pool).await.expect("roster announcement team");
    let admin = AuthUser {
        id: admin_id,
        username: "ann-root".into(),
        display_name: "Ann Root".into(),
        user_type: UserType::SuperAdmin,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let team = AuthUser {
        id: user_id,
        username: "ann-team".into(),
        display_name: "Ann Team".into(),
        user_type: UserType::Team,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let service = AnnouncementService::new(pool.clone());
    let created = service
        .create(
            contest_id,
            CreateRequest {
                title: "Notice".into(),
                body: "Initial".into(),
                pinned: false,
                scheduled_at: None,
            },
            &admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("publish announcement");
    assert_eq!(service.list(contest_id, false, &team).await.expect("team list").len(), 1);
    let updated = service
        .update(
            created.id,
            UpdateRequest {
                title: None,
                body: Some("Updated".into()),
                pinned: None,
                expected_version: 0,
            },
            &admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("edit announcement");
    assert_eq!(updated.body, "Updated");
    assert_eq!(updated.version, 1);
    let pinned = service
        .pin(created.id, true, &admin, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("pin announcement");
    assert!(pinned.pinned);
    service
        .withdraw(created.id, &admin, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("withdraw announcement");
    assert!(
        service
            .list(contest_id, false, &team)
            .await
            .expect("team list after withdrawal")
            .is_empty()
    );
    let history = service.list(contest_id, true, &admin).await.expect("administrator history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].status, "WITHDRAWN");
    assert!(service.pin(created.id, false, &admin, IpAddr::V4(Ipv4Addr::LOCALHOST)).await.is_err());
    let public_events = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM realtime_outbox WHERE contest_id = $1 AND event_type = 'ANNOUNCEMENT_UPDATED' AND scope = 'PUBLIC'",
    ).bind(contest_id).fetch_one(&pool).await.expect("count announcement events");
    assert_eq!(public_events, 4);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn scheduled_announcements_can_be_changed_cancelled_and_published_once(pool: PgPool) {
    let admin_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username,password_hash,display_name,user_type) VALUES ('schedule-root','test-hash','Schedule Root','SUPER_ADMIN') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert administrator");
    let contest_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests (name,status,visibility,start_at,end_at) VALUES ('Scheduled Announcements','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '2 hours') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert contest");
    let admin = AuthUser {
        id: admin_id,
        username: "schedule-root".into(),
        display_name: "Schedule Root".into(),
        user_type: UserType::SuperAdmin,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let service = AnnouncementService::new(pool.clone());
    let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let scheduled = service
        .create(
            contest_id,
            CreateRequest {
                title: "Later".into(),
                body: "First draft".into(),
                pinned: true,
                scheduled_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(10)),
            },
            &admin,
            ip,
        )
        .await
        .expect("schedule announcement");
    assert_eq!(scheduled.status, "SCHEDULED");
    assert!(scheduled.published_at.is_none());
    assert!(service.list(contest_id, false, &admin).await.expect("public list").is_empty());

    let changed = service
        .update_scheduled(
            scheduled.id,
            CreateRequest {
                title: "Later updated".into(),
                body: "Second draft".into(),
                pinned: false,
                scheduled_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(20)),
            },
            &admin,
            ip,
        )
        .await
        .expect("reschedule announcement");
    assert_eq!(changed.version, 1);
    assert_eq!(changed.title, "Later updated");
    let cancelled =
        service.cancel_scheduled(scheduled.id, &admin, ip).await.expect("cancel schedule");
    assert_eq!(cancelled.status, "CANCELLED");
    assert!(!cancelled.pinned);

    let due = service
        .create(
            contest_id,
            CreateRequest {
                title: "Due".into(),
                body: "Publish me".into(),
                pinned: false,
                scheduled_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(5)),
            },
            &admin,
            ip,
        )
        .await
        .expect("schedule due announcement");
    sqlx::query("UPDATE announcements SET scheduled_at=now()-interval '1 second' WHERE id=$1")
        .bind(due.id)
        .execute(&pool)
        .await
        .expect("make announcement due");
    let first = AnnouncementScheduleRunner::new(pool.clone());
    let second = AnnouncementScheduleRunner::new(pool.clone());
    let (a, b) = tokio::join!(first.publish_due(), second.publish_due());
    assert_eq!(a.expect("first runner") + b.expect("second runner"), 1);
    let published = super::load(&pool, due.id).await.expect("load published announcement");
    assert_eq!(published.status, "PUBLISHED");
    assert!(published.published_at.is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_logs WHERE target_type='ANNOUNCEMENT' AND target_id=$1 AND action='ANNOUNCEMENT_PUBLISHED'",
        )
        .bind(due.id.to_string())
        .fetch_one(&pool)
        .await
        .expect("count publish audits"),
        1
    );

    let expired = service
        .create(
            contest_id,
            CreateRequest {
                title: "Expired".into(),
                body: "Do not publish".into(),
                pinned: true,
                scheduled_at: Some(OffsetDateTime::now_utc() + time::Duration::minutes(5)),
            },
            &admin,
            ip,
        )
        .await
        .expect("schedule expiring announcement");
    sqlx::query("UPDATE announcements SET scheduled_at=now()-interval '1 second' WHERE id=$1")
        .bind(expired.id)
        .execute(&pool)
        .await
        .expect("make expired announcement due");
    sqlx::query("UPDATE contests SET status='ENDED' WHERE id=$1")
        .bind(contest_id)
        .execute(&pool)
        .await
        .expect("end contest");
    assert_eq!(
        AnnouncementScheduleRunner::new(pool.clone())
            .publish_due()
            .await
            .expect("cancel expired schedule"),
        1
    );
    let expired = super::load(&pool, expired.id).await.expect("load expired announcement");
    assert_eq!(expired.status, "CANCELLED");
    assert!(expired.cancelled_at.is_some());
}
