use super::*;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn shell(board: bool) -> (ShellState, Cleanup) {
    let alias = if board {
        "slack-list-board-grouping"
    } else {
        "slack-list-view-grouping"
    };
    let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
    crate::slack_lists::fixture::view_grouping::dynamic_for_test(&mut s);
    let k = Key::current(&s).unwrap();
    (s, Cleanup(k))
}
fn selected(s: &ShellState) -> (u64, u64) {
    let e = entry(s).unwrap();
    assert!(super::super::super::grouping::change(
        s,
        e.serial,
        generation(s).unwrap(),
        e.grouping_choice,
        &ViewGroupingChoice::Column {
            column_id: "C_CHECK".into()
        },
        ViewGroupingChoice::Column {
            column_id: "C_SELECT".into()
        }
    ));
    (e.serial, generation(s).unwrap())
}
#[test]
fn owned_gate_requires_exact_flag_and_four_aliases() {
    for alias in [
        "slack-list-view-grouping",
        "slack-list-view-grouping-narrow",
        "slack-list-board-grouping",
        "slack-list-board-grouping-narrow",
    ] {
        assert!(gate(Some("1"), Some(alias)));
        for f in [None, Some("true"), Some("0")] {
            assert!(!gate(f, Some(alias)));
        }
    }
    for alias in [
        None,
        Some("slack-list"),
        Some("slack-list-view-grouping-extra"),
    ] {
        assert!(!gate(Some("1"), alias));
    }
}
#[test]
fn actual_grouping_permit_builds_five_fields_and_keeps_full_raw_source() {
    for board in [false, true] {
        let (mut s, _cleanup) = shell(board);
        let (serial, _) = selected(&s);
        let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let raw = original.records.raw.clone();
        let (g, intent) = prepare(&mut s, serial).unwrap();
        let permit = admitted(
            &s,
            &g,
            &intent,
            &network::Metadata {
                document: original.document.clone(),
                allow: original.allow_lists.clone(),
                personal: original.personal_task_lists.clone().unwrap(),
            },
        )
        .unwrap();
        let req = permit.request(&s.core.client, &session());
        let RequestBody::Form(fields) = req.body else {
            panic!("form")
        };
        assert_eq!(
            fields,
            vec![
                ("token".into(), "fixture-not-a-token".into()),
                ("list_id".into(), "F_LIST_FIXTURE".into()),
                ("id".into(), "V_GROUP".into()),
                ("grouping".into(), "{\"group_by\":\"status\"}".into()),
                ("_x_reason".into(), "update-view".into())
            ]
        );
        assert_eq!(
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .records
                .raw,
            raw
        );
        assert_eq!(raw["records"].as_array().unwrap().len(), 73);
    }
}
#[test]
fn replacement_requires_accepted_conflict_not_original_then_real_old_checked_refuses() {
    for board in [false, true] {
        let (mut s, cleanup) = shell(board);
        let (serial, _) = selected(&s);
        {
            let v = s.slack_lists.view.as_mut().unwrap();
            v.layout.find.input = "Launch item 0".into();
            v.layout.find.applied = "Launch item 0".into();
            v.layout.find.revision = next_id();
        }
        crate::slack_lists::rebuild_rows(&mut s);
        let original = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let find = s.slack_lists.view.as_ref().unwrap().layout.find.clone();
        let (write, intent) = prepare(&mut s, serial).unwrap();
        admitted(
            &s,
            &write,
            &intent,
            &network::Metadata {
                document: original.document.clone(),
                allow: original.allow_lists.clone(),
                personal: original.personal_task_lists.clone().unwrap(),
            },
        )
        .unwrap();
        super::super::super::completed(
            &mut s,
            &write,
            Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
                "lost".into(),
            ))),
        );
        let mut conflict = original.clone();
        set_grouping(&mut conflict.document.raw, json!({"group_by":"other"})).unwrap();
        conflict.document = Document::parse(conflict.document.raw).unwrap();
        // Actual accepted Conflict read, not fixture assignment.
        let check = Guard::capture_read(&s, cleanup.0.clone(), serial).unwrap();
        with_registry(|r| {
            let e = r.entries.get_mut(&cleanup.0).unwrap();
            e.job = Some(check.job);
            e.stage = Stage::Checking;
        });
        assert!(super::super::super::checked(
            &mut s,
            &check,
            Ok(network::Metadata {
                document: conflict.document.clone(),
                allow: conflict.allow_lists.clone(),
                personal: conflict.personal_task_lists.clone().unwrap()
            })
        ));
        assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
        let held = Guard::capture_read(&s, cleanup.0.clone(), serial).unwrap();
        with_registry(|r| {
            let e = r.entries.get_mut(&cleanup.0).unwrap();
            e.job = Some(held.job);
            e.stage = Stage::Checking;
        });
        let old = generation(&s).unwrap();
        crate::slack_lists::close_shell(&mut s);
        s.core.account_epoch += 1;
        assert!(crate::slack_lists::open_shell(
            &mut s,
            serde_json::from_value(conflict.document.raw.clone()).unwrap()
        ));
        crate::slack_lists::fixture::refresh(&mut s);
        let visit = generation(&s).unwrap();
        assert_ne!(old, visit);
        assert!(seed_reopened(&mut s, visit, original.clone(), find.clone()).is_err());
        assert!(seed_reopened(&mut s, old, conflict.clone(), find.clone()).is_err());
        seed_reopened(&mut s, visit, conflict.clone(), find.clone()).unwrap();
        let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
        let mut desired = original.document.raw.clone();
        set_grouping(&mut desired, json!({"group_by":"status"})).unwrap();
        assert!(!super::super::super::checked(
            &mut s,
            &held,
            Ok(network::Metadata {
                document: Document::parse(desired).unwrap(),
                allow: original.allow_lists,
                personal: original.personal_task_lists.unwrap()
            })
        ));
        let v = s.slack_lists.view.as_ref().unwrap();
        assert_eq!(v.loaded.as_ref().unwrap().document, before.document);
        assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
        assert_eq!(v.layout.find.revision, find.revision);
        assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
        assert_eq!(entry(&s).unwrap().original, original.document);
    }
}

