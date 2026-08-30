use project_balloon_api::features::contests::ContestStatus;
use project_balloon_api::features::submissions::SubmissionStatus;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn domain_status_check_constraints_match_rust_enums(pool: PgPool) {
    let constraints: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT conname, pg_get_constraintdef(oid)
        FROM pg_constraint
        WHERE conname IN ('contest_status_known', 'submission_status_known')
        ORDER BY conname
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("load domain status check constraints");
    assert_eq!(constraints.len(), 2, "both domain status CHECK constraints must exist");

    let contest_definition = constraint_definition(&constraints, "contest_status_known");
    let submission_definition = constraint_definition(&constraints, "submission_status_known");

    // Forward: every Rust enum literal (including CANCELLED) must be accepted
    // by the schema, otherwise a legitimate write trips the constraint.
    for literal in contest_status_literals() {
        assert!(
            contest_definition.contains(literal),
            "contest_status_known is missing the Rust variant {literal}"
        );
    }
    for literal in submission_status_literals() {
        assert!(
            submission_definition.contains(literal),
            "submission_status_known is missing the Rust variant {literal}"
        );
    }

    // Reverse: every literal in the constraint must be a Rust variant, so the
    // schema cannot accumulate zombie values the application never writes.
    for literal in constraint_literals(&contest_definition) {
        assert!(
            contest_status_literals().contains(&literal),
            "contest_status_known allows the non-enum value {literal}"
        );
    }
    for literal in constraint_literals(&submission_definition) {
        assert!(
            submission_status_literals().contains(&literal),
            "submission_status_known allows the non-enum value {literal}"
        );
    }
}

fn contest_status_literals() -> Vec<&'static str> {
    use ContestStatus as Status;
    let statuses = [
        Status::Draft,
        Status::FrozenConfig,
        Status::Running,
        Status::Paused,
        Status::Ended,
        Status::Archived,
    ];
    for status in statuses {
        // No wildcard arm: a new variant fails to compile, which is the
        // signal to extend this list and the migration CHECK together.
        match status {
            Status::Draft
            | Status::FrozenConfig
            | Status::Running
            | Status::Paused
            | Status::Ended
            | Status::Archived => {}
        }
    }
    statuses.into_iter().map(Status::as_str).collect()
}

fn submission_status_literals() -> Vec<&'static str> {
    use SubmissionStatus as Status;
    let statuses = [
        Status::Pending,
        Status::Judging,
        Status::Cancelled,
        Status::Accepted,
        Status::WrongAnswer,
        Status::CompileError,
        Status::RuntimeError,
        Status::TimeLimitExceeded,
        Status::MemoryLimitExceeded,
        Status::OutputLimitExceeded,
        Status::SystemError,
    ];
    for status in statuses {
        match status {
            Status::Pending
            | Status::Judging
            | Status::Cancelled
            | Status::Accepted
            | Status::WrongAnswer
            | Status::CompileError
            | Status::RuntimeError
            | Status::TimeLimitExceeded
            | Status::MemoryLimitExceeded
            | Status::OutputLimitExceeded
            | Status::SystemError => {}
        }
    }
    statuses.into_iter().map(Status::as_str).collect()
}

fn constraint_definition(constraints: &[(String, String)], name: &str) -> String {
    constraints
        .iter()
        .find(|(constraint_name, _)| constraint_name == name)
        .map(|(_, definition)| definition.clone())
        .expect("constraint should exist")
}

/// Extracts the single-quoted literals from a `pg_get_constraintdef` output;
/// every quoted segment in these definitions is a status value.
fn constraint_literals(definition: &str) -> Vec<&str> {
    definition
        .split('\'')
        .enumerate()
        .filter(|(index, _)| index % 2 == 1)
        .map(|(_, literal)| literal)
        .collect()
}
