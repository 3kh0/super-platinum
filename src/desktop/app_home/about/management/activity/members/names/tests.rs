use super::*;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn subscribed_shell(shell: ShellState) -> (VirtualDom, Signal<ShellState>, Rc<Cell<usize>>) {
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
fn canceled_boot_retires_only_internal_nonce_without_notifying_subscribers() {
    let (mut shell, visit, query, observation) = acquired();
    let ctx = &query.owner.ctx;
    let read = BootRead::begin(&mut shell.core, &ctx.transport, &ctx.session).unwrap();
    // Native text can retire the caller while its actual boot nonce remains
    // owned. Cleanup must not cause controlled fields to overwrite that text.
    super::super::edit(&mut shell, visit, "uncommitted successor".into());
    assert!(!query.current(&shell));
    assert!(read.current(&shell.core));
    let (mut dom, state, renders) = subscribed_shell(shell);
    let mut pending = PendingBoot::new(state, read);
    assert!(pending.cancel());
    assert!(!pending.cancel());
    dom.render_immediate_to_vec();
    assert_eq!(renders.get(), 1);
    assert!(observation.authority.current(&state.peek().core));
    assert_eq!(
        observed_policy(&state.peek(), visit),
        Some(&observation.policy)
    );
}

#[test]
fn dropped_boot_cleanup_cannot_settle_successor_or_publish_policy() {
    let (mut shell, visit, query, observation) = acquired();
    let ctx = &query.owner.ctx;
    let old = BootRead::begin(&mut shell.core, &ctx.transport, &ctx.session).unwrap();
    let successor = BootRead::begin(&mut shell.core, &ctx.transport, &ctx.session).unwrap();
    let (mut dom, state, renders) = subscribed_shell(shell);
    drop(PendingBoot::new(state, old));
    assert!(successor.current(&state.peek().core));
    dom.render_immediate_to_vec();
    assert_eq!(renders.get(), 1);
    let installed = state.peek();
    assert!(Arc::ptr_eq(
        &document(&installed, visit)
            .unwrap()
            .activity
            .member_names
            .as_ref()
            .unwrap()
            .identity,
        &observation.identity
    ));
}

// Build the policy through the actual accepted BootRead and preference
// decoders. A typed user or management permission snapshot is not name policy.
fn acquired() -> (ShellState, Visit, Query, Observation) {
    let (mut s, visit) = super::super::tests::setup();
    let q = Query::capture(&mut s, visit).unwrap();
    let ctx = &q.owner.ctx;
    let boot = json!({"ok":true,"team":{"id":ctx.session.team_id},
        "self":{"id":ctx.session.user_id,"team_id":ctx.session.team_id},
        "workspaces":[{"id":ctx.session.team_id}],
        "read_only_channels":[],"thread_only_channels":[]});
    let read = BootRead::begin(&mut s.core, &ctx.transport, &ctx.session).unwrap();
    assert!(read.apply(&mut s.core, boot, false).unwrap());
    let authority = Authority::capture(&s.core, &ctx.session.team_id).unwrap();
    let (membership, enterprise) = authority
        .name_policy_membership(&s.core, policy::OrdinaryClientV2)
        .unwrap();
    let raw_user = json!({"ok":true,"prefs":{"display_real_names_override":1}});
    let raw_enterprise = enterprise.as_ref().map(|_| {
        json!({
        "ok":true,"org_prefs":{},"locked_prefs":{}})
    });
    let enterprise_prefs = enterprise
        .as_ref()
        .zip(raw_enterprise.as_ref())
        .map(|(id, raw)| policy::EnterprisePrefs::decode(id, raw).unwrap());
    let policy = policy::Snapshot::new(
        policy::OrdinaryClientV2,
        membership,
        policy::UserPrefs::decode(&raw_user).unwrap(),
        enterprise_prefs,
    )
    .unwrap();
    assert_eq!(policy.should_display_real_names(), Ok(true));
    let gantry = json!({"ok":true,"self":{"id":ctx.session.user_id},
        "team":{"id":ctx.session.team_id},"workspaces":[{"id":ctx.session.team_id}],
        "default_workspace":ctx.session.team_id});
    let default_workspace = validate_gantry(&gantry, &ctx.session, policy.workspace_ids()).unwrap();
    let observation = Observation {
        identity: Arc::new(()),
        owner: q.owner.clone(),
        visit,
        document: q.document,
        revision: s.app_home.workflow_names_revision,
        cookie: s.core.session.as_ref().unwrap().d_cookie.clone(),
        authority,
        policy,
        history: History::default(),
        enterprise,
        raw_user,
        raw_enterprise,
        raw_teams: BTreeMap::new(),
        gantry,
        default_workspace,
    };
    document_mut(&mut s, visit).unwrap().activity.member_names = Some(observation.clone());
    assert!(observation.current(&s, &q));
    (s, visit, q, observation)
}

#[test]
fn acquired_policy_survives_picker_generation_and_filters_without_reusing_query() {
    let (mut s, visit, old, observation) = acquired();
    let raw = observation.raw_user.clone();
    super::super::edit(&mut s, visit, "different search".into());
    super::super::super::edit(&mut s, visit, Field::Status, "ERROR".into());
    assert!(!old.current(&s));
    assert_eq!(observed_policy(&s, visit), Some(&observation.policy));
    let new = Query::capture(&mut s, visit).unwrap();
    assert!(observation.current(&s, &new));
    let installed = document(&s, visit)
        .unwrap()
        .activity
        .member_names
        .as_ref()
        .unwrap();
    assert!(Arc::ptr_eq(&installed.identity, &observation.identity));
    assert_eq!(installed.raw_user, raw);
    assert_eq!(installed.gantry, observation.gantry);
    assert_eq!(installed.default_workspace, observation.default_workspace);
}

#[test]
fn observed_policy_rejects_account_credentials_transport_socket_revision_and_document_changes() {
    for kind in 0..9 {
        let (mut s, visit, q, _) = acquired();
        let team = q.owner.ctx.session.team_id.clone();
        match kind {
            0 => s.core.active_account = Some("replacement-account".into()),
            1 => s.core.account_epoch += 1,
            2 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("-changed"),
            3 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token
                .push_str("-changed"),
            4 => {
                s.core.transport = Some(Arc::new(
                    super_platinum_core::slack::Transport::disconnected_fixture(
                        s.core.session.as_ref().unwrap().d_cookie.clone(),
                    ),
                ))
            }
            5 => s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            6 => s.app_home.workflow_names_revision += 1,
            7 => document_mut(&mut s, visit).unwrap().id = next(),
            8 => s.app_home.about.management.surface.as_mut().unwrap().id = next(),
            _ => unreachable!(),
        }
        assert!(observed_policy(&s, visit).is_none(), "change {kind}");
    }
}

#[test]
fn replacing_actual_boot_authority_retires_installed_name_policy() {
    let (mut s, visit, q, old) = acquired();
    let ctx = &q.owner.ctx;
    let replacement = BootRead::begin(&mut s.core, &ctx.transport, &ctx.session).unwrap();
    assert!(replacement.apply(&mut s.core, json!({"ok":true,
        "team":{"id":ctx.session.team_id},"self":{"id":ctx.session.user_id},
        "workspaces":[{"id":ctx.session.team_id}],"read_only_channels":[],"thread_only_channels":[]}), false).unwrap());
    assert!(!old.authority.current(&s.core));
    assert!(observed_policy(&s, visit).is_none());
}

#[test]
fn detail_reuses_acquired_policy_without_starting_or_changing_picker() {
    let (mut s, visit, q, observation) = acquired();
    let generation = document(&s, visit).unwrap().activity.member_generation;
    let detail = super::super::super::reads::Read::begin(&mut s, visit).unwrap();
    assert!(!q.current(&s));
    assert!(observation.scope_current(&s, Scope::Detail(&detail)));
    assert_eq!(observed_policy(&s, visit), Some(&observation.policy));
    assert_eq!(
        document(&s, visit).unwrap().activity.member_generation,
        generation
    );
    assert!(
        Fence::capture(&s, Scope::Detail(&detail))
            .unwrap()
            .current(&s, Scope::Detail(&detail))
    );
}

#[test]
fn leaving_activity_hides_policy_without_discarding_its_observed_facts() {
    let (mut s, visit, _, observation) = acquired();
    document_mut(&mut s, visit).unwrap().activity.active = false;
    assert!(observed_policy(&s, visit).is_none());
    assert!(document(&s, visit).unwrap().activity.member_names.is_some());
    document_mut(&mut s, visit).unwrap().activity.active = true;
    assert_eq!(observed_policy(&s, visit), Some(&observation.policy));
}

#[test]
fn cold_detail_fence_retires_on_preferences_cookie_lease_and_competing_install() {
    for kind in 0..4 {
        let (mut s, visit, _, observation) = acquired();
        document_mut(&mut s, visit).unwrap().activity.member_names = None;
        assert!(observed_policy(&s, visit).is_none());
        let detail = super::super::super::reads::Read::begin(&mut s, visit).unwrap();
        let scope = Scope::Detail(&detail);
        let fence = Fence::capture(&s, scope).unwrap();
        assert!(fence.current(&s, scope));
        match kind {
            0 => s.app_home.workflow_names_revision += 1,
            1 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("-rotated"),
            2 => s.app_home.about.management.surface.as_mut().unwrap().read = None,
            3 => document_mut(&mut s, visit).unwrap().activity.member_names = Some(observation),
            _ => unreachable!(),
        }
        assert!(!fence.current(&s, scope), "change {kind}");
    }
}

#[test]
fn same_source_replacement_identity_cannot_be_overwritten_by_older_capture() {
    let (mut s, visit, q, old) = acquired();
    let fence = Fence::capture(&s, Scope::Query(&q)).unwrap();
    let mut replacement = old.clone();
    replacement.identity = Arc::new(());
    document_mut(&mut s, visit).unwrap().activity.member_names = Some(replacement.clone());
    assert!(!old.current(&s, &q));
    assert!(!fence.current(&s, Scope::Query(&q)));
    assert!(replacement.current(&s, &q));
    assert_eq!(observed_policy(&s, visit), Some(&replacement.policy));
}

#[test]
fn gantry_names_require_exact_observed_self_team_and_complete_internal_membership() {
    let (mut s, visit) = super::super::tests::setup();
    let q = Query::capture(&mut s, visit).unwrap();
    let session = &q.owner.ctx.session;
    let raw = json!({"ok":true,"self":{"id":session.user_id},"team":{"id":session.team_id},
        "workspaces":[{"id":session.team_id},{"id":"TEXTERNAL","allow_external_users":true}],
        "default_workspace":session.team_id});
    assert_eq!(
        validate_gantry(&raw, session, [session.team_id.as_str()].into_iter()).unwrap(),
        Some(session.team_id.clone())
    );
    assert!(validate_gantry(&raw, session, ["TMISSING"].into_iter()).is_err());
    let mut other = raw.clone();
    other["self"]["id"] = json!("UOTHER123");
    assert!(validate_gantry(&other, session, [session.team_id.as_str()].into_iter()).is_err());
    let mut duplicate = raw.clone();
    duplicate["workspaces"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":session.team_id}));
    assert!(validate_gantry(&duplicate, session, [session.team_id.as_str()].into_iter()).is_err());
    let mut failed = raw.clone();
    failed["error"] = json!("ratelimited");
    assert!(validate_gantry(&failed, session, [session.team_id.as_str()].into_iter()).is_err());
    let mut malformed = raw;
    malformed["default_workspace"] = json!(false);
    assert!(validate_gantry(&malformed, session, [session.team_id.as_str()].into_iter()).is_err());
}
