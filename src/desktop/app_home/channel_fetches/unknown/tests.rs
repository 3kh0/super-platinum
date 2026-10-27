use super::*;
use serde_json::json;
const ID: &str = "CMISSING01";
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
    (s, owner)
}
fn attempted(s: &mut ShellState, owner: &Owner) -> (Arc<Selection>, Attempt) {
    let selected = select(s, &owner.context.session.team_id, ID).unwrap();
    assert!(selected.lookup().is_transient());
    let duplicate = select(s, &owner.context.session.team_id, ID).unwrap();
    assert!(selected.current(s) && duplicate.current(s));
    assert_eq!(
        s.app_home.channel_fetches.unknown.slots[&owner.context.key()]
            .manager
            .pending(),
        &[ID]
    );
    let wake = arm(s, owner).unwrap();
    match worker::begin(s, &wake).unwrap() {
        worker::Admission::Ready(a) => (selected, a),
        _ => panic!("initialized missing selector must begin"),
    }
}
fn drain_source(s: &mut ShellState, owner: &Owner) {
    let ctx = &owner.context;
    while s.core.workspaces[&ctx.session.team_id]
        .directory_channels
        .has_pending()
    {
        let observed = observe(s, ctx).unwrap();
        let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
        if let Some(commit) = w
            .directory_channels
            .drain_source_action(
                &observed,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &w.channels,
            )
            .unwrap()
        {
            (w.directory_channels, w.channels) = commit.into_parts();
            super::super::advance(s, owner).unwrap();
            continue;
        }
        let facts = members::receipt(s, owner, &[]).unwrap().facts;
        let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
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
        super::super::advance(s, owner).unwrap();
    }
}
#[test]
fn missing_selection_is_immediate_deduplicated_and_terminal_requires_owned_queue() {
    let (mut s, owner) = prepared();
    let (selected, a) = attempted(&mut s, &owner);
    assert!(
        observe(&s, &owner.context)
            .unwrap()
            .rows()
            .iter()
            .all(|r| r["id"] != ID)
    );
    assert_eq!(selected.lookup().kind(), LookupKind::Unknown);
    publish::finish(&mut s, &a).unwrap();
    assert!(!selected.current(&s));
    drain_source(&mut s, &owner);
    let terminal = select(&mut s, &owner.context.session.team_id, ID).unwrap();
    assert_eq!(terminal.lookup().kind(), LookupKind::NonExistent);
    assert!(!terminal.lookup().is_transient());
    assert!(terminal.current(&s));
    assert!(publish::finish(&mut s, &a).is_err());
}
#[test]
fn accepted_missing_response_preserves_raw_fields_and_does_not_become_terminal() {
    let (mut s, owner) = prepared();
    let (_, a) = attempted(&mut s, &owner);
    let row = json!({"id":ID,"name":"received-channel","is_channel":true,"is_member":true,"updated":102,"opaque":{"keep":1}});
    publish::rows_in(&mut s, &a, std::slice::from_ref(&row), Some(&[ID.into()])).unwrap();
    assert!(a.current(&s));
    publish::finish(&mut s, &a).unwrap();
    drain_source(&mut s, &owner);
    let hit = select(&mut s, &owner.context.session.team_id, ID).unwrap();
    assert_eq!(hit.lookup().kind(), LookupKind::Known);
    assert_eq!(hit.lookup().row().unwrap()["opaque"], row["opaque"]);
    assert_eq!(hit.lookup().row().unwrap()["name"], "received-channel");
}
#[test]
fn replaced_account_socket_source_and_dispatch_refusal_cannot_terminalize() {
    for kind in ["account", "socket", "source"] {
        let (mut s, owner) = prepared();
        let (selected, a) = attempted(&mut s, &owner);
        match kind {
            "account" => s.core.account_epoch += 1,
            "socket" => {
                s.core
                    .workspaces
                    .get_mut(&owner.context.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
            _ => {
                let ctx = &owner.context;
                let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
                w.directory_channels
                    .enqueue(
                        &ctx.account,
                        &ctx.session,
                        ctx.epoch,
                        Origin::Direct,
                        vec![json!({"id":"CINIT0001","name":"changed"})],
                    )
                    .unwrap();
                super::super::advance(&mut s, &owner).unwrap();
            }
        }
        assert!(!selected.current(&s), "{kind}");
        assert!(!a.current(&s), "{kind}");
        assert!(publish::finish(&mut s, &a).is_err(), "{kind}");
    }
    let (mut s, owner) = prepared();
    let (_, a) = attempted(&mut s, &owner);
    s.app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap()
        .retry_at = Some(Instant::now() + Duration::from_secs(30));
    assert!(a.current(&s));
    assert!(!a.dispatch(&s));
    assert!(a.refused_dispatch.get());
    // A dispatch fence refusal is distinct from a wrapper failure under an
    // unchanged owner; the worker refuses terminal completion in this state.
    assert!(publish::finish(&mut s, &a).is_err());
}

#[test]
fn joined_only_placeholder_is_requested_and_terminalized_without_returned_object() {
    let (mut s, owner) = prepared();
    let (_, a) = attempted(&mut s, &owner);
    assert_eq!(read::requested(&s, &a).unwrap(), [ID]);
    publish::rows_in(&mut s, &a, &[], Some(&[ID.into()])).unwrap();
    assert!(a.shadow.borrow().is_unknown(ID).unwrap());
    assert!(a.accepted_ids.borrow().is_empty());
    assert_eq!(read::requested(&s, &a).unwrap(), [ID]);
    assert!(a.accepted_ids.borrow().is_empty());
    publish::finish(&mut s, &a).unwrap();
    drain_source(&mut s, &owner);
    let hit = select(&mut s, &owner.context.session.team_id, ID).unwrap();
    assert_eq!(hit.lookup().kind(), LookupKind::NonExistent);
    assert!(!hit.lookup().is_transient());
}

#[test]
fn accepted_sibling_survives_timeout_while_joined_only_sibling_remains_unresolved() {
    const SECOND: &str = "CMISSING02";
    let (mut s, owner) = prepared();
    select(&mut s, &owner.context.session.team_id, SECOND).unwrap();
    let selected = select(&mut s, &owner.context.session.team_id, ID).unwrap();
    let wake = arm(&mut s, &owner).unwrap();
    let worker::Admission::Ready(mut a) = worker::begin(&mut s, &wake).unwrap() else {
        panic!("ready two-ID flight");
    };
    assert_eq!(read::requested(&s, &a).unwrap(), [SECOND, ID]);
    // A real partial object counts as returned; a membership-only sibling does not.
    let row = json!({"id":ID,"updated":102});
    publish::rows_in(&mut s, &a, &[row], Some(&[SECOND.into()])).unwrap();
    assert!(!selected.current(&s));
    assert_eq!(
        a.accepted_ids
            .borrow()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [ID]
    );
    let runtime = s
        .app_home
        .channel_fetches
        .unknown
        .slots
        .get_mut(&owner.context.key())
        .unwrap();
    a.flight = runtime
        .manager
        .retry_timeout(&runtime.scope, &a.flight)
        .unwrap()
        .unwrap();
    assert_eq!(a.flight.attempt(), 2);
    assert_eq!(read::requested(&s, &a).unwrap(), [SECOND]);
    assert_eq!(
        a.accepted_ids
            .borrow()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [ID]
    );
    publish::finish(&mut s, &a).unwrap();
    drain_source(&mut s, &owner);
    assert_eq!(
        select(&mut s, &owner.context.session.team_id, ID)
            .unwrap()
            .lookup()
            .kind(),
        LookupKind::Known
    );
    assert_eq!(
        select(&mut s, &owner.context.session.team_id, SECOND)
            .unwrap()
            .lookup()
            .kind(),
        LookupKind::NonExistent
    );
}

#[test]
fn accepted_source_advances_and_drains_when_secondary_freshness_fails() {
    let (mut s, owner) = prepared();
    let (_, a) = attempted(&mut s, &owner);
    let old_publication = a.publication.borrow().clone();
    let row = json!({"id":ID,"name":"received-channel","is_channel":true,"updated":102,"opaque":{"keep":1}});
    publish::rows_with_freshness(&mut s, &a, &[row.clone()], None, |_, _, _, _, _| {
        Err(Error::Api("secondary_freshness_failed".into()))
    })
    .unwrap();
    assert!(!Arc::ptr_eq(&old_publication, &a.publication.borrow()));
    assert!(owner.publication_current(&s) && a.current(&s));
    assert_eq!(
        s.app_home.channel_fetches.slots[&owner.context.key()]
            .errors
            .len(),
        1
    );
    assert!(a.accepted_ids.borrow().contains(ID));
    publish::finish(&mut s, &a).unwrap();
    drain_source(&mut s, &owner);
    let hit = select(&mut s, &owner.context.session.team_id, ID).unwrap();
    assert_eq!(hit.lookup().kind(), LookupKind::Known);
    assert_eq!(hit.lookup().row().unwrap()["opaque"], row["opaque"]);
}
