use std::net::{IpAddr, Ipv4Addr};

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::OffsetDateTime;

use super::*;
use crate::features::awards::handlers::{csv_field, percent_encode_filename};
use crate::features::awards::service::{certificate_value, select_rows, validate_category};
use crate::features::{
    auth::model::{AuthUser, UserType},
    scoreboard::{ScoreboardResponse, ScoreboardRow},
};
#[test]
fn rules_are_closed() {
    assert!(
        validate_category(CategoryRequest {
            code: "gold".into(),
            name: "Gold".into(),
            display_order: 1,
            include_star: false,
            group_name: None,
            participation_type: Some("official".into()),
            first_blood: false,
            rule: RuleRequest {
                rule_type: "fixed_count".into(),
                ratio: None,
                fixed_count: Some(3),
                rank_from: None,
                rank_to: None
            }
        })
        .is_ok()
    );
}

#[test]
fn award_csv_blocks_spreadsheet_formulas_and_escapes_quotes() {
    assert_eq!(csv_field("=cmd()"), "\"'=cmd()\"");
    assert_eq!(csv_field("A \"Team\""), "\"A \"\"Team\"\"\"");
}

#[test]
fn certificate_csv_is_excel_safe_and_filename_is_rfc5987_compatible() {
    assert_eq!(certificate_value(Some("=cmd()")), "'=cmd()");
    assert_eq!(certificate_value(Some("Alice, A")), "\"Alice, A\"");
    assert_eq!(
        percent_encode_filename("华东-证书.csv"),
        "%E5%8D%8E%E4%B8%9C-%E8%AF%81%E4%B9%A6.csv"
    );
}

#[test]
fn first_blood_category_uses_snapshot_cell_markers_instead_of_rank_rule() {
    let mut first = board_row_for_award_test(1, true);
    let second = board_row_for_award_test(2, false);
    first.problems[0].first_blood = true;
    let category = category_response_for_award_test(true);
    let rows = vec![&first, &second];
    let selected = select_rows(&rows, &category);
    assert_eq!(selected.iter().map(|row| row.team_id).collect::<Vec<_>>(), vec![1]);
}

fn board_row_for_award_test(team_id: i64, solved: bool) -> ScoreboardRow {
    ScoreboardRow {
        rank: u32::try_from(team_id).expect("rank"),
        official_rank: Some(u32::try_from(team_id).expect("official rank")),
        team_id,
        team_name: format!("Team {team_id}"),
        school: None,
        participation_type: "OFFICIAL".into(),
        group_name: None,
        is_star: false,
        solved_count: i32::from(solved),
        penalty_minutes: 0,
        total_score_milli: if solved { 100_000 } else { 0 },
        last_solved_at: None,
        problems: vec![crate::features::scoreboard::ScoreboardCell {
            problem_id: 1,
            wrong_attempts: 0,
            solved,
            solved_at: None,
            penalty_minutes: 0,
            score_milli: if solved { 100_000 } else { 0 },
            first_blood: false,
        }],
    }
}

fn category_response_for_award_test(first_blood: bool) -> CategoryResponse {
    CategoryResponse {
        id: 1,
        contest_id: 1,
        code: "FB".into(),
        name: "First Blood".into(),
        display_order: 1,
        include_star: false,
        group_name: None,
        participation_type: None,
        first_blood,
        version: 0,
        rule_type: "FIXED_COUNT".into(),
        ratio: None,
        fixed_count: Some(100),
        rank_from: None,
        rank_to: None,
    }
}

