use sqlx::PgPool;

use crate::features::auth::model::UserType;
use crate::features::staff_accounts::StaffAccountService;
use crate::features::staff_accounts::model::ValidatedCreate;

fn create_request(username: &str) -> ValidatedCreate {
    ValidatedCreate {
        username: username.to_owned(),
        display_name: "Created Staff".to_owned(),
        user_type: UserType::Staff,
        permissions: vec![],
        initial_password: "Initial-Passw0rd!".to_owned(),
        require_password_reset: true,
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn create_then_reset_password_revokes_sessions(pool: PgPool) {
    let actor_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users(username, password_hash, display_name, user_type) VALUES ('staff-test-actor', 'hash', 'Staff Actor', 'SUPER_ADMIN') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert actor");
    let service = StaffAccountService::new(pool.clone());

    let created = service
        .create(create_request("new.staff"), actor_id, "10.0.0.9".parse().expect("ip"))
        .await
        .expect("create staff account");
    assert_eq!(created.username, "new.staff");
    assert!(created.enabled);

    let original_hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .expect("load hash");
    sqlx::query(
        "INSERT INTO auth_sessions(token_hash, user_id, access_fingerprint, expires_at) VALUES ($1, $2, $3, now() + interval '1 hour')",
    )
    .bind("a".repeat(64))
    .bind(created.id)
    .bind("b".repeat(64))
    .execute(&pool)
    .await
    .expect("insert session");

    let reset = service
        .reset_password(
            created.id,
            "Fresh-Passw0rd!".to_owned(),
            false,
            actor_id,
            "10.0.0.9".parse().expect("ip"),
        )
        .await
        .expect("reset password");
    assert_eq!(reset.id, created.id);

    let updated_hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .expect("load updated hash");
    assert_ne!(updated_hash, original_hash);
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_sessions WHERE user_id = $1")
        .bind(created.id)
        .fetch_one(&pool)
        .await
        .expect("count sessions");
    assert_eq!(sessions, 0);
    let reset_required: bool =
        sqlx::query_scalar("SELECT password_reset_required FROM users WHERE id = $1")
            .bind(created.id)
            .fetch_one(&pool)
            .await
            .expect("load reset flag");
    assert!(!reset_required);

    let duplicate = service
        .create(create_request("new.staff"), actor_id, "10.0.0.9".parse().expect("ip"))
        .await
        .expect_err("duplicate username must conflict");
    assert_eq!(duplicate.code(), "USERNAME_TAKEN");
}
