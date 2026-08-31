use std::net::IpAddr;

use getrandom::fill;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use subtle::ConstantTimeEq;
use time::OffsetDateTime;

use crate::{config::DeploymentMode, error::AppError, features::auth::model::AuthUser};

use crate::features::competition::model::{
    ActiveContestResponse, BindWorkstationRequest, CompetitionSessionResponse,
    CreateWorkstationRequest, DeploymentInfoResponse, UpdateWorkstationRequest,
    WorkstationBindingResponse, WorkstationLoginGrant, WorkstationResponse,
};

const PAIRING_CODE_BYTES: usize = 8;
const PAIRING_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Clone)]
pub struct CompetitionService {
    database: PgPool,
}

impl CompetitionService {
    #[must_use]
    pub const fn new(database: PgPool) -> Self {
        Self { database }
    }

    pub async fn deployment_info(
        &self,
        mode: DeploymentMode,
    ) -> Result<DeploymentInfoResponse, AppError> {
        let active_contest =
            if mode.is_competition() { self.active_contest().await? } else { None };
        Ok(DeploymentInfoResponse { mode: mode.as_str(), active_contest })
    }

    pub async fn validate_schedule_integrity(&self) -> Result<(), AppError> {
        let overlap = sqlx::query_as::<_, (i64, String, i64, String)>(
            r#"
            SELECT first.id, first.name, second.id, second.name
            FROM contests first
            JOIN contests second
              ON first.id < second.id
             AND first.deleted_at IS NULL
             AND second.deleted_at IS NULL
             AND first.start_at IS NOT NULL AND first.end_at IS NOT NULL
             AND second.start_at IS NOT NULL AND second.end_at IS NOT NULL
             AND first.start_at < second.end_at
             AND second.start_at < first.end_at
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("validate competition schedules", error))?;
        if let Some((first_id, first_name, second_id, second_name)) = overlap {
            tracing::error!(
                first_id,
                first_name,
                second_id,
                second_name,
                "competition schedules overlap"
            );
            return Err(AppError::conflict(
                "COMPETITION_SCHEDULE_OVERLAP",
                "Contest schedules must not overlap in competition mode",
            ));
        }
        Ok(())
    }

