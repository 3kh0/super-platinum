use super::*;

#[test]
fn permission_form_refuses_malformed_acquired_run_values() {
    let (s, _, id) = prepared();
    let original = &run::attempt(&s, id).unwrap().original.editor;
    assert!(supported(original));
    for (permission, entities) in [
        ("named_entities", json!([])),
        ("app_collaborators", Value::Null),
    ] {
        let mut load = EditorLoad::new(original.workflow_id()).unwrap();
        for (kind, raw) in original.responses() {
            let mut raw = raw.clone();
            if *kind == protocol::ReadKind::RunPermissions {
                raw["permission_type"] = json!(permission);
                raw["team_ids"] = entities.clone();
            }
            load.accept(*kind, raw).unwrap();
        }
        assert!(!supported(&load.editor().unwrap()));
    }
}
#[test]
fn acquired_internal_run_selections_validate_without_inventing_team_policy() {
    assert!(valid_unchanged_run(Some(
        &json!({"permission_type":"named_entities","user_ids":["U1"],"channel_ids":["C1"]})
    )));
    assert!(valid_unchanged_run(Some(
        &json!({"permission_type":"named_entities","user_ids":vec!["U1";100]})
    )));
    for raw in [
        json!({"permission_type":"named_entities"}),
        json!({"permission_type":"named_entities","user_ids":vec!["U1";101]}),
        json!({"permission_type":"everyone","user_ids":[null]}),
    ] {
        assert!(!valid_unchanged_run(Some(&raw)));
    }
}
pub(super) fn prepared() -> (ShellState, u64, u64) {
    let mut s = super::super::tests::shell();
    let owner = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .unwrap()
        .owner
        .clone();
    let mut load = EditorLoad::new("WF_REVIEW").unwrap();
    while let Some(r) = load.requests().into_iter().next() {
        let mut raw = fixture::response(
            &mut s,
            &r.plan.request(&owner.ctx.client, &owner.ctx.session),
        )
        .unwrap();
        if r.kind == protocol::ReadKind::Latest {
            raw["decorated_workflow"]["is_published"] = json!(true);
            raw["decorated_workflow"]["input_parameters"] = json!({});
        }
        if r.kind == protocol::ReadKind::CopyPermissions {
            raw["permission_type"] = json!("everyone");
        }
        load.accept(r.kind, raw).unwrap();
    }
    let editor = load.editor().unwrap();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let visit = h.id;
    let doc = next();
    s.app_home.about.management.documents.push(Document {
        activity: activity::State::default(),
        id: doc,
        account: s.core.active_account.clone(),
        team: owner.ctx.session.team_id.clone(),
        user: owner.ctx.session.user_id.clone(),
        editor,
        title: "Local \u{4e2d} e\u{301}".into(),
        description: "Retained\n\u{1f98a}".into(),
        revision: next(),
        observed: None,
        pending: None,
        notice: None,
    });
    s.app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .document = Some(doc);
    let original = Original::capture(&s, visit).unwrap();
    let id = original.id;
    let plan = MutationPlan::new(
        &original.editor,
        protocol::Command::CopyPermission("collaborators".into()),
    )
    .unwrap();
    let lease = read::Lease::begin(&mut s, visit).unwrap();
    document_mut(&mut s, visit).unwrap().pending = Some(id);
    s.app_home
        .about
        .management
        .permissions
        .attempts
        .push(Attempt {
            original,
            lease,
            plan,
            copy_changed: true,
            run_group: None,
            manager_group: None,
            manager_source: None,
            departure: managers::departure::Departure::default(),
            settings_source: None,
            replies: vec![None],
            finished: false,
            complete: false,
            resolved: false,
            desired: "collaborators".into(),
            readback: None,
            no_send: false,
            release: None,
        });
    (s, visit, id)
}
fn sent(s: &mut ShellState, id: u64) {
    run::attempt_mut(s, id).unwrap().plan.mark_sent(0).unwrap();
}
fn current(s: &ShellState, id: u64, changed: bool) -> Editor {
    let original = &run::attempt(s, id).unwrap().original.editor;
    let mut load = EditorLoad::new(original.workflow_id()).unwrap();
    for (kind, raw) in original.responses() {
        let mut raw = raw.clone();
        if changed && *kind == protocol::ReadKind::CopyPermissions {
            raw["permission_type"] = json!("collaborators");
        }
        load.accept(*kind, raw).unwrap();
    }
    load.editor().unwrap()
}
#[test]
fn exact_copy_ack_and_independent_readback_resolve_without_replacing_dirty_metadata() {
    let (mut s, visit, id) = prepared();
    let before = document(&s, visit).unwrap().clone();
    sent(&mut s, id);
    let ack = json!({"ok":true,"permission_type":"collaborators","opaque":{"receipt":7}});
    run::settle(&mut s, id, Ok(ack.clone()));
    assert!(run::attempt(&s, id).unwrap().plan.primary_succeeded());
    assert!(owns(&s, visit));
    assert!(!editable(&s, visit));
    let b = native::Finish {
        title: before.title.clone(),
        description: before.description.clone(),
    };
    finish::finish_owned(&mut s, id, "", true, Some(&b));
    let fresh = current(&s, id, true);
    readback::adopt(&mut s, visit, id, fresh, &b, true);
    assert!(run::attempt(&s, id).unwrap().resolved);
    assert_eq!(document(&s, visit).unwrap().pending, None);
    assert_eq!(document(&s, visit).unwrap().title, before.title);
    assert_eq!(document(&s, visit).unwrap().description, before.description);
    assert_eq!(
        document(&s, visit).unwrap().editor.raw_workflow(),
        before.editor.raw_workflow()
    );
}
#[test]
fn malformed_or_wrong_copy_ack_and_unknown_delivery_never_resolve_from_matching_readback() {
    for raw in [
        json!({"ok":true}),
        json!({"ok":true,"permission_type":"everyone"}),
        json!({"ok":true,"permission_type":"collaborators","error":false}),
        json!({"ok":false,"error":"denied"}),
    ] {
        let (mut s, visit, id) = prepared();
        sent(&mut s, id);
        run::settle(&mut s, id, Ok(raw.clone()));
        let b = native::Finish {
            title: document(&s, visit).unwrap().title.clone(),
            description: document(&s, visit).unwrap().description.clone(),
        };
        finish::finish_owned(&mut s, id, "retained", true, Some(&b));
        let fresh = current(&s, id, true);
        readback::adopt(&mut s, visit, id, fresh, &b, true);
        let a = run::attempt(&s, id).unwrap();
        assert!(!a.complete && !a.resolved);
        assert_eq!(document(&s, visit).unwrap().pending, Some(id));
        assert!(matches!(&a.replies[0],Some(Reply::Raw(v)) if v==&raw));
    }
    let (mut s, visit, id) = prepared();
    sent(&mut s, id);
    run::settle(&mut s, id, Err(Error::Transport("lost".into())));
    let b = native::Finish {
        title: document(&s, visit).unwrap().title.clone(),
        description: document(&s, visit).unwrap().description.clone(),
    };
    finish::finish_owned(&mut s, id, "retained", true, Some(&b));
    let fresh = current(&s, id, true);
    readback::adopt(&mut s, visit, id, fresh, &b, true);
    assert!(!run::attempt(&s, id).unwrap().resolved);
    assert!(matches!(
        run::attempt(&s, id).unwrap().plan.slots()[0].outcome(),
        Outcome::Unknown
    ));
}
#[test]
fn ack_requires_matching_readback_and_current_native_authority() {
    for (changed, native_current) in [(false, true), (true, false)] {
        let (mut s, visit, id) = prepared();
        sent(&mut s, id);
        run::settle(
            &mut s,
            id,
            Ok(json!({"ok":true,"permission_type":"collaborators"})),
        );
        let b = native::Finish {
            title: document(&s, visit).unwrap().title.clone(),
            description: document(&s, visit).unwrap().description.clone(),
        };
        finish::finish_owned(&mut s, id, "", true, Some(&b));
        let fresh = current(&s, id, changed);
        readback::adopt(&mut s, visit, id, fresh, &b, native_current);
        assert!(!run::attempt(&s, id).unwrap().resolved);
        assert!(owns(&s, visit));
    }
}
#[test]
fn retired_original_can_retain_receipt_without_authorizing_current_document() {
    let (mut s, visit, id) = prepared();
    sent(&mut s, id);
    let before = document(&s, visit).unwrap().clone();
    s.core.active_account = Some("successor".into());
    assert!(!exact(&s, id));
    run::settle(
        &mut s,
        id,
        Ok(json!({"ok":true,"permission_type":"collaborators"})),
    );
    finish::finish_owned(&mut s, id, "retained", false, None);
    let d = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .find(|d| d.id == before.id)
        .unwrap();
    assert_eq!(d.title, before.title);
    assert_eq!(d.revision, before.revision);
    assert!(run::attempt(&s, id).unwrap().finished);
}

