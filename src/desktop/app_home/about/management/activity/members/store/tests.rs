use super::*;
use serde_json::json;
use std::collections::BTreeSet;
use super_platinum_core::state::directory_members::Origin;

fn setup() -> (ShellState, Query, String, Flags, Pending) {
    let (mut s, visit) = super::super::tests::setup();
    let account = s.core.active_account.clone().unwrap();
    let owner = s.app_home.about.surface.as_ref().unwrap().owner.clone();
    let w = s
        .core
        .workspaces
        .get_mut(&owner.ctx.session.team_id)
        .unwrap();
    w.users
        .retain(|id, _| id == &owner.ctx.session.user_id || id == &owner.bot);
    // Explicit synthetic wire rows, including real profile objects. Serialized
    // native defaults (the old bot had profile:null) are not source replies.
    let self_row =
        json!({"id":owner.ctx.session.user_id,"name":"Reviewer","is_stranger":false,"profile":{}});
    let bot_row =
        json!({"id":owner.bot,"name":"Review bot","is_bot":true,"is_stranger":false,"profile":{}});
    for raw in [&self_row, &bot_row] {
        let user: super_platinum_core::slack::models::User =
            serde_json::from_value(raw.clone()).unwrap();
        w.users.insert(user.id.clone(), user);
    }
    w.directory_members
        .begin(
            &account,
            &owner.ctx.session,
            &BTreeSet::from([owner.ctx.session.user_id.clone(), owner.bot.clone()]),
        )
        .unwrap();
    w.directory_members
        .enqueue(
            &account,
            &owner.ctx.session,
            Origin::Directory,
            vec![
                json!({"id":"UZFIRST12","name":"First","updated":17,"is_stranger":false,"profile":{"display_name":"First"}}),
                self_row,
                bot_row,
            ],
            json!({"UZFIRST12":true,(owner.ctx.session.user_id.clone()):true,(owner.bot.clone()):true}),
        )
        .unwrap();
    let q = Query::capture(&mut s, visit).unwrap();
    let flags = Flags::decode(&json!({"ok":true,"assignments":{}})).unwrap();
    let w = &s.core.workspaces[&q.owner.ctx.session.team_id];
    let p = w
        .directory_members
        .prepare(
            &account,
            &q.owner.ctx.session,
            q.owner.ctx.epoch,
            &q.generation(),
            flags.clone(),
            &w.users,
        )
        .unwrap();
    (s, q, account, flags, p)
}
// ASCII-only synthetic renderer receipt. Native Unicode/locale behavior is a
// separate actual helper gate, never proved by this regression adapter.
fn reply(p: &Pending) -> RendererReply {
    RendererReply {
        generation: p.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: p.fields().to_vec(),
        lowered: p.fields().iter().map(|s| s.to_lowercase()).collect(),
    }
}

#[test]
fn successful_commit_installs_exact_native_and_source_order_together() {
    let (mut s, q, account, flags, p) = setup();
    let r = reply(&p);
    assert!(snapshot(&s, &q).is_err());
    install(&mut s, &q, &account, &flags, p, r).unwrap();
    let current = snapshot(&s, &q).unwrap();
    assert!(current.current(&s, &q));
    assert_eq!(current.rows()[0].source["id"], "UZFIRST12");
    assert_eq!(
        s.core.workspaces[&q.owner.ctx.session.team_id].users["UZFIRST12"]
            .name
            .as_deref(),
        Some("First")
    );
    assert!(!pending(&s, &q, &account).unwrap());
    s.core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .unwrap()
        .users
        .get_mut("UZFIRST12")
        .unwrap()
        .name = Some("New typed-only value".into());
    assert!(!current.current(&s, &q));
}

