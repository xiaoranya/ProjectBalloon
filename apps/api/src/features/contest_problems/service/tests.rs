use std::net::{IpAddr, Ipv4Addr};

use sqlx::PgPool;

use crate::features::contest_problems::service::ContestProblemService;
use crate::features::{
    auth::model::{AuthUser, UserType},
    contest_problems::model::{
        ValidatedAssignment, ValidatedAssignmentUpdate, ValidatedReorderEntry,
    },
};

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn assignment_is_locked_after_configuration_freeze(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO users
            (username, password_hash, display_name, user_type, enabled,
             password_reset_required)
        VALUES ('admin', 'test-hash', 'Admin', 'SUPER_ADMIN', true, false)
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert admin");
    let contest_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests (name, status, visibility) VALUES ('Test', 'DRAFT', 'PRIVATE') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert contest");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title, created_by) VALUES ('sum', 'Sum', $1) RETURNING id",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("insert problem");
    let actor = AuthUser {
        id: user_id,
        username: "admin".into(),
        display_name: "Admin".into(),
        user_type: UserType::SuperAdmin,
        permissions: vec![],
        password_reset_required: false,
    };
    let service = ContestProblemService::new(pool.clone());
    service
        .assign(
            contest_id,
            ValidatedAssignment {
                problem_id,
                alias: "A".into(),
                display_order: 1,
                color: Some("red".into()),
            },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("draft assignment must succeed");
    let second_problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title, created_by) VALUES ('difference', 'Difference', $1) RETURNING id",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("insert second problem");
    service
        .assign(
            contest_id,
            ValidatedAssignment {
                problem_id: second_problem_id,
                alias: "B".into(),
                display_order: 2,
                color: Some("blue".into()),
            },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("second draft assignment must succeed");
    let reordered = service
        .reorder(
            contest_id,
            vec![
                ValidatedReorderEntry { problem_id, display_order: 2 },
                ValidatedReorderEntry { problem_id: second_problem_id, display_order: 1 },
            ],
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("position exchange must succeed atomically");
    assert_eq!(reordered[0].problem_id, second_problem_id);
    assert_eq!(reordered[1].problem_id, problem_id);

    let incomplete = service
        .reorder(
            contest_id,
            vec![ValidatedReorderEntry { problem_id, display_order: 1 }],
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await;
    assert!(incomplete.is_err());
    let stored_order = sqlx::query_scalar::<_, i32>(
        "SELECT display_order FROM contest_problems WHERE contest_id = $1 AND problem_id = $2",
    )
    .bind(contest_id)
    .bind(problem_id)
    .fetch_one(&pool)
    .await
    .expect("read order after rejected request");
    assert_eq!(stored_order, 2);

    sqlx::query("UPDATE contests SET status = 'FROZEN_CONFIG' WHERE id = $1")
        .bind(contest_id)
        .execute(&pool)
        .await
        .expect("freeze contest");
    let frozen_update = service
        .update(
            contest_id,
            problem_id,
            ValidatedAssignmentUpdate { alias: Some("B".into()), display_order: None, color: None },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await;
    assert!(frozen_update.is_err());
    let frozen_reorder = service
        .reorder(
            contest_id,
            vec![
                ValidatedReorderEntry { problem_id, display_order: 1 },
                ValidatedReorderEntry { problem_id: second_problem_id, display_order: 2 },
            ],
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await;
    assert!(frozen_reorder.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn contest_manager_can_assign_and_remove_within_scope(pool: PgPool) {
    let admin_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('problem-manager', 'test-hash', 'Problem Manager', 'STAFF', true, false) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert contest admin");
    let contest_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests (name, status, visibility) VALUES ('Scoped Assignment', 'DRAFT', 'PRIVATE') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert contest");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title, created_by) VALUES ('scoped-assignment', 'Scoped Assignment', $1) RETURNING id",
    )
    .bind(admin_id)
    .fetch_one(&pool)
    .await
    .expect("insert problem");
    sqlx::query("INSERT INTO contest_management_assignments (user_id, contest_id) VALUES ($1, $2)")
        .bind(admin_id)
        .bind(contest_id)
        .execute(&pool)
        .await
        .expect("assign contest admin scope");
    let actor = AuthUser {
        id: admin_id,
        username: "problem-manager".into(),
        display_name: "Problem Manager".into(),
        user_type: UserType::Staff,
        permissions: vec!["CONTEST_MANAGE".into()],
        password_reset_required: false,
    };
    let service = ContestProblemService::new(pool.clone());
    service
        .assign(
            contest_id,
            ValidatedAssignment {
                problem_id,
                alias: "A".into(),
                display_order: 1,
                color: Some("red".into()),
            },
            &actor,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("scoped contest admin can assign problem");
    service
        .remove(contest_id, problem_id, &actor, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("scoped contest admin can remove problem");
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM contest_problems WHERE contest_id = $1 AND problem_id = $2",
    )
    .bind(contest_id)
    .bind(problem_id)
    .fetch_one(&pool)
    .await
    .expect("count removed assignment");
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn rostered_team_sees_only_started_contest_with_safe_preferred_statement(pool: PgPool) {
    let team_user_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO users
            (username, password_hash, display_name, user_type, enabled,
             password_reset_required)
        VALUES ('team-1', 'test-hash', 'Team 1', 'TEAM', true, false)
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert team user");
    let team_id =
        sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ('Team 1') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert team");
    sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
        .bind(team_user_id)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("link team account");
    let contest_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests (name, status, visibility) VALUES ('Team Test', 'DRAFT', 'PRIVATE') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert contest");
    sqlx::query(
        "INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')",
    )
    .bind(contest_id)
    .bind(team_id)
    .execute(&pool)
    .await
    .expect("insert roster");
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title, default_lang_code) VALUES ('sum', 'Sum', 'en') RETURNING id",
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
    sqlx::query(
        r#"
        INSERT INTO problem_statements (problem_id, lang_code, body)
        VALUES
            ($1, 'en', '# Sum'),
            ($1, 'zh-CN', '# 求和<script>alert(1)</script>')
        "#,
    )
    .bind(problem_id)
    .execute(&pool)
    .await
    .expect("insert statements");
    let actor = AuthUser {
        id: team_user_id,
        username: "team-1".into(),
        display_name: "Team 1".into(),
        user_type: UserType::Team,
        permissions: vec![],
        password_reset_required: false,
    };
    let service = ContestProblemService::new(pool.clone());
    assert!(service.list_readable(contest_id, &actor, Some("zh-CN".into())).await.is_err());

    sqlx::query("UPDATE contests SET status = 'RUNNING' WHERE id = $1")
        .bind(contest_id)
        .execute(&pool)
        .await
        .expect("start contest");
    let problems = service
        .list_readable(contest_id, &actor, Some("zh-CN".into()))
        .await
        .expect("rostered team can list started contest problems");
    assert_eq!(problems.len(), 1);
    let statement = problems[0].statement.as_ref().expect("preferred statement");
    assert_eq!(statement.lang_code, "zh-CN");
    assert!(statement.rendered_html.contains("求和"));
    assert!(!statement.rendered_html.contains("<script"));
}
