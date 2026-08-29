use std::net::IpAddr;

use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;

use crate::{
    error::AppError,
    features::auth::model::AuthUser,
    features::scoreboard::{ScoreboardResponse, ScoreboardRow},
};

use super::super::model::{
    AwardCandidateResponse, AwardConflict, AwardResolverRunResponse, AwardSetResponse,
    CategoryResponse, CertificateRow, ManualRecipientRequest, RecipientResponse,
};
use super::{
    AwardService, audit, category_query_tx, require_active_contest, require_active_contest_tx,
    require_operator, stale,
};

impl AwardService {
    pub async fn completed_resolver_runs(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<AwardResolverRunResponse>, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        sqlx::query_as("SELECT id,completed_at FROM resolver_runs WHERE contest_id=$1 AND official AND status='COMPLETED' ORDER BY completed_at DESC,id DESC")
            .bind(contest).fetch_all(&self.database).await
            .map_err(|error| AppError::internal("list completed Resolver runs", error))
    }

    pub async fn candidates(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<Vec<AwardCandidateResponse>, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        let snapshot = sqlx::query_scalar::<_, String>("SELECT snapshot.payload_json FROM award_sets award JOIN scoreboard_snapshots snapshot ON snapshot.id=award.final_scoreboard_snapshot_id WHERE award.contest_id=$1")
            .bind(contest).fetch_optional(&self.database).await
            .map_err(|error| AppError::internal("load award candidates", error))?
            .ok_or_else(|| AppError::not_found("AWARD_SET_NOT_FOUND", "Award set was not found"))?;
        let board: ScoreboardResponse = serde_json::from_str(&snapshot)
            .map_err(|error| AppError::internal("decode award candidates", error))?;
        Ok(board
            .rows
            .into_iter()
            .map(|row| AwardCandidateResponse {
                team_id: row.team_id,
                team_name: row.team_name,
                school: row.school,
                rank: row.rank,
                participation_type: row.participation_type,
                group_name: row.group_name,
                is_star: row.is_star,
            })
            .collect())
    }

