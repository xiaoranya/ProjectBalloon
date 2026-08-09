use sqlx::PgPool;

use super::{
    BankProblem, BankProblemRow, BankQuery, ProgressRequest, SetItemRequest, SetRequest,
    load_public_training_items, load_public_training_set, load_public_training_sets, validate_page,
    validate_progress_request, validate_set_request, write_set,
};

#[test]
fn training_queries_and_sets_reject_invalid_bounds_and_duplicate_items() {
    assert_eq!(
        validate_page(&BankQuery { page: 2, size: 25, tag: None, difficulty: None })
            .expect("valid page"),
        (25, 50)
    );
    assert!(validate_page(&BankQuery { page: 2, size: 0, tag: None, difficulty: None }).is_err());
    assert!(validate_page(&BankQuery { page: 2, size: 101, tag: None, difficulty: None }).is_err());

    let mut valid = SetRequest {
        slug: "graphs-101".into(),
        title: "Graphs".into(),
        description: String::new(),
        visibility: String::new(),
        items: vec![SetItemRequest { problem_id: 7, required: true }],
    };
    assert_eq!(validate_set_request(&valid).expect("default visibility"), "DRAFT");
    valid.visibility = "PUBLIC".into();
    assert_eq!(validate_set_request(&valid).expect("public set"), "PUBLIC");

    valid.items.push(SetItemRequest { problem_id: 7, required: false });
    assert!(validate_set_request(&valid).is_err());
    valid.items[1].problem_id = 0;
    assert!(validate_set_request(&valid).is_err());
}

#[test]
fn client_training_progress_cannot_claim_a_solution_or_score() {
    let valid = ProgressRequest { problem_id: 7, status: "IN_PROGRESS".into(), score: 0 };
    assert!(validate_progress_request(&valid).is_ok());
    assert!(
        validate_progress_request(&ProgressRequest { status: "SOLVED".into(), ..valid }).is_err()
    );
    assert!(
        validate_progress_request(&ProgressRequest {
            problem_id: 7,
            status: "IN_PROGRESS".into(),
            score: 100,
        })
        .is_err()
    );
    assert!(
        validate_progress_request(&ProgressRequest {
            problem_id: 0,
            status: "IN_PROGRESS".into(),
            score: 0,
        })
        .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn public_training_sets_only_expose_active_public_problems(pool: PgPool) {
    let user_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type, enabled, password_reset_required) VALUES ('training-admin', 'test-hash', 'Training Admin', 'SUPER_ADMIN', true, false) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert training admin");
    let public_problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('training-public', 'Public') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert public problem");
    let private_problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('training-private', 'Private') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert private problem");
    let deleted_problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('training-deleted', 'Deleted') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert deleted problem");
    sqlx::query(
        "INSERT INTO problem_bank_entries (problem_id, visibility, tags, published_at) VALUES ($1, 'PUBLIC', '[]', now()), ($2, 'PUBLIC', '[]', now()), ($3, 'PUBLIC', '[]', now())",
    )
    .bind(public_problem_id)
    .bind(private_problem_id)
    .bind(deleted_problem_id)
    .execute(&pool)
    .await
    .expect("insert problem bank entries");

    let request = SetRequest {
        slug: "mixed-training".into(),
        title: "Mixed Training".into(),
        description: String::new(),
        visibility: "PUBLIC".into(),
        items: vec![
            SetItemRequest { problem_id: public_problem_id, required: true },
            SetItemRequest { problem_id: private_problem_id, required: false },
            SetItemRequest { problem_id: deleted_problem_id, required: false },
        ],
    };
    let mut tx = pool.begin().await.expect("begin training set transaction");
    let set_id = write_set(&mut tx, None, &request, "PUBLIC", user_id)
        .await
        .expect("create initially public training set");
    tx.commit().await.expect("commit training set");

    sqlx::query("UPDATE problem_bank_entries SET visibility = 'PRIVATE' WHERE problem_id = $1")
        .bind(private_problem_id)
        .execute(&pool)
        .await
        .expect("make problem private");
    sqlx::query("UPDATE problems SET deleted_at = now() WHERE id = $1")
        .bind(deleted_problem_id)
        .execute(&pool)
        .await
        .expect("soft-delete problem");

    let sets = load_public_training_sets(&pool).await.expect("list public training sets");
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].item_count, 1);
    let summary = load_public_training_set(&pool, set_id)
        .await
        .expect("load public training set")
        .expect("public training set exists");
    assert_eq!(summary.item_count, 1);
    let items =
        load_public_training_items(&pool, set_id).await.expect("load public training items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].problem_id, public_problem_id);

    let mut tx = pool.begin().await.expect("begin invalid training set update");
    assert!(write_set(&mut tx, Some(set_id), &request, "PUBLIC", user_id).await.is_err());
    tx.rollback().await.expect("rollback invalid training set update");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn private_problem_publication_response_allows_missing_published_at(pool: PgPool) {
    let problem_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO problems (slug, title) VALUES ('training-private-response', 'Private response') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert private problem");
    sqlx::query(
        "INSERT INTO problem_bank_entries (problem_id, visibility, tags, published_at) VALUES ($1, 'PRIVATE', '[]', NULL)",
    )
    .bind(problem_id)
    .execute(&pool)
    .await
    .expect("insert private publication");

    let row = sqlx::query_as::<_, BankProblemRow>(
        "SELECT p.id,p.slug,p.title,s.body AS statement,b.difficulty,b.tags::jsonb AS tags,b.published_at,p.languages FROM problems p JOIN problem_bank_entries b ON b.problem_id=p.id LEFT JOIN problem_statements s ON s.problem_id=p.id AND s.lang_code=p.default_lang_code WHERE p.id=$1",
    )
    .bind(problem_id)
    .fetch_one(&pool)
    .await
    .expect("load private publication response");
    let row: BankProblem = row.try_into().expect("decode private publication response");

    assert_eq!(row.published_at, None);
}
