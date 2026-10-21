use super::*;
fn prepared() -> (ShellState, u64, u64) {
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
    let plan = MutationPlan::new(&original.editor, protocol::Command::Published(false)).unwrap();
    let lease = read::Lease::begin(&mut s, visit).unwrap();
    document_mut(&mut s, visit).unwrap().pending = Some(id);
    s.app_home
        .about
        .management
        .unpublishing
        .attempts
        .push(Attempt {
            original,
            lease,
            plan,
            replies: vec![None],
            finished: false,
            complete: false,
            resolved: false,
            followup: None,
            follow_replies: vec![],
            readback: None,
            no_send: false,
            release: None,
        });
    (s, visit, id)
}
fn primary(s: &ShellState, id: u64) -> Value {
    let mut raw = run::attempt(s, id)
        .unwrap()
        .original
        .editor
        .latest_response()
        .clone();
    raw["decorated_workflow"]["is_published"] = json!(false);
    raw
}
fn sent(s: &mut ShellState, id: u64) {
    run::attempt_mut(s, id).unwrap().plan.mark_sent(0).unwrap();
}
#[test]
fn no_op_sync_still_validates_actual_primary_and_retains_dirty_draft() {
    let (mut s, visit, id) = prepared();
    let before = document(&s, visit).unwrap().clone();
    let raw = primary(&s, id);
    sent(&mut s, id);
    run::settle(&mut s, id, true, Ok(raw.clone()));
    let a = run::attempt(&s, id).unwrap();
    assert!(a.followup.as_ref().unwrap().fully_succeeded());
    assert_eq!(
        a.followup.as_ref().unwrap().current_workflow(),
        &raw["decorated_workflow"]
    );
    assert_eq!(document(&s, visit).unwrap().title, before.title);
    assert!(
        document(&s, visit)
            .unwrap()
            .editor
            .same_revision(&before.editor)
    );
    assert!(owns(&s, visit));
    assert!(!editable(&s, visit));
}
#[test]
fn malformed_primary_cannot_authorize_sync_or_replace_immutable_raw_receipt() {
    for change in [
        "id",
        "app_id",
        "is_published",
        "available_data",
        "error",
        "bare",
    ] {
        let (mut s, visit, id) = prepared();
        let mut raw = primary(&s, id);
        match change {
            "id" | "app_id" => raw["decorated_workflow"][change] = json!("OTHER"),
            "is_published" => raw["decorated_workflow"][change] = json!(true),
            "available_data" => raw[change] = Value::Null,
            "error" => raw[change] = json!(false),
            _ => raw = json!({"ok":true}),
        }
        sent(&mut s, id);
        run::settle(&mut s, id, true, Ok(raw.clone()));
        run::settle(&mut s, id, true, Ok(json!({"ok":true})));
        let a = run::attempt(&s, id).unwrap();
        assert!(a.followup.is_none());
        assert!(matches!(a.plan.slots()[0].outcome(), Outcome::Unknown));
        assert_eq!(snapshot(&s, visit)["attempts"][0]["slots"][0]["raw"], raw);
        assert!(a.plan.ready().is_empty());
    }
}
#[test]
fn metadata_observe_cannot_resolve_unknown_publication_and_old_source_retires() {
    let (mut s, visit, id) = prepared();
    sent(&mut s, id);
    run::settle(
        &mut s,
        id,
        true,
        Err(Error::Transport("synthetic lost response".into())),
    );
    let original = run::attempt(&s, id).unwrap().original.clone();
    document_mut(&mut s, visit).unwrap().observed = Some(original.editor.clone());
    write::observe(&mut s, visit);
    assert_eq!(document(&s, visit).unwrap().pending, Some(id));
    s.core.active_account = Some("replacement".into());
    assert!(!exact(&s, id));
    let duplicate = primary(&s, id);
    run::settle(&mut s, id, true, Ok(duplicate)); // Duplicate cannot rewrite Unknown.
    assert!(matches!(
        run::attempt(&s, id).unwrap().plan.slots()[0].outcome(),
        Outcome::Unknown
    ));
}
#[test]
fn published_state_and_pending_admission_are_not_inferred_from_edit_permission() {
    let (s, visit, id) = prepared();
    let a = run::attempt(&s, id).unwrap();
    assert!(supported(&a.original.editor));
    assert!(!available(&s, visit)); // Pending operation independently seals the UI.
    assert!(!supported(&super::super::tests::editor(
        &mut super::super::tests::shell()
    )));
}

fn fresh(s: &mut ShellState, raw: Value) -> Editor {
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
        let response = if r.kind == protocol::ReadKind::Latest {
            raw.clone()
        } else {
            fixture::response(s, &r.plan.request(&owner.ctx.client, &owner.ctx.session)).unwrap()
        };
        load.accept(r.kind, response).unwrap();
    }
    load.editor().unwrap()
}
#[test]
fn verified_complete_readback_adopts_only_baseline_and_restores_metadata_editing() {
    let (mut s, visit, id) = prepared();
    let before = document(&s, visit).unwrap().clone();
    let raw = primary(&s, id);
    sent(&mut s, id);
    run::settle(&mut s, id, true, Ok(raw.clone()));
    run::finish_owned(&mut s, id, "Retained", true, None);
    let current = fresh(&mut s, raw);
    let buffers = native::Finish {
        title: before.title.clone(),
        description: before.description.clone(),
    };
    readback::adopt(&mut s, visit, id, current, &buffers, true);
    let d = document(&s, visit).unwrap();
    assert_eq!(
        (&d.title, &d.description),
        (&before.title, &before.description)
    );
    assert_eq!(d.editor.raw_workflow()["is_published"], false);
    assert!(d.dirty());
    assert!(d.pending.is_none());
    assert!(editable(&s, visit));
    assert!(run::attempt(&s, id).unwrap().resolved);
    assert!(!available(&s, visit));
}
#[test]
fn ahead_finish_readback_cannot_resolve_and_refused_finish_never_overwrites_draft() {
    let (mut s, visit, id) = prepared();
    let before = document(&s, visit).unwrap().clone();
    let raw = primary(&s, id);
    sent(&mut s, id);
    run::settle(&mut s, id, true, Ok(raw.clone()));
    run::finish_owned(&mut s, id, "Refused", false, None);
    let d = document(&s, visit).unwrap();
    assert_eq!(d.revision, before.revision);
    assert_eq!(d.notice, before.notice);
    assert!(
        s.app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .read
            .is_none()
    );
    let current = fresh(&mut s, raw);
    let buffers = native::Finish {
        title: format!("{} ahead", before.title),
        description: before.description,
    };
    readback::adopt(&mut s, visit, id, current, &buffers, false);
    assert_eq!(document(&s, visit).unwrap().title, buffers.title);
    assert_eq!(document(&s, visit).unwrap().pending, Some(id));
    assert!(!run::attempt(&s, id).unwrap().resolved);
}