    pub async fn generate(
        &self,
        contest: i64,
        run_id: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|e| AppError::internal("begin award generation", e))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("awards:{contest}"))
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("lock award generation", e))?;
        let (snapshot_id, payload) = sqlx::query_as::<_, (i64, String)>(r#"
            SELECT run.source_final_snapshot_id, snapshot.payload_json
            FROM resolver_runs run JOIN scoreboard_snapshots snapshot ON snapshot.id = run.source_final_snapshot_id
            WHERE run.id = $1 AND run.contest_id = $2 AND run.official AND run.status = 'COMPLETED'
        "#).bind(run_id).bind(contest).fetch_optional(&mut *tx).await
            .map_err(|e| AppError::internal("load official Resolver result", e))?
            .ok_or_else(|| AppError::conflict("AWARD_RESOLVER_NOT_FINAL", "Awards require the completed official Resolver run"))?;
        let board: ScoreboardResponse = serde_json::from_str(&payload)
            .map_err(|e| AppError::internal("decode final award board", e))?;
        let categories = category_query_tx(&mut tx, contest).await?;
        if categories.is_empty() {
            return Err(AppError::validation(
                "categories",
                "must configure at least one award category",
            ));
        }
        let set_id = sqlx::query_scalar::<_, i64>("INSERT INTO award_sets (contest_id, resolver_run_id, final_scoreboard_snapshot_id, generated_by_user_id) VALUES ($1,$2,$3,$4) ON CONFLICT (contest_id) DO UPDATE SET resolver_run_id=excluded.resolver_run_id, final_scoreboard_snapshot_id=excluded.final_scoreboard_snapshot_id, generated_by_user_id=excluded.generated_by_user_id, generated_at=now(), version=award_sets.version+1 WHERE award_sets.status='DRAFT' RETURNING id")
            .bind(contest).bind(run_id).bind(snapshot_id).bind(actor.id).fetch_optional(&mut *tx).await
            .map_err(|e| AppError::internal("persist award set", e))?
            .ok_or_else(|| AppError::conflict("AWARD_SET_FROZEN", "Frozen awards cannot be regenerated"))?;
        sqlx::query("DELETE FROM award_recipients WHERE contest_id=$1 AND NOT is_manual")
            .bind(contest)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("replace generated awards", e))?;
        for category in &categories {
            let mut eligible =
                board.rows.iter().filter(|row| eligible(row, category)).collect::<Vec<_>>();
            eligible.sort_by_key(|row| row.rank);
            let selected = select_rows(&eligible, category);
            for row in selected {
                insert_recipient(&mut tx, contest, category.id, row, snapshot_id, false).await?;
            }
        }
        audit(&mut tx, actor.id, "AWARDS_GENERATED", set_id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit award generation", e))?;
        self.load_set(contest, actor).await
    }

    pub async fn manual_add(
        &self,
        contest: i64,
        request: ManualRecipientRequest,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        let mut tx =
            self.database.begin().await.map_err(|e| AppError::internal("begin manual award", e))?;
        let (set_id, snapshot_id, version) = lock_set(&mut tx, contest).await?;
        if version != request.expected_set_version {
            return Err(stale());
        }
        let payload = sqlx::query_scalar::<_, String>(
            "SELECT payload_json FROM scoreboard_snapshots WHERE id=$1",
        )
        .bind(snapshot_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("load award snapshot", e))?;
        let board: ScoreboardResponse = serde_json::from_str(&payload)
            .map_err(|e| AppError::internal("decode award snapshot", e))?;
        let row =
            board.rows.iter().find(|row| row.team_id == request.team_id).ok_or_else(|| {
                AppError::validation("teamId", "team is not present in the final snapshot")
            })?;
        let category_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM award_categories WHERE id=$1 AND contest_id=$2)",
        )
        .bind(request.category_id)
        .bind(contest)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::internal("check award category", e))?;
        if !category_exists {
            return Err(AppError::not_found(
                "AWARD_CATEGORY_NOT_FOUND",
                "Award category was not found",
            ));
        }
        insert_recipient(&mut tx, contest, request.category_id, row, snapshot_id, true).await?;
        sqlx::query("UPDATE award_sets SET version=version+1 WHERE id=$1")
            .bind(set_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal("version manual award", e))?;
        audit(&mut tx, actor.id, "AWARD_RECIPIENT_ADDED", set_id, ip).await?;
        tx.commit().await.map_err(|e| AppError::internal("commit manual award", e))?;
        self.load_set(contest, actor).await
    }

    pub async fn manual_remove(
        &self,
        recipient: i64,
        expected_version: i32,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin manual award removal", error))?;
        let (contest, manual) = sqlx::query_as::<_, (i64, bool)>(
            "SELECT contest_id,is_manual FROM award_recipients WHERE id=$1 FOR UPDATE",
        )
        .bind(recipient)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("lock award recipient", error))?
        .ok_or_else(|| {
            AppError::not_found("AWARD_RECIPIENT_NOT_FOUND", "Award recipient was not found")
        })?;
        require_active_contest_tx(&mut tx, contest).await?;
        let (set_id, _, version) = lock_set(&mut tx, contest).await?;
        if version != expected_version {
            return Err(stale());
        }
        if !manual {
            return Err(AppError::conflict(
                "AWARD_RECIPIENT_GENERATED",
                "Generated recipients must be changed by regenerating awards",
            ));
        }
        sqlx::query("DELETE FROM award_recipients WHERE id=$1")
            .bind(recipient)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("delete manual award recipient", error))?;
        sqlx::query("UPDATE award_sets SET version=version+1 WHERE id=$1")
            .bind(set_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| AppError::internal("version manual award removal", error))?;
        audit(&mut tx, actor.id, "AWARD_RECIPIENT_REMOVED", recipient, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit manual award removal", error))?;
        self.load_set(contest, actor).await
    }

    pub async fn freeze(
        &self,
        contest: i64,
        expected: i32,
        frozen: bool,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        let mut tx =
            self.database.begin().await.map_err(|e| AppError::internal("begin award freeze", e))?;
        let changed = sqlx::query("UPDATE award_sets SET status=CASE WHEN $3 THEN 'FROZEN' ELSE 'DRAFT' END, frozen_at=CASE WHEN $3 THEN now() ELSE NULL END, frozen_by_user_id=CASE WHEN $3 THEN $4 ELSE NULL END, version=version+1 WHERE contest_id=$1 AND version=$2 AND status=CASE WHEN $3 THEN 'DRAFT' ELSE 'FROZEN' END")
            .bind(contest).bind(expected).bind(frozen).bind(actor.id).execute(&mut *tx).await
            .map_err(|e| AppError::internal("freeze award set", e))?.rows_affected();
        if changed != 1 {
            return Err(stale());
        }
        if frozen {
            snapshot_certificates(&mut tx, contest).await?;
        } else {
            sqlx::query("DELETE FROM award_certificate_rows WHERE contest_id=$1")
                .bind(contest)
                .execute(&mut *tx)
                .await
                .map_err(|error| AppError::internal("clear award certificate snapshot", error))?;
        }
        audit(
            &mut tx,
            actor.id,
            if frozen { "AWARDS_FROZEN" } else { "AWARDS_UNFROZEN" },
            contest,
            ip,
        )
        .await?;
        tx.commit().await.map_err(|e| AppError::internal("commit award freeze", e))?;
        self.load_set(contest, actor).await
    }

    pub async fn certificate_csv(
        &self,
        contest: i64,
        actor: &AuthUser,
        ip: IpAddr,
    ) -> Result<(String, String), AppError> {
        require_operator(actor)?;
        let mut tx = self
            .database
            .begin()
            .await
            .map_err(|error| AppError::internal("begin award certificate export", error))?;
        let contest_name = sqlx::query_scalar::<_, String>(
            "SELECT name FROM contests WHERE id=$1 AND deleted_at IS NULL",
        )
        .bind(contest)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load certificate contest", error))?
        .ok_or_else(|| AppError::not_found("CONTEST_NOT_FOUND", "Contest was not found"))?;
        let frozen = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM award_sets WHERE contest_id=$1 AND status='FROZEN')",
        )
        .bind(contest)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("check certificate award freeze", error))?;
        if !frozen {
            return Err(AppError::conflict(
                "AWARD_CERTIFICATE_EXPORT_NOT_FROZEN",
                "Freeze the award list before exporting certificates",
            ));
        }
        let row_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM award_certificate_rows WHERE contest_id=$1",
        )
        .bind(contest)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("count certificate snapshot", error))?;
        let recipient_count = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM award_recipients WHERE contest_id=$1",
        )
        .bind(contest)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| AppError::internal("count certificate recipients", error))?;
        if row_count == 0 && recipient_count > 0 {
            // Compatibility backfill for contests frozen before certificate
            // snapshots were implemented by the Rust service.
            snapshot_certificates(&mut tx, contest).await?;
        }
        let rows = sqlx::query_as::<_, CertificateRow>(
            "SELECT certificate_no,contest_id,contest_name,award_code,award_name,problem_alias,team_id,team_name,school,source_member_id,recipient_name,recipient_role,seat_no,group_name,participation_type,rank FROM award_certificate_rows WHERE contest_id=$1 ORDER BY export_order",
        )
        .bind(contest)
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| AppError::internal("load certificate snapshot", error))?;
        audit(&mut tx, actor.id, "AWARD_CERTIFICATES_EXPORTED", contest, ip).await?;
        tx.commit()
            .await
            .map_err(|error| AppError::internal("commit award certificate export", error))?;
        Ok((contest_name, certificate_csv(&rows)))
    }

    pub async fn load_set(
        &self,
        contest: i64,
        actor: &AuthUser,
    ) -> Result<AwardSetResponse, AppError> {
        require_operator(actor)?;
        require_active_contest(&self.database, contest).await?;
        let row = sqlx::query_as::<_, (i64,i64,i64,String,i32,OffsetDateTime,Option<OffsetDateTime>)>("SELECT id,resolver_run_id,final_scoreboard_snapshot_id,status,version,generated_at,frozen_at FROM award_sets WHERE contest_id=$1")
            .bind(contest).fetch_optional(&self.database).await.map_err(|e| AppError::internal("load award set", e))?
            .ok_or_else(|| AppError::not_found("AWARD_SET_NOT_FOUND", "Award set was not found"))?;
        let recipients = recipient_query(&self.database, contest).await?;
        let conflicts = conflicts(&recipients);
        Ok(AwardSetResponse {
            id: row.0,
            contest_id: contest,
            resolver_run_id: row.1,
            final_scoreboard_snapshot_id: row.2,
            status: row.3,
            version: row.4,
            generated_at: row.5,
            frozen_at: row.6,
            recipients,
            conflicts,
        })
    }
}

