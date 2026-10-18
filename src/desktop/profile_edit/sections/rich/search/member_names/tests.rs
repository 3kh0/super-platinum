use super::*;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use super_platinum_core::{
    config::{Session, WorkspaceSession},
    slack::Transport,
    state::Workspace,
};
const TEAM: &str = "TNAMES001";
fn setup() -> (ShellState, Arc<Transport>, WorkspaceSession) {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let owner = WorkspaceSession {
        team_id: TEAM.into(),
        user_id: "UNAMES001".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "synthetic".into(),
    };
    let transport = Arc::new(Transport::disconnected_fixture("synthetic"));
    shell.core.active_account = Some("A".into());
    shell.core.active_team = Some(TEAM.into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::from([(TEAM.into(), true)]),
        workspaces: BTreeMap::from([(TEAM.into(), owner.clone())]),
    });
    shell.core.transport = Some(transport.clone());
    shell
        .core
        .workspaces
        .insert(TEAM.into(), Workspace::from_session(&owner));
    (shell, transport, owner)
}
fn boot(owner: &WorkspaceSession) -> Value {
    json!({"ok":true,"team":{"id":owner.team_id},"self":{"id":owner.user_id,"team_id":owner.team_id},
        "workspaces":[{"id":owner.team_id}],"read_only_channels":[],"thread_only_channels":[]})
}
fn acquired() -> (ShellState, Arc<Transport>, WorkspaceSession, Authority) {
    let (mut shell, transport, owner) = setup();
    assert!(Authority::capture(&shell.core, TEAM).is_err());
    let read = BootRead::begin(&mut shell.core, &transport, &owner).unwrap();
    assert!(read.apply(&mut shell.core, boot(&owner), false).unwrap());
    let authority = Authority::capture(&shell.core, TEAM).unwrap();
    (shell, transport, owner, authority)
}
fn policy(shell: &ShellState, authority: &Authority, prefs: Value) -> policy::Snapshot {
    let (membership, enterprise) = authority
        .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
        .unwrap();
    assert!(enterprise.is_none());
    policy::Snapshot::new(
        policy::OrdinaryClientV2,
        membership,
        policy::UserPrefs::decode(&prefs).unwrap(),
        None,
    )
    .unwrap()
}
fn subscribed(shell: ShellState) -> (VirtualDom, Signal<ShellState>, Rc<Cell<usize>>) {
    type Props = (
        Rc<RefCell<Option<ShellState>>>,
        Rc<Cell<Option<Signal<ShellState>>>>,
        Rc<Cell<usize>>,
    );
    let initial = Rc::new(RefCell::new(Some(shell)));
    let slot = Rc::new(Cell::new(None));
    let renders = Rc::new(Cell::new(0));
    let mut dom = VirtualDom::new_with_props(
        |(initial, slot, renders): Props| {
            let state = use_signal(|| initial.borrow_mut().take().unwrap());
            slot.set(Some(state));
            renders.set(renders.get() + 1);
            let epoch = state.read().core.account_epoch;
            rsx! {span {"{epoch}"}}
        },
        (initial, slot.clone(), renders.clone()),
    );
    dom.rebuild_in_place();
    (dom, slot.get().unwrap(), renders)
}

#[test]
fn bound_boot_without_membership_requires_actual_name_policy_reacquisition() {
    let (mut shell, transport, owner) = setup();
    let mut incomplete = boot(&owner);
    incomplete.as_object_mut().unwrap().remove("workspaces");
    let read = BootRead::begin(&mut shell.core, &transport, &owner).unwrap();
    assert!(
        read.apply(&mut shell.core, incomplete.clone(), false)
            .unwrap()
    );
    let posting = Authority::capture(&shell.core, TEAM).unwrap();
    assert!(posting.current(&shell.core));
    assert!(name_authority(&shell, TEAM).is_none());

    // Another successful but incomplete boot cannot satisfy the name purpose.
    let read = BootRead::begin(&mut shell.core, &transport, &owner).unwrap();
    assert!(read.apply(&mut shell.core, incomplete, false).unwrap());
    assert!(name_authority(&shell, TEAM).is_none());

    let read = BootRead::begin(&mut shell.core, &transport, &owner).unwrap();
    assert!(read.apply(&mut shell.core, boot(&owner), false).unwrap());
    let names = name_authority(&shell, TEAM).unwrap();
    let (membership, _) = names
        .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
        .unwrap();
    assert_eq!(membership.workspace_ids().collect::<Vec<_>>(), [TEAM]);
    assert!(!posting.current(&shell.core));
}

#[test]
fn purpose_ready_fast_path_retains_authority_and_does_not_start_a_boot() {
    let (shell, _, _, authority) = acquired();
    let selected = name_authority(&shell, TEAM).unwrap();
    assert!(authority.current(&shell.core));
    assert!(selected.current(&shell.core));
    // Repeated acquisition eligibility is a read, preserving the same owner.
    assert!(name_authority(&shell, TEAM).is_some());
    assert!(authority.current(&shell.core));
}

