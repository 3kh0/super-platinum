//! Actual request receipt ownership and TTL are independent of native projection.
use super::*;
use serde_json::json;
use super_platinum_core::slack::transport::ChannelReadAttempt;
const ID: &str = "CINIT0001";
fn prepared() -> (ShellState, Owner) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    super::super::fixture::install(&mut s);
    let team = s.core.active_team.clone().unwrap();
    let ctx = Arc::new(Context::capture(&s, &team).unwrap());
    let facts = Facts {
        self_id: ctx.session.user_id.clone(),
        current_team: team.clone(),
        omit_team: team.clone(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::new(),
    };
    let w = s.core.workspaces.get_mut(&team).unwrap();
    let p = w
        .directory_channels
        .prepare(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels,
            facts.clone(),
        )
        .unwrap();
    let reply = RendererReply {
        generation: p.generation().into(),
        fields: p.fields().to_vec(),
        values: p.fields().iter().map(|s| s.to_ascii_lowercase()).collect(),
    };
    let (store, native) = w
        .directory_channels
        .finish(
            p,
            &facts,
            reply,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels,
        )
        .unwrap()
        .into_parts();
    w.directory_channels = store;
    w.channels = native;
    let owner = initial::begin(
        &mut s,
        ctx.clone(),
        Arc::new(members::Transformations::decode(&json!({"ok":true,"assignments":{}})).unwrap()),
        true,
        false,
    )
    .unwrap();
    // Isolate the receipt scheduler after the actual source store is mounted.
    initial::retire(&mut s, &owner);
    crate::app_home::cfm_lifecycle::initialized(&mut s, &ctx);
    ensure(&mut s, ctx).unwrap();
    (s, owner)
}
fn attempted(s: &mut ShellState, owner: &Owner) -> Attempt {
    let observation = facts(s, &owner.context).unwrap();
    let at = now(s).unwrap();
    let r = s
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&owner.context.key())
        .unwrap();
    for _ in 0..2 {
        r.manager
            .maybe_refetch(&r.scope, at, observation, ID, false)
            .unwrap();
    }
    assert_eq!(r.manager.pending(), [ID]);
    let wake = arm(s, owner).unwrap();
    match worker::begin(s, &wake).unwrap() {
        worker::Admission::Ready(a) => a,
        _ => panic!("ready source must admit"),
    }
}
#[test]
fn empty_initial_manager_does_not_blanket_certify_inventory_and_duplicate_hits_coalesce() {
    let (mut s, owner) = prepared();
    assert!(fresh_ids(&s, &owner).unwrap().is_empty());
    let a = attempted(&mut s, &owner);
    assert_eq!(a.batch.ids(), [ID]);
    let progress = read::Progress::default();
    let request = ChannelReadAttempt::generic(
        &owner.context.client,
        &owner.context.session,
        &BTreeMap::from([(ID.into(), 0)]),
    );
    progress.register(&[ID.into()], request.receipt());
    assert!(progress.attempted(&a.batch).is_empty());
    worker::complete(&mut s, &a, &[]).unwrap();
    assert!(fresh_ids(&s, &owner).unwrap().is_empty());
    assert_eq!(
        s.app_home.channel_fetches.conditional.slots[&owner.context.key()]
            .manager
            .pending(),
        [ID]
    );
}
#[test]
fn source_only_publication_retires_original_receipt_without_native_revision_change() {
    let (mut s, owner) = prepared();
    let a = attempted(&mut s, &owner);
    let old_revision = s.core.workspaces[&owner.context.session.team_id]
        .channel_lifecycle
        .revision();
    let observed = observe(&s, &owner.context).unwrap();
    let ctx = &owner.context;
    let rows = vec![json!({"id":ID,"name":"newer-source"})];
    let f = members::receipt(&s, &owner, &rows).unwrap().facts;
    let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    let accepted = w
        .directory_channels
        .capture_receipt_ingress(
            &observed,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels,
            Origin::Direct,
            rows,
            f,
        )
        .unwrap();
    w.directory_channels
        .enqueue_ingress(accepted, &ctx.account, &ctx.session, ctx.epoch)
        .unwrap();
    advance(&mut s, &owner).unwrap();
    assert_eq!(
        s.core.workspaces[&ctx.session.team_id]
            .channel_lifecycle
            .revision(),
        old_revision
    );
    assert!(owner.publication_current(&s));
    assert!(!a.current(&s));
    worker::complete(&mut s, &a, &[ID.into()]).unwrap();
    assert!(fresh_ids(&s, &owner).unwrap().is_empty());
}
#[test]
fn attempted_failure_marks_predecessor_fresh_but_topic_retains_exact_source() {
    let (mut s, owner) = prepared();
    let a = attempted(&mut s, &owner);
    let before = observe(&s, &owner.context).unwrap();
    worker::complete(&mut s, &a, &[ID.into()]).unwrap();
    assert_eq!(fresh_ids(&s, &owner).unwrap(), [ID]);
    stale(&mut s, &owner, &[ID.into()]).unwrap();
    assert!(fresh_ids(&s, &owner).unwrap().is_empty());
    assert!(observed_current(&s, &owner.context, &before));
    assert_eq!(observe(&s, &owner.context).unwrap().rows(), before.rows());
}
#[test]
fn same_team_transport_or_account_epoch_replacement_refuses_completion() {
    for replace_transport in [false, true] {
        let (mut s, owner) = prepared();
        let a = attempted(&mut s, &owner);
        if replace_transport {
            s.core.transport = Some(Arc::new(Transport::disconnected_fixture(
                "replacement-cookie",
            )));
        } else {
            s.core.account_epoch += 1;
        }
        assert!(!a.current(&s));
        assert!(!a.accounting_current(&s));
        assert!(worker::complete(&mut s, &a, &[ID.into()]).is_err());
    }
}

mod pending;
