use crate::features::auth::model::{AuthUser, UserType};
use crate::features::presentation::model::{
    CommandRequest, ConfigRequest, HeartbeatRequest, RegisterRequest,
};
use crate::features::presentation::service::{
    PresentationService, token_hash, validate_mode, validate_template, validate_view,
};

use std::net::{IpAddr, Ipv4Addr};

use sqlx::PgPool;

#[test]
fn presentation_domains_are_closed() {
    assert_eq!(validate_mode("screen").expect("screen"), "SCREEN");
    assert!(validate_mode("OBS").is_err());
    assert_eq!(validate_view("awards").expect("awards"), "AWARDS");
    assert!(validate_view("javascript:").is_err());
    assert_eq!(validate_template(" CINEMATIC ").expect("template"), "CINEMATIC");
    assert!(validate_template("CUSTOM_HTML").is_err());
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL"]
async fn screen_registration_heartbeat_commands_and_revocation_are_atomic(pool: PgPool) {
    let user = sqlx::query_scalar::<_, i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('screen-op','hash','Screen Op','STAFF') RETURNING id")
        .fetch_one(&pool).await.expect("screen operator");
    let contest = sqlx::query_scalar::<_, i64>("INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Screen Contest','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours') RETURNING id")
        .fetch_one(&pool).await.expect("contest");
    let actor = AuthUser {
        id: user,
        username: "screen-op".into(),
        display_name: "Screen Op".into(),
        user_type: UserType::Staff,
        permissions: vec!["SCREEN_MANAGE".into()],
        password_reset_required: false,
    };
    let custom_template = sqlx::query_scalar::<_, i64>(
        "INSERT INTO presentation_templates(name,created_by_user_id) VALUES('Integration Template',$1) RETURNING id",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .expect("custom presentation template");
    let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let service = PresentationService::new(pool.clone());
    assert!(
        service
            .register(RegisterRequest { contest_id: contest, name: "Main Hall".into() }, ip)
            .await
            .is_err()
    );
    let config = service
        .update_config(
            contest,
            "SCREEN",
            ConfigRequest {
                enabled: true,
                title: Some("Finals".into()),
                subtitle: None,
                accent_color: "#22c55e".into(),
                row_limit: 12,
                show_announcements: true,
                announcement_interval_seconds: 10,
                template: Some("CUSTOM".into()),
                custom_template_id: Some(custom_template),
            },
            &actor,
            ip,
        )
        .await
        .expect("publish screen");
    assert!(config.enabled);
    assert_eq!(config.template, "CUSTOM");
    assert_eq!(config.custom_template_id, Some(custom_template));
    let registration = service
        .register(RegisterRequest { contest_id: contest, name: " Main Hall ".into() }, ip)
        .await
        .expect("register");
    assert_eq!(registration.name, "Main Hall");
    let stored_hash = sqlx::query_scalar::<_, String>(
        "SELECT client_token_hash FROM screen_instances WHERE id=$1",
    )
    .bind(registration.instance_id)
    .fetch_one(&pool)
    .await
    .expect("token hash");
    assert_ne!(stored_hash, registration.client_token);
    assert_eq!(stored_hash, token_hash(&registration.client_token));
    assert!(service.instances(contest, &actor).await.expect("instances")[0].online);
    service
        .command(
            contest,
            registration.instance_id,
            CommandRequest { target_view: "SCOREBOARD".into() },
            &actor,
            ip,
        )
        .await
        .expect("first command");
    let latest = service
        .command(
            contest,
            registration.instance_id,
            CommandRequest { target_view: "AWARDS".into() },
            &actor,
            ip,
        )
        .await
        .expect("latest command");
    let heartbeat = service
        .heartbeat(
            registration.instance_id,
            HeartbeatRequest {
                client_token: registration.client_token.clone(),
                current_view: "STATISTICS".into(),
            },
            ip,
        )
        .await
        .expect("heartbeat");
    assert_eq!(heartbeat.command_id, Some(latest.id));
    assert_eq!(heartbeat.target_view.as_deref(), Some("AWARDS"));
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM screen_commands WHERE screen_instance_id=$1 AND acknowledged_at IS NOT NULL").bind(registration.instance_id).fetch_one(&pool).await.expect("acked"), 2);
    let next_heartbeat = service
        .heartbeat(
            registration.instance_id,
            HeartbeatRequest {
                client_token: registration.client_token.clone(),
                current_view: "AWARDS".into(),
            },
            ip,
        )
        .await
        .expect("next heartbeat");
    assert_eq!(next_heartbeat.command_id, None);
    assert_eq!(next_heartbeat.target_view, None);
    assert!(
        service
            .heartbeat(
                registration.instance_id,
                HeartbeatRequest {
                    client_token: "wrong".into(),
                    current_view: "SCOREBOARD".into()
                },
                ip
            )
            .await
            .is_err()
    );
    sqlx::query("UPDATE screen_instances SET last_seen_at=now()-interval '1 minute' WHERE id=$1")
        .bind(registration.instance_id)
        .execute(&pool)
        .await
        .expect("age heartbeat");
    assert!(!service.instances(contest, &actor).await.expect("offline instance")[0].online);
    let group = sqlx::query_scalar::<_, i64>("INSERT INTO screen_groups(contest_id,name,created_by_user_id) VALUES($1,'Hall',$2) RETURNING id").bind(contest).bind(user).fetch_one(&pool).await.expect("group");
    sqlx::query("INSERT INTO screen_group_members(group_id,screen_instance_id) VALUES($1,$2)")
        .bind(group)
        .bind(registration.instance_id)
        .execute(&pool)
        .await
        .expect("group member");
    service.revoke(contest, registration.instance_id, &actor, ip).await.expect("revoke");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM screen_group_members WHERE screen_instance_id=$1"
        )
        .bind(registration.instance_id)
        .fetch_one(&pool)
        .await
        .expect("members"),
        0
    );
    assert!(
        service
            .heartbeat(
                registration.instance_id,
                HeartbeatRequest {
                    client_token: registration.client_token,
                    current_view: "SCOREBOARD".into()
                },
                ip
            )
            .await
            .is_err()
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM audit_logs WHERE actor_user_id=$1 AND action IN ('PRESENTATION_CONFIG_UPDATED','SCREEN_COMMAND_SENT','SCREEN_INSTANCE_REVOKED')").bind(user).fetch_one(&pool).await.expect("audit"), 4);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM realtime_outbox WHERE contest_id=$1 AND event_type='PRESENTATION_UPDATED'").bind(contest).fetch_one(&pool).await.expect("outbox"), 1);
}
