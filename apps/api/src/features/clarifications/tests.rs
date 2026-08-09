use std::net::{IpAddr, Ipv4Addr};

use sqlx::PgPool;

use super::model::{AskRequest, ConvertRequest, ReplyRequest};
use super::service::ClarificationService;
use crate::features::auth::model::{AuthUser, UserType};

#[test]
fn scope_problem_shape_and_reply_visibility_are_closed() {
    assert!(
        AskRequest { scope: "GENERAL".into(), problem_id: None, question: "Question".into() }
            .validate()
            .is_ok()
    );
    assert!(
        AskRequest { scope: "GENERAL".into(), problem_id: Some(1), question: "Question".into() }
            .validate()
            .is_err()
    );
    assert!(
        AskRequest { scope: "PROBLEM".into(), problem_id: None, question: "Question".into() }
            .validate()
            .is_err()
    );
    assert!(
        ReplyRequest { reply: "Answer".into(), visibility: "PUBLIC".into() }.validate().is_ok()
    );
    assert!(
        ReplyRequest { reply: "Answer".into(), visibility: "GLOBAL".into() }.validate().is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn private_workflow_is_rate_limited_scoped_and_transactional(pool: PgPool) {
    let admin_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('clar-root', 'test-hash', 'Clar Root', 'SUPER_ADMIN') RETURNING id",
    )
    .fetch_one(&pool).await.expect("insert clarification administrator");
    let team_user_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('clar-team', 'test-hash', 'Clar Team', 'TEAM') RETURNING id",
    )
    .fetch_one(&pool).await.expect("insert clarification team account");
    let other_user_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type) VALUES ('clar-other', 'test-hash', 'Other Team', 'TEAM') RETURNING id",
    )
    .fetch_one(&pool).await.expect("insert other team account");
    let team_id =
        sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ('Clar Team') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert clarification team");
    let other_team_id =
        sqlx::query_scalar::<_, i64>("INSERT INTO teams (name) VALUES ('Other Team') RETURNING id")
            .fetch_one(&pool)
            .await
            .expect("insert other team");
    for (user_id, linked_team_id) in [(team_user_id, team_id), (other_user_id, other_team_id)] {
        sqlx::query("INSERT INTO team_accounts (user_id, team_id) VALUES ($1, $2)")
            .bind(user_id)
            .bind(linked_team_id)
            .execute(&pool)
            .await
            .expect("link clarification team account");
    }
    let contest_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO contests (name, status, visibility, start_at, end_at)
        VALUES ('Clarification Contest', 'RUNNING', 'PRIVATE',
                now() - interval '1 hour', now() + interval '1 hour') RETURNING id
    "#,
    )
    .fetch_one(&pool)
    .await
    .expect("insert clarification contest");
    for linked_team_id in [team_id, other_team_id] {
        sqlx::query("INSERT INTO contest_teams (contest_id, team_id, participation_type) VALUES ($1, $2, 'OFFICIAL')")
            .bind(contest_id).bind(linked_team_id).execute(&pool).await
            .expect("roster clarification team");
    }
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('clar-a', 'Clar A') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert clarification problem");
    sqlx::query("INSERT INTO contest_problems (contest_id, problem_id, alias, display_order) VALUES ($1, $2, 'A', 1)")
        .bind(contest_id).bind(problem_id).execute(&pool).await
        .expect("assign clarification problem");
    let team = AuthUser {
        id: team_user_id,
        username: "clar-team".into(),
        display_name: "Clar Team".into(),
        user_type: UserType::Team,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let other = AuthUser {
        id: other_user_id,
        username: "clar-other".into(),
        display_name: "Other Team".into(),
        user_type: UserType::Team,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let admin = AuthUser {
        id: admin_id,
        username: "clar-root".into(),
        display_name: "Clar Root".into(),
        user_type: UserType::SuperAdmin,
        permissions: Vec::new(),
        password_reset_required: false,
    };
    let service = ClarificationService::new(pool.clone());
    let asked = service
        .ask(
            contest_id,
            AskRequest {
                scope: "PROBLEM".into(),
                problem_id: Some(problem_id),
                question: "Is input sorted?".into(),
            }
            .validate()
            .expect("valid clarification"),
            &team,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("ask clarification");
    assert_eq!(asked.problem_alias.as_deref(), Some("A"));
    assert_eq!(service.list_mine(contest_id, &team).await.expect("list mine").len(), 1);
    assert!(service.list_mine(contest_id, &other).await.expect("list other").is_empty());
    assert!(
        service
            .ask(
                contest_id,
                AskRequest { scope: "GENERAL".into(), problem_id: None, question: "Second".into() }
                    .validate()
                    .expect("valid second clarification"),
                &team,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .is_err()
    );
    let replied = service
        .reply(
            asked.id,
            ReplyRequest { reply: "No.".into(), visibility: "PRIVATE".into() }
                .validate()
                .expect("valid private reply"),
            &admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("reply privately");
    assert_eq!(replied.reply_visibility.as_deref(), Some("PRIVATE"));
    service
        .close(asked.id, &admin, IpAddr::V4(Ipv4Addr::LOCALHOST))
        .await
        .expect("close clarification");
    assert!(
        service
            .reply(
                asked.id,
                ReplyRequest { reply: "Changed".into(), visibility: "PUBLIC".into() }
                    .validate()
                    .expect("valid later reply"),
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .is_err()
    );
    let team_recipients = sqlx::query_scalar::<_, Option<i64>>(
        r#"
        SELECT team_id
        FROM realtime_outbox
        WHERE contest_id = $1 AND event_type = 'CLARIFICATION_UPDATED' AND scope = 'TEAM'
        ORDER BY created_at
    "#,
    )
    .bind(contest_id)
    .fetch_all(&pool)
    .await
    .expect("load team recipients");
    assert_eq!(team_recipients, vec![Some(team_id), Some(team_id), Some(team_id)]);
    let audit_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM audit_logs WHERE target_type = 'CLARIFICATION' AND target_id = $1",
    )
    .bind(asked.id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count clarification audits");
    assert_eq!(audit_count, 3);

    let public_question = service
        .ask(
            contest_id,
            AskRequest {
                scope: "GENERAL".into(),
                problem_id: None,
                question: "What is the rule?".into(),
            }
            .validate()
            .expect("valid public clarification"),
            &other,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("ask public clarification");
    service
        .reply(
            public_question.id,
            ReplyRequest { reply: "The public answer.".into(), visibility: "PUBLIC".into() }
                .validate()
                .expect("valid public reply"),
            &admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("reply publicly");
    let announcement = service
        .convert(
            public_question.id,
            ConvertRequest { title: None, body: None },
            &admin,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .await
        .expect("convert public clarification");
    assert_eq!(announcement.source_clarification_id, Some(public_question.id));
    assert_eq!(announcement.body, "The public answer.");
    assert!(
        service
            .convert(
                public_question.id,
                ConvertRequest { title: None, body: None },
                &admin,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
            )
            .await
            .is_err()
    );
    let linked = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT converted_announcement_id FROM clarifications WHERE id = $1",
    )
    .bind(public_question.id)
    .fetch_one(&pool)
    .await
    .expect("load converted link");
    assert_eq!(linked, Some(announcement.id));
}
