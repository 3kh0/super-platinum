use super::*;
use serde_json::json;
use super_platinum_core::{config::Session, state::Workspace};

fn core() -> CoreAppState {
    let credential = WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "fixture-token".into(),
    };
    let mut core = CoreAppState::new(Default::default());
    core.active_account = Some("account-one".into());
    core.active_team = Some("T1".into());
    core.workspaces
        .insert("T1".into(), Workspace::from_session(&credential));
    core.workspaces
        .get_mut("T1")
        .unwrap()
        .channels
        .insert("C1".into(), channel("C1", "original"));
    core.transport = Some(Arc::new(Transport::new("fixture-cookie").unwrap()));
    core.session = Some(Session {
        d_cookie: "fixture-cookie".into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [("T1".into(), credential)].into(),
    });
    core
}
fn owner(core: &CoreAppState) -> Owner {
    Owner::capture(
        core,
        core.transport.as_ref().unwrap(),
        &core.session.as_ref().unwrap().workspaces["T1"],
    )
    .unwrap()
}
fn channel(id: &str, name: &str) -> Channel {
    serde_json::from_value(json!({"id":id,"name":name,"is_channel":true,"is_member":true})).unwrap()
}
fn page(channels: Vec<Channel>) -> GenericInfoPage {
    serde_json::from_value(json!({"channels":channels})).unwrap()
}

#[test]
fn switched_account_or_transport_cannot_publish_to_same_team() {
    for changed in 0..7 {
        let mut core = core();
        let owner = owner(&core);
        let read = owner.begin(&core, ["C1"]).unwrap();
        match changed {
            0 => core.active_account = Some("account-two".into()),
            1 => core.account_epoch += 1,
            2 => core.transport = Some(Arc::new(Transport::new("fixture-cookie").unwrap())),
            3 => core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut("T1")
                .unwrap()
                .token
                .push_str("-new"),
            4 => core.session.as_mut().unwrap().d_cookie.push_str("-new"),
            5 => core.workspaces.get_mut("T1").unwrap().self_user_id = "U2".into(),
            _ => {
                core.session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .user_id = "U2".into()
            }
        }
        assert!(!read.refresh(&mut core, page(vec![channel("C1", "stale")])));
        assert!(!read.hydrate(&mut core, vec![channel("C1", "stale")]));
        assert_eq!(
            core.workspaces["T1"].channels["C1"].name.as_deref(),
            Some("original")
        );
        assert!(owner.begin(&core, ["C2"]).is_none());
    }
}

#[test]
fn background_team_keeps_its_owner_but_wrong_request_credentials_are_rejected() {
    let mut core = core();
    let owner = owner(&core);
    core.active_team = Some("T2".into());
    assert!(owner.current(&core));
    let mut credential = core.session.as_ref().unwrap().workspaces["T1"].clone();
    credential.url.push_str("/wrong");
    assert!(Owner::capture(&core, core.transport.as_ref().unwrap(), &credential).is_none());
    let transport = Transport::new("fixture-cookie").unwrap();
    assert!(Owner::capture(&core, &transport, &owner.session).is_none());
}

#[test]
fn foreign_or_duplicate_rows_reject_whole_in_flight_batch() {
    let mut core = core();
    let owner = owner(&core);
    let read = owner.begin(&core, ["C1"]).unwrap();
    for rows in [
        vec![channel("C1", "changed"), channel("C2", "foreign")],
        vec![channel("C1", "one"), channel("C1", "two")],
    ] {
        assert!(!read.refresh(&mut core, page(rows.clone())));
        assert!(!read.hydrate(&mut core, rows));
        assert_eq!(
            core.workspaces["T1"].channels["C1"].name.as_deref(),
            Some("original")
        );
        assert!(!core.workspaces["T1"].channels.contains_key("C2"));
    }
}

#[test]
fn socket_rename_survives_refresh_and_surface_hydration_started_before_it() {
    for hydration in [false, true] {
        let mut core = core();
        let owner = owner(&core);
        let read = owner.begin(&core, ["C1"]).unwrap();
        let event = super_platinum_core::slack::realtime::parse_event(&json!({
            "type":"channel_rename","channel":{"id":"C1","name":"new-socket-name"},"event_ts":"50.1"
        }).to_string()).unwrap();
        assert!(
            core.workspaces
                .get_mut("T1")
                .unwrap()
                .apply_channel_event(&event)
        );
        let rows = vec![channel("C1", "old-http-name")];
        assert!(if hydration {
            read.hydrate(&mut core, rows)
        } else {
            read.refresh(&mut core, page(rows))
        });
        assert_eq!(
            core.workspaces["T1"].channels["C1"].name.as_deref(),
            Some("new-socket-name")
        );
    }
}

#[test]
fn incomplete_hydration_retries_missing_rows_without_erasing_another_batch() {
    let mut core = core();
    let owner = owner(&core);
    let read = owner.begin(&core, ["C1", "C2"]).unwrap();
    core.channel_hydrated.extend([
        ("T1".into(), "C1".into()),
        ("T1".into(), "C2".into()),
        ("T1".into(), "C3".into()),
    ]);
    assert!(read.hydrate(&mut core, vec![channel("C1", "loaded")]));
    assert!(core.channel_hydrated.contains(&("T1".into(), "C1".into())));
    assert!(!core.channel_hydrated.contains(&("T1".into(), "C2".into())));
    assert!(core.channel_hydrated.contains(&("T1".into(), "C3".into())));
    read.retry_hydration(&mut core);
    assert!(!core.channel_hydrated.contains(&("T1".into(), "C1".into())));
    core.channel_hydrated.insert(("T1".into(), "C2".into()));
    core.account_epoch += 1;
    read.retry_hydration(&mut core);
    assert!(core.channel_hydrated.contains(&("T1".into(), "C2".into())));
}
