use std::net::{IpAddr, Ipv4Addr};

use sqlx::PgPool;

use crate::features::auth::model::{AuthUser, UserType};
use crate::features::auth::permissions;
use crate::features::teams::model::{
    ParticipationType, ValidatedBatchImport, ValidatedContestTeamAssignment, ValidatedCreateTeam,
    ValidatedTeamAccount,
};
use crate::features::teams::service::TeamService;

const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

async fn seed_actor(
    pool: &PgPool,
    username: &str,
    user_type: UserType,
    permission_codes: &[&str],
) -> AuthUser {
    let user_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO users (username, password_hash, display_name, user_type)
         VALUES ($1, 'test-hash', $1, $2) RETURNING id",
    )
    .bind(username)
    .bind(user_type.as_str())
    .fetch_one(pool)
    .await
    .expect("insert actor");
    for code in permission_codes {
        sqlx::query(
            "INSERT INTO user_permissions (user_id, permission_id)
             SELECT $1, id FROM permissions WHERE code = $2",
        )
        .bind(user_id)
        .bind(code)
        .execute(pool)
        .await
        .expect("grant permission");
    }
    AuthUser {
        id: user_id,
        username: username.to_owned(),
        display_name: username.to_owned(),
        user_type,
        permissions: permission_codes.iter().map(|code| (*code).to_owned()).collect(),
        password_reset_required: false,
    }
}

async fn seed_contest(pool: &PgPool, name: &str, status: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO contests (name, status, visibility, start_at, end_at)
         VALUES ($1, $2, 'PRIVATE', now() - interval '1 hour', now() + interval '1 hour')
         RETURNING id",
    )
    .bind(name)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("insert contest")
}

async fn assign_manager(pool: &PgPool, user_id: i64, contest_id: i64) {
    sqlx::query(
        "INSERT INTO contest_management_assignments (user_id, contest_id, assigned_by_user_id)
         VALUES ($1, $2, $1)",
    )
    .bind(user_id)
    .bind(contest_id)
    .execute(pool)
    .await
    .expect("assign contest manager");
}

