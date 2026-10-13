use super::*;
use serde_json::json;
use std::sync::Arc;
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
};
use tokio::sync::mpsc;

fn file() -> File {
    serde_json::from_value(
        json!({"id":"FL1","mode":"list","title":"Plan","list_metadata":{"schema":[]}}),
    )
    .unwrap()
}
fn shell() -> ShellState {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.core.workspaces[&team].self_user_id.clone();
    shell.self_account.user_id = user.clone();
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        workspaces: [(
            team.clone(),
            WorkspaceSession {
                team_id: team,
                user_id: user,
                enterprise_id: None,
                name: "Fixture".into(),
                url: "https://example.test".into(),
                token: "synthetic".into(),
            },
        )]
        .into(),
    });
    shell.core.transport = Some(Arc::new(Transport::new("synthetic").unwrap()));
    assert!(open_shell(&mut shell, file()));
    shell
}
fn socket(shell: &mut ShellState, capacity: usize) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel(capacity);
    let team = shell.core.active_team.clone().unwrap();
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    workspace.rt_generation += 1;
    workspace.rt = RealtimeStatus::Connected(realtime::Connection::from_sender(tx));
    rx
}
fn one(shell: &mut ShellState, now: u64, kind: Kind) -> Request {
    let mut requests = plan(shell, true, now);
    assert_eq!(requests.len(), 1);
    let request = requests.pop().unwrap();
    assert_eq!(request.kind, kind);
    request
}
fn lease(shell: &ShellState) -> &Lease {
    shell.slack_lists.viewing.entries.values().next().unwrap()
}
fn opened(shell: &mut ShellState, now: u64) {
    let request = one(shell, now, Kind::Open);
    finish(shell, &request, Ok(true), now);
}
fn frame(rx: &mut mpsc::Receiver<String>, name: &str) {
    let value: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(value["type"], name);
    assert_eq!(value["channel_id"], "FL1");
}
#[test]
fn open_ping_focus_and_explicit_false_stop() {
    let mut shell = shell();
    let mut rx = socket(&mut shell, 8);
    let open = one(&mut shell, 100, Kind::Open);
    assert!(plan(&mut shell, true, 101).is_empty());
    assert!(rx.try_recv().is_err());
    finish(&mut shell, &open, Ok(true), 100);
    assert!(plan(&mut shell, true, 100).is_empty());
    frame(&mut rx, "temp_channel_sub");
    assert!(plan(&mut shell, true, 30_099).is_empty());
    let requests = plan(&mut shell, false, 30_100);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].kind, Kind::Ping { focused: false });
    finish(&mut shell, &requests[0], Ok(false), 30_101);
    assert!(plan(&mut shell, true, 30_101).is_empty());
    frame(&mut rx, "temp_channel_unsub");
    assert!(plan(&mut shell, true, 900_000).is_empty());
}
#[test]
fn full_queue_retries_and_closed_queue_never_counts_as_subscribed() {
    let mut shell = shell();
    let mut rx = socket(&mut shell, 1);
    let team = shell.core.active_team.clone().unwrap();
    if let RealtimeStatus::Connected(connection) = &shell.core.workspaces[&team].rt {
        connection.try_send("occupied".into()).unwrap();
    }
    opened(&mut shell, 0);
    plan(&mut shell, true, 1);
    assert_eq!(lease(&shell).queued_generation, None);
    assert_eq!(rx.try_recv().unwrap(), "occupied");
    plan(&mut shell, true, 999);
    assert!(rx.try_recv().is_err());
    plan(&mut shell, true, 1001);
    frame(&mut rx, "temp_channel_sub");
    assert!(lease(&shell).queued_generation.is_some());
    let rx = socket(&mut shell, 1);
    drop(rx);
    plan(&mut shell, true, 1002);
    assert_eq!(lease(&shell).queued_generation, None);
}
#[test]
fn close_waits_for_open_and_releases_only_after_delay() {
    let mut shell = shell();
    let mut rx = socket(&mut shell, 8);
    opened(&mut shell, 0);
    plan(&mut shell, true, 0);
    frame(&mut rx, "temp_channel_sub");
    close_shell(&mut shell);
    let close = one(&mut shell, 1, Kind::Close);
    assert!(plan(&mut shell, true, 2).is_empty());
    finish(&mut shell, &close, Ok(false), 3);
    plan(&mut shell, true, 60_002);
    assert!(rx.try_recv().is_err());
    plan(&mut shell, true, 60_003);
    frame(&mut rx, "temp_channel_unsub");
    assert!(shell.slack_lists.viewing.entries.is_empty());
    assert!(plan(&mut shell, true, 900_000).is_empty());
}
#[test]
fn late_open_after_close_gets_cleanup_and_no_subscription() {
    let mut shell = shell();
    let mut rx = socket(&mut shell, 8);
    let open = one(&mut shell, 0, Kind::Open);
    close_shell(&mut shell);
    assert!(plan(&mut shell, true, 1).is_empty());
    finish(&mut shell, &open, Ok(true), 2);
    let close = one(&mut shell, 3, Kind::Close);
    assert!(rx.try_recv().is_err());
    finish(&mut shell, &close, Ok(false), 4);
    plan(&mut shell, true, 5);
    assert!(shell.slack_lists.viewing.entries.is_empty());
}
#[test]
fn reopen_serializes_close_then_new_open_without_old_delayed_release() {
    let mut shell = shell();
    let mut rx = socket(&mut shell, 8);
    opened(&mut shell, 0);
    plan(&mut shell, true, 0);
    frame(&mut rx, "temp_channel_sub");
    close_shell(&mut shell);
    let close = one(&mut shell, 1, Kind::Close);
    assert!(open_shell(&mut shell, file()));
    assert!(plan(&mut shell, true, 2).is_empty());
    finish(&mut shell, &close, Ok(false), 3);
    let open = one(&mut shell, 4, Kind::Open);
    assert!(rx.try_recv().is_err());
    finish(&mut shell, &close, Ok(false), 5);
    assert!(lease(&shell).in_flight.is_some());
    finish(&mut shell, &open, Ok(true), 6);
    plan(&mut shell, true, 7);
    assert!(lease(&shell).queued_generation.is_some());
    assert!(rx.try_recv().is_err());
    assert!(lease(&shell).close_due.is_none());
}
#[test]
fn close_reopen_between_ticks_renews_even_previously_refused_open() {
    let mut shell = shell();
    let _rx = socket(&mut shell, 8);
    let open = one(&mut shell, 0, Kind::Open);
    finish(&mut shell, &open, Ok(false), 1);
    close_shell(&mut shell);
    assert!(open_shell(&mut shell, file()));
    let close = one(&mut shell, 2, Kind::Close);
    finish(&mut shell, &close, Ok(false), 3);
    let open = one(&mut shell, 4, Kind::Open);
    finish(&mut shell, &open, Ok(true), 5);
    assert!(lease(&shell).eligible);
}
#[test]
fn reconnect_resubscribes_only_eligible_lease() {
    let mut shell = shell();
    let mut old = socket(&mut shell, 8);
    opened(&mut shell, 0);
    plan(&mut shell, true, 0);
    frame(&mut old, "temp_channel_sub");
    let mut next = socket(&mut shell, 8);
    assert!(plan(&mut shell, true, 1).is_empty());
    frame(&mut next, "temp_channel_sub");
    let ping = one(&mut shell, 30_000, Kind::Ping { focused: true });
    finish(
        &mut shell,
        &ping,
        Err(Error::Api("file_not_found".into())),
        30_001,
    );
    plan(&mut shell, true, 30_001);
    frame(&mut next, "temp_channel_unsub");
    let mut latest = socket(&mut shell, 8);
    assert!(plan(&mut shell, true, 90_000).is_empty());
    assert!(latest.try_recv().is_err());
}
#[test]
fn transient_open_failure_retries_with_throttle_and_no_assumed_subscription() {
    let mut shell = shell();
    let mut rx = socket(&mut shell, 8);
    let open = one(&mut shell, 0, Kind::Open);
    finish(
        &mut shell,
        &open,
        Err(Error::RateLimited {
            retry_after_secs: Some(120),
        }),
        10,
    );
    assert!(!lease(&shell).eligible);
    assert!(plan(&mut shell, true, 120_009).is_empty());
    assert!(rx.try_recv().is_err());
    let open = one(&mut shell, 120_010, Kind::Open);
    finish(&mut shell, &open, Ok(true), 120_011);
    plan(&mut shell, true, 120_012);
    frame(&mut rx, "temp_channel_sub");
}
#[test]
fn old_workspace_cleanup_retains_scope_but_account_replacement_rejects_completion() {
    let mut shell = shell();
    let _rx = socket(&mut shell, 8);
    opened(&mut shell, 0);
    shell.core.active_team = None;
    poll(&mut shell);
    let close = one(&mut shell, 1, Kind::Close);
    assert!(close.scope.request_in_account(&shell).is_some());
    assert!(!close.scope.current(&shell));
    shell.core.account_epoch += 1;
    assert!(plan(&mut shell, true, 2).is_empty());
    finish(&mut shell, &close, Ok(false), 3);
    assert!(shell.slack_lists.viewing.entries.is_empty());
}

