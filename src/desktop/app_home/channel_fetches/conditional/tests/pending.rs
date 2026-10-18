use super::*;
use super_platinum_core::slack::models::Channel;
use super_platinum_core::state::directory_channels::Store;
fn enqueue(shell: &mut ShellState, owner: &Owner, rows: Vec<Value>) {
    let input = source_values(shell, owner, &rows).unwrap();
    let observed = observe(shell, &owner.context).unwrap();
    let facts = members::receipt(shell, owner, &rows).unwrap().facts;
    let ctx = &owner.context;
    let w = shell.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    let ingress = w
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
            facts.clone(),
        )
        .unwrap();
    let boot = ingress.boot_in_flight();
    w.directory_channels
        .enqueue_ingress(ingress, &ctx.account, &ctx.session, ctx.epoch)
        .unwrap();
    let next = source_fresh(shell, owner, input, &facts, boot).unwrap();
    continuation::advance(shell, owner, next).unwrap();
}
fn stage(shell: &ShellState, owner: &Owner) -> (Store, BTreeMap<String, Channel>) {
    let ctx = &owner.context;
    let w = &shell.core.workspaces[&ctx.session.team_id];
    let (mut store, mut native) = (w.directory_channels.clone(), w.channels.clone());
    while store.has_pending() {
        let current = store
            .observation(&ctx.account, &ctx.session, ctx.epoch, GENERATION, &native)
            .unwrap();
        if let Some(commit) = store
            .drain_source_action(
                &current,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &native,
            )
            .unwrap()
        {
            (store, native) = commit.into_parts();
            continue;
        }
        let facts = current.receipt_facts().unwrap().clone();
        let pending = store
            .prepare(
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &native,
                facts.clone(),
            )
            .unwrap();
        let reply = RendererReply {
            generation: pending.generation().into(),
            fields: pending.fields().to_vec(),
            values: pending
                .fields()
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect(),
        };
        (store, native) = store
            .finish(
                pending,
                &facts,
                reply,
                &ctx.account,
                &ctx.session,
                ctx.epoch,
                GENERATION,
                &native,
            )
            .unwrap()
            .into_parts();
    }
    (store, native)
}
#[test]
fn queued_raw_successor_uses_exact_version_and_restores_freshness() {
    let (mut s, owner) = prepared();
    enqueue(
        &mut s,
        &owner,
        vec![json!({"id":ID,"name":"queued-first","updated":102})],
    );
    let ctx = &owner.context;
    let o = observe(&s, ctx).unwrap();
    let w = &s.core.workspaces[&ctx.session.team_id];
    assert!(
        o.classification(
            &w.directory_channels,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels
        )
        .unwrap()
        .conditional_version(ID)
        .is_err()
    );
    assert_eq!(continuation::version(&s, &owner, ID).unwrap(), 102);
    stale(&mut s, &owner, &[ID.into()]).unwrap();
    assert!(fresh_ids(&s, &owner).unwrap().is_empty());
    enqueue(
        &mut s,
        &owner,
        vec![json!({"id":ID,"name":"queued-second","updated":103})],
    );
    assert_eq!(continuation::version(&s, &owner, ID).unwrap(), 103);
    assert_eq!(fresh_ids(&s, &owner).unwrap(), [ID]);
    assert_eq!(
        s.core.workspaces[&ctx.session.team_id].channels[ID]
            .name
            .as_deref(),
        Some("warm-harbor")
    );
}
#[test]
fn accepted_conditional_sibling_shadow_survives_later_sibling_failure() {
    let (mut s, owner) = prepared();
    const OTHER: &str = "COTHER001";
    enqueue(
        &mut s,
        &owner,
        vec![json!({"id":OTHER,"name":"other","is_channel":true,"updated":7})],
    );
    let (store, native) = stage(&s, &owner);
    let w = s
        .core
        .workspaces
        .get_mut(&owner.context.session.team_id)
        .unwrap();
    w.directory_channels = store;
    w.channels = native;
    advance(&mut s, &owner).unwrap();
    continuation::clear(&mut s, &owner);
    stale(&mut s, &owner, &[OTHER.into()]).unwrap();
    let observation = facts(&mut s, &owner.context).unwrap();
    let at = now(&s).unwrap();
    let runtime = s
        .app_home
        .channel_fetches
        .conditional
        .slots
        .get_mut(&owner.context.key())
        .unwrap();
    for id in [ID, OTHER] {
        runtime
            .manager
            .maybe_refetch(&runtime.scope, at, observation, id, false)
            .unwrap();
    }
    let wake = arm(&mut s, &owner).unwrap();
    let a = match worker::begin(&mut s, &wake).unwrap() {
        worker::Admission::Ready(a) => a,
        _ => panic!("ready batch"),
    };
    assert_eq!(a.batch.ids(), [ID, OTHER]);
    publish::rows_in(
        &mut s,
        &a,
        &[json!({"id":ID,"name":"conditional","updated":102})],
        Some(&[OTHER.into()]),
    )
    .unwrap();
    assert_eq!(continuation::version(&s, &owner, ID).unwrap(), 102);
    // A failed sibling does not call the publication sink or rewrite this shadow.
    failure(&mut s, &a, Error::Api("fixture_sibling_failure".into()));
    worker::complete(&mut s, &a, &[ID.into(), OTHER.into()]).unwrap();
    assert_eq!(continuation::version(&s, &owner, OTHER).unwrap(), 7);
    enqueue(
        &mut s,
        &owner,
        vec![json!({"id":ID,"name":"next-source","updated":104})],
    );
    assert_eq!(continuation::version(&s, &owner, ID).unwrap(), 104);
    assert!(!a.current(&s));
    let (store, native) = stage(&s, &owner);
    let ctx = &owner.context;
    let published = store
        .observation(&ctx.account, &ctx.session, ctx.epoch, GENERATION, &native)
        .unwrap();
    assert_eq!(
        published
            .rows()
            .iter()
            .find(|row| row["id"] == OTHER)
            .unwrap()["is_member"],
        true
    );
    assert_eq!(continuation::version(&s, &owner, OTHER).unwrap(), 7);
}
#[test]
fn unchanged_and_auxiliary_action_carries_do_not_reset_version_or_ttl() {
    let (mut s, owner) = prepared();
    enqueue(
        &mut s,
        &owner,
        vec![json!({"id":ID,"name":"queued","updated":102})],
    );
    let next = continuation::capture(&s, &owner);
    let ctx = &owner.context;
    let observed = observe(&s, ctx).unwrap();
    let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
    let (store, native) = w
        .directory_channels
        .queue_resync_unchanged(
            &observed,
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            GENERATION,
            &w.channels,
            &[ID.into()],
        )
        .unwrap()
        .into_parts();
    w.directory_channels = store;
    w.channels = native;
    continuation::advance(&mut s, &owner, next).unwrap();
    stale(&mut s, &owner, &[ID.into()]).unwrap();
    let next = continuation::capture(&s, &owner);
    s.core
        .workspaces
        .get_mut(&ctx.session.team_id)
        .unwrap()
        .channels
        .get_mut(ID)
        .unwrap()
        .is_starred = true;
    continuation::advance(&mut s, &owner, next).unwrap();
    assert_eq!(continuation::version(&s, &owner, ID).unwrap(), 102);
    assert!(fresh_ids(&s, &owner).unwrap().is_empty());
}
#[test]
fn dropped_private_drain_and_final_atomic_sink_never_replay_freshness() {
    let (mut s, owner) = prepared();
    enqueue(
        &mut s,
        &owner,
        vec![json!({"id":ID,"name":"queued","updated":102})],
    );
    let (cancelled_store, cancelled_native) = stage(&s, &owner);
    drop((cancelled_store, cancelled_native)); // timeout/cancellation drops only private staging
    stale(&mut s, &owner, &[ID.into()]).unwrap();
    assert_eq!(continuation::version(&s, &owner, ID).unwrap(), 102);
    let original = observe(&s, &owner.context).unwrap();
    let (old_store, old_native) = stage(&s, &owner);
    enqueue(
        &mut s,
        &owner,
        vec![json!({"id":ID,"name":"newer-pending","updated":103})],
    );
    stale(&mut s, &owner, &[ID.into()]).unwrap();
    let token = Arc::new(());
    s.app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap()
        .drain = Some(token.clone());
    assert!(!drain::publish(&mut s, &owner, &token, &original, old_store, old_native).unwrap());
    assert_eq!(continuation::version(&s, &owner, ID).unwrap(), 103);
    assert!(fresh_ids(&s, &owner).unwrap().is_empty());
    let observed = observe(&s, &owner.context).unwrap();
    let (store, native) = stage(&s, &owner);
    assert!(drain::publish(&mut s, &owner, &token, &observed, store, native).unwrap());
    assert!(continuation::capture(&s, &owner).is_none());
    assert!(fresh_ids(&s, &owner).unwrap().is_empty());
    let published = observe(&s, &owner.context).unwrap();
    let row = published.rows().iter().find(|row| row["id"] == ID).unwrap();
    assert_eq!(row["updated"], 103);
    assert_eq!(row["name"], "newer-pending");
}
#[test]
fn successor_source_rejects_foreign_publication_and_unrepresented_native_drift() {
    for native in [false, true] {
        let (mut s, owner) = prepared();
        enqueue(
            &mut s,
            &owner,
            vec![json!({"id":ID,"name":"queued","updated":102})],
        );
        if native {
            s.core
                .workspaces
                .get_mut(&owner.context.session.team_id)
                .unwrap()
                .channels
                .get_mut(ID)
                .unwrap()
                .name = Some("unrepresented".into());
        } else {
            advance(&mut s, &owner).unwrap();
        } // exact receipt replaced outside accepted continuation
        assert!(continuation::capture(&s, &owner).is_none());
        assert!(source_values(&s, &owner, &[json!({"id":ID,"name":"refused"})]).is_err());
    }
}
#[test]
fn accepted_socket_rebind_preserves_pending_source_but_not_old_http_receipt() {
    let (mut s, owner) = prepared();
    let a = attempted(&mut s, &owner);
    publish::rows_in(
        &mut s,
        &a,
        &[json!({"id":ID,"name":"queued","updated":102})],
        None,
    )
    .unwrap();
    let team = owner.context.session.team_id.clone();
    s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    let generation = s.core.workspaces[&team].rt_generation;
    events::connected(&mut s, &team, generation).unwrap();
    let successor = super::super::owner(&s, &Context::capture(&s, &team).unwrap()).unwrap();
    assert!(!a.current(&s));
    assert!(owner.current(&s) == false);
    assert_eq!(continuation::version(&s, &successor, ID).unwrap(), 102);
    enqueue(
        &mut s,
        &successor,
        vec![json!({"id":ID,"name":"successor","updated":105})],
    );
    assert_eq!(continuation::version(&s, &successor, ID).unwrap(), 105);
    s.core.account_epoch += 1;
    assert!(continuation::capture(&s, &successor).is_none());
    assert!(source_values(&s, &successor, &[json!({"id":ID})]).is_err());
}

#[test]
fn continuation_cannot_cross_credentials_transport_or_account_retirement() {
    for kind in 0..4 {
        let (mut s, owner) = prepared();
        enqueue(
            &mut s,
            &owner,
            vec![json!({"id":ID,"name":"queued","updated":102})],
        );
        match kind {
            0 => s.core.active_account = Some("different-account".into()),
            1 => s.core.account_epoch += 1,
            2 => s.core.transport = Some(Arc::new(Transport::disconnected_fixture("replacement"))),
            _ => s.core.session.as_mut().unwrap().d_cookie = "changed-credentials".into(),
        }
        assert!(continuation::capture(&s, &owner).is_none());
        assert!(source_values(&s, &owner, &[json!({"id":ID,"updated":104})]).is_err());
    }
}
