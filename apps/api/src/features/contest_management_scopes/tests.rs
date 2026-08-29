use std::net::{IpAddr, Ipv4Addr};

use sqlx::PgPool;

use crate::features::contest_management_scopes::ContestManagementScopeService;

fn audit_ip() -> IpAddr {
    Ipv4Addr::new(10, 0, 0, 9).into()
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn replace_updates_scopes_and_records_audit(pool: PgPool) {
    let manager_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users(username, password_hash, display_name, user_type) VALUES ('scope-manager', 'hash', 'Scope Manager', 'STAFF') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert manager");
    sqlx::query(
        "INSERT INTO user_permissions(user_id, permission_id) SELECT $1, id FROM permissions WHERE code = 'CONTEST_MANAGE'",
    )
    .bind(manager_id)
    .execute(&pool)
    .await
    .expect("grant CONTEST_MANAGE");
    let actor_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users(username, password_hash, display_name, user_type) VALUES ('scope-actor', 'hash', 'Scope Actor', 'SUPER_ADMIN') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert actor");
    let contest_a = sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests(name, status, visibility) VALUES ('Scope A', 'RUNNING', 'PRIVATE') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert contest a");
    let contest_b = sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests(name, status, visibility) VALUES ('Scope B', 'RUNNING', 'PRIVATE') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert contest b");
    let service = ContestManagementScopeService::new(pool.clone());

    let response = service
        .replace(manager_id, vec![contest_a, contest_b], actor_id, audit_ip())
        .await
        .expect("assign scopes");
    assert_eq!(response.contest_ids, vec![contest_a, contest_b]);
    let audit_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'CONTEST_MANAGEMENT_SCOPE_UPDATED' AND target_id = $1",
    )
    .bind(manager_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count audit rows");
    assert_eq!(audit_rows, 1);

    let response =
        service.replace(manager_id, vec![], actor_id, audit_ip()).await.expect("clear scopes");
    assert!(response.contest_ids.is_empty());

    let error = service
        .replace(manager_id, vec![999_999], actor_id, audit_ip())
        .await
        .expect_err("unknown contest must be rejected");
    assert_eq!(error.code(), "CONTEST_NOT_FOUND");

    let plain_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users(username, password_hash, display_name, user_type) VALUES ('scope-plain', 'hash', 'Scope Plain', 'STAFF') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert plain staff");
    let error = service
        .replace(plain_id, vec![contest_a], actor_id, audit_ip())
        .await
        .expect_err("non-manager must be rejected");
    assert_eq!(error.code(), "CONTEST_MANAGER_NOT_FOUND");

    let scopes = service.list().await.expect("list scopes");
    let manager_scope =
        scopes.iter().find(|scope| scope.user_id == manager_id).expect("manager listed");
    assert!(manager_scope.contest_ids.is_empty());
    assert!(!scopes.iter().any(|scope| scope.user_id == plain_id));
}