    pub async fn active_contest(&self) -> Result<Option<ActiveContestResponse>, AppError> {
        let contests = sqlx::query_as::<_, (i64, String, OffsetDateTime, OffsetDateTime)>(
            r#"
            SELECT id, name, start_at, end_at
            FROM contests
            WHERE deleted_at IS NULL
              AND status IN ('RUNNING', 'PAUSED')
              AND start_at <= now() AND end_at > now()
            ORDER BY start_at, id
            LIMIT 2
            "#,
        )
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("load active competition contest", error))?;
        if contests.len() > 1 {
            return Err(AppError::conflict(
                "COMPETITION_SCHEDULE_OVERLAP",
                "More than one contest is active",
            ));
        }
        Ok(contests.into_iter().next().map(|(id, name, start_at, end_at)| ActiveContestResponse {
            id,
            name,
            start_at,
            end_at,
        }))
    }

    pub async fn login_grant(
        &self,
        mode: DeploymentMode,
        ip: IpAddr,
        pairing_code: &str,
    ) -> Result<WorkstationLoginGrant, AppError> {
        if !mode.is_competition() {
            return Err(AppError::not_found(
                "COMPETITION_MODE_DISABLED",
                "Competition workstation login is disabled",
            ));
        }
        let contest = self.active_contest().await?.ok_or_else(|| {
            AppError::conflict("NO_ACTIVE_CONTEST", "No contest is currently active")
        })?;
        let ip = ip.to_string();
        let supplied_hash = digest(pairing_code);
        let row = sqlx::query_as::<_, (i64, i64, i64, String, String)>(
            r#"
            SELECT binding.id, account.user_id, workstation.id, workstation.seat_no,
                   binding.pairing_code_hash
            FROM competition_workstations workstation
            JOIN contest_workstation_bindings binding
              ON binding.workstation_id = workstation.id
             AND binding.contest_id = $2
             AND binding.revoked_at IS NULL
            JOIN team_accounts account ON account.team_id = binding.team_id
            JOIN users user_account
              ON user_account.id = account.user_id
             AND user_account.enabled = true
             AND user_account.user_type = 'TEAM'
            WHERE workstation.ip_address = $1 AND workstation.enabled = true
            "#,
        )
        .bind(&ip)
        .bind(contest.id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("load workstation pairing", error))?
        .ok_or_else(|| {
            AppError::unauthorized(
                "WORKSTATION_NOT_BOUND",
                "This IP address is not bound for the active contest",
            )
        })?;
        if !constant_time_equal(&row.4, &supplied_hash) {
            return Err(AppError::unauthorized("PAIRING_CODE_INVALID", "Pairing code is invalid"));
        }
        sqlx::query(
            "UPDATE competition_workstations SET last_seen_at=now(),updated_at=now() WHERE id=$1",
        )
        .bind(row.2)
        .execute(&self.database)
        .await
        .map_err(|error| AppError::internal("update workstation presence", error))?;
        Ok(WorkstationLoginGrant {
            binding_id: row.0,
            user_id: row.1,
            bound_ip: ip,
            competition: CompetitionSessionResponse {
                contest_id: contest.id,
                contest_name: contest.name,
                workstation_id: row.2,
                seat_no: row.3,
            },
            expires_at: contest.end_at,
        })
    }

    pub async fn validate_session(
        &self,
        mode: DeploymentMode,
        binding_id: i64,
        user_id: i64,
        request_ip: IpAddr,
    ) -> Result<CompetitionSessionResponse, AppError> {
        if !mode.is_competition() {
            return Err(not_authenticated());
        }
        let row = sqlx::query_as::<_, (i64, String, i64, String, String)>(
            r#"
            SELECT contest.id, contest.name, workstation.id, workstation.seat_no,
                   workstation.ip_address
            FROM contest_workstation_bindings binding
            JOIN competition_workstations workstation
              ON workstation.id = binding.workstation_id AND workstation.enabled = true
            JOIN contests contest
              ON contest.id = binding.contest_id
             AND contest.deleted_at IS NULL
             AND contest.status IN ('RUNNING', 'PAUSED')
             AND contest.start_at <= now() AND contest.end_at > now()
            JOIN team_accounts account
              ON account.team_id = binding.team_id AND account.user_id = $2
            WHERE binding.id = $1 AND binding.revoked_at IS NULL
            "#,
        )
        .bind(binding_id)
        .bind(user_id)
        .fetch_optional(&self.database)
        .await
        .map_err(|error| AppError::internal("validate workstation session", error))?
        .ok_or_else(not_authenticated)?;
        if row.4 != request_ip.to_string() {
            return Err(not_authenticated());
        }
        Ok(CompetitionSessionResponse {
            contest_id: row.0,
            contest_name: row.1,
            workstation_id: row.2,
            seat_no: row.3,
        })
    }

    pub async fn list_workstations(&self) -> Result<Vec<WorkstationResponse>, AppError> {
        sqlx::query_as(
            "SELECT id,ip_address,seat_no,label,enabled,last_seen_at,version,created_at,updated_at FROM competition_workstations ORDER BY seat_no,id",
        )
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list competition workstations", error))
    }

    pub async fn create_workstation(
        &self,
        request: CreateWorkstationRequest,
    ) -> Result<WorkstationResponse, AppError> {
        let (ip, seat, label) =
            validate_workstation_input(request.ip_address, request.seat_no, request.label)?;
        sqlx::query_as(
            "INSERT INTO competition_workstations(ip_address,seat_no,label) VALUES($1,$2,$3) RETURNING id,ip_address,seat_no,label,enabled,last_seen_at,version,created_at,updated_at",
        )
        .bind(ip)
        .bind(seat)
        .bind(label)
        .fetch_one(&self.database)
        .await
        .map_err(map_workstation_write_error)
    }

    pub async fn update_workstation(
        &self,
        id: i64,
        request: UpdateWorkstationRequest,
    ) -> Result<WorkstationResponse, AppError> {
        let (ip, seat, label) =
            validate_workstation_input(request.ip_address, request.seat_no, request.label)?;
        sqlx::query_as(
            "UPDATE competition_workstations SET ip_address=$1,seat_no=$2,label=$3,enabled=$4,version=version+1,updated_at=now() WHERE id=$5 AND version=$6 RETURNING id,ip_address,seat_no,label,enabled,last_seen_at,version,created_at,updated_at",
        )
        .bind(ip)
        .bind(seat)
        .bind(label)
        .bind(request.enabled)
        .bind(id)
        .bind(request.expected_version)
        .fetch_optional(&self.database)
        .await
        .map_err(map_workstation_write_error)?
        .ok_or_else(|| {
            AppError::conflict(
                "WORKSTATION_UPDATE_STALE",
                "Workstation was changed; reload and retry",
            )
        })
    }

    pub async fn list_bindings(
        &self,
        contest_id: i64,
        actor: &AuthUser,
    ) -> Result<Vec<WorkstationBindingResponse>, AppError> {
        require_manage(&self.database, contest_id, actor).await?;
        let mut rows = sqlx::query_as::<_, WorkstationBindingResponse>(
            r#"
            SELECT binding.id,binding.contest_id,binding.workstation_id,workstation.ip_address,
                   workstation.seat_no,binding.team_id,team.name AS team_name,
                   NULL::text AS pairing_code,binding.bound_at,binding.revoked_at
            FROM contest_workstation_bindings binding
            JOIN competition_workstations workstation ON workstation.id=binding.workstation_id
            JOIN teams team ON team.id=binding.team_id
            WHERE binding.contest_id=$1
            ORDER BY workstation.seat_no,binding.id
            "#,
        )
        .bind(contest_id)
        .fetch_all(&self.database)
        .await
        .map_err(|error| AppError::internal("list workstation bindings", error))?;
        for row in &mut rows {
            row.pairing_code = None;
        }
        Ok(rows)
    }

    pub async fn bind(
        &self,
        contest_id: i64,
        request: BindWorkstationRequest,
        actor: &AuthUser,
    ) -> Result<WorkstationBindingResponse, AppError> {
        if request.workstation_id <= 0 || request.team_id <= 0 {
            return Err(AppError::validation("binding", "IDs must be positive"));
        }
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin workstation binding", error))?;
        require_manage_tx(&mut transaction, contest_id, actor).await?;
        let code = pairing_code()?;
        let row = sqlx::query_as::<_, WorkstationBindingResponse>(
            r#"
            INSERT INTO contest_workstation_bindings
                (contest_id,workstation_id,team_id,pairing_code_hash,bound_by_user_id)
            SELECT $1,$2,$3,$4,$5
            FROM competition_workstations workstation
            JOIN contest_teams roster ON roster.contest_id=$1 AND roster.team_id=$3
            WHERE workstation.id=$2 AND workstation.enabled=true
            RETURNING id,contest_id,workstation_id,
                (SELECT ip_address FROM competition_workstations WHERE id=workstation_id) AS ip_address,
                (SELECT seat_no FROM competition_workstations WHERE id=workstation_id) AS seat_no,
                team_id,(SELECT name FROM teams WHERE id=team_id) AS team_name,
                NULL::text AS pairing_code,bound_at,revoked_at
            "#,
        )
        .bind(contest_id)
        .bind(request.workstation_id)
        .bind(request.team_id)
        .bind(digest(&code))
        .bind(actor.id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_binding_write_error)?
        .ok_or_else(|| {
            AppError::not_found(
                "WORKSTATION_OR_TEAM_NOT_FOUND",
                "Enabled workstation or contest team was not found",
            )
        })?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit workstation binding", error))?;
        Ok(WorkstationBindingResponse { pairing_code: Some(code), ..row })
    }

    pub async fn rotate_pairing_code(
        &self,
        contest_id: i64,
        binding_id: i64,
        actor: &AuthUser,
    ) -> Result<WorkstationBindingResponse, AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin pairing code rotation", error))?;
        require_manage_tx(&mut transaction, contest_id, actor).await?;
        let code = pairing_code()?;
        let row = sqlx::query_as::<_, WorkstationBindingResponse>(
            r#"
            UPDATE contest_workstation_bindings binding
            SET pairing_code_hash=$1,updated_at=now()
            FROM competition_workstations workstation,teams team
            WHERE binding.id=$2 AND binding.contest_id=$3 AND binding.revoked_at IS NULL
              AND workstation.id=binding.workstation_id AND team.id=binding.team_id
            RETURNING binding.id,binding.contest_id,binding.workstation_id,
                workstation.ip_address,workstation.seat_no,binding.team_id,team.name AS team_name,
                NULL::text AS pairing_code,binding.bound_at,binding.revoked_at
            "#,
        )
        .bind(digest(&code))
        .bind(binding_id)
        .bind(contest_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| AppError::internal("rotate pairing code", error))?
        .ok_or_else(binding_not_found)?;
        sqlx::query("DELETE FROM auth_sessions WHERE workstation_binding_id=$1")
            .bind(binding_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("revoke rotated workstation sessions", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit pairing code rotation", error))?;
        Ok(WorkstationBindingResponse { pairing_code: Some(code), ..row })
    }

    pub async fn revoke(
        &self,
        contest_id: i64,
        binding_id: i64,
        actor: &AuthUser,
    ) -> Result<(), AppError> {
        let mut transaction = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin workstation revocation", error))?;
        require_manage_tx(&mut transaction, contest_id, actor).await?;
        let changed = sqlx::query("UPDATE contest_workstation_bindings SET revoked_at=now(),updated_at=now() WHERE id=$1 AND contest_id=$2 AND revoked_at IS NULL")
            .bind(binding_id).bind(contest_id).execute(&mut *transaction).await
            .map_err(|error| AppError::internal("revoke workstation binding", error))?.rows_affected();
        if changed != 1 {
            return Err(binding_not_found());
        }
        sqlx::query("DELETE FROM auth_sessions WHERE workstation_binding_id=$1")
            .bind(binding_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| AppError::internal("revoke workstation sessions", error))?;
        transaction
            .commit()
            .await
            .map_err(|error| AppError::internal("commit workstation revocation", error))
    }
}

