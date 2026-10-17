use super::*;
use serde_json::json;

struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn setup(channel: bool) -> (ShellState, Cleanup) {
    let mut s = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        if channel {
            "slack-list-channel-field-create"
        } else {
            "slack-list-people-field-create"
        },
    );
    if channel {
        crate::slack_lists::fixture::text_field_create::channel::cache::seed(&mut s).unwrap();
    } else {
        crate::slack_lists::fixture::text_field_create::people::cache::seed(&mut s).unwrap();
    }
    let stamp = Stamp::capture(&s).unwrap();
    begin(&s, stamp, "".into()).unwrap();
    let key = Key::current(&s).unwrap();
    (s, Cleanup(key))
}
fn apply(s: &ShellState, e: Edit) {
    assert!(edit_shell(s, Stamp::capture(s).unwrap(), e));
}
fn options(s: &ShellState) -> EntityCreateOptions {
    presentation(s).unwrap().options
}

#[test]
fn actual_configuration_callbacks_preserve_order_single_retention_and_null_omission() {
    for channel in [false, true] {
        let (s, _cleanup) = setup(channel);
        let (a, b) = if channel {
            ("C00000001", "G00000002")
        } else {
            ("U00000001", "U00000002")
        };
        apply(&s, Edit::Format(EntityCreateFormat::Multiple));
        apply(&s, Edit::Activate(b.into()));
        apply(&s, Edit::Activate(a.into()));
        assert_eq!(options(&s).selected_ids(), [b, a]);
        let old = Stamp::capture(&s).unwrap();
        apply(&s, Edit::Format(EntityCreateFormat::Single));
        assert_eq!(options(&s).selected_ids(), [b, a]);
        assert_eq!(options(&s).single_display_default(), Some(b));
        assert!(!edit_shell(&s, old, Edit::Activate(a.into())));
        apply(&s, Edit::Activate(b.into()));
        assert_eq!(options(&s).selected_ids(), [b]);
        apply(&s, Edit::Activate(b.into()));
        assert_eq!(options(&s).wire()["default_value"], json!(null));
        apply(&s, Edit::Format(EntityCreateFormat::Multiple));
        apply(&s, Edit::Activate(a.into()));
        apply(&s, Edit::Remove(a.into()));
        let wire = options(&s).wire();
        assert!(wire.get("default_value").is_none());
        assert!(wire.get("default_value_typed").is_none());
        assert!(crate::slack_lists::rename::close_entry(
            &s,
            Stamp::capture(&s).unwrap().serial
        ));
    }
}
#[test]
fn selection_admits_only_current_results_and_current_eligible_cache() {
    let (mut s, _cleanup) = setup(false);
    assert!(!edit_shell(
        &s,
        Stamp::capture(&s).unwrap(),
        Edit::Activate("U88888888".into())
    ));
    let stamp = Stamp::capture(&s).unwrap();
    begin(&s, stamp, "Alice".into()).unwrap();
    assert!(!edit_shell(&s, stamp, Edit::Activate("U00000002".into())));
    let team = s.core.active_team.clone().unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut("U00000001")
        .unwrap()
        .deleted = true;
    assert!(!edit_shell(&s, stamp, Edit::Activate("U00000001".into())));
    assert!(options(&s).selected_ids().is_empty());
    assert!(crate::slack_lists::rename::close_entry(&s, stamp.serial));
}
#[test]
fn actual_prepare_freezes_full_config_and_retires_all_setting_callbacks() {
    let (mut s, _cleanup) = setup(false);
    apply(&s, Edit::Format(EntityCreateFormat::Multiple));
    apply(&s, Edit::Activate("U00000002".into()));
    apply(&s, Edit::Activate("U00000001".into()));
    apply(&s, Edit::Format(EntityCreateFormat::Single));
    apply(&s, Edit::ShowNames(false));
    let p = presentation(&s).unwrap();
    assert!(input_shell(
        &s,
        p.stamp.serial,
        p.stamp.visit,
        p.stamp.selection,
        p.stamp.choice,
        "Reviewers".into()
    ));
    let expected = p.options.wire();
    let (_, intent) = crate::slack_lists::rename::prepare(&mut s, p.stamp.serial).unwrap();
    let Intent::NewTextField(intent) = intent else {
        panic!()
    };
    assert_eq!(intent.options(), Some(expected.clone()));
    let locked = presentation(&s).unwrap();
    assert!(!locked.enabled);
    assert_eq!(locked.options, p.options);
    assert_eq!(locked.selected, p.selected);
    assert!(!edit_shell(&s, p.stamp, Edit::ShowNames(true)));
    assert!(!edit_shell(&s, p.stamp, Edit::Activate("U00000002".into())));
    assert!(crate::slack_lists::rename::prepare(&mut s, p.stamp.serial).is_none());
    assert_eq!(intent.options(), Some(expected));
}
#[test]
fn current_ineligibility_blocks_capture_but_removal_and_channel_visibility_rules_remain_local() {
    let (mut s, _cleanup) = setup(true);
    assert!(!edit_shell(
        &s,
        Stamp::capture(&s).unwrap(),
        Edit::ShowNames(false)
    ));
    apply(&s, Edit::Format(EntityCreateFormat::Multiple));
    apply(&s, Edit::Activate("C00000001".into()));
    let team = s.core.active_team.clone().unwrap();
    s.core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .channels
        .get_mut("C00000001")
        .unwrap()
        .is_archived = true;
    assert!(capture(&s, &entry(&s).unwrap()).is_err());
    apply(&s, Edit::Remove("C00000001".into()));
    assert!(capture(&s, &entry(&s).unwrap()).is_ok());
    let p = presentation(&s).unwrap();
    assert!(kind_shell(
        &s,
        p.stamp.serial,
        p.stamp.visit,
        p.stamp.selection,
        p.stamp.choice,
        "text"
    ));
    let c = choice(&s).unwrap();
    assert!(kind_shell(
        &s,
        p.stamp.serial,
        p.stamp.visit,
        p.stamp.selection,
        c,
        "channel"
    ));
    assert!(!edit_shell(
        &s,
        p.stamp,
        Edit::Format(EntityCreateFormat::Multiple)
    ));
    assert_eq!(
        options(&s),
        EntityCreateOptions::new(EntityCreateDomain::Channel)
    );
    assert!(crate::slack_lists::rename::close_entry(&s, p.stamp.serial));
}
#[test]
fn retained_settings_are_readable_but_never_mutable_after_admission_or_stale_population() {
    let (mut s, _cleanup) = setup(false);
    apply(&s, Edit::Format(EntityCreateFormat::Multiple));
    apply(&s, Edit::Activate("U00000002".into()));
    apply(&s, Edit::Activate("U00000001".into()));
    apply(&s, Edit::ShowNames(false));
    let before = presentation(&s).unwrap();
    assert!(before.enabled);
    let key = Key::current(&s).unwrap();
    for stage in [
        Stage::Preparing,
        Stage::Saving,
        Stage::Uncertain,
        Stage::Checking,
        Stage::Conflict,
        Stage::Observed,
        Stage::NotRequested,
    ] {
        with_registry(|r| r.entries.get_mut(&key).unwrap().stage = stage);
        let p = presentation(&s).unwrap();
        assert!(!p.enabled, "{stage:?}");
        assert_eq!(p.options, before.options);
        assert_eq!(p.selected, before.selected);
        assert!(p.directory == before.directory);
        assert!(!edit_shell(&s, p.stamp, Edit::Remove("U00000002".into())));
        assert!(begin(&s, p.stamp, "new".into()).is_none());
    }
    with_registry(|r| r.entries.get_mut(&key).unwrap().stage = Stage::Editing);
    let l = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    let mut raw = l.records.raw.clone();
    raw["response_metadata"]["next_cursor"] = json!("changed");
    l.records = RecordsPage::parse(raw).unwrap();
    let p = presentation(&s).unwrap();
    assert!(!p.enabled);
    assert_eq!(p.options, before.options);
    assert_eq!(p.selected, before.selected);
    assert!(!edit_shell(&s, p.stamp, Edit::ShowNames(true)));
    // A different real account loses presentation too; no cross-owner retention.
    s.core.account_epoch += 1;
    assert!(presentation(&s).is_none());
}