#[test]
fn static_grouping_cancel_reopens_dynamic_and_refuses_stale_or_preparing_close() {
    for alias in ["slack-list-view-grouping", "slack-list-board-grouping"] {
        let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
        let cleanup = Cleanup(Key::current(&s).unwrap());
        let old = entry(&s).unwrap();
        assert_eq!(old.target, Target::Grouping("V_GROUP".into()));
        assert_eq!(old.stage, Stage::Editing);
        assert!(!super::super::super::close_entry(
            &s,
            old.serial.checked_add(1).unwrap()
        ));
        assert_eq!(entry(&s).unwrap().serial, old.serial);
        crate::slack_lists::fixture::view_grouping::dynamic_for_test(&mut s);
        let fresh = entry(&s).unwrap();
        assert_ne!(fresh.serial, old.serial);
        assert_eq!(fresh.stage, Stage::Editing);
        assert!(!super::super::super::close_entry(&s, old.serial));
        assert_eq!(entry(&s).unwrap().serial, fresh.serial);
        assert_eq!(
            super::super::super::presentation(&s)
                .unwrap()
                .view_grouping
                .unwrap()
                .choices
                .len(),
            3
        );
        let (serial, _) = selected(&s);
        let (guard, _) = prepare(&mut s, serial).unwrap();
        let pending = entry(&s).unwrap();
        assert_eq!(pending.stage, Stage::Preparing);
        assert_eq!(pending.job, Some(guard.job));
        assert!(!close_static_for_test(&s));
        let retained = entry(&s).unwrap();
        assert_eq!(retained.serial, pending.serial);
        assert_eq!(retained.job, pending.job);
        assert_eq!(retained.stage, pending.stage);
        assert!(retained.intent.is_some());
        drop(cleanup);
    }
}