fn validate_workstation_input(
    ip: String,
    seat: String,
    label: Option<String>,
) -> Result<(String, String, Option<String>), AppError> {
    let ip = ip
        .trim()
        .parse::<IpAddr>()
        .map_err(|_| AppError::validation("ipAddress", "must be a valid IPv4 or IPv6 address"))?;
    let seat = seat.trim();
    if seat.is_empty() || seat.chars().count() > 64 {
        return Err(AppError::validation("seatNo", "must contain 1 to 64 characters"));
    }
    let label = label.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
    if label.as_ref().is_some_and(|value| value.chars().count() > 128) {
        return Err(AppError::validation("label", "must contain at most 128 characters"));
    }
    Ok((ip.to_string(), seat.to_owned(), label))
}

async fn require_manage(
    database: &PgPool,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    let mut transaction = database
        .begin()
        .await
        .map_err(|error| AppError::internal("begin competition access check", error))?;
    require_manage_tx(&mut transaction, contest_id, actor).await?;
    transaction
        .commit()
        .await
        .map_err(|error| AppError::internal("commit competition access check", error))
}

async fn require_manage_tx(
    transaction: &mut Transaction<'_, Postgres>,
    contest_id: i64,
    actor: &AuthUser,
) -> Result<(), AppError> {
    if actor.is_super_admin() {
        return Ok(());
    }
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM contest_management_assignments WHERE contest_id=$1 AND user_id=$2)",
    )
    .bind(contest_id)
    .bind(actor.id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|error| AppError::internal("check workstation management scope", error))?;
    if allowed { Ok(()) } else { Err(binding_not_found()) }
}