#[test]
fn unsent_open_is_cancelled_after_close_without_creating_a_close_request() {
    let mut shell = shell();
    let _rx = socket(&mut shell, 8);
    let open = one(&mut shell, 0, Kind::Open);
    assert!(open.dispatchable(&shell));
    close_shell(&mut shell);
    assert!(!open.dispatchable(&shell));
    open.cancel_unsent(&mut shell);
    assert!(plan(&mut shell, true, 1).is_empty());
    assert!(shell.slack_lists.viewing.entries.is_empty());
}
#[test]
fn old_visit_ping_cannot_dispatch_after_reopen_but_cleanup_can() {
    let mut shell = shell();
    let _rx = socket(&mut shell, 8);
    opened(&mut shell, 0);
    let ping = one(&mut shell, 30_000, Kind::Ping { focused: true });
    close_shell(&mut shell);
    assert!(open_shell(&mut shell, file()));
    assert!(ping.current(&shell));
    assert!(!ping.dispatchable(&shell));
    ping.cancel_unsent(&mut shell);
    let close = one(&mut shell, 30_001, Kind::Close);
    assert!(close.dispatchable(&shell));
    finish(&mut shell, &close, Ok(false), 30_002);
    let open = one(&mut shell, 30_003, Kind::Open);
    assert!(open.dispatchable(&shell));
    shell.core.account_epoch += 1;
    assert!(!open.dispatchable(&shell));
    assert!(!close.dispatchable(&shell));
}

