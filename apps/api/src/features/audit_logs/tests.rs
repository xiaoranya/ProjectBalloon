use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

use super::model::AuditLogQuery;
use super::service::AuditLogService;

async fn seed_log(
    pool: &PgPool,
    actor_user_id: Option<i64>,
    action: &str,
    result: &str,
    created_at: OffsetDateTime,
) -> i64 {
    sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO audit_logs(actor_user_id,action,result,created_at)
        VALUES($1,$2,$3,$4)
        RETURNING id
        "#,
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(result)
    .bind(created_at)
    .fetch_one(pool)
    .await
    .expect("insert audit log")
}

fn list_query(
    actor_user_id: Option<i64>,
    action: Option<&str>,
    result: Option<&str>,
) -> AuditLogQuery {
    AuditLogQuery {
        actor_user_id,
        action: action.map(ToOwned::to_owned),
        result: result.map(ToOwned::to_owned),
        from: None,
        to: None,
        page: 0,
        size: 25,
        sort: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn list_filters_by_actor(pool: PgPool) {
    let base = OffsetDateTime::UNIX_EPOCH;
    seed_log(&pool, Some(7), "USER_LOGIN", "SUCCESS", base).await;
    seed_log(&pool, Some(9), "USER_LOGIN", "SUCCESS", base + Duration::seconds(1)).await;
    let service = AuditLogService::new(pool);
    let page = service
        .list(list_query(Some(7), None, None).validate().expect("valid query"))
        .await
        .expect("list audit logs");
    assert_eq!(page.total_elements, 1);
    assert_eq!(page.content.len(), 1);
    assert_eq!(page.content[0].actor_user_id, Some(7));
    assert_eq!(page.content[0].action, "USER_LOGIN");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn list_filters_action_case_insensitively_and_escapes_like_metacharacters(pool: PgPool) {
    let base = OffsetDateTime::UNIX_EPOCH;
    seed_log(&pool, Some(1), "USER_LOGIN", "SUCCESS", base).await;
    seed_log(&pool, Some(1), "user_login", "SUCCESS", base + Duration::seconds(1)).await;
    // An unescaped LIKE would treat the underscore as a wildcard and match this row too.
    seed_log(&pool, Some(1), "useralogin", "SUCCESS", base + Duration::seconds(2)).await;
    seed_log(&pool, Some(1), "USER_LOGOUT", "SUCCESS", base + Duration::seconds(3)).await;
    let service = AuditLogService::new(pool);
    let page = service
        .list(list_query(None, Some("user_login"), None).validate().expect("valid query"))
        .await
        .expect("list audit logs");
    assert_eq!(page.total_elements, 2);
    assert!(page.content.iter().all(|log| log.action.eq_ignore_ascii_case("user_login")));
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn list_filters_by_result_and_created_at_range(pool: PgPool) {
    let base = OffsetDateTime::UNIX_EPOCH;
    seed_log(&pool, Some(1), "USER_LOGIN", "SUCCESS", base).await;
    seed_log(&pool, Some(1), "USER_LOGOUT", "FAILURE", base + Duration::seconds(1)).await;
    seed_log(&pool, Some(1), "BALLOON_CLAIM", "SUCCESS", base + Duration::seconds(2)).await;
    let service = AuditLogService::new(pool);
    let mut query = list_query(None, None, Some("success"));
    query.from = Some(base + Duration::seconds(1));
    query.to = Some(base + Duration::seconds(2));
    let page = service.list(query.validate().expect("valid query")).await.expect("list audit logs");
    assert_eq!(page.total_elements, 1);
    assert_eq!(page.content[0].action, "BALLOON_CLAIM");
    assert_eq!(page.content[0].result, "SUCCESS");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn list_orders_newest_first_and_paginates(pool: PgPool) {
    let base = OffsetDateTime::UNIX_EPOCH;
    let first = seed_log(&pool, Some(1), "FIRST", "SUCCESS", base).await;
    let second = seed_log(&pool, Some(1), "SECOND", "SUCCESS", base).await;
    seed_log(&pool, Some(1), "THIRD", "SUCCESS", base + Duration::seconds(1)).await;
    seed_log(&pool, Some(1), "FOURTH", "SUCCESS", base + Duration::seconds(2)).await;
    let service = AuditLogService::new(pool);
    let mut query = list_query(None, None, None);
    query.size = 2;
    let first_page =
        service.list(query.validate().expect("valid query")).await.expect("list first page");
    assert_eq!(first_page.total_elements, 4);
    assert_eq!(first_page.total_pages, 2);
    assert_eq!(first_page.page, 0);
    assert_eq!(
        first_page.content.iter().map(|log| log.action.as_str()).collect::<Vec<_>>(),
        vec!["FOURTH", "THIRD"]
    );
    let mut query = list_query(None, None, None);
    query.size = 2;
    query.page = 1;
    let second_page =
        service.list(query.validate().expect("valid query")).await.expect("list second page");
    assert_eq!(
        second_page.content.iter().map(|log| log.action.as_str()).collect::<Vec<_>>(),
        vec!["SECOND", "FIRST"]
    );
    // Identical timestamps fall back to the descending id tie-break.
    assert!(second_page.content[0].id > second_page.content[1].id);
    assert_eq!(second_page.content[1].id, first);
    assert_eq!(second_page.content[0].id, second);
}
