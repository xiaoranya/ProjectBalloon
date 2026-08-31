mod common;

use project_balloon_api::features::contests::ContestStatus;
use project_balloon_api::features::submissions::SubmissionStatus;
use project_balloon_contracts::JudgeVerdict;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn domain_status_check_constraints_match_rust_enums(pool: PgPool) {
    let constraints: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT conname, pg_get_constraintdef(oid)
        FROM pg_constraint
        WHERE conname IN (
            'contest_status_known',
            'submission_status_known',
            'submission_verdict_known',
            'submission_status_verdict_consistent'
        )
        ORDER BY conname
        "#,
    )
    .fetch_all(&pool)
    .await
    .expect("load domain status check constraints");
    assert_eq!(constraints.len(), 4, "all domain status CHECK constraints must exist");

    let contest_definition = constraint_definition(&constraints, "contest_status_known");
    let submission_definition = constraint_definition(&constraints, "submission_status_known");
    let verdict_definition = constraint_definition(&constraints, "submission_verdict_known");

    // Forward: every Rust enum literal must be accepted by the schema,
    // otherwise a legitimate write trips the constraint.
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
    for literal in verdict_literals() {
        assert!(
            verdict_definition.contains(literal),
            "submission_verdict_known is missing the JudgeVerdict value {literal}"
        );
    }

    // Reverse: every literal in the constraints must be a Rust variant, so the
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
    for literal in constraint_literals(&verdict_definition) {
        assert!(
            verdict_literals().contains(&literal),
            "submission_verdict_known allows the non-enum value {literal}"
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
    let statuses = [Status::Pending, Status::Judging, Status::Completed];
    for status in statuses {
        match status {
            Status::Pending | Status::Judging | Status::Completed => {}
        }
    }
    statuses.into_iter().map(Status::as_str).collect()
}

fn verdict_literals() -> Vec<&'static str> {
    let verdicts = [
        JudgeVerdict::Accepted,
        JudgeVerdict::WrongAnswer,
        JudgeVerdict::CompileError,
        JudgeVerdict::RuntimeError,
        JudgeVerdict::TimeLimitExceeded,
        JudgeVerdict::MemoryLimitExceeded,
        JudgeVerdict::OutputLimitExceeded,
        JudgeVerdict::SystemError,
        JudgeVerdict::Cancelled,
    ];
    verdicts.iter().map(|verdict| verdict.as_str()).collect()
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

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn submission_status_and_verdict_stay_consistent(pool: PgPool) {
    let contest_id = common::insert_contest(&pool, "Split Status", "RUNNING", "PRIVATE").await;
    let team_id = common::insert_team(&pool, "Split Status Team").await;
    let problem_id =
        common::insert_problem(&pool, "split-status", "Split Status", "problems/split.zip", None)
            .await;
    let submission_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO submissions
            (contest_id, problem_id, team_id, language, source_object_key, source_size_bytes, status)
        VALUES ($1, $2, $3, 'cpp', 'sources/split.cpp', 1, 'PENDING')
        RETURNING id
        "#,
    )
    .bind(contest_id)
    .bind(problem_id)
    .bind(team_id)
    .fetch_one(&pool)
    .await
    .expect("insert pending submission");

    // A PENDING row must not carry a verdict.
    let state = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, verdict FROM submissions WHERE id = $1",
    )
    .bind(submission_id)
    .fetch_one(&pool)
    .await
    .expect("load pending submission");
    assert_eq!(state, ("PENDING".to_owned(), None));

    // Completing without a verdict violates the consistency CHECK.
    assert!(
        sqlx::query("UPDATE submissions SET status = 'COMPLETED' WHERE id = $1")
            .bind(submission_id)
            .execute(&pool)
            .await
            .is_err(),
        "COMPLETED without a verdict must be rejected"
    );
    // A verdict while still pending violates it the same way.
    assert!(
        sqlx::query("UPDATE submissions SET verdict = 'ACCEPTED' WHERE id = $1")
            .bind(submission_id)
            .execute(&pool)
            .await
            .is_err(),
        "a verdict on a PENDING submission must be rejected"
    );

    // The legitimate transition writes the pair together.
    sqlx::query("UPDATE submissions SET status = 'COMPLETED', verdict = 'ACCEPTED', judged_at = now() WHERE id = $1")
        .bind(submission_id)
        .execute(&pool)
        .await
        .expect("complete submission with a verdict");
    // Unknown verdict values stay rejected by submission_verdict_known.
    assert!(
        sqlx::query("UPDATE submissions SET verdict = 'NOT_A_VERDICT' WHERE id = $1")
            .bind(submission_id)
            .execute(&pool)
            .await
            .is_err(),
        "unknown verdict literals must be rejected"
    );
}