async fn snapshot_certificates(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM award_certificate_rows WHERE contest_id=$1")
        .bind(contest)
        .execute(&mut **tx)
        .await
        .map_err(|error| AppError::internal("replace award certificate snapshot", error))?;
    sqlx::query(
        r#"
        INSERT INTO award_certificate_rows(
            contest_id,award_recipient_id,source_member_id,certificate_no,export_order,
            contest_name,award_code,award_name,problem_alias,team_id,team_name,school,
            recipient_name,recipient_role,seat_no,group_name,participation_type,rank
        )
        SELECT
            recipient.contest_id,
            recipient.id,
            member.id,
            'XCPC-' || recipient.contest_id || '-R' || recipient.id || '-' ||
                CASE WHEN member.id IS NULL THEN 'TEAM' ELSE 'M' || member.id END,
            row_number() OVER (
                ORDER BY category.display_order,
                    recipient.rank NULLS LAST,
                    recipient.team_id,
                    recipient.id,
                    member.created_at NULLS FIRST,
                    member.id NULLS FIRST
            )::integer,
            contest.name,
            category.code,
            category.name,
            recipient.problem_alias,
            recipient.team_id,
            coalesce(recipient.team_name, ''),
            recipient.school,
            coalesce(member.name, recipient.team_name, ''),
            coalesce(member.role_name, 'TEAM'),
            recipient.seat_no,
            recipient.group_name,
            recipient.participation_type,
            recipient.rank
        FROM award_recipients recipient
        JOIN contests contest ON contest.id=recipient.contest_id AND contest.deleted_at IS NULL
        JOIN award_categories category ON category.id=recipient.category_id
        LEFT JOIN team_members member ON member.team_id=recipient.team_id
        WHERE recipient.contest_id=$1
        "#,
    )
    .bind(contest)
    .execute(&mut **tx)
    .await
    .map_err(|error| AppError::internal("snapshot award certificates", error))?;
    Ok(())
}