#[test]
fn permanent_open_errors_stop_until_a_new_visit() {
    for error in [
        Error::Api("invalid_auth".into()),
        Error::Api("access_denied".into()),
        Error::Api("malformed_file_viewing_response".into()),
        Error::HttpStatus {
            status: 403,
            retry_after_secs: None,
        },
    ] {
        let mut shell = shell();
        let mut rx = socket(&mut shell, 8);
        let open = one(&mut shell, 0, Kind::Open);
        finish(&mut shell, &open, Err(error), 1);
        assert!(!lease(&shell).eligible);
        assert!(!lease(&shell).failed_open);
        assert!(plan(&mut shell, true, 900_000).is_empty());
        assert!(rx.try_recv().is_err());
        close_shell(&mut shell);
        assert!(open_shell(&mut shell, file()));
        let close = one(&mut shell, 900_001, Kind::Close);
        finish(&mut shell, &close, Ok(false), 900_002);
        assert!(one(&mut shell, 900_003, Kind::Open).dispatchable(&shell));
    }
}
#[test]
fn transient_open_failures_retry_without_granting_eligibility() {
    for error in [
        Error::Offline("offline".into()),
        Error::Transport("reset".into()),
        Error::HttpStatus {
            status: 503,
            retry_after_secs: None,
        },
        Error::HttpStatus {
            status: 408,
            retry_after_secs: None,
        },
    ] {
        let mut shell = shell();
        let mut rx = socket(&mut shell, 8);
        let open = one(&mut shell, 0, Kind::Open);
        finish(&mut shell, &open, Err(error), 1);
        assert!(!lease(&shell).eligible);
        assert!(lease(&shell).failed_open);
        assert!(plan(&mut shell, true, 30_000).is_empty());
        assert!(rx.try_recv().is_err());
        assert!(one(&mut shell, 30_001, Kind::Open).dispatchable(&shell));
    }
}

#[test]
fn renew_close_completion_after_dialog_close_still_releases_subscription() {
    let mut shell = shell();
    let mut rx = socket(&mut shell, 8);
    opened(&mut shell, 0);
    plan(&mut shell, true, 0);
    frame(&mut rx, "temp_channel_sub");
    close_shell(&mut shell);
    assert!(open_shell(&mut shell, file()));
    let close = one(&mut shell, 1, Kind::Close);
    assert!(lease(&shell).desired.is_some());
    // No planner tick occurs between the user closing and HTTP completion.
    close_shell(&mut shell);
    finish(&mut shell, &close, Ok(false), 2);
    assert!(plan(&mut shell, true, 3).is_empty());
    assert!(plan(&mut shell, true, 60_001).is_empty());
    assert!(rx.try_recv().is_err());
    assert!(plan(&mut shell, true, 60_002).is_empty());
    frame(&mut rx, "temp_channel_unsub");
    assert!(shell.slack_lists.viewing.entries.is_empty());
}