fn attach_run(s: &mut ShellState, id: u64) {
    let a = run::attempt_mut(s, id).unwrap();
    a.run_group = Some(
        groups::Run::new(
            &a.original.editor,
            protocol::Command::RunPermission {
                permission_type: "everyone".into(),
                user_ids: None,
                channel_ids: None,
                team_ids: None,
                org_ids: None,
            },
        )
        .unwrap(),
    );
    a.run_group.as_mut().unwrap().plan.mark_sent(0).unwrap();
}
#[test]
fn independent_run_rejection_and_copy_ack_preserve_both_receipts_and_draft() {
    let (mut s, visit, id) = prepared();
    let before = document(&s, visit).unwrap().clone();
    sent(&mut s, id);
    attach_run(&mut s, id);
    groups::settle(
        &mut s,
        id,
        true,
        Ok(json!({"ok":false,"error":"not_allowed"})),
    );
    run::settle(
        &mut s,
        id,
        Ok(json!({"ok":true,"permission_type":"collaborators"})),
    );
    let a = run::attempt(&s, id).unwrap();
    assert!(a.plan.primary_succeeded());
    assert!(matches!(
        a.run_group.as_ref().unwrap().plan.slots()[0].outcome(),
        Outcome::Rejected(_)
    ));
    assert!(a.run_group.as_ref().unwrap().followup.is_none());
    let b = native::Finish {
        title: before.title.clone(),
        description: before.description.clone(),
    };
    finish::finish_owned(&mut s, id, "retained", true, Some(&b));
    assert!(!run::attempt(&s, id).unwrap().complete);
    assert_eq!(document(&s, visit).unwrap().pending, Some(id));
    assert_eq!(document(&s, visit).unwrap().title, before.title);
    groups::settle(
        &mut s,
        id,
        true,
        Ok(json!({"ok":true,"permission_type":"everyone"})),
    );
    assert!(matches!(
        run::attempt(&s, id)
            .unwrap()
            .run_group
            .as_ref()
            .unwrap()
            .plan
            .slots()[0]
            .outcome(),
        Outcome::Rejected(_)
    ));
}
#[test]
fn run_ack_requires_actual_restriction_followup_and_late_receipt_never_authorizes_successor() {
    let (mut s, visit, id) = prepared();
    attach_run(&mut s, id);
    let before = document(&s, visit).unwrap().clone();
    s.core.active_account = Some("replacement".into());
    assert!(!exact(&s, id));
    groups::settle(
        &mut s,
        id,
        true,
        Ok(json!({"ok":true,"permission_type":"everyone","opaque":{"retained":true}})),
    );
    let g = run::attempt(&s, id).unwrap().run_group.as_ref().unwrap();
    assert!(g.plan.primary_succeeded());
    assert!(!g.complete());
    assert!(matches!(
        g.followup.as_ref().unwrap().slots()[0].outcome(),
        Outcome::Pending
    ));
    finish::finish_owned(&mut s, id, "retained", false, None);
    let d = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .find(|d| d.id == before.id)
        .unwrap();
    assert_eq!(d.title, before.title);
    assert_eq!(d.revision, before.revision);
}
#[test]
fn run_unknown_never_becomes_ack_from_late_second_settlement() {
    let (mut s, _, id) = prepared();
    attach_run(&mut s, id);
    groups::settle(
        &mut s,
        id,
        true,
        Err(Error::Transport("lost acknowledgement".into())),
    );
    groups::settle(
        &mut s,
        id,
        true,
        Ok(json!({"ok":true,"permission_type":"everyone"})),
    );
    let g = run::attempt(&s, id).unwrap().run_group.as_ref().unwrap();
    assert!(matches!(g.plan.slots()[0].outcome(), Outcome::Unknown));
    assert!(g.followup.is_none());
    assert!(!g.complete());
}