fn certificate_csv(rows: &[CertificateRow]) -> String {
    let mut output = "\u{feff}证书编号,比赛编号,比赛名称,奖项代码,奖项名称,题目标识,队伍编号,队伍名称,学校,成员编号,获奖人,成员角色,座位号,组别,参赛类型,名次\r\n".to_string();
    for row in rows {
        let fields = [
            certificate_value(Some(&row.certificate_no)),
            row.contest_id.to_string(),
            certificate_value(Some(&row.contest_name)),
            certificate_value(Some(&row.award_code)),
            certificate_value(Some(&row.award_name)),
            certificate_value(row.problem_alias.as_deref()),
            row.team_id.to_string(),
            certificate_value(Some(&row.team_name)),
            certificate_value(row.school.as_deref()),
            row.source_member_id.map_or_else(String::new, |value| value.to_string()),
            certificate_value(Some(&row.recipient_name)),
            certificate_value(row.recipient_role.as_deref()),
            certificate_value(row.seat_no.as_deref()),
            certificate_value(row.group_name.as_deref()),
            certificate_value(row.participation_type.as_deref()),
            row.rank.map_or_else(String::new, |value| value.to_string()),
        ];
        output.push_str(&fields.join(","));
        output.push_str("\r\n");
    }
    output
}

