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
    let s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-field-delete",
    );
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
fn delete_loaded(original: &crate::slack_lists::Loaded) -> crate::slack_lists::Loaded {
    let mut desired = original.clone();
    delete_field(&mut desired.document.raw).unwrap();
    desired.document = Document::parse(desired.document.raw).unwrap();
    desired
}
#[test]
fn adapter_requires_exact_flag_and_only_two_field_delete_aliases() {
    for alias in ["slack-list-field-delete", "slack-list-field-delete-narrow"] {
        assert!(gate(Some("1"), Some(alias)));
        for flag in [None, Some("true"), Some("0"), Some("")] {
            assert!(!gate(flag, Some(alias)));
        }
    }
    for alias in [
        None,
        Some("slack-list"),
        Some("slack-list-column-rename"),
        Some("slack-list-field-delete-extra"),
    ] {
        assert!(!gate(Some("1"), alias));
    }
}
#[test]
fn synthetic_delete_changes_only_schema_and_view_columns() {
    let (s, _cleanup) = shell();
    let original = &s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document
        .raw;
    let mut desired = original.clone();
    delete_field(&mut desired).unwrap();
    let mut restored = desired.clone();
    restored["list_metadata"]["schema"] = original["list_metadata"]["schema"].clone();
    for (before, after) in original["list_metadata"]["views"]
        .as_array()
        .unwrap()
        .iter()
        .zip(restored["list_metadata"]["views"].as_array_mut().unwrap())
    {
        after["columns"] = before["columns"].clone();
    }
    assert_eq!(&restored, original);
    assert_eq!(
        desired["list_metadata"]["views"][2],
        original["list_metadata"]["views"][2]
    );
    assert_eq!(
        desired["list_metadata"]["integrations"],
        original["list_metadata"]["integrations"]
    );
    assert!(delete_field(&mut desired).is_err());
}
#[test]
fn stale_confirmation_and_policy_changes_cannot_admit_the_replaced_field() {
    let (mut s, _cleanup) = shell();
    let initial = entry(&s).unwrap();
    let visit = generation(&s).unwrap();
    assert!(crate::slack_lists::rename::field_delete::choose_shell(
        &s,
        initial.serial,
        visit,
        "C_ABSENT"
    ));
    let changed = entry(&s).unwrap();
    assert_ne!(changed.serial, initial.serial);
    assert!(super::super::super::prepare(&mut s, initial.serial).is_none());
    assert!(!super::super::super::close_entry(&s, initial.serial));
    assert_eq!(entry(&s).unwrap().serial, changed.serial);
    let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    for flag in [Some(true), None] {
        s.slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap()
            .list_experiments = flag.map(|on| super_platinum_core::slack::api::ListExperiments {
            schema_owner_only: on,
            ..Default::default()
        });
        assert!(
            !crate::slack_lists::rename::presentation(&s)
                .unwrap()
                .can_confirm
        );
        assert!(super::super::super::prepare(&mut s, changed.serial).is_none());
        let retained = entry(&s).unwrap();
        assert_eq!(retained.stage, Stage::Editing);
        assert!(retained.job.is_none() && retained.intent.is_none());
    }
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .list_experiments = original.list_experiments;
    assert!(crate::slack_lists::rename::field_delete::choose_shell(
        &s,
        changed.serial,
        visit,
        "C_HIDDEN"
    ));
    assert!(
        crate::slack_lists::rename::presentation(&s)
            .unwrap()
            .can_confirm
    );
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
fn real_permit_lost_completion_conflict_and_observation_retain_raw_population() {
    let (mut s, cleanup) = shell();
    let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let serial = entry(&s).unwrap().serial;
    let (guard, intent) = super::super::super::prepare(&mut s, serial).unwrap();
    let permit = super::super::super::admitted(&s, &guard, &intent, &metadata(&original)).unwrap();
    let request = permit.request(&s.core.client, &session());
    assert_eq!(
        url::Url::parse(&request.url).unwrap().path(),
        "/api/lists.columns.delete"
    );
    let RequestBody::Form(fields) = request.body else {
        panic!("form")
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-not-a-token".into()),
            ("list_id".into(), "F_LIST_FIXTURE".into()),
            ("key".into(), "hidden".into()),
            ("_x_reason".into(), "delete-column".into())
        ]
    );
    super::super::super::completed(
        &mut s,
        &guard,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
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
        original.document
    );
    let check = begin_check(&s, &cleanup.0);
    assert!(super::super::super::checked(
        &mut s,
        &check,
        Ok(metadata(&original))
    ));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let check = begin_check(&s, &cleanup.0);
    let desired = delete_loaded(&original);
    assert!(super::super::super::checked(
        &mut s,
        &check,
        Ok(metadata(&desired))
    ));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let now = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(now.document, desired.document);
    assert!(now.document.column("C_HIDDEN").is_none());
    assert_eq!(now.records, original.records);
    assert_eq!(
        now.records.raw["response_metadata"]["next_cursor"],
        "precision-opaque+/="
    );
    assert_eq!(now.records.records.len(), 73);
    for (i, row) in now.records.records.iter().enumerate() {
        assert_eq!(row.fields["hidden"]["checkbox"], json!(i % 2 == 0));
        assert_eq!(row.fields["hidden"]["opaque"], json!({"keep":"raw-cell"}));
    }
}
#[test]
fn retired_check_cannot_replace_reopened_population_or_retained_intent() {
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
    let serial = entry(&s).unwrap().serial;
    let (write, intent) = super::super::super::prepare(&mut s, serial).unwrap();
    super::super::super::admitted(&s, &write, &intent, &metadata(&original)).unwrap();
    super::super::super::completed(
        &mut s,
        &write,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
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
    let desired = delete_loaded(&original);
    assert!(seed_reopened(&mut s, visit, desired.clone(), find.clone()).is_err());
    seed_reopened(&mut s, visit, original.clone(), find.clone()).unwrap();
    assert!(!super::super::super::checked(
        &mut s,
        &held,
        Ok(metadata(&desired))
    ));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(entry(&s).unwrap().original, original.document);
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.loaded.as_ref().unwrap().document, original.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, original.records);
    assert_eq!(v.layout.find.revision, find.revision);
    let current = begin_check(&s, &cleanup.0);
    assert!(super::super::super::checked(
        &mut s,
        &current,
        Ok(metadata(&desired))
    ));
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
        original.records
    );
}
#[test]
fn experiment_read_uses_exact_existing_authenticated_form() {
    let request = super_platinum_core::slack::api::list_experiments(
        &super_platinum_core::slack::SlackClient::default(),
        &session(),
    );
    let raw = policy_response(&request).unwrap().unwrap();
    assert!(
        !super_platinum_core::slack::api::parse_list_experiments(&raw)
            .unwrap()
            .schema_owner_only
    );
    let mut malformed = request;
    let RequestBody::Form(fields) = &mut malformed.body else {
        panic!("form")
    };
    fields.push(("_x_reason".into(), "guessed".into()));
    assert!(policy_response(&malformed).is_err());
}