fn import_request(
    contest_id: Option<i64>,
    idempotency_key: &str,
    request_hash: &str,
) -> ValidatedBatchImport {
    ValidatedBatchImport {
        teams: vec![
            ValidatedCreateTeam {
                name: "Imported Wolves".into(),
                school: Some("Wolf School".into()),
                seat_no: None,
                group_name: None,
                star: false,
                account: Some(ValidatedTeamAccount {
                    username: "wolves-login".into(),
                    initial_password: "Wolves-2026!".into(),
                }),
                require_password_reset: true,
            },
            ValidatedCreateTeam {
                name: "Imported Foxes".into(),
                school: None,
                seat_no: Some("A-17".into()),
                group_name: None,
                star: true,
                account: Some(ValidatedTeamAccount {
                    username: "foxes-login".into(),
                    initial_password: "Foxes-2026!".into(),
                }),
                require_password_reset: false,
            },
        ],
        contest_id,
        participation_type: ParticipationType::Official,
        require_password_reset: true,
        idempotency_key: idempotency_key.to_owned(),
        request_hash: request_hash.to_owned(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn batch_import_creates_rosters_and_replays_idempotently(pool: PgPool) {
    let admin = seed_actor(&pool, "import-root", UserType::SuperAdmin, &[]).await;
    let contest_id = seed_contest(&pool, "Import Contest", "RUNNING").await;
    let service = TeamService::new(pool.clone());

    let response = service
        .batch_import(import_request(Some(contest_id), "import-key-1", "hash-1"), &admin, LOCALHOST)
        .await
        .expect("batch import succeeds");
    assert_eq!(response.total_requested, 2);
    assert_eq!(response.created.len(), 2);
    assert!(response.created.iter().all(|row| row.user_id.is_some()));

    // Star teams always carry STAR participation, regardless of the request.
    let participation: Vec<(String, String)> = sqlx::query_as(
        "SELECT t.name, ct.participation_type FROM contest_teams ct
         JOIN teams t ON t.id = ct.team_id ORDER BY t.name",
    )
    .fetch_all(&pool)
    .await
    .expect("load contest roster");
    assert_eq!(
        participation,
        vec![
            ("Imported Foxes".into(), "STAR".into()),
            ("Imported Wolves".into(), "OFFICIAL".into()),
        ]
    );

    // Team accounts are usable logins with the requested password-reset flag.
    let account_flags: Vec<(String, bool)> = sqlx::query_as(
        "SELECT u.username, u.password_reset_required FROM users u
         JOIN team_accounts ta ON ta.user_id = u.id ORDER BY u.username",
    )
    .fetch_all(&pool)
    .await
    .expect("load team accounts");
    assert_eq!(account_flags, vec![("foxes-login".into(), false), ("wolves-login".into(), true)]);

    // Per-team and per-staff realtime events were queued, plus one audit row.
    let events =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM realtime_outbox WHERE contest_id = $1")
            .bind(contest_id)
            .fetch_one(&pool)
            .await
            .expect("count realtime events");
    assert_eq!(events, 3);
    let audits = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM audit_logs WHERE action = 'TEAM_BATCH_IMPORTED'",
    )
    .fetch_one(&pool)
    .await
    .expect("count import audits");
    assert_eq!(audits, 1);

    // Replaying the exact request returns the stored response and adds nothing.
    let replay = service
        .batch_import(import_request(Some(contest_id), "import-key-1", "hash-1"), &admin, LOCALHOST)
        .await
        .expect("idempotent replay");
    assert_eq!(replay.batch_id, response.batch_id);
    let team_count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM teams")
        .fetch_one(&pool)
        .await
        .expect("count teams after replay");
    assert_eq!(team_count, 2, "replay must not create teams");

    // The same key with a different payload is rejected as a conflict.
    let clash = service
        .batch_import(import_request(Some(contest_id), "import-key-1", "hash-2"), &admin, LOCALHOST)
        .await
        .expect_err("conflicting hash must fail");
    assert_eq!(clash.code(), "TEAM_IMPORT_IDEMPOTENCY_CONFLICT");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn batch_import_scopes_imports_to_the_callers_authority(pool: PgPool) {
    let manager =
        seed_actor(&pool, "import-manager", UserType::Staff, &[permissions::CONTEST_MANAGE]).await;
    let bare_staff = seed_actor(&pool, "import-clerk", UserType::Staff, &[]).await;
    let service = TeamService::new(pool.clone());

    let missing_contest = service
        .batch_import(import_request(None, "mgr-key-1", "hash-1"), &manager, LOCALHOST)
        .await
        .expect_err("contest managers must name a managed contest");
    assert_eq!(missing_contest.code(), "CONTEST_REQUIRED");

    let forbidden = service
        .batch_import(import_request(None, "clerk-key-1", "hash-1"), &bare_staff, LOCALHOST)
        .await
        .expect_err("staff without the permission cannot import globally");
    assert_eq!(forbidden.code(), "FORBIDDEN");

    let contest_id = seed_contest(&pool, "Managed Contest", "RUNNING").await;
    let unmanaged = service
        .batch_import(import_request(Some(contest_id), "mgr-key-2", "hash-1"), &manager, LOCALHOST)
        .await
        .expect_err("unmanaged contests must stay invisible to the manager");
    assert_eq!(unmanaged.code(), "CONTEST_NOT_FOUND");

    assign_manager(&pool, manager.id, contest_id).await;
    service
        .batch_import(import_request(Some(contest_id), "mgr-key-3", "hash-1"), &manager, LOCALHOST)
        .await
        .expect("assigned manager may import into the contest");
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires a PostgreSQL server named by DATABASE_URL"]
async fn delete_guard_blocks_assigned_teams_then_retires_the_account(pool: PgPool) {
    let admin = seed_actor(&pool, "delete-root", UserType::SuperAdmin, &[]).await;
    let contest_id = seed_contest(&pool, "Roster Contest", "RUNNING").await;
    let service = TeamService::new(pool.clone());
    let team = service
        .create(
            ValidatedCreateTeam {
                name: "Retired Foxes".into(),
                school: None,
                seat_no: None,
                group_name: None,
                star: false,
                account: Some(ValidatedTeamAccount {
                    username: "retired-login".into(),
                    initial_password: "Retired-2026!".into(),
                }),
                require_password_reset: false,
            },
            admin.id,
            LOCALHOST,
        )
        .await
        .expect("create team");

    // An OPEN/RUNNING contest accepts roster changes.
    service
        .assign_to_contest(
            contest_id,
            ValidatedContestTeamAssignment {
                team_id: team.id,
                participation_type: ParticipationType::Practice,
                group_name: None,
            },
            &admin,
            LOCALHOST,
        )
        .await
        .expect("assign team to contest");

    let blocked = service.delete(team.id, &admin, LOCALHOST).await.expect_err("assigned team");
    assert_eq!(blocked.code(), "TEAM_IN_USE");

    service
        .remove_from_contest(contest_id, team.id, &admin, LOCALHOST)
        .await
        .expect("remove from contest");
    service.delete(team.id, &admin, LOCALHOST).await.expect("delete unassigned team");

    let row = sqlx::query_as::<_, (bool, bool)>(
        "SELECT t.deleted_at IS NOT NULL, u.enabled FROM teams t
         JOIN team_accounts ta ON ta.team_id = t.id
         JOIN users u ON u.id = ta.user_id
         WHERE t.id = $1",
    )
    .bind(team.id)
    .fetch_one(&pool)
    .await
    .expect("load deleted team");
    assert_eq!(row, (true, false), "team is soft deleted and its login is disabled");
}