fn fixed_category(code: &str, order: i32) -> CategoryRequest {
    CategoryRequest {
        code: code.into(),
        name: code.into(),
        display_order: order,
        include_star: false,
        group_name: None,
        participation_type: Some("official".into()),
        first_blood: false,
        rule: RuleRequest {
            rule_type: "fixed_count".into(),
            ratio: None,
            fixed_count: Some(1),
            rank_from: None,
            rank_to: None,
        },
    }
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL"]
async fn awards_use_official_resolver_snapshot_and_freeze(pool: PgPool) {
    let user = sqlx::query_scalar::<_, i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('award-op','hash','Award Op','STAFF') RETURNING id").fetch_one(&pool).await.expect("user");
    let contest = sqlx::query_scalar::<_, i64>("INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Award Contest','ENDED','PUBLIC',now()-interval '3 hours',now()-interval '2 hours',now()-interval '1 hour') RETURNING id").fetch_one(&pool).await.expect("contest");
    let mut rows = Vec::new();
    for rank in 1_u32..=2 {
        let team = sqlx::query_scalar::<_, i64>("INSERT INTO teams(name) VALUES($1) RETURNING id")
            .bind(format!("Award Team {rank}"))
            .fetch_one(&pool)
            .await
            .expect("team");
        rows.push(ScoreboardRow {
            rank,
            official_rank: Some(rank),
            team_id: team,
            team_name: format!("Award Team {rank}"),
            school: None,
            participation_type: "OFFICIAL".into(),
            group_name: None,
            is_star: false,
            solved_count: 3 - i32::try_from(rank).expect("rank"),
            penalty_minutes: i64::from(rank) * 60,
            total_score_milli: i64::from(3 - i32::try_from(rank).expect("rank")) * 100_000,
            last_solved_at: None,
            problems: Vec::new(),
        });
    }
    let board = ScoreboardResponse {
        contest_id: contest,
        variant: "ADMIN".into(),
        frozen: false,
        scoring_mode: "ICPC".into(),
        score_aggregation: "BEST".into(),
        generated_at: OffsetDateTime::now_utc(),
        problems: Vec::new(),
        rows,
    };
    let payload = serde_json::to_string(&board).expect("board");
    let sha = hex::encode(Sha256::digest(payload.as_bytes()));
    let final_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO scoreboard_snapshots(contest_id,variant,version,frozen,generated_at,payload_json,payload_sha256,created_by,created_by_user_id) VALUES($1,'ADMIN',1,false,now(),$2,$3,'award-op',$4) RETURNING id",
    )
    .bind(contest).bind(&payload).bind(&sha).bind(user).fetch_one(&pool).await.expect("final");
    let public_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO scoreboard_snapshots(contest_id,variant,version,frozen,generated_at,payload_json,payload_sha256,created_by,created_by_user_id) VALUES($1,'PUBLIC',1,true,now(),$2,$3,'award-op',$4) RETURNING id",
    )
    .bind(contest).bind(&payload).bind(&sha).bind(user).fetch_one(&pool).await.expect("public");
    let run = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO resolver_runs
            (contest_id,official,status,current_step,total_steps,source_public_snapshot_id,
             source_final_snapshot_id,plan_sha256,created_by_user_id,started_at,completed_at)
        VALUES($1,true,'COMPLETED',0,0,$2,$3,$4,$5,now(),now())
        RETURNING id
        "#,
    )
    .bind(contest)
    .bind(public_id)
    .bind(final_id)
    .bind("a".repeat(64))
    .bind(user)
    .fetch_one(&pool)
    .await
    .expect("resolver");
    let actor = AuthUser {
        id: user,
        username: "award-op".into(),
        display_name: "Award Op".into(),
        user_type: UserType::Staff,
        permissions: vec!["AWARD_MANAGE".into()],
        password_reset_required: false,
    };
    let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let service = AwardService::new(pool.clone());
    service.create_category(contest, fixed_category("GOLD", 1), &actor, ip).await.expect("gold");
    let silver = service
        .create_category(contest, fixed_category("SILVER", 2), &actor, ip)
        .await
        .expect("silver");
    let generated = service.generate(contest, run, &actor, ip).await.expect("generate");
    assert_eq!(generated.final_scoreboard_snapshot_id, final_id);
    assert_eq!(generated.recipients.len(), 2);
    assert_eq!(generated.conflicts.len(), 1);
    assert_eq!(service.completed_resolver_runs(contest, &actor).await.expect("runs").len(), 1);
    assert_eq!(service.candidates(contest, &actor).await.expect("candidates").len(), 2);
    let with_manual = service
        .manual_add(
            contest,
            ManualRecipientRequest {
                category_id: silver.id,
                team_id: board.rows[1].team_id,
                expected_set_version: generated.version,
            },
            &actor,
            ip,
        )
        .await
        .expect("memberless team certificate recipient");
    let first_team = board.rows[0].team_id;
    let member = sqlx::query_scalar::<_, i64>("INSERT INTO team_members(team_id,name,role_name) VALUES($1,'Alice, A','CAPTAIN') RETURNING id")
            .bind(first_team).fetch_one(&pool).await.expect("member");
    sqlx::query("INSERT INTO team_members(team_id,name,role_name) VALUES($1,'Bob','COACH')")
        .bind(first_team)
        .execute(&pool)
        .await
        .expect("coach");
    let frozen =
        service.freeze(contest, with_manual.version, true, &actor, ip).await.expect("freeze");
    assert_eq!(frozen.status, "FROZEN");
    sqlx::query("UPDATE team_members SET name='Changed after freeze' WHERE id=$1")
        .bind(member)
        .execute(&pool)
        .await
        .expect("edit member after freeze");
    let (_, certificates) =
        service.certificate_csv(contest, &actor, ip).await.expect("certificate export");
    assert!(certificates.starts_with("\u{feff}证书编号"));
    assert!(certificates.contains(&format!("XCPC-{contest}-R")));
    assert!(certificates.contains("\"Alice, A\""));
    assert!(certificates.contains(",Bob,COACH,"));
    assert!(certificates.contains(",TEAM,"));
    assert!(!certificates.contains("Changed after freeze"));
    let presentation = service.presentation(contest).await.expect("public presentation");
    assert_eq!(presentation.categories.len(), 2);
    assert_eq!(presentation.status, "WAITING");
    assert_eq!(presentation.current_category_id, presentation.categories[0].id);
    let silver_id = presentation.categories[1].id;
    let controlled = service
        .update_presentation(
            contest,
            PresentationRequest {
                current_category_id: Some(silver_id),
                status: "presenting".into(),
                auto_rotate: true,
                interval_seconds: 12,
            },
            &actor,
            ip,
        )
        .await
        .expect("control presentation");
    assert_eq!(controlled.current_category_id, silver_id);
    assert_eq!(controlled.status, "PRESENTING");
    assert!(controlled.auto_rotate);
    let script = service.host_script(contest).await.expect("default host script");
    assert_eq!(script.version, None);
    assert!(script.sections[1].current);
    let saved = service
        .save_host_script(
            contest,
            HostScriptRequest {
                opening_text: "Welcome".into(),
                closing_text: "Goodbye".into(),
                sections: vec![HostScriptSectionRequest {
                    category_id: silver_id,
                    cue_text: "Silver teams, please come to the stage.".into(),
                }],
                expected_version: None,
            },
            &actor,
            ip,
        )
        .await
        .expect("save host script");
    assert_eq!(saved.version, Some(0));
    assert_eq!(saved.opening_text, "Welcome");
    assert_eq!(saved.sections[1].cue_text, "Silver teams, please come to the stage.");
    assert!(
        service
            .save_host_script(
                contest,
                HostScriptRequest {
                    opening_text: "stale".into(),
                    closing_text: String::new(),
                    sections: Vec::new(),
                    expected_version: None,
                },
                &actor,
                ip,
            )
            .await
            .is_err()
    );
    let published = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM realtime_outbox WHERE contest_id=$1 AND event_type='AWARDS_UPDATED' AND scope='PUBLIC'")
            .bind(contest).fetch_one(&pool).await.expect("presentation event");
    assert_eq!(published, 1);
    assert!(
        service.create_category(contest, fixed_category("BRONZE", 3), &actor, ip).await.is_err()
    );
    assert_eq!(
        service.freeze(contest, frozen.version, false, &actor, ip).await.expect("unfreeze").status,
        "DRAFT"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM award_certificate_rows WHERE contest_id=$1"
        )
        .bind(contest)
        .fetch_one(&pool)
        .await
        .expect("certificate rows"),
        0
    );
    assert!(service.certificate_csv(contest, &actor, ip).await.is_err());
}
