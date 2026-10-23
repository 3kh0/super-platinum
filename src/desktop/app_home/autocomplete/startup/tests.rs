use super::*;
use serde_json::json;
use std::time::Instant;
use super_platinum_core::{
    slack::{realtime::Connection, transport::Health},
    state::{RealtimeStatus, posting_metadata::authority::BootRead},
};

fn setup() -> (ShellState, String) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.core.active_account = Some("startup-observations".into());
    crate::app_home::fixture::install(&mut shell);
    let ctx = Context::capture(&shell).unwrap();
    let team = ctx.session.team_id.clone();
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .unified_user_client_enabled
        .insert(team.clone(), false);
    let boot = BootRead::begin(&mut shell.core, &ctx.transport, &ctx.session).unwrap();
    assert!(
        boot.apply(
            &mut shell.core,
            json!({"ok":true,"team":{"id":team,"plan":"plus"},
                "self":{"id":ctx.session.user_id,"team_id":team},
                "workspaces":[{"id":team}],"read_only_channels":[],"thread_only_channels":[]}),
            false,
        )
        .unwrap()
    );
    shell.signed_in = true;
    shell.loading = false;
    shell.connection.routable = true;
    shell.connection.routed_at = Some(Instant::now());
    let (sender, _) = tokio::sync::mpsc::channel(1);
    shell.core.workspaces.get_mut(&team).unwrap().rt =
        RealtimeStatus::Connected(Connection::from_sender(sender));
    ctx.transport.set_health(Health::Online);
    // Startup is independent of any Home view/Share query.
    shell.app_home.surface = None;
    (shell, team)
}

#[test]
fn startup_observations_require_actual_active_online_boot_before_reserving() {
    for missing in 0..7 {
        let (mut shell, team) = setup();
        match missing {
            0 => shell.signed_in = false,
            1 => shell.loading = true,
            2 => shell.connection.routed_at = None,
            3 => shell.connection.routable = false,
            4 => shell
                .core
                .transport
                .as_ref()
                .unwrap()
                .set_health(Health::Offline),
            5 => shell.core.workspaces.get_mut(&team).unwrap().rt = RealtimeStatus::Disconnected,
            _ => shell
                .core
                .workspaces
                .get_mut(&team)
                .unwrap()
                .posting_metadata
                .invalidate(),
        }
        assert!(
            Original::capture(&mut shell, &team).is_none(),
            "case {missing}"
        );
    }
    let (mut shell, team) = setup();
    assert!(Original::capture(&mut shell, "OTHER").is_none());
    let original = Original::capture(&mut shell, &team).unwrap();
    assert!(original.current(&shell));
    assert!(Original::capture(&mut shell, &team).is_none());
}

#[test]
fn startup_observations_original_does_not_follow_replaced_account_socket_or_self() {
    for change in 0..7 {
        let (mut shell, team) = setup();
        let original = Original::capture(&mut shell, &team).unwrap();
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_account = Some("successor".into()),
            2 => shell.core.session.as_mut().unwrap().d_cookie.push('x'),
            3 => shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            4 => shell.app_home.workflow_names_revision += 1,
            5 => {
                let w = shell.core.workspaces.get_mut(&team).unwrap();
                w.users.get_mut(&w.self_user_id).unwrap().name = Some("changed self".into());
            }
            _ => shell.connection.routable = false,
        }
        assert!(!original.current(&shell), "change {change}");
    }
}

#[test]
fn startup_observations_pending_publication_does_not_replace_schedule_lease() {
    let (mut shell, team) = setup();
    let original = Original::capture(&mut shell, &team).unwrap();
    let pending = observations::Pending::capture(
        &mut shell,
        original.context.clone(),
        original.authority.clone(),
    )
    .unwrap();
    assert!(pending.current(&shell));
    assert!(original.current(&shell));
    assert!(Original::capture(&mut shell, &team).is_none());
}
