use super::*;
use serde_json::json;
use super_platinum_core::state::directory_channels::Store;

const OLD: &str = "CINIT0001";
const NEW: &str = "CSEARCH01";
fn prepared() -> (ShellState, Owner) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    super::super::fixture::install(&mut shell);
    let team = shell.core.active_team.clone().unwrap();
    let ctx = Arc::new(Context::capture(&shell, &team).unwrap());
    let facts = Facts {
        self_id: ctx.session.user_id.clone(),
        current_team: team.clone(),
        omit_team: team.clone(),
        retain_public_membership_on_archive: false,
        members: BTreeMap::new(),
    };
    let w = shell.core.workspaces.get_mut(&team).unwrap();
    let pending = w
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
        generation: pending.generation().into(),
        fields: pending.fields().to_vec(),
        values: pending
            .fields()
            .iter()
            .map(|v| v.to_ascii_lowercase())
            .collect(),
    };
    let (store, native) = w
        .directory_channels
        .finish(
            pending,
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
        &mut shell,
        ctx.clone(),
        Arc::new(members::Transformations::decode(&json!({"ok":true,"assignments":{}})).unwrap()),
        true,
        false,
    )
    .unwrap();
    initial::retire(&mut shell, &owner);
    crate::app_home::cfm_lifecycle::initialized(&mut shell, &ctx);
    (shell, owner)
}
fn receipt(shell: &mut ShellState, owner: &Owner) -> Receipt {
    Receipt::capture(shell, &owner.context.session.team_id, &|_| true).unwrap()
}
fn row() -> Value {
    json!({"id":NEW,"name":"directory-result","is_channel":true,"updated":42,"opaque":{"kept":[1,2]}})
}