pub(crate) fn certificate_value(value: Option<&str>) -> String {
    let value = value.unwrap_or("");
    let safe = if matches!(value.as_bytes().first(), Some(b'=' | b'+' | b'-' | b'@')) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if safe.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

fn eligible(row: &ScoreboardRow, c: &CategoryResponse) -> bool {
    (c.include_star || !row.is_star)
        && c.group_name.as_ref().is_none_or(|g| row.group_name.as_ref() == Some(g))
        && c.participation_type.as_ref().is_none_or(|p| &row.participation_type == p)
}

pub(crate) fn select_rows<'a>(
    rows: &[&'a ScoreboardRow],
    c: &CategoryResponse,
) -> Vec<&'a ScoreboardRow> {
    if c.first_blood {
        return rows
            .iter()
            .filter(|row| row.problems.iter().any(|cell| cell.first_blood))
            .copied()
            .collect();
    }
    match c.rule_type.as_str() {
        "FIXED_COUNT" => rows.iter().take(c.fixed_count.unwrap_or(0) as usize).copied().collect(),
        "RATIO" => {
            let n = ((rows.len() as f64) * c.ratio.unwrap_or(0.0)).ceil() as usize;
            rows.iter().take(n).copied().collect()
        }
        "RANK_RANGE" => rows
            .iter()
            .filter(|r| {
                c.rank_from.is_some_and(|a| r.rank >= a as u32)
                    && c.rank_to.is_some_and(|b| r.rank <= b as u32)
            })
            .copied()
            .collect(),
        _ => Vec::new(),
    }
}

async fn insert_recipient(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
    category: i64,
    row: &ScoreboardRow,
    snapshot: i64,
    manual: bool,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO award_recipients(contest_id,category_id,team_id,rank,solved,penalty_minutes,team_name,school,group_name,is_star,is_manual,participation_type,source_scoreboard_snapshot_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(contest_id,category_id,team_id,award_key) DO UPDATE SET rank=excluded.rank,solved=excluded.solved,penalty_minutes=excluded.penalty_minutes,team_name=excluded.team_name,school=excluded.school,group_name=excluded.group_name,is_star=excluded.is_star,is_manual=award_recipients.is_manual OR excluded.is_manual,participation_type=excluded.participation_type,source_scoreboard_snapshot_id=excluded.source_scoreboard_snapshot_id,updated_at=now(),version=award_recipients.version+1").bind(contest).bind(category).bind(row.team_id).bind(i32::try_from(row.rank).unwrap_or(i32::MAX)).bind(row.solved_count).bind(row.penalty_minutes).bind(&row.team_name).bind(row.school.as_deref()).bind(row.group_name.as_deref()).bind(row.is_star).bind(manual).bind(&row.participation_type).bind(snapshot).execute(&mut**tx).await.map(|_|()).map_err(|e|AppError::internal("insert award recipient",e))
}

const RECIPIENT_SQL: &str = "SELECT r.id,r.category_id,c.code AS category_code,c.name AS category_name,r.team_id,coalesce(r.team_name,'') AS team_name,r.school,r.rank,r.solved,r.penalty_minutes,r.participation_type,r.group_name,r.is_star,r.is_manual FROM award_recipients r JOIN award_categories c ON c.id=r.category_id WHERE r.contest_id=$1 ORDER BY c.display_order,r.rank NULLS LAST,r.team_id";
async fn recipient_query(db: &PgPool, c: i64) -> Result<Vec<RecipientResponse>, AppError> {
    sqlx::query_as(RECIPIENT_SQL)
        .bind(c)
        .fetch_all(db)
        .await
        .map_err(|e| AppError::internal("list award recipients", e))
}

fn conflicts(rows: &[RecipientResponse]) -> Vec<AwardConflict> {
    let mut map = std::collections::BTreeMap::<i64, (String, Vec<String>)>::new();
    for r in rows {
        let e = map.entry(r.team_id).or_insert_with(|| (r.team_name.clone(), Vec::new()));
        e.1.push(r.category_code.clone());
    }
    map.into_iter()
        .filter(|(_, (_, c))| c.len() > 1)
        .map(|(team_id, (team_name, category_codes))| AwardConflict {
            team_id,
            team_name,
            category_codes,
        })
        .collect()
}

async fn lock_set(
    tx: &mut Transaction<'_, Postgres>,
    contest: i64,
) -> Result<(i64, i64, i32), AppError> {
    require_active_contest_tx(tx, contest).await?;
    sqlx::query_as("SELECT id,final_scoreboard_snapshot_id,version FROM award_sets WHERE contest_id=$1 AND status='DRAFT' FOR UPDATE").bind(contest).fetch_optional(&mut**tx).await.map_err(|e|AppError::internal("lock award set",e))?.ok_or_else(||AppError::conflict("AWARD_SET_NOT_MUTABLE","A draft award set is required"))
}