#[test]
fn actual_boot_membership_crosschecks_gantry_not_credential_inventory() {
    let (shell, _, owner, authority) = acquired();
    let (membership, _) = authority
        .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
        .unwrap();
    let expected = membership
        .workspace_ids()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut raw = boot(&owner);
    raw["default_workspace"] = json!(TEAM);
    assert_eq!(
        facts::validate_gantry(&raw, &owner, Some(&expected)).unwrap(),
        Some(TEAM.into())
    );
    raw["workspaces"] = json!([]);
    assert!(facts::validate_gantry(&raw, &owner, Some(&expected)).is_err());
    raw = boot(&owner);
    raw["self"]["id"] = json!("UFOREIGN1");
    assert!(facts::validate_gantry(&raw, &owner, Some(&expected)).is_err());
}

#[test]
fn missing_team_map_is_unavailable_while_actual_complete_empty_map_is_observed() {
    let (shell, _, owner, authority) = acquired();
    let mut policy = policy(&shell, &authority, json!({"ok":true,"prefs":{}}));
    let mut raw = boot(&owner);
    observe_gantry_prefs(&mut policy, &raw).unwrap();
    assert_eq!(policy.missing_team_preferences(), [TEAM]);
    assert!(policy.should_display_real_names().is_err());
    raw["workspaces"][0]["prefs"] = json!({});
    observe_gantry_prefs(&mut policy, &raw).unwrap();
    assert!(policy.missing_team_preferences().is_empty());
    assert_eq!(policy.should_display_real_names(), Ok(false));
}

#[test]
fn user_override_and_actual_team_preferences_keep_existing_policy_semantics() {
    let (shell, _, owner, authority) = acquired();
    let mut raw = boot(&owner);
    raw["workspaces"][0]["prefs"] = json!({"display_real_names":true});
    let mut team = policy(&shell, &authority, json!({"ok":true,"prefs":{}}));
    observe_gantry_prefs(&mut team, &raw).unwrap();
    assert_eq!(team.should_display_real_names(), Ok(true));
    let explicit = policy(
        &shell,
        &authority,
        json!({"ok":true,"prefs":{"display_real_names_override":-1}}),
    );
    assert_eq!(explicit.should_display_real_names(), Ok(false));
    let explicit = policy(
        &shell,
        &authority,
        json!({"ok":true,"prefs":{"display_real_names_override":1}}),
    );
    assert_eq!(explicit.should_display_real_names(), Ok(true));
    let mut malformed = policy(&shell, &authority, json!({"ok":true,"prefs":{}}));
    raw["workspaces"][0]["prefs"] = json!(false);
    assert!(observe_gantry_prefs(&mut malformed, &raw).is_err());
}

#[test]
fn retained_authority_refuses_account_cookie_transport_and_replaced_boot() {
    for change in 0..4 {
        let (mut shell, transport, owner, authority) = acquired();
        match change {
            0 => shell.core.active_account = Some("B".into()),
            1 => shell.core.session.as_mut().unwrap().d_cookie = "rotated".into(),
            2 => {
                shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")))
            }
            3 => {
                let read = BootRead::begin(&mut shell.core, &transport, &owner).unwrap();
                assert!(read.apply(&mut shell.core, boot(&owner), false).unwrap());
            }
            _ => unreachable!(),
        }
        assert!(!authority.current(&shell.core));
        assert!(
            authority
                .name_policy_membership(&shell.core, policy::OrdinaryClientV2)
                .is_err()
        );
    }
}

#[test]
fn canceled_pending_boot_is_silent_and_cannot_publish_authority() {
    let (mut shell, transport, owner) = setup();
    let read = BootRead::begin(&mut shell.core, &transport, &owner).unwrap();
    let (mut dom, state, renders) = subscribed(shell);
    let mut pending = PendingBoot::new(state, read);
    assert!(pending.cancel());
    assert!(!pending.cancel());
    dom.render_immediate_to_vec();
    assert_eq!(renders.get(), 1);
    assert!(Authority::capture(&state.peek().core, TEAM).is_err());
}

#[test]
fn dropped_old_pending_boot_cannot_settle_successor_nonce() {
    let (mut shell, transport, owner) = setup();
    let old = BootRead::begin(&mut shell.core, &transport, &owner).unwrap();
    let next = BootRead::begin(&mut shell.core, &transport, &owner).unwrap();
    let (mut dom, state, renders) = subscribed(shell);
    drop(PendingBoot::new(state, old));
    assert!(next.current(&state.peek().core));
    dom.render_immediate_to_vec();
    assert_eq!(renders.get(), 1);
}
