use super::*;
use super_platinum_core::slack::transport::ListColumnRenameFailure;
fn shell() -> ShellState {
    let mut s = super::super::tests::make_shell();
    s.core.active_account = Some(format!("rename-test-{}", next_id()));
    let mut raw = super::super::tests::loaded(false).document.raw;
    raw["list_metadata"]["schema"][0]["type"] = json!("text");
    raw["list_metadata"]["schema"][0]["options"] = json!({"format":"text"});
    raw["list_metadata"]["opaque"] = json!({"keep":1});
    raw["created"] = json!(10);
    raw["user"] = json!("U1");
    let doc = Document::parse(raw.clone()).unwrap();
    open_shell(&mut s, "name", 0); // Stale callback must not insert.
    assert!(super::super::open_shell(
        &mut s,
        serde_json::from_value(raw).unwrap()
    ));
    let mut loaded = super::super::tests::loaded(false);
    loaded.document = doc;
    loaded.personal_task_lists = Some(
        PersonalTaskLists::from_response(
            &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .unwrap(),
    );
    loaded.records=RecordsPage::parse(json!({"ok":true,"records":loaded.records.raw["records"],"response_metadata":{"next_cursor":"opaque+/=","keep":true}})).unwrap();
    super::super::tests::read(&mut s, loaded);
    s
}
fn metadata(s: &ShellState, name: Option<&str>) -> network::Metadata {
    let loaded = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let mut raw = loaded.document.raw.clone();
    if let Some(name) = name {
        raw["list_metadata"]["schema"][0]["name"] = json!(name);
    }
    network::Metadata {
        document: Document::parse(raw).unwrap(),
        allow: Some("full".into()),
        personal: loaded.personal_task_lists.clone().unwrap(),
    }
}
fn begin(s: &mut ShellState) -> (Guard, Intent) {
    let visit = generation(s).unwrap();
    assert!(open_shell(s, "name", visit));
    let key = Key::current(s).unwrap();
    let serial = entry(s).unwrap().serial;
    with_registry(|r| r.entries.get_mut(&key).unwrap().name = "Delivery".into());
    prepare(s, serial).unwrap()
}
fn sent(s: &mut ShellState) -> Guard {
    let (g, i) = begin(s);
    assert!(admitted(s, &g, &i, &metadata(s, None)).is_some());
    g
}
fn uncertain(s: &mut ShellState) -> Guard {
    let g = sent(s);
    completed(
        s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    g
}
fn checking(s: &ShellState) -> Guard {
    let key = Key::current(s).unwrap();
    let e = entry(s).unwrap();
    let g = Guard::capture(s, key.clone(), e.serial).unwrap();
    with_registry(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.job = Some(g.job);
        e.stage = Stage::Checking;
    });
    g
}
#[test]
fn stale_schema_preflight_and_same_stamp_changes_never_admit() {
    let mut s = shell();
    let (g, i) = begin(&mut s);
    let mut changed = metadata(&s, None);
    changed.document.raw["list_metadata"]["opaque"]["keep"] = json!(2);
    changed.document = Document::parse(changed.document.raw).unwrap();
    assert!(admitted(&s, &g, &i, &changed).is_none());
    assert_eq!(entry(&s).unwrap().stage, Stage::Preparing);
    s.slack_lists.view.as_mut().unwrap().revision = next_id();
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_none());
}
#[test]
fn epoch_cookie_enterprise_transport_and_visit_fence_original_attempt() {
    let mut s = shell();
    let (g, _) = begin(&mut s);
    assert!(g.current(&s, Stage::Preparing));
    s.core.session.as_mut().unwrap().d_cookie.push('x');
    assert!(!g.current(&s, Stage::Preparing));
    s.core.session.as_mut().unwrap().d_cookie.pop();
    let team = s.core.active_team.clone().unwrap();
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = Some("E".into());
    assert!(!g.current(&s, Stage::Preparing));
    s.core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .enterprise_id = None;
    s.core.account_epoch += 1;
    assert!(!g.current(&s, Stage::Preparing));
    s.core.account_epoch -= 1;
    s.slack_lists.view.as_mut().unwrap().generation = next_id();
    assert!(!g.current(&s, Stage::Preparing));
}
#[test]
fn unknown_ack_and_duplicate_completion_never_project_or_replay() {
    let mut s = shell();
    let g = sent(&mut s);
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document
        .clone();
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::NotRequested(Error::Offline(
            "offline".into(),
        ))),
    );
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document,
        before
    );
    assert!(prepare(&mut s, g.serial).is_none());
}
#[test]
fn retained_receipt_survives_real_shell_replacement_and_late_response() {
    let mut s = shell();
    let g = sent(&mut s);
    let original = s.core.active_account.clone();
    let mut replacement = shell();
    assert!(!blocks_mutation(&replacement));
    completed(
        &mut replacement,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert!(!blocks_mutation(&replacement));
    replacement.core.active_account = original;
    let file =
        serde_json::from_value(with_registry(|r| r.entries[&g.key].original.raw.clone())).unwrap();
    assert!(super::super::open_shell(&mut replacement, file));
    assert_eq!(entry(&replacement).unwrap().stage, Stage::Uncertain);
}
#[test]
fn late_rate_limit_retains_owner_deadline_before_projection_and_shell_return() {
    let mut s = shell();
    let g = sent(&mut s);
    super::super::close_shell(&mut s);
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::HttpStatus {
            status: 503,
            retry_after_secs: Some(120),
        })),
    );
    assert!(registry::blocked(&g.key.owner));
    let other = shell();
    assert!(!registry::blocked(&Key::current(&other).unwrap().owner));
}
#[test]
fn full_positive_readback_changes_only_document_not_partial_rows() {
    let mut s = shell();
    uncertain(&mut s);
    let g = checking(&s);
    let before = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    let fresh = metadata(&s, Some("Delivery"));
    assert!(checked(&mut s, &g, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        before
    );
    assert!(!before.complete());
}
#[test]
fn stale_close_reopen_readback_never_projects_or_releases() {
    let mut s = shell();
    uncertain(&mut s);
    let g = checking(&s);
    let fresh = metadata(&s, Some("Delivery"));
    s.slack_lists.view.as_mut().unwrap().generation = next_id();
    assert!(!checked(&mut s, &g, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
}
#[test]
fn original_or_changed_metadata_read_requires_explicit_current_review() {
    let mut s = shell();
    uncertain(&mut s);
    let g = checking(&s);
    let original = metadata(&s, None);
    assert!(checked(&mut s, &g, Ok(original)));
    let e = entry(&s).unwrap();
    assert_eq!(e.stage, Stage::Conflict);
    assert!(e.review.unwrap().current(&s));
    let g = checking(&s);
    let mut partial = metadata(&s, Some("Delivery"));
    partial.document.raw["list_metadata"]
        .as_object_mut()
        .unwrap()
        .remove("opaque");
    partial.document = Document::parse(partial.document.raw).unwrap();
    assert!(checked(&mut s, &g, Ok(partial)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    assert!(entry(&s).unwrap().review.unwrap().current(&s));
    s.slack_lists.view.as_mut().unwrap().revision = next_id();
    assert!(!entry(&s).unwrap().review.unwrap().current(&s));
}
#[test]
fn contradictory_foreign_tombstone_and_malformed_file_reads_are_not_positive() {
    let s = shell();
    let raw = metadata(&s, None).document.raw;
    for value in [
        json!({"ok":true,"error":"bad","file":raw}),
        json!({"ok":false,"file":raw}),
        json!({"file":raw}),
    ] {
        assert!(network::document(value, "FL1").is_err());
    }
    for field in ["is_deleted", "is_tombstoned", "mode"] {
        let mut f = raw.clone();
        f[field] = json!("malformed");
        assert!(network::document(json!({"ok":true,"file":f}), "FL1").is_err());
    }
    assert!(network::document(json!({"ok":true,"file":raw}), "foreign").is_err());
}
#[test]
fn rename_serializes_existing_mutation_and_editor_admission() {
    let mut s = shell();
    let visit = generation(&s).unwrap();
    assert!(open_shell(&mut s, "name", visit));
    assert!(begin_edit(&mut s, "row1", "done").is_err());
    assert!(editor::begin_shell(&mut s, "row1", "name", visit).is_err());
    assert!(!create::presentation(&s).can_open);
}
#[test]
fn duplicate_old_check_cannot_retire_newer_read_job() {
    let mut s = shell();
    uncertain(&mut s);
    let old = checking(&s);
    assert!(!checked(&mut s, &old, Err(Error::Transport("lost".into()))));
    let new = checking(&s);
    assert_ne!(old.job, new.job);
    assert!(!checked(
        &mut s,
        &old,
        Err(Error::Transport("duplicate".into()))
    ));
    assert!(new.current(&s, Stage::Checking));
    let fresh = metadata(&s, Some("Delivery"));
    assert!(checked(&mut s, &new, Ok(fresh)));
}
#[test]
fn stale_current_view_can_explicitly_check_without_new_mutation() {
    let mut s = shell();
    uncertain(&mut s);
    s.slack_lists.view.as_mut().unwrap().stale = true;
    s.slack_lists.view.as_mut().unwrap().revision = next_id();
    let key = Key::current(&s).unwrap();
    let old = entry(&s).unwrap();
    assert!(!owner::available(&s));
    assert!(owner::readable(&s));
    let g = Guard::capture_read(&s, key.clone(), old.serial).unwrap();
    with_registry(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.job = Some(g.job);
        e.stage = Stage::Checking;
    });
    let fresh = metadata(&s, Some("Delivery"));
    assert!(checked(&mut s, &g, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert!(s.slack_lists.view.as_ref().unwrap().stale);
}
#[test]
fn existing_tick_publishes_cooldown_expiry_without_unrelated_events() {
    let mut s = shell();
    let key = Key::current(&s).unwrap();
    let now = std::time::Instant::now();
    registry::record(
        &key.owner,
        &Error::RateLimited {
            retry_after_secs: Some(30),
        },
    );
    assert!(poll_at(&mut s, now));
    assert!(!poll_at(&mut s, now));
    assert!(poll_at(&mut s, now + std::time::Duration::from_secs(31)));
    assert!(!poll_at(&mut s, now + std::time::Duration::from_secs(31)));
}
#[test]
fn changed_sibling_view_type_or_deleted_target_is_explicit_conflict_not_observed() {
    for kind in ["sibling", "view", "type", "removed"] {
        let mut s = shell();
        uncertain(&mut s);
        let g = checking(&s);
        let mut fresh = metadata(&s, Some("Delivery"));
        match kind {
            "sibling" => {
                fresh.document.raw["list_metadata"]["schema"][1]["name"] = json!("Changed sibling")
            }
            "view" => {
                fresh.document.raw["list_metadata"]["views"] =
                    json!([{ "id":"V_OTHER","name":"Other view" }])
            }
            "type" => fresh.document.raw["list_metadata"]["schema"][0]["type"] = json!("rich_text"),
            _ => {
                fresh.document.raw["list_metadata"]["schema"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
        }
        fresh.document = Document::parse(fresh.document.raw).unwrap();
        assert!(checked(&mut s, &g, Ok(fresh)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
        assert!(entry(&s).unwrap().review.unwrap().current(&s));
    }
}
#[test]
fn foreign_incarnation_never_projects_or_offers_conflict_retirement() {
    let mut s = shell();
    uncertain(&mut s);
    let g = checking(&s);
    let before = metadata(&s, None).document;
    let mut fresh = metadata(&s, Some("Delivery"));
    fresh.document.raw["created"] = json!(900);
    fresh.document = Document::parse(fresh.document.raw).unwrap();
    assert!(!checked(&mut s, &g, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(entry(&s).unwrap().review.is_none());
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document,
        before
    );
}

#[test]
fn editing_changed_document_or_stale_view_disables_confirmation_and_keeps_input() {
    for changed_document in [true, false] {
        let mut s = shell();
        let visit = generation(&s).unwrap();
        assert!(open_shell(&mut s, "name", visit));
        let key = Key::current(&s).unwrap();
        let original = entry(&s).unwrap();
        with_registry(|r| r.entries.get_mut(&key).unwrap().name = "Keep this name".into());
        assert!(presentation(&s).unwrap().can_confirm);
        let view = s.slack_lists.view.as_mut().unwrap();
        if changed_document {
            let mut raw = view.loaded.as_ref().unwrap().document.raw.clone();
            raw["list_metadata"]["schema"][0]["name"] = json!("Realtime replacement");
            view.loaded.as_mut().unwrap().document = Document::parse(raw).unwrap();
        } else {
            view.stale = true;
        }
        view.revision = next_id();
        let shown = presentation(&s).unwrap();
        assert_eq!(shown.stage, Stage::Editing); // Cancel remains available.
        assert!(!shown.can_confirm);
        assert!(shown.message.contains("Cancel, close the List, refresh it"));
        assert_eq!(shown.name, "Keep this name");
        assert!(prepare(&mut s, original.serial).is_none());
        let retained = entry(&s).unwrap();
        assert_eq!(retained.serial, original.serial);
        assert_eq!(retained.original, original.original);
        assert_eq!(retained.name, "Keep this name");
        assert_eq!(retained.stage, Stage::Editing);
        assert!(retained.intent.is_none() && retained.job.is_none());
    }
}

mod views;

mod view_create;
