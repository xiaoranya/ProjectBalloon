use crate::features::auth::model::{AuthUser, UserType};
use crate::features::contests::model::{
    ContestSchedule, ContestStatus, ContestVisibility, ValidatedContestClone,
    ValidatedCreateContest,
};
use crate::features::contests::service::ContestService;
use sqlx::PgPool;
use std::net::{IpAddr, Ipv4Addr};
use time::{Duration, OffsetDateTime};

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL"]
async fn competition_mode_rejects_overlapping_schedules(pool: PgPool) {
    let service = ContestService::new(pool).with_competition_mode(true);
    let start = OffsetDateTime::now_utc() + Duration::hours(1);
    let schedule = |offset_minutes| ContestSchedule {
        start_at: start + Duration::minutes(offset_minutes),
        freeze_at: start + Duration::minutes(offset_minutes + 30),
        end_at: start + Duration::minutes(offset_minutes + 60),
    };
    service
        .create(
            ValidatedCreateContest {
                name: "Test round".into(),
                visibility: ContestVisibility::Private,
                schedule: Some(schedule(0)),
            },
            1,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("first contest");
    let error = service
        .create(
            ValidatedCreateContest {
                name: "Official round".into(),
                visibility: ContestVisibility::Private,
                schedule: Some(schedule(30)),
            },
            1,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect_err("overlap must be rejected");
    assert_eq!(error.code(), "COMPETITION_SCHEDULE_OVERLAP");

    service
        .create(
            ValidatedCreateContest {
                name: "Later round".into(),
                visibility: ContestVisibility::Private,
                schedule: Some(schedule(60)),
            },
            1,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("touching half-open intervals do not overlap");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL"]
async fn clone_copies_configuration_and_optionally_active_teams(pool: PgPool) {
    let actor=sqlx::query_scalar::<_,i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('clone-root','hash','Clone Root','SUPER_ADMIN') RETURNING id").fetch_one(&pool).await.expect("actor");
    let source=sqlx::query_scalar::<_,i64>("INSERT INTO contests(name,status,visibility) VALUES('Clone Source','ENDED','PRIVATE') RETURNING id").fetch_one(&pool).await.expect("source");
    let problem = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems(slug,title) VALUES('clone-a','Clone A') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("problem");
    sqlx::query("INSERT INTO contest_problems(contest_id,problem_id,alias,display_order,color) VALUES($1,$2,'A',1,'#ff0000')").bind(source).bind(problem).execute(&pool).await.expect("assignment");
    let team = sqlx::query_scalar::<_, i64>(
        "INSERT INTO teams(name,school) VALUES('Clone Team','School') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("team");
    sqlx::query("INSERT INTO contest_teams(contest_id,team_id,participation_type,group_name) VALUES($1,$2,'OFFICIAL','North')").bind(source).bind(team).execute(&pool).await.expect("roster");
    let start = OffsetDateTime::now_utc() + Duration::hours(1);
    let response = ContestService::new(pool.clone())
        .clone_contest(
            source,
            ValidatedContestClone {
                name: "Clone Target".into(),
                visibility: ContestVisibility::Public,
                schedule: Some(ContestSchedule {
                    start_at: start,
                    freeze_at: start + Duration::hours(4),
                    end_at: start + Duration::hours(5),
                }),
                copy_teams: true,
            },
            actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("clone");
    assert_eq!(response.source_contest_id, source);
    assert_eq!(response.contest.status, ContestStatus::Draft);
    assert_eq!(response.problems_copied, 1);
    assert_eq!(response.teams_copied, 1);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT color FROM contest_problems WHERE contest_id=$1")
            .bind(response.contest.id)
            .fetch_one(&pool)
            .await
            .expect("color"),
        "#ff0000"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT group_name FROM contest_teams WHERE contest_id=$1")
            .bind(response.contest.id)
            .fetch_one(&pool)
            .await
            .expect("group"),
        "North"
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL"]
async fn archive_requires_quiescence_and_database_enforces_read_only(pool: PgPool) {
    let actor_id=sqlx::query_scalar::<_,i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('archive-root','hash','Archive Root','SUPER_ADMIN') RETURNING id").fetch_one(&pool).await.expect("actor");
    let contest=sqlx::query_scalar::<_,i64>("INSERT INTO contests(name,status,visibility) VALUES('Archive Contest','ENDED','PRIVATE') RETURNING id").fetch_one(&pool).await.expect("contest");
    let announcement=sqlx::query_scalar::<_,i64>("INSERT INTO announcements(contest_id,title,body,created_by) VALUES($1,'Final','Body',$2) RETURNING id").bind(contest).bind(actor_id).fetch_one(&pool).await.expect("announcement");
    let award_category=sqlx::query_scalar::<_,i64>("INSERT INTO award_categories(contest_id,code,name,display_order) VALUES($1,'CHAMPION','Champion',1) RETURNING id").bind(contest).fetch_one(&pool).await.expect("award category");
    let award_rule=sqlx::query_scalar::<_,i64>("INSERT INTO award_rules(category_id,rule_type,fixed_count) VALUES($1,'FIXED_COUNT',1) RETURNING id").bind(award_category).fetch_one(&pool).await.expect("award rule");
    let run = sqlx::query_scalar::<_, i64>(
        "INSERT INTO resolver_runs(contest_id,official) VALUES($1,true) RETURNING id",
    )
    .bind(contest)
    .fetch_one(&pool)
    .await
    .expect("resolver run");
    let actor = AuthUser {
        id: actor_id,
        username: "archive-root".into(),
        display_name: "Archive Root".into(),
        user_type: UserType::SuperAdmin,
        permissions: vec![],
        password_reset_required: false,
    };
    let service = ContestService::new(pool.clone());
    assert!(
        service
            .transition(contest, ContestStatus::Archived, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .is_err()
    );
    sqlx::query("UPDATE resolver_runs SET status='COMPLETED',started_at=now(),completed_at=now() WHERE id=$1")
        .bind(run)
        .execute(&pool)
        .await
        .expect("complete resolver");
    let archived = service
        .transition(contest, ContestStatus::Archived, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("archive");
    assert_eq!(archived.to, ContestStatus::Archived);
    assert!(
        sqlx::query("UPDATE announcements SET title='Changed' WHERE id=$1")
            .bind(announcement)
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("INSERT INTO announcements(contest_id,title,body,created_by) VALUES($1,'Late','Body',$2)").bind(contest).bind(actor_id).execute(&pool).await.is_err());
    assert!(
        sqlx::query("UPDATE award_rules SET fixed_count=2 WHERE id=$1")
            .bind(award_rule)
            .execute(&pool)
            .await
            .is_err()
    );
}