fn pairing_code() -> Result<String, AppError> {
    let mut bytes = [0_u8; PAIRING_CODE_BYTES];
    fill(&mut bytes).map_err(|error| AppError::internal("generate pairing code", error))?;
    Ok(bytes
        .into_iter()
        .map(|byte| PAIRING_ALPHABET[usize::from(byte) % PAIRING_ALPHABET.len()] as char)
        .collect())
}

fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    left.as_bytes().ct_eq(right.as_bytes()).into()
}

fn map_workstation_write_error(error: sqlx::Error) -> AppError {
    if error.as_database_error().and_then(sqlx::error::DatabaseError::constraint).is_some() {
        AppError::conflict(
            "WORKSTATION_IDENTITY_TAKEN",
            "IP address or seat number is already registered",
        )
    } else {
        AppError::internal("write competition workstation", error)
    }
}

fn map_binding_write_error(error: sqlx::Error) -> AppError {
    match error.as_database_error().and_then(sqlx::error::DatabaseError::constraint) {
        Some("idx_contest_workstation_active_terminal") => AppError::conflict(
            "WORKSTATION_ALREADY_BOUND",
            "Workstation is already bound for this contest",
        ),
        Some("idx_contest_workstation_active_team") => AppError::conflict(
            "TEAM_WORKSTATION_ALREADY_BOUND",
            "Team is already bound to a workstation for this contest",
        ),
        _ => AppError::internal("write workstation binding", error),
    }
}

fn binding_not_found() -> AppError {
    AppError::not_found("WORKSTATION_BINDING_NOT_FOUND", "Workstation binding was not found")
}

