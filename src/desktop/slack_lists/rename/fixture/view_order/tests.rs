use super::*;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn shell() -> (ShellState, Cleanup) {
    let mut s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-view-order",
    );
    // Explicit offline test setup before exercising real worker transitions.
    let view = s.slack_lists.view.as_mut().unwrap();
    view.loading = false;
    view.stale = false;
    view.unavailable = false;
    let key = Key::current(&s).unwrap();
    (s, Cleanup(key))
}
fn metadata(loaded: &crate::slack_lists::Loaded) -> network::Metadata {
    network::Metadata {
        document: loaded.document.clone(),
        allow: loaded.allow_lists.clone(),
        personal: loaded.personal_task_lists.clone().unwrap(),
        experiments: loaded.list_experiments,
    }
}
fn begin_check(shell: &ShellState, key: &Key) -> Guard {
    let serial = entry(shell).unwrap().serial;
    let guard = Guard::capture_read(shell, key.clone(), serial).unwrap();
    with_registry(|r| {
        let e = r.entries.get_mut(key).unwrap();
        e.stage = Stage::Checking;
        e.job = Some(guard.job);
    });
    guard
}
fn desired(original: &crate::slack_lists::Loaded) -> crate::slack_lists::Loaded {
    let mut out = original.clone();
    set_position(&mut out.document.raw).unwrap();
    out.document = Document::parse(out.document.raw).unwrap();
    out
}
fn admitted_write(s: &mut ShellState, original: &crate::slack_lists::Loaded) -> Guard {
    let serial = entry(s).unwrap().serial;
    let (guard, intent) = super::super::super::prepare(s, serial).unwrap();
    let permit = super::super::super::admitted(s, &guard, &intent, &metadata(original)).unwrap();
    let request = permit.request(&s.core.client, &session());
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/lists.views.update"
    );
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-not-a-token".into()),
            ("list_id".into(), "F_LIST_FIXTURE".into()),
            ("id".into(), "V_ORDER_SELECTED".into()),
            ("position".into(), "0000000000".into()),
            ("_x_reason".into(), "update-view".into())
        ]
    );
    assert!(super::super::super::admitted(s, &guard, &intent, &metadata(original)).is_none());
    guard
}
fn lost(s: &mut ShellState, guard: &Guard) {
    super::super::super::completed(
        s,
        guard,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    assert_eq!(entry(s).unwrap().stage, Stage::Uncertain);
}
#[test]
fn adapter_gate_requires_exact_flag_and_two_order_aliases() {
    for alias in ["slack-list-view-order", "slack-list-view-order-narrow"] {
        assert!(gate(Some("1"), Some(alias)));
        for flag in [None, Some(""), Some("true"), Some("0")] {
            assert!(!gate(flag, Some(alias)));
        }
    }
    for alias in [
        None,
        Some("slack-list"),
        Some("slack-list-view-rename"),
        Some("slack-list-view-order-extra"),
    ] {
        assert!(!gate(Some("1"), alias));
    }
}
#[test]
fn synthetic_position_changes_only_target_member_and_refuses_replay() {
    let (s, _cleanup) = shell();
    let original = entry(&s).unwrap().original.raw;
    let mut after = original.clone();
    set_position(&mut after).unwrap();
    let mut restored = after.clone();
    restored["list_metadata"]["views"][1]["position"] = json!("0000000002");
    assert_eq!(restored, original);
    assert_eq!(
        after["list_metadata"]["views"][0],
        original["list_metadata"]["views"][0]
    );
    assert_eq!(
        after["list_metadata"]["views"][2],
        original["list_metadata"]["views"][2]
    );
    assert!(set_position(&mut after).is_err());
}
#[test]
fn canceled_save_callback_cannot_admit_or_close_new_immutable_move() {
    let (mut s, _cleanup) = shell();
    let first = entry(&s).unwrap();
    assert!(super::super::super::close_entry(&s, first.serial));
    let visit = generation(&s).unwrap();
    let selected = selection(&s).unwrap();
    assert!(super::super::super::view_order::open_shell(
        &mut s,
        visit,
        selected,
        "V_ORDER_SELECTED",
        Direction::Up
    ));
    let current = entry(&s).unwrap();
    assert_ne!(first.serial, current.serial);
    assert!(super::super::super::prepare(&mut s, first.serial).is_none());
    assert!(!super::super::super::close_entry(&s, first.serial));
    let after = entry(&s).unwrap();
    assert_eq!(after.serial, current.serial);
    assert_eq!(after.target, target());
    assert!(after.job.is_none() && after.intent.is_none());
    assert_eq!(after.original, first.original);
}
#[test]
fn real_worker_conflict_and_observation_preserve_selection_raw_rows_and_cursor() {
    let (mut s, cleanup) = shell();
    let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let before_selection = selection(&s);
    let selected = selected_view(&s).unwrap().to_owned();
    let guard = admitted_write(&mut s, &original);
    lost(&mut s, &guard);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document,
        original.document
    );
    let check = begin_check(&s, &cleanup.0);
    assert!(super::super::super::checked(
        &mut s,
        &check,
        Ok(metadata(&original))
    ));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let expected = desired(&original);
    let check = begin_check(&s, &cleanup.0);
    assert!(super::super::super::checked(
        &mut s,
        &check,
        Ok(metadata(&expected))
    ));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let loaded = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(loaded.document, expected.document);
    assert_eq!(loaded.records, original.records);
    assert_eq!(loaded.records.records.len(), 73);
    assert_eq!(
        loaded.records.raw["response_metadata"]["next_cursor"],
        "precision-opaque+/="
    );
    assert_eq!(selection(&s), before_selection);
    assert_eq!(selected_view(&s), Some(selected.as_str()));
    assert!(!super::super::super::view_order::can_open(
        &s,
        Direction::Up
    ));
}
#[test]
fn retired_read_cannot_install_position_into_reopened_owner() {
    let (mut s, cleanup) = shell();
    {
        let v = s.slack_lists.view.as_mut().unwrap();
        v.layout.find.input = "Launch item 0".into();
        v.layout.find.applied = "Launch item 0".into();
        v.layout.find.revision = next_id();
    }
    crate::slack_lists::rebuild_rows(&mut s);
    let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let find = s.slack_lists.view.as_ref().unwrap().layout.find.clone();
    let guard = admitted_write(&mut s, &original);
    lost(&mut s, &guard);
    let conflict = begin_check(&s, &cleanup.0);
    assert!(super::super::super::checked(
        &mut s,
        &conflict,
        Ok(metadata(&original))
    ));
    let held = begin_check(&s, &cleanup.0);
    let old_visit = generation(&s).unwrap();
    crate::slack_lists::close_shell(&mut s);
    s.core.account_epoch += 1;
    assert!(crate::slack_lists::open_shell(
        &mut s,
        serde_json::from_value(original.document.raw.clone()).unwrap()
    ));
    crate::slack_lists::fixture::refresh(&mut s);
    let visit = generation(&s).unwrap();
    assert_ne!(old_visit, visit);
    assert!(seed_reopened(&mut s, old_visit, original.clone(), find.clone()).is_err());
    let expected = desired(&original);
    assert!(seed_reopened(&mut s, visit, expected.clone(), find.clone()).is_err());
    seed_reopened(&mut s, visit, original.clone(), find.clone()).unwrap();
    assert!(!super::super::super::checked(
        &mut s,
        &held,
        Ok(metadata(&expected))
    ));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.loaded.as_ref().unwrap().document, original.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, original.records);
    assert_eq!(v.layout.find.revision, find.revision);
    assert_eq!(selected_view(&s), Some("V_ORDER_SELECTED"));
    let current = begin_check(&s, &cleanup.0);
    assert!(super::super::super::checked(
        &mut s,
        &current,
        Ok(metadata(&expected))
    ));
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        original.records
    );
}
#[test]
fn all_three_metadata_reads_are_exact_existing_forms() {
    let client = super_platinum_core::slack::SlackClient::default();
    let session = session();
    for request in [
        super_platinum_core::slack::api::file_info(&client, &session, "F_LIST_FIXTURE"),
        super_platinum_core::slack::api::list_allow_lists(&client, &session),
        super_platinum_core::slack::api::list_personal_task_preferences(&client, &session),
    ] {
        read_form(&request).unwrap();
        let mut bad = request;
        let RequestBody::Form(fields) = &mut bad.body else {
            panic!()
        };
        fields.push(("extra".into(), "true".into()));
        assert!(read_form(&bad).is_err());
    }
}