#[test]
fn changed_native_during_eval_cannot_install_either_half() {
    let (mut s, q, account, flags, p) = setup();
    let r = reply(&p);
    let w = s
        .core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .unwrap();
    w.users.get_mut(&q.owner.ctx.session.user_id).unwrap().name =
        Some("Changed during eval".into());
    let before = serde_json::to_value(&w.users).unwrap();
    assert!(install(&mut s, &q, &account, &flags, p, r).is_err());
    let w = &s.core.workspaces[&q.owner.ctx.session.team_id];
    assert_eq!(serde_json::to_value(&w.users).unwrap(), before);
    assert!(
        w.directory_members
            .has_pending(&account, &q.owner.ctx.session)
            .unwrap()
    );
    assert!(!w.users.contains_key("UZFIRST12"));
}

#[test]
fn retired_query_does_not_consume_owned_pending_batch() {
    let (mut s, q, account, flags, p) = setup();
    let r = reply(&p);
    edit(&mut s, q.visit, "Replacement query".into());
    assert!(install(&mut s, &q, &account, &flags, p, r).is_err());
    let w = &s.core.workspaces[&q.owner.ctx.session.team_id];
    assert!(
        w.directory_members
            .has_pending(&account, &q.owner.ctx.session)
            .unwrap()
    );
    assert!(!w.users.contains_key("UZFIRST12"));
}

#[test]
fn recovery_captures_native_pre_request_and_source_stamp_independently() {
    let (mut s, q, account, flags, p) = setup();
    let r = reply(&p);
    install(&mut s, &q, &account, &flags, p, r).unwrap();
    assert!(Recovery::capture(&s, &q).unwrap().is_none());
    let team = q.owner.ctx.session.team_id.clone();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("UZFIRST12")
        .unwrap()
        .name = Some("Typed change".into());
    let plan = Recovery::capture(&s, &q).unwrap().unwrap();
    assert_eq!(plan.requested(), &["UZFIRST12"]);
    assert!(plan.current(&s, &q));
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("UZFIRST12")
        .unwrap()
        .name = Some("Later edit".into());
    assert!(!plan.current(&s, &q));
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("UZFIRST12")
        .unwrap()
        .name = Some("Typed change".into());
    assert!(plan.current(&s, &q));
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .directory_members
        .enqueue(
            &account,
            &q.owner.ctx.session,
            Origin::DirectUpsert,
            vec![json!({"id":"UZFIRST12","name":"New raw event"})],
            json!({}),
        )
        .unwrap();
    assert!(!plan.current(&s, &q));
}

#[test]
fn explicit_legacy_start_does_not_rebind_different_account_or_grant_ready() {
    let (mut s, q, account, _, _) = setup();
    let team = q.owner.ctx.session.team_id.clone();
    s.core.workspaces.get_mut(&team).unwrap().directory_members = Default::default();
    assert!(initialize(&mut s, &q).unwrap());
    assert!(!initialize(&mut s, &q).unwrap());
    assert!(snapshot(&s, &q).is_err());
    let captured = Recovery::capture(&s, &q).unwrap().unwrap();
    assert!(captured.prior.interactions().is_empty());
    assert!(!captured.requested().is_empty());
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .directory_members
        .begin("different-account", &q.owner.ctx.session, &BTreeSet::new())
        .unwrap();
    assert!(initialize(&mut s, &q).is_err());
    assert!(
        s.core.workspaces[&team]
            .directory_members
            .recovery_state(&account, &q.owner.ctx.session)
            .is_err()
    );
}

fn repair_setup() -> (
    ShellState,
    Query,
    Recovery,
    Flags,
    super_platinum_core::state::directory_members::NativeRepair,
    RendererReply,
) {
    let (mut s, q, account, flags, pending) = setup();
    let response = reply(&pending);
    install(&mut s, &q, &account, &flags, pending, response).unwrap();
    let w = s
        .core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .unwrap();
    w.users.get_mut("UZFIRST12").unwrap().name = Some("Native drift".into());
    let witness = Recovery::capture(&s, &q).unwrap().unwrap();
    let w = &s.core.workspaces[&q.owner.ctx.session.team_id];
    // Synthetic fresh wire in this pure installation test, not a claim that
    // an actual network/renderer acquisition has executed.
    let pending = w
        .directory_members
        .prepare_native_repair(
            &account,
            &q.owner.ctx.session,
            q.owner.ctx.epoch,
            &q.generation(),
            flags.clone(),
            vec![
                json!({"id":"UZFIRST12","name":"First","updated":17,"is_stranger":false,
            "profile":{"display_name":"First"}}),
            ],
            &w.users,
        )
        .unwrap();
    let response = RendererReply {
        generation: pending.generation().into(),
        operation: "ordinary-toLowerCase".into(),
        fields: pending.fields().to_vec(),
        lowered: pending.fields().iter().map(|s| s.to_lowercase()).collect(),
    };
    (s, q, witness, flags, pending, response)
}