fn not_authenticated() -> AppError {
    AppError::unauthorized("NOT_AUTHENTICATED", "Not authenticated")
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr},
        time::Duration,
    };

    use sqlx::PgPool;

    use crate::features::competition::service::{CompetitionService, validate_workstation_input};
    use crate::{
        config::DeploymentMode,
        features::{
            auth::{
                AuthService,
                model::{AuthUser, UserType},
            },
            competition::model::{BindWorkstationRequest, CreateWorkstationRequest},
        },
    };

    #[test]
    fn workstation_input_normalizes_ip_and_text() {
        let (ip, seat, label) = validate_workstation_input(
            " 2001:0db8::1 ".to_owned(),
            " A01 ".to_owned(),
            Some(" Main lab ".to_owned()),
        )
        .expect("valid workstation");
        assert_eq!(ip, "2001:db8::1");
        assert_eq!(seat, "A01");
        assert_eq!(label.as_deref(), Some("Main lab"));
    }

    #[test]
    fn pairing_codes_are_normalized_for_human_entry() {
        assert_eq!(super::super::model::normalize_pairing_code("ab-cd 23"), "ABCD23");
        assert!(
            super::super::model::WorkstationLoginRequest { pairing_code: "AB-CD 23".to_owned() }
                .validate()
                .is_ok()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn pairing_is_bound_to_active_contest_ip_and_revocation(pool: PgPool) {
        let actor_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users(username,password_hash,display_name,user_type) VALUES('root','hash','Root','SUPER_ADMIN') RETURNING id",
        ).fetch_one(&pool).await.expect("actor");
        let user_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users(username,password_hash,display_name,user_type) VALUES('team-login','hash','Team','TEAM') RETURNING id",
        ).fetch_one(&pool).await.expect("team user");
        let team_id =
            sqlx::query_scalar::<_, i64>("INSERT INTO teams(name) VALUES('Team One') RETURNING id")
                .fetch_one(&pool)
                .await
                .expect("team");
        sqlx::query("INSERT INTO team_accounts(team_id,user_id) VALUES($1,$2)")
            .bind(team_id)
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("account");
        let contest_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Final','RUNNING','PRIVATE',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours') RETURNING id",
        ).fetch_one(&pool).await.expect("contest");
        sqlx::query("INSERT INTO contest_teams(contest_id,team_id,participation_type) VALUES($1,$2,'OFFICIAL')")
            .bind(contest_id).bind(team_id).execute(&pool).await.expect("roster");
        let service = CompetitionService::new(pool.clone());
        let workstation = service
            .create_workstation(CreateWorkstationRequest {
                ip_address: "192.0.2.10".into(),
                seat_no: "A01".into(),
                label: None,
            })
            .await
            .expect("workstation");
        let actor = AuthUser {
            id: actor_id,
            username: "root".into(),
            display_name: "Root".into(),
            user_type: UserType::SuperAdmin,
            permissions: vec![],
            password_reset_required: false,
        };
        let binding = service
            .bind(
                contest_id,
                BindWorkstationRequest { workstation_id: workstation.id, team_id },
                &actor,
            )
            .await
            .expect("binding");
        let code = binding.pairing_code.expect("one-time code");
        let ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));
        assert_eq!(
            service
                .login_grant(DeploymentMode::Competition, ip, &code)
                .await
                .expect("login")
                .user_id,
            user_id
        );
        assert_eq!(
            service
                .login_grant(DeploymentMode::Competition, IpAddr::V4(Ipv4Addr::LOCALHOST), &code)
                .await
                .expect_err("wrong IP")
                .code(),
            "WORKSTATION_NOT_BOUND"
        );
        assert_eq!(
            service
                .login_grant(DeploymentMode::Competition, ip, "WRONG99")
                .await
                .expect_err("wrong code")
                .code(),
            "PAIRING_CODE_INVALID"
        );

        let auth = AuthService::new(pool.clone(), Duration::from_secs(3600), false);
        let grant =
            service.login_grant(DeploymentMode::Competition, ip, &code).await.expect("grant");
        let (session, _) = auth.create_workstation_session(grant).await.expect("session");
        assert!(auth.authenticate(&session.session_token).await.is_ok());
        service.revoke(contest_id, binding.id, &actor).await.expect("revoke");
        assert!(auth.authenticate(&session.session_token).await.is_err());
    }

    #[test]
    fn pairing_code_comparison_is_constant_time_and_exact() {
        assert!(super::constant_time_equal("A1B2C3", "A1B2C3"));
        assert!(!super::constant_time_equal("A1B2C3", "A1B2C4"));
        assert!(!super::constant_time_equal("A1B2C3", "A1B2C3X"));
        assert!(!super::constant_time_equal("", "A"));
        assert!(super::constant_time_equal("", ""));
    }
}
