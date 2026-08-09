use std::net::{IpAddr, Ipv4Addr};

use sqlx::PgPool;

use super::model::{GroupControlRequest, GroupRequest, PlaylistItemRequest, PlaylistRequest};
use super::service::OrchestrationService;
use crate::features::{
    auth::model::{AuthUser, UserType},
    presentation::{ConfigRequest, HeartbeatRequest, PresentationService, RegisterRequest},
};

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires PostgreSQL"]
async fn playlists_groups_and_heartbeat_share_an_optimistic_timeline(pool: PgPool) {
    let user = sqlx::query_scalar::<_, i64>("INSERT INTO users(username,password_hash,display_name,user_type) VALUES('orchestration-op','hash','Orchestration Op','STAFF') RETURNING id")
.fetch_one(&pool).await.expect("screen operator");
    let contest = sqlx::query_scalar::<_, i64>("INSERT INTO contests(name,status,visibility,start_at,freeze_at,end_at) VALUES('Orchestration Contest','RUNNING','PUBLIC',now()-interval '1 hour',now()+interval '1 hour',now()+interval '2 hours') RETURNING id")
.fetch_one(&pool).await.expect("contest");
    let actor = AuthUser {
        id: user,
        username: "orchestration-op".into(),
        display_name: "Orchestration Op".into(),
        user_type: UserType::Staff,
        permissions: vec!["SCREEN_MANAGE".into()],
        password_reset_required: false,
    };
    let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let presentation = PresentationService::new(pool.clone());
    presentation
        .update_config(
            contest,
            "SCREEN",
            ConfigRequest {
                enabled: true,
                title: None,
                subtitle: None,
                accent_color: "#22c55e".into(),
                row_limit: 12,
                show_announcements: true,
                announcement_interval_seconds: 10,
                template: None,
                custom_template_id: None,
            },
            &actor,
            ip,
        )
        .await
        .expect("publish");
    let registration = presentation
        .register(RegisterRequest { contest_id: contest, name: "Hall A".into() }, ip)
        .await
        .expect("register");
    let service = OrchestrationService::new(pool.clone());
    let playlist = service
        .create_playlist(
            contest,
            PlaylistRequest {
                name: " Ceremony ".into(),
                loop_enabled: true,
                items: vec![
                    PlaylistItemRequest { target_view: "scoreboard".into(), duration_seconds: 10 },
                    PlaylistItemRequest { target_view: "AWARDS".into(), duration_seconds: 5 },
                ],
                expected_version: None,
            },
            &actor,
            ip,
        )
        .await
        .expect("playlist");
    assert_eq!(playlist.name, "Ceremony");
    assert_eq!(playlist.items.len(), 2);
    let group = service
        .create_group(
            contest,
            GroupRequest {
                name: "Main Hall".into(),
                instance_ids: vec![registration.instance_id, registration.instance_id],
                expected_version: None,
            },
            &actor,
            ip,
        )
        .await
        .expect("group");
    assert_eq!(group.instance_ids, vec![registration.instance_id]);
    let playing = service
        .control(
            group.id,
            GroupControlRequest {
                action: "PLAY".into(),
                playlist_id: Some(playlist.id),
                target_view: None,
                expected_version: Some(group.version),
            },
            &actor,
            ip,
        )
        .await
        .expect("play");
    assert_eq!(playing.playback_status, "PLAYING");
    assert_eq!(playing.version, 1);
    assert!(
        service
            .update_playlist(
                playlist.id,
                PlaylistRequest {
                    name: playlist.name.clone(),
                    loop_enabled: false,
                    items: vec![PlaylistItemRequest {
                        target_view: "AWARDS".into(),
                        duration_seconds: 10
                    }],
                    expected_version: Some(playlist.version)
                },
                &actor,
                ip
            )
            .await
            .is_err()
    );
    assert!(
        service
            .control(
                group.id,
                GroupControlRequest {
                    action: "STOP".into(),
                    playlist_id: None,
                    target_view: None,
                    expected_version: Some(0)
                },
                &actor,
                ip
            )
            .await
            .is_err()
    );
    let heartbeat = presentation
        .heartbeat(
            registration.instance_id,
            HeartbeatRequest {
                client_token: registration.client_token,
                current_view: "SCOREBOARD".into(),
            },
            ip,
        )
        .await
        .expect("heartbeat");
    let playback = heartbeat.group_playback.expect("group playback");
    assert_eq!(playback.group_id, group.id);
    assert_eq!(playback.playlist_id, Some(playlist.id));
    assert_eq!(playback.items.len(), 2);
    let paused = service
        .control(
            group.id,
            GroupControlRequest {
                action: "PAUSE".into(),
                playlist_id: None,
                target_view: None,
                expected_version: Some(playing.version),
            },
            &actor,
            ip,
        )
        .await
        .expect("pause");
    assert_eq!(paused.playback_status, "PAUSED");
    let resumed = service
        .control(
            group.id,
            GroupControlRequest {
                action: "RESUME".into(),
                playlist_id: None,
                target_view: None,
                expected_version: Some(paused.version),
            },
            &actor,
            ip,
        )
        .await
        .expect("resume");
    assert_eq!(resumed.playback_status, "PLAYING");
    let stopped = service
        .control(
            group.id,
            GroupControlRequest {
                action: "STOP".into(),
                playlist_id: None,
                target_view: None,
                expected_version: Some(resumed.version),
            },
            &actor,
            ip,
        )
        .await
        .expect("stop");
    assert_eq!(stopped.playback_status, "STOPPED");
    assert!(stopped.playlist_id.is_none());
    service.delete_group(group.id, &actor, ip).await.expect("delete group");
    service.delete_playlist(playlist.id, &actor, ip).await.expect("delete playlist");
}