#[test]
fn native_repair_installs_both_owned_parts_without_changing_source_order() {
    let (mut s, q, witness, flags, pending, response) = repair_setup();
    let before = vec![
        "UZFIRST12".to_owned(),
        q.owner.ctx.session.user_id.clone(),
        s.app_home.about.surface.as_ref().unwrap().owner.bot.clone(),
    ];
    assert!(snapshot(&s, &q).is_err());
    repair::install_repair(&mut s, &q, &witness, &flags, pending, response).unwrap();
    let ready = snapshot(&s, &q).unwrap();
    assert!(ready.current(&s, &q));
    assert_eq!(
        ready
            .rows()
            .iter()
            .map(|r| r.source["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(
        s.core.workspaces[&q.owner.ctx.session.team_id].users["UZFIRST12"]
            .name
            .as_deref(),
        Some("First")
    );
}

#[test]
fn late_native_edit_prevents_both_repair_installations() {
    let (mut s, q, witness, flags, pending, response) = repair_setup();
    let w = s
        .core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .unwrap();
    w.users.get_mut("UZFIRST12").unwrap().name = Some("Later native edit".into());
    let before = serde_json::to_value(&w.users).unwrap();
    assert!(repair::install_repair(&mut s, &q, &witness, &flags, pending, response).is_err());
    assert_eq!(
        serde_json::to_value(&s.core.workspaces[&q.owner.ctx.session.team_id].users).unwrap(),
        before
    );
    assert!(snapshot(&s, &q).is_err());
}

#[test]
fn source_arrival_during_renderer_eval_preserves_both_canonical_halves() {
    let (mut s, q, account, flags, pending) = setup();
    let response = reply(&pending);
    let w = s
        .core
        .workspaces
        .get_mut(&q.owner.ctx.session.team_id)
        .unwrap();
    w.directory_members
        .enqueue(
            &account,
            &q.owner.ctx.session,
            Origin::DirectUpsert,
            vec![json!({"id":"UZLATER12","name":"Later raw event","profile":{}})],
            json!({}),
        )
        .unwrap();
    let before_source = w.directory_members.cached();
    let before_native = serde_json::to_value(&w.users).unwrap();
    assert!(install(&mut s, &q, &account, &flags, pending, response).is_err());
    let w = &s.core.workspaces[&q.owner.ctx.session.team_id];
    assert_eq!(w.directory_members.cached(), before_source);
    assert_eq!(serde_json::to_value(&w.users).unwrap(), before_native);
}

#[test]
fn account_and_auth_retirement_cannot_install_a_native_repair() {
    for change in 0..3 {
        let (mut s, q, witness, flags, pending, response) = repair_setup();
        match change {
            0 => s.core.active_account = Some("different-account".into()),
            1 => s
                .core
                .session
                .as_mut()
                .unwrap()
                .d_cookie
                .push_str("-rotated"),
            _ => {
                s.core
                    .workspaces
                    .get_mut(&q.owner.ctx.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
        }
        let w = &s.core.workspaces[&q.owner.ctx.session.team_id];
        let before_source = w.directory_members.cached();
        let before_native = serde_json::to_value(&w.users).unwrap();
        assert!(repair::install_repair(&mut s, &q, &witness, &flags, pending, response).is_err());
        let w = &s.core.workspaces[&q.owner.ctx.session.team_id];
        assert_eq!(w.directory_members.cached(), before_source);
        assert_eq!(serde_json::to_value(&w.users).unwrap(), before_native);
    }
}
