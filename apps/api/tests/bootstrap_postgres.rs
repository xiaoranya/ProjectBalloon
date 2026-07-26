use project_balloon_api::bootstrap::{BootstrapAdmin, BootstrapError, bootstrap_super_admin};
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn bootstrap_is_atomic_and_one_time(pool: PgPool) {
    let user_id = bootstrap_super_admin(
        &pool,
        BootstrapAdmin::new(
            "admin".to_owned(),
            "Platform Administrator".to_owned(),
            "integration-test-password".to_owned(),
        )
        .expect("valid bootstrap input"),
    )
    .await
    .expect("fresh database must bootstrap");

    let row = sqlx::query_as::<_, (String, String, bool, bool, String)>(
        r#"
        SELECT u.username, u.user_type, u.enabled, u.password_reset_required, r.code
        FROM users u
        JOIN user_roles ur ON ur.user_id = u.id
        JOIN roles r ON r.id = ur.role_id
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("bootstrapped user must be readable");
    assert_eq!(row.0, "admin");
    assert_eq!(row.1, "SUPER_ADMIN");
    assert!(row.2);
    assert!(row.3);
    assert_eq!(row.4, "SUPER_ADMIN");

    let audit_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM audit_logs WHERE action = 'SUPER_ADMIN_BOOTSTRAPPED'",
    )
    .fetch_one(&pool)
    .await
    .expect("bootstrap audit row must be readable");
    assert_eq!(audit_count, 1);

    let second = bootstrap_super_admin(
        &pool,
        BootstrapAdmin::new(
            "another-admin".to_owned(),
            "Another Administrator".to_owned(),
            "another-integration-password".to_owned(),
        )
        .expect("valid second input"),
    )
    .await;
    assert!(matches!(second, Err(BootstrapError::AlreadyInitialized)));
}
