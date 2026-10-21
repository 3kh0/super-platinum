use super::*;
fn prepared() -> (ShellState, u64) {
    let mut s = super::super::tests::shell();
    let old = super::super::tests::editor(&mut s);
    let mut load = EditorLoad::new("WfREVIEW01").unwrap();
    while let Some(r) = load.requests().into_iter().next() {
        let mut raw = old.responses()[&r.kind].clone();
        match r.kind {
            protocol::ReadKind::Latest => {
                let w = &mut raw["decorated_workflow"];
                w["id"] = json!("WfREVIEW01");
                w["is_published"] = json!(true);
                w["last_published_version_id"] = json!("V1");
                w["unpublished_change_count"] = json!(2);
                w["input_parameters"] = json!({});
            }
            protocol::ReadKind::TriggerInfo => {
                raw["trigger"]["workflow"]["workflow_id"] = json!("WfREVIEW01")
            }
            protocol::ReadKind::Triggers => {
                raw["triggers"][0]["workflow"]["workflow_id"] = json!("WfREVIEW01")
            }
            _ => {}
        }
        load.accept(r.kind, raw).unwrap();
    }
    let editor = load.editor().unwrap();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let visit = h.id;
    let owner = h.owner.clone();
    let id = next();
    s.app_home.about.management.documents.push(Document {
        id,
        account: s.core.active_account.clone(),
        team: owner.ctx.session.team_id.clone(),
        user: owner.ctx.session.user_id.clone(),
        title: editor.raw_workflow()["title"].as_str().unwrap().into(),
        description: editor.raw_workflow()["description"]
            .as_str()
            .unwrap()
            .into(),
        editor,
        activity: activity::State::default(),
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
        .document = Some(id);
    (s, visit)
}
fn admitted(s: &mut ShellState, visit: u64) -> u64 {
    let original = Original::capture(s, visit).unwrap();
    let id = original.id;
    let plan = MutationPlan::new(&original.editor, protocol::Command::Published(true)).unwrap();
    let lease = read::Lease::begin(s, visit).unwrap();
    document_mut(s, visit).unwrap().pending = Some(id);
    s.app_home
        .about
        .management
        .publishing
        .attempts
        .push(Attempt {
            original,
            lease,
            plan,
            replies: vec![None],
            validation: validation::Facts::default(),
            validation_only: false,
            finished: false,
            complete: false,
            resolved: false,
            followup: None,
            follow_replies: vec![],
            readback: None,
            no_send: false,
            release: None,
        });
    id
}
#[test]
fn publish_requires_saved_original_metadata_and_recognized_prior_publication() {
    let (mut s, visit) = prepared();
    assert!(shown(&s, visit));
    assert!(available(&s, visit));
    document_mut(&mut s, visit)
        .unwrap()
        .title
        .push_str(" \u{4e2d} e\u{301}");
    assert!(!available(&s, visit));
    assert!(Original::capture(&s, visit).is_none());
    assert!(s.app_home.about.management.publishing.attempts.is_empty());
    let mut other = super::super::tests::shell();
    let old = super::super::tests::editor(&mut other);
    assert!(!supported(&old));
}
#[test]
fn published_legacy_fixture_and_short_source_id_do_not_expose_publish() {
    let (s, visit) = prepared();
    let original = &document(&s, visit).unwrap().editor;
    for id in ["WF_REVIEW", "WfSHORT"] {
        let mut load = EditorLoad::new(id).unwrap();
        while let Some(request) = load.requests().into_iter().next() {
            let mut raw = original.responses()[&request.kind].clone();
            match request.kind {
                protocol::ReadKind::Latest => raw["decorated_workflow"]["id"] = json!(id),
                protocol::ReadKind::Triggers => {
                    raw["triggers"][0]["workflow"]["workflow_id"] = json!(id)
                }
                protocol::ReadKind::TriggerInfo => {
                    raw["trigger"]["workflow"]["workflow_id"] = json!(id)
                }
                _ => {}
            }
            load.accept(request.kind, raw).unwrap();
        }
        let editor = load.editor().unwrap();
        assert_eq!(editor.raw_workflow()["is_published"], json!(true));
        assert_eq!(
            editor.raw_workflow()["last_published_version_id"],
            json!("V1")
        );
        assert_eq!(editor.raw_workflow()["unpublished_change_count"], json!(2));
        assert!(!supported(&editor));
    }
}
#[test]
fn prior_publication_caption_requires_exact_boolean_and_nonnegative_integer_count() {
    for (published, count, expected) in [
        (false, 0, Some("Publish")),
        (false, 2, Some("Publish Changes")),
        (true, 0, None),
        (true, 2, Some("Publish Changes")),
    ] {
        let raw = json!({"is_published":published,"last_published_version_id":"V1",
            "unpublished_change_count":count});
        assert_eq!(prior_publication_caption(&raw), expected);
    }
    let original = json!({"is_published":false,"last_published_version_id":"V1",
        "unpublished_change_count":0});
    for (key, values) in [
        ("is_published", vec![Value::Null, json!(0), json!("false")]),
        (
            "last_published_version_id",
            vec![Value::Null, json!(""), json!(1)],
        ),
        (
            "unpublished_change_count",
            vec![Value::Null, json!(-1), json!(0.5), json!("0")],
        ),
    ] {
        let mut absent = original.clone();
        absent.as_object_mut().unwrap().remove(key);
        assert_eq!(prior_publication_caption(&absent), None, "missing {key}");
        for value in values {
            let mut raw = original.clone();
            raw[key] = value;
            assert_eq!(prior_publication_caption(&raw), None, "{raw}");
        }
    }
}
#[test]
fn republish_uses_original_caption_and_preserves_clean_draft_and_owner_guards() {
    for count in [0, 2] {
        let (mut s, visit) = prepared();
        let original = document(&s, visit).unwrap().editor.clone();
        let mut load = EditorLoad::new(original.workflow_id()).unwrap();
        while let Some(request) = load.requests().into_iter().next() {
            let mut raw = original.responses()[&request.kind].clone();
            if request.kind == protocol::ReadKind::Latest {
                raw["decorated_workflow"]["is_published"] = json!(false);
                raw["decorated_workflow"]["unpublished_change_count"] = json!(count);
            }
            load.accept(request.kind, raw).unwrap();
        }
        let editor = load.editor().unwrap();
        let label = if count == 0 {
            "Publish"
        } else {
            "Publish Changes"
        };
        assert!(supported(&editor));
        assert_eq!(caption(&editor), Some(label));
        assert_eq!(
            ready_notice(&editor),
            format!("Publication checks are ready. Use {label} to publish the saved workflow.")
        );
        assert_eq!(
            editor.raw_workflow()["last_published_version_id"],
            json!("V1")
        );
        document_mut(&mut s, visit).unwrap().editor = editor;
        assert!(shown(&s, visit));
        assert!(available(&s, visit));
        let captured = Original::capture(&s, visit).unwrap();
        document_mut(&mut s, visit)
            .unwrap()
            .title
            .push_str(" unsaved");
        assert!(shown(&s, visit));
        assert!(!available(&s, visit));
        assert!(!captured.current(&s, None));
        document_mut(&mut s, visit).unwrap().title = captured.title.clone();
        assert!(captured.current(&s, None));
        s.core.account_epoch += 1;
        assert!(!captured.current(&s, None));
        assert_eq!(original.raw_workflow()["is_published"], json!(true));
    }
}
#[test]
fn original_revision_and_account_retirement_cannot_admit_a_late_slot() {
    for account in [false, true] {
        let (mut s, visit) = prepared();
        let id = admitted(&mut s, visit);
        assert!(exact(&s, id));
        if account {
            s.core.account_epoch += 1;
        } else {
            document_mut(&mut s, visit).unwrap().revision = next();
        }
        assert!(!exact(&s, id));
        assert!(matches!(
            attempt(&s, id).unwrap().plan.slots()[0].outcome(),
            Outcome::Pending
        ));
    }
}
#[test]
fn stale_published_reply_is_unknown_and_cannot_authorize_trigger_sync() {
    let (mut s, visit) = prepared();
    let id = admitted(&mut s, visit);
    let raw = attempt(&s, id)
        .unwrap()
        .original
        .editor
        .latest_response()
        .clone();
    attempt_mut(&mut s, id).unwrap().plan.mark_sent(0).unwrap();
    run::settle(&mut s, id, true, Ok(raw.clone()));
    let a = attempt(&s, id).unwrap();
    assert!(matches!(a.plan.slots()[0].outcome(), Outcome::Unknown));
    assert!(a.followup.is_none());
    assert!(matches!(&a.replies[0],Some(Reply::Raw(v)) if v==&raw));
    assert!(owns(&s, visit));
    assert!(!editable(&s, visit));
    assert!(!activity::permitted(&s, visit));
}
#[test]
fn late_new_version_receipt_is_retained_without_adopting_or_unlocking() {
    let (mut s, visit) = prepared();
    let id = admitted(&mut s, visit);
    let original = document(&s, visit).unwrap().clone();
    let mut raw = attempt(&s, id)
        .unwrap()
        .original
        .editor
        .latest_response()
        .clone();
    raw["decorated_workflow"]["last_published_version_id"] = json!("V2");
    raw["decorated_workflow"]["unpublished_change_count"] = json!(0);
    attempt_mut(&mut s, id).unwrap().plan.mark_sent(0).unwrap();
    s.core.account_epoch += 1;
    run::settle(&mut s, id, true, Ok(raw.clone()));
    let a = attempt(&s, id).unwrap();
    assert!(a.plan.primary_succeeded());
    assert!(!exact(&s, id));
    assert!(matches!(&a.replies[0],Some(Reply::Raw(v)) if v==&raw));
    let d = s
        .app_home
        .about
        .management
        .documents
        .iter()
        .find(|d| d.id == original.id)
        .unwrap();
    assert!(d.editor.same_revision(&original.editor));
    assert_eq!(d.title, original.title);
    assert_eq!(d.pending, Some(id));
}
