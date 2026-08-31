mod common;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use project_balloon_api::error::AppError;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn archived_contest_child_write_maps_to_read_only_conflict(pool: PgPool) {
    let contest_id = common::insert_contest(&pool, "archived-readonly", "ARCHIVED", "PUBLIC").await;
    let user_id =
        common::insert_user(&pool, "archived-readonly", "Archived Readonly", "INDIVIDUAL").await;

    let rejection = sqlx::query(
        "INSERT INTO announcements(contest_id, title, body, created_by) VALUES($1, 't', 'b', $2)",
    )
    .bind(contest_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .expect_err("archived contest child write must be rejected");

    let response = AppError::internal("create announcement", rejection).into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.expect("body").to_bytes())
            .expect("json body");
    assert_eq!(body["code"], "CONTEST_ARCHIVED_READ_ONLY");
    assert_eq!(body["message"], "Archived contest data is read-only");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn unrelated_database_error_stays_internal(pool: PgPool) {
    sqlx::query(
        "INSERT INTO users(username, password_hash, display_name, user_type) VALUES('duplicate-user', 'hash', 'Duplicate User', 'INDIVIDUAL')",
    )
    .execute(&pool)
    .await
    .expect("insert user");

    let duplicate = sqlx::query(
        "INSERT INTO users(username, password_hash, display_name, user_type) VALUES('duplicate-user', 'hash', 'Duplicate User', 'INDIVIDUAL')",
    )
    .execute(&pool)
    .await
    .expect_err("duplicate username must be rejected");

    let response = AppError::internal("create user", duplicate).into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.expect("body").to_bytes())
            .expect("json body");
    assert_eq!(body["code"], "INTERNAL_ERROR");
}
