use super::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use super_platinum_core::{
    block_actions::workflow_management::member_hydration::{Flags, RendererReply},
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::{directory_members::Origin, posting_metadata::authority::BootRead},
};
pub(super) const EDITOR: &str = "UATTR0001";
pub(super) fn setup() -> (ShellState, Comments, Ticket, Arc<()>) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    shell.canvas = Some(crate::state::CanvasTarget {
        id: "Ffixture".into(),
        title: "Fixture".into(),
        permalink: None,
    });
    shell.overlay = Some(crate::state::Overlay::Canvas);
    let team = shell.core.active_team.clone().unwrap();
    let user = shell.self_account.user_id.clone();
    let session = WorkspaceSession {
        team_id: team.clone(),
        user_id: user.clone(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://fixture.slack.com".into(),
        token: "synthetic".into(),
    };
    let transport = Arc::new(Transport::disconnected_fixture("synthetic"));
    shell.core.active_account = Some("canvas-attribution-test".into());
    shell.core.transport = Some(transport.clone());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::from([(team.clone(), true)]),
        workspaces: BTreeMap::from([(team.clone(), session.clone())]),
    });
    let boot = BootRead::begin(&mut shell.core, &transport, &session).unwrap();
    assert!(
        boot.apply(
            &mut shell.core,
            json!({"ok":true,"team":{"id":team,"plan":"plus"},
        "self":{"id":user,"team_id":team},"workspaces":[{"id":team}],
        "read_only_channels":[],"thread_only_channels":[]}),
            false
        )
        .unwrap()
    );
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    workspace.users.clear();
    workspace
        .directory_members
        .begin("canvas-attribution-test", &session, &BTreeSet::new())
        .unwrap();
    adopt(
        &mut shell,
        json!({"id":EDITOR,"profile":{"display_name":"  Canonical  ","real_name":"Real"},"opaque":{"retained":true}}),
    );
    let root: Message =
        serde_json::from_value(json!({"type":"message","subtype":"document_comment_root",
        "ts":"1719800000.000100","user":"U1","text":"Original body",
        "document_comment":{"thread_id":"quip-original","most_recent_editor":EDITOR}}))
        .unwrap();
    let token = Arc::new(());
    let ticket = Ticket {
        owner: Owner::capture(&shell).unwrap(),
        serial: 3,
        query: Query::Replies(root.ts.clone().unwrap()),
        cursor: None,
        direction: super::super::Direction::First,
    };
    let model = Comments {
        owner: Some(ticket.owner.clone()),
        serial: ticket.serial,
        query: Some(ticket.query.clone()),
        root: Some(root),
        attribution: State {
            token: Some(token.clone()),
            ..Default::default()
        },
        ..Default::default()
    };
    (shell, model, ticket, token)
}
pub(super) fn adopt(shell: &mut ShellState, row: Value) {
    let team = shell.core.active_team.clone().unwrap();
    let session = shell.core.session.as_ref().unwrap().workspaces[&team].clone();
    let account = shell.core.active_account.clone().unwrap();
    let epoch = shell.core.account_epoch;
    let w = shell.core.workspaces.get_mut(&team).unwrap();
    w.directory_members
        .enqueue(&account, &session, Origin::Directory, vec![row], json!({}))
        .unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let pending = w
        .directory_members
        .prepare(
            &account,
            &session,
            epoch,
            "canvas-attribution-test",
            flags.clone(),
            &w.users,
        )
        .unwrap();
    assert!(pending.fields().iter().all(|s| s.is_ascii()));
    let reply = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending
            .fields()
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect(),
    };
    w.users = w
        .directory_members
        .adopt(
            pending,
            reply,
            &flags,
            &account,
            &session,
            epoch,
            "canvas-attribution-test",
            &w.users,
        )
        .unwrap()
        .into_parts()
        .1;
}
#[test]
fn canonical_source_survives_unrelated_member_but_retires_changed_native_or_source() {
    let (mut shell, _, ticket, _) = setup();
    let selected = member::Member::capture(&shell, &ticket.owner, EDITOR).unwrap();
    adopt(
        &mut shell,
        json!({"id":"UATTR0002","profile":{"display_name":"Other"}}),
    );
    assert!(selected.current(&shell));
    let team = shell.core.active_team.clone().unwrap();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut(EDITOR)
        .unwrap()
        .real_name = Some("Native drift".into());
    assert!(!selected.current(&shell));
    assert!(member::Member::capture(&shell, &ticket.owner, EDITOR).is_err());
    let (mut shell, _, ticket, _) = setup();
    let selected = member::Member::capture(&shell, &ticket.owner, EDITOR).unwrap();
    adopt(
        &mut shell,
        json!({"id":EDITOR,"profile":{"display_name":"Changed"}}),
    );
    assert!(!selected.current(&shell));
}
#[test]
fn typed_only_member_and_bot_do_not_create_human_attribution() {
    let (mut shell, _, ticket, _) = setup();
    let team = shell.core.active_team.clone().unwrap();
    shell.core.workspaces.get_mut(&team).unwrap().users.insert(
        "UMISSING1".into(),
        serde_json::from_value(json!({"id":"UMISSING1","profile":{"display_name":"Typed only"}}))
            .unwrap(),
    );
    assert!(member::Member::capture(&shell, &ticket.owner, "UMISSING1").is_err());
    for row in [
        json!({"id":EDITOR,"is_bot":true}),
        json!({"id":EDITOR,"is_app_user":true}),
        json!({"id":EDITOR,"bot_id":"B1"}),
    ] {
        let (mut shell, _, ticket, _) = setup();
        adopt(&mut shell, row);
        assert!(member::Member::capture(&shell, &ticket.owner, EDITOR).is_err());
    }
}
#[test]
fn read_serial_slot_and_original_owner_cannot_be_rebound() {
    let (mut shell, mut model, ticket, token) = setup();
    assert!(slot_current(&shell, &model, &ticket, &token));
    model.attribution.token = Some(Arc::new(()));
    assert!(!slot_current(&shell, &model, &ticket, &token));
    model.attribution.token = Some(token.clone());
    model.next_serial();
    assert!(!slot_current(&shell, &model, &ticket, &token));
    shell.core.account_epoch += 1;
    assert!(!ticket.owner.current(&shell));
}
#[test]
fn preference_events_require_original_canvas_team_and_socket() {
    use super_platinum_core::slack::events::RtEvent;
    let (mut shell, _, ticket, _) = setup();
    let team = ticket.owner.scope.team.clone();
    let socket = shell.core.workspaces[&team].rt_generation;
    let revision = crate::app_home::autocomplete::preferences_revision(&shell);
    let event = RtEvent::PreferencesChanged {
        name: "display_real_names_override".into(),
        value: json!(1),
    };
    assert!(observes_preferences(&shell, &team, socket));
    crate::app_home::realtime(&mut shell, "TOTHER", socket, &event);
    crate::app_home::realtime(&mut shell, &team, socket + 1, &event);
    assert_eq!(
        crate::app_home::autocomplete::preferences_revision(&shell),
        revision
    );
    crate::app_home::realtime(&mut shell, &team, socket, &event);
    assert_eq!(
        crate::app_home::autocomplete::preferences_revision(&shell),
        revision + 1
    );
}