// Exercise the real renderer-neutral pending/finish contract with ASCII-only
// test fields, then the exact production atomic sink. No second live drainer.
fn staged(shell: &ShellState, owner: &Owner) -> (Store, BTreeMap<String, Channel>) {
    let ctx = &owner.context;
    let w = &shell.core.workspaces[&ctx.session.team_id];
    let (mut store, mut native) = (w.directory_channels.clone(), w.channels.clone());
    while store.has_pending() {
        let observed = store
            .observation(&ctx.account, &ctx.session, ctx.epoch, GENERATION, &native)
            .unwrap();
        let facts = observed.receipt_facts().unwrap().clone();
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
        assert_eq!(pending.origin(), Origin::Directory);
        let reply = RendererReply {
            generation: pending.generation().into(),
            fields: pending.fields().to_vec(),
            values: pending
                .fields()
                .iter()
                .map(|v| {
                    assert!(v.is_ascii());
                    v.to_ascii_lowercase()
                })
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
fn publish(shell: &mut ShellState, owner: &Owner) {
    let observed = observe(shell, &owner.context).unwrap();
    let (store, native) = staged(shell, owner);
    let token = Arc::new(());
    shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap()
        .drain = Some(token.clone());
    assert!(drain::publish(shell, owner, &token, &observed, store, native).unwrap());
    shell
        .app_home
        .channel_fetches
        .slots
        .get_mut(&owner.context.key())
        .unwrap()
        .drain = None;
}

#[test]
fn actual_directory_page_queues_verbatim_then_merges_atomically_without_membership_invention() {
    let (mut shell, owner) = prepared();
    let ctx = &owner.context;
    let before =
        serde_json::to_value(&shell.core.workspaces[&ctx.session.team_id].channels[OLD]).unwrap();
    let wire = vec![
        json!({"id":OLD,"name":"renamed-from-directory","updated":43}),
        row(),
        json!({"id":"GSEARCH01","name":"private-result","is_group":true,"is_private":true}),
    ];
    let page = receipt(&mut shell, &owner);
    page.accept(&mut shell, &wire, &|_| true).unwrap();
    assert!(!page.current(&shell));
    assert_eq!(observe(&shell, ctx).unwrap().incoming(), wire);
    assert!(
        !shell.core.workspaces[&ctx.session.team_id]
            .channels
            .contains_key(NEW)
    );
    assert_eq!(
        serde_json::to_value(&shell.core.workspaces[&ctx.session.team_id].channels[OLD]).unwrap(),
        before
    );
    publish(&mut shell, &owner);
    let catalog = canonical::capture(&mut shell, &ctx.session.team_id).unwrap();
    assert_eq!(
        catalog
            .rows()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![OLD, NEW, "GSEARCH01"]
    );
    assert_eq!(catalog.row(NEW).unwrap()["opaque"], wire[1]["opaque"]);
    assert_eq!(catalog.row(NEW).unwrap()["is_member"], false);
    assert!(
        catalog.row("GSEARCH01").unwrap()["is_private"]
            .as_bool()
            .unwrap()
    );
    let after =
        serde_json::to_value(&shell.core.workspaces[&ctx.session.team_id].channels[OLD]).unwrap();
    assert_eq!(after["name"], "renamed-from-directory");
    for key in [
        "is_member",
        "unread_count",
        "unread_count_display",
        "mention_count",
        "has_unreads",
    ] {
        assert_eq!(after.get(key), before.get(key), "{key}");
    }
}

#[test]
fn stale_query_cannot_accept_but_accepted_shared_queue_survives_query_retirement() {
    let (mut shell, owner) = prepared();
    let page = receipt(&mut shell, &owner);
    let before = observe(&shell, &owner.context).unwrap();
    assert!(page.accept(&mut shell, &[row()], &|_| false).is_err());
    assert!(observed_current(&shell, &owner.context, &before));
    page.accept(&mut shell, &[row()], &|_| true).unwrap();
    // Query closes now. Source acceptance already happened and its shared
    // renderer/publication custody remains intact, independent of delivery.
    publish(&mut shell, &owner);
    assert!(
        canonical::capture(&mut shell, &owner.context.session.team_id)
            .unwrap()
            .row(NEW)
            .is_some()
    );
}

#[test]
fn newer_source_only_page_refuses_original_read_even_without_native_revision_change() {
    let (mut shell, owner) = prepared();
    let old = receipt(&mut shell, &owner);
    let new = receipt(&mut shell, &owner);
    let revision = shell.core.workspaces[&owner.context.session.team_id]
        .channel_lifecycle
        .revision();
    new.accept(&mut shell, &[row()], &|_| true).unwrap();
    assert_eq!(
        shell.core.workspaces[&owner.context.session.team_id]
            .channel_lifecycle
            .revision(),
        revision
    );
    assert!(!old.current(&shell));
    assert!(
        old.accept(
            &mut shell,
            &[json!({"id":NEW,"name":"older-response"})],
            &|_| true
        )
        .is_err()
    );
    assert_eq!(observe(&shell, &owner.context).unwrap().incoming(), [row()]);
}

#[test]
fn original_socket_account_transport_and_assignment_receipts_are_required() {
    for kind in 0..6 {
        let (mut shell, owner) = prepared();
        let page = receipt(&mut shell, &owner);
        let ctx = &owner.context;
        match kind {
            0 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&ctx.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
            1 => shell.core.account_epoch += 1,
            2 => shell.core.active_account = Some("replacement-account".into()),
            3 => {
                shell.core.transport = Some(Arc::new(Transport::disconnected_fixture(
                    "replacement-cookie",
                )))
            }
            4 => shell.core.session.as_mut().unwrap().d_cookie = "replacement-cookie".into(),
            _ => {
                shell
                    .app_home
                    .channel_fetches
                    .slots
                    .get_mut(&ctx.key())
                    .unwrap()
                    .transformations = Arc::new(
                    members::Transformations::decode(&json!({"ok":true,"assignments":{}})).unwrap(),
                )
            }
        }
        assert!(!page.current(&shell), "{kind}");
        assert!(
            page.accept(&mut shell, &[row()], &|_| true).is_err(),
            "{kind}"
        );
        assert!(
            !shell.core.workspaces[&ctx.session.team_id]
                .directory_channels
                .has_pending()
        );
    }
}

#[test]
fn native_drift_or_successor_queue_prevents_old_private_publication() {
    for source_only in [false, true] {
        let (mut shell, owner) = prepared();
        receipt(&mut shell, &owner)
            .accept(&mut shell, &[row()], &|_| true)
            .unwrap();
        let observed = observe(&shell, &owner.context).unwrap();
        let (store, native) = staged(&shell, &owner);
        let token = Arc::new(());
        shell
            .app_home
            .channel_fetches
            .slots
            .get_mut(&owner.context.key())
            .unwrap()
            .drain = Some(token.clone());
        if source_only {
            let later = receipt(&mut shell, &owner);
            later
                .accept(&mut shell, &[json!({"id":NEW,"name":"newer"})], &|_| true)
                .unwrap();
            assert!(!drain::publish(&mut shell, &owner, &token, &observed, store, native).unwrap());
        } else {
            shell
                .core
                .workspaces
                .get_mut(&owner.context.session.team_id)
                .unwrap()
                .channels
                .get_mut(OLD)
                .unwrap()
                .name = Some("socket-won".into());
            assert!(drain::publish(&mut shell, &owner, &token, &observed, store, native).is_err());
        }
        assert!(
            !shell.core.workspaces[&owner.context.session.team_id]
                .channels
                .contains_key(NEW)
        );
    }
}

#[test]
fn empty_or_malformed_page_does_not_manufacture_absence_or_partial_ingress() {
    let (mut shell, owner) = prepared();
    let page = receipt(&mut shell, &owner);
    assert!(page.accept(&mut shell, &[], &|_| true).unwrap().is_empty());
    assert!(page.current(&shell));
    assert!(
        page.accept(
            &mut shell,
            &[row(), json!({"id":"invalid","name":"bad"})],
            &|_| true
        )
        .is_err()
    );
    assert!(page.current(&shell));
    assert!(
        !shell.core.workspaces[&owner.context.session.team_id]
            .directory_channels
            .has_pending()
    );
}

#[test]
fn unavailable_dm_member_facts_refuse_entire_page_without_dropping_the_dm() {
    let (mut shell, owner) = prepared();
    let page = receipt(&mut shell, &owner);
    let dm = json!({"id":"DSEARCH01","is_im":true,"user":"UPEER0001"});
    // Typed-only peer cannot supply a canonical member receipt. It is never
    // converted to source or silently excluded to admit the ordinary sibling.
    shell
        .core
        .workspaces
        .get_mut(&owner.context.session.team_id)
        .unwrap()
        .users
        .insert(
            "UPEER0001".into(),
            serde_json::from_value(json!({"id":"UPEER0001","name":"typed-only"})).unwrap(),
        );
    assert!(page.accept(&mut shell, &[row(), dm], &|_| true).is_err());
    assert!(
        !shell.core.workspaces[&owner.context.session.team_id]
            .directory_channels
            .has_pending()
    );
    assert!(
        !shell.core.workspaces[&owner.context.session.team_id]
            .channels
            .contains_key(NEW)
    );
}
