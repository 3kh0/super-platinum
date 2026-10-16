use super::*;
use crate::slack_lists::rename::text_create;
use super_platinum_core::slack::client::RequestBody;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn text_shell() -> (ShellState, Cleanup) {
    let mut s = shell();
    let view = s.slack_lists.view.as_mut().unwrap();
    let loaded = view.loaded.as_mut().unwrap();
    let mut raw = loaded.document.raw.clone();
    raw["access"] = json!("owner");
    raw["list_limits"] = json!({"over_column_maximum":false});
    raw["list_metadata"]["schema"].as_array_mut().unwrap().extend([
        json!({"id":"C_COUNT","key":"count","name":"Count","type":"number","options":{"precision":2}})]);
    raw["list_metadata"]["views"] = json!([{"id":"V_TEXT","name":"Launch","type":"table","columns":[{"id":"name","key":"name","visible":true},{"id":"C_COUNT","key":"count","visible":true},{"id":"done","key":"done","visible":true}],"sorts":[{"key":"count","ascending":true}],"filters":[{"key":"done","operator":"is_not_empty","values":[]}],"info_column_filters":[],"grouping":{},"opaque":{"keep":1}}]);
    raw["list_metadata"]["default_view"] = json!("V_TEXT");
    loaded.document = Document::parse(raw).unwrap();
    let records=(0..73).map(|n|{let mut row=json!({"id":format!("R_{n:03}"),"fields":[{"key":"name","text":format!("Item {n:02}")},{"key":"count","number":[n%7]},{"key":"done","checkbox":n%2==1}],"opaque":{"keep":n}});if n==11 {row["parent_record_id"]=json!("R_010");}row}).collect::<Vec<_>>();
    loaded.records=RecordsPage::parse(json!({"ok":true,"records":records,"response_metadata":{"next_cursor":"create-opaque+/=","keep":true}})).unwrap();
    view.layout = Default::default();
    view.layout.find.input = "Item 0".into();
    view.layout.find.applied = "Item 0".into();
    view.layout.find.revision = 17;
    rebuild_rows(&mut s);
    assert_eq!(selected_view(&s), Some("V_TEXT"));
    let cleanup = Cleanup(Key::current(&s).unwrap());
    (s, cleanup)
}
fn open(s: &mut ShellState) -> u64 {
    let (visit, selection) = (generation(s).unwrap(), super::super::selection(s).unwrap());
    assert!(text_create::open_shell(s, visit, selection));
    entry(s).unwrap().serial
}
fn name(s: &ShellState, serial: u64, value: &str) -> bool {
    text_create::input_shell(
        s,
        serial,
        generation(s).unwrap(),
        super::super::selection(s).unwrap(),
        text_create::choice(s).unwrap(),
        value.into(),
    )
}
fn begin_text(s: &mut ShellState) -> (Guard, Intent) {
    let serial = open(s);
    assert!(name(s, serial, " Release notes "));
    prepare(s, serial).unwrap()
}
fn sent_text(s: &mut ShellState) -> (Guard, Intent) {
    let (g, i) = begin_text(s);
    assert!(
        admitted(s, &g, &i, &metadata(s, None))
            .unwrap()
            .is_text_creation()
    );
    (g, i)
}
fn addition(s: &ShellState, visible: bool) -> network::Metadata {
    let mut m = metadata(s, None);
    m.document.raw["list_metadata"]["schema"].as_array_mut().unwrap().insert(1,json!({"id":"ACTUAL_SERVER_FIELD","key":"actual_server_key","name":"Release notes","type":"text","options":{"format":"text"},"opaque":{"server":7}}));
    m.document.raw["list_metadata"]["views"][0]["columns"].as_array_mut().unwrap().insert(1,json!({"id":"ACTUAL_SERVER_FIELD","key":"actual_server_key","visible":visible,"opaque":{"width":9}}));
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
fn order(s: &ShellState) -> Vec<String> {
    s.slack_lists
        .view
        .as_ref()
        .unwrap()
        .layout
        .entries
        .iter()
        .filter_map(|e| match e {
            crate::slack_lists::layout::TableEntry::Row(r) => Some(r.id.clone()),
            _ => None,
        })
        .collect()
}
#[test]
fn text_cancel_keeps_population_active_find_sort_filter_and_shared_exclusion() {
    let (mut s, _cleanup) = text_shell();
    let before = metadata(&s, None).document;
    let records = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .clone();
    let rows = order(&s);
    let serial = open(&mut s);
    assert!(name(&s, serial, "Staged only"));
    let visit = generation(&s).unwrap();
    for target in [
        Target::Column("name".into()),
        Target::Sort("V_TEXT".into()),
        Target::Filter("V_TEXT".into()),
        Target::Visibility {
            view: "V_TEXT".into(),
            column: "C_COUNT".into(),
        },
        Target::NewView(super_platinum_core::slack_lists::ViewLayout::Table, None),
    ] {
        assert!(!open_target(&mut s, target, visit));
    }
    assert!(entry(&s).unwrap().intent.is_none() && entry(&s).unwrap().job.is_none());
    assert!(!close_entry(&s, serial + 1));
    assert!(close_entry(&s, serial));
    assert!(entry(&s).is_none());
    assert_eq!(metadata(&s, None).document, before);
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        records
    );
    assert_eq!(order(&s), rows);
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().layout.find.applied,
        "Item 0"
    );
}
#[test]
fn typed_text_permit_has_exact_fixed_form_no_position_and_is_admitted_once() {
    let (mut s, _cleanup) = text_shell();
    let (g, i) = begin_text(&mut s);
    let permit = admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    assert!(permit.is_text_creation() && !permit.is_creation());
    let request = permit.request(&s.core.client, &fixture::session());
    let RequestBody::Form(fields) = request.body else {
        panic!()
    };
    assert_eq!(
        fields,
        vec![
            ("token".into(), "fixture-not-a-token".into()),
            ("list_id".into(), "FL1".into()),
            ("view_id".into(), "V_TEXT".into()),
            ("name".into(), "Release notes".into()),
            ("type".into(), "text".into()),
            ("options".into(), "{\"format\":\"text\"}".into()),
            ("_x_reason".into(), "create-column".into())
        ]
    );
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_none());
    assert!(prepare(&mut s, g.serial).is_none());
}
#[test]
fn positive_ack_is_not_projection_matching_observation_preserves_all73_raw_rows() {
    let (mut s, _cleanup) = text_shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let rows = order(&s);
    assert_eq!(rows, vec!["R_007", "R_001", "R_009", "R_003", "R_005"]);
    let (g, _) = sent_text(&mut s);
    let ack = json!({"ok":true,"list":{"id":"FL1","columns":[{"id":"DO_NOT_PROJECT"}]}});
    completed(&mut s, &g, Ok(ack.clone()));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(entry(&s).unwrap().raw_reply, Some(ack));
    assert_eq!(metadata(&s, None).document, before.document);
    assert!(entry(&s).unwrap().observed_field.is_none());
    assert!(!close_entry(&s, g.serial));
    let check = checking(&s);
    let m = addition(&s, true);
    assert!(checked(&mut s, &check, Ok(m)));
    let e = entry(&s).unwrap();
    assert_eq!(e.stage, Stage::Observed);
    let field = e.observed_field.unwrap();
    assert_eq!(
        (field.id.as_str(), field.key.as_str()),
        ("ACTUAL_SERVER_FIELD", "actual_server_key")
    );
    assert!(e.available.is_none());
    assert_eq!(order(&s), rows);
    let view = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(view.loaded.as_ref().unwrap().records.records.len(), 73);
    assert_eq!(
        view.loaded.as_ref().unwrap().records.next_cursor.as_deref(),
        Some("create-opaque+/=")
    );
    assert_eq!(view.layout.find.applied, "Item 0");
    assert_eq!(view.layout.find.revision, 17);
    assert_eq!(selected_view(&s), Some("V_TEXT"));
    assert!(view.layout.columns.iter().any(|c| c.id == field.id));
    let mut restored = metadata(&s, None).document.raw;
    restored["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    restored["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .remove(1);
    assert_eq!(restored, before.document.raw);
    assert!(close_entry(&s, g.serial));
    let next = open(&mut s);
    assert_ne!(next, g.serial);
    assert_eq!(entry(&s).unwrap().input, Input::NewTextField(String::new()));
}
#[test]
fn matching_but_hidden_or_absent_table_reference_is_conflict_not_visible_field() {
    for absent in [false, true] {
        let (mut s, _cleanup) = text_shell();
        let (g, _) = sent_text(&mut s);
        completed(&mut s, &g, Ok(json!({"ok":true})));
        let check = checking(&s);
        let mut m = addition(&s, false);
        if absent {
            m.document.raw["list_metadata"]["views"][0]["columns"]
                .as_array_mut()
                .unwrap()
                .remove(1);
            m.document = Document::parse(m.document.raw).unwrap();
        }
        assert!(checked(&mut s, &check, Ok(m)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
        assert!(entry(&s).unwrap().observed_field.is_none());
        assert!(
            !s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .layout
                .columns
                .iter()
                .any(|c| c.id == "ACTUAL_SERVER_FIELD")
        );
        assert!(close_entry(&s, g.serial));
    }
}
#[test]
fn open_snapshot_and_dispatch_guard_refuse_same_revision_population_and_cursor_changes() {
    for during_dispatch in [false, true] {
        let (mut s, _cleanup) = text_shell();
        let serial = open(&mut s);
        assert!(name(&s, serial, "Release notes"));
        let prepared = during_dispatch.then(|| prepare(&mut s, serial).unwrap());
        let loaded = s
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        loaded.records.raw["response_metadata"]["next_cursor"] = json!("OTHER");
        loaded.records = RecordsPage::parse(loaded.records.raw.clone()).unwrap();
        if let Some((g, i)) = prepared {
            assert!(!g.current(&s, Stage::Preparing));
            assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_none());
        } else {
            assert!(prepare(&mut s, serial).is_none());
            let p = presentation(&s).unwrap();
            assert!(!p.can_confirm);
            assert_eq!(p.name, "Release notes");
            assert!(p.message.contains("loaded items changed"));
            assert!(p.message.contains("field name is kept"));
        }
        assert!(!name(&s, serial, "Later"));
    }
}
#[test]
fn changed_population_during_check_never_projects_matching_metadata() {
    let (mut s, _cleanup) = text_shell();
    let (g, _) = sent_text(&mut s);
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let check = checking(&s);
    let m = addition(&s, true);
    let before = metadata(&s, None).document;
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    loaded.records.raw["records"][0]["fields"][0]["text"] = json!("Changed without view revision");
    loaded.records = RecordsPage::parse(loaded.records.raw.clone()).unwrap();
    assert!(!checked(&mut s, &check, Ok(m)));
    assert_eq!(metadata(&s, None).document, before);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(entry(&s).unwrap().observed_field.is_none());
}

#[test]
fn later_stable_population_can_review_conflict_without_rebasing_original_create() {
    let (mut s, _cleanup) = text_shell();
    let (g, _) = sent_text(&mut s);
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let old = entry(&s).unwrap();
    let original = old.original_records.clone().unwrap();
    let loaded = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    loaded.records.raw["records"][0]["fields"][0]["text"] = json!("Later population");
    loaded.records = RecordsPage::parse(loaded.records.raw.clone()).unwrap();
    let current = loaded.records.clone();
    let p = presentation(&s).unwrap();
    assert!(p.can_check && !p.can_confirm && p.message.contains("loaded items changed"));
    let key = Key::current(&s).unwrap();
    // Actual production Check boundary; the write-only capture must still refuse.
    assert!(Guard::capture(&s, key.clone(), old.serial).is_none());
    let check = Guard::capture_read(&s, key.clone(), old.serial).unwrap();
    with_registry(|r| {
        let e = r.entries.get_mut(&key).unwrap();
        e.job = Some(check.job);
        e.stage = Stage::Checking;
    });
    assert!(check.current(&s, Stage::Checking));
    let fresh = addition(&s, true);
    assert!(checked(&mut s, &check, Ok(fresh)));
    let e = entry(&s).unwrap();
    assert_eq!(e.stage, Stage::Conflict);
    assert!(e.observed_field.is_none() && e.available.is_none());
    assert_eq!(e.original_records.as_deref(), Some(original.as_ref()));
    let Some(Intent::NewTextField(intent)) = e.intent.as_ref() else {
        panic!()
    };
    assert_eq!(intent.original_records(), original.as_ref());
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records,
        current
    );
    assert!(e.review.as_ref().unwrap().navigation_current(&s));
    assert!(presentation(&s).unwrap().current_review);
    assert!(prepare(&mut s, e.serial).is_none());
    assert!(admitted(&s, &g, e.intent.as_ref().unwrap(), &metadata(&s, None)).is_none());
    assert!(close_entry(&s, e.serial));
    assert!(entry(&s).is_none());
    assert!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .columns
            .iter()
            .any(|c| c.id == "ACTUAL_SERVER_FIELD")
    );
}

#[test]
fn observed_text_can_renew_retired_read_review_without_rearming_creation() {
    for row_change in [false, true] {
        let (mut s, _cleanup) = text_shell();
        let (write, _) = sent_text(&mut s);
        completed(&mut s, &write, Ok(json!({"ok":true})));
        let first = checking(&s);
        let fresh = addition(&s, true);
        assert!(checked(&mut s, &first, Ok(fresh)));
        let observed = entry(&s).unwrap();
        assert_eq!(observed.stage, Stage::Observed);
        assert!(!checkable(&s, &observed));
        assert!(!presentation(&s).unwrap().checkable);
        let original = observed.original_records.clone().unwrap();
        if row_change {
            let loaded = s
                .slack_lists
                .view
                .as_mut()
                .unwrap()
                .loaded
                .as_mut()
                .unwrap();
            loaded.records.raw["response_metadata"]["next_cursor"] = json!("later-page");
            loaded.records = RecordsPage::parse(loaded.records.raw.clone()).unwrap();
        } else {
            // Same view, new selection clock: a delayed prior review is retired.
            s.slack_lists
                .view
                .as_mut()
                .unwrap()
                .layout
                .selection_generation = next_id();
        }
        assert!(!close_entry(&s, observed.serial));
        assert!(checkable(&s, &entry(&s).unwrap()));
        let p = presentation(&s).unwrap();
        assert!(p.checkable && p.can_check && !p.current_review && !p.can_confirm);
        assert!(p.observed_field.is_none());
        assert!(p.message.contains("changed since the field was observed"));
        assert!(prepare(&mut s, observed.serial).is_none());
        let key = Key::current(&s).unwrap();
        let check = Guard::capture_read(&s, key.clone(), observed.serial).unwrap();
        with_registry(|r| {
            let e = r.entries.get_mut(&key).unwrap();
            e.job = Some(check.job);
            e.stage = Stage::Checking;
        });
        let current = metadata(&s, None);
        assert!(checked(&mut s, &check, Ok(current)));
        let renewed = entry(&s).unwrap();
        assert_eq!(
            renewed.stage,
            if row_change {
                Stage::Conflict
            } else {
                Stage::Observed
            }
        );
        assert_eq!(renewed.original_records.as_deref(), Some(original.as_ref()));
        let Some(Intent::NewTextField(intent)) = renewed.intent.as_ref() else {
            panic!()
        };
        assert_eq!(intent.original_records(), original.as_ref());
        assert_eq!(renewed.observed_field.is_some(), !row_change);
        assert!(renewed.review.as_ref().unwrap().navigation_current(&s));
        assert!(prepare(&mut s, renewed.serial).is_none());
        assert!(
            admitted(
                &s,
                &write,
                renewed.intent.as_ref().unwrap(),
                &metadata(&s, None)
            )
            .is_none()
        );
        assert!(close_entry(&s, renewed.serial));
    }
}

#[test]
fn population_bound_and_representation_refusals_do_not_allocate_a_create_entry() {
    for oversize in [false, true] {
        let (mut s, _cleanup) = text_shell();
        let loaded = s
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        if oversize {
            loaded.records.raw["extra"] = json!("x".repeat(registry::MAX_DOCUMENT + 1));
            loaded.records = RecordsPage::parse(loaded.records.raw.clone()).unwrap();
        } else {
            loaded.records.next_cursor = Some("forged typed cursor".into());
        }
        let visit = generation(&s).unwrap();
        let selection = super::super::selection(&s).unwrap();
        assert!(!text_create::open_shell(&mut s, visit, selection));
        assert!(entry(&s).is_none());
    }
}
#[test]
fn input_visit_selection_and_wrong_variant_refuse_without_rebinding() {
    let (mut s, _cleanup) = text_shell();
    let serial = open(&mut s);
    let visit = generation(&s).unwrap();
    let selection = super::super::selection(&s).unwrap();
    assert!(!text_create::input_shell(
        &s,
        serial + 1,
        visit,
        selection,
        text_create::choice(&s).unwrap(),
        "Notes".into()
    ));
    assert!(!text_create::input_shell(
        &s,
        serial,
        visit + 1,
        selection,
        text_create::choice(&s).unwrap(),
        "Notes".into()
    ));
    assert!(!text_create::input_shell(
        &s,
        serial,
        visit,
        selection + 1,
        text_create::choice(&s).unwrap(),
        "Notes".into()
    ));
    assert!(!name(&s, serial, &"x".repeat(257)));
    assert!(name(&s, serial, "Notes"));
    let key = Key::current(&s).unwrap();
    with_registry(|r| r.entries.get_mut(&key).unwrap().input = Input::Name("Notes".into()));
    assert!(prepare(&mut s, serial).is_none());
    with_registry(|r| r.entries.get_mut(&key).unwrap().input = Input::NewTextField("Notes".into()));
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(prepare(&mut s, serial).is_none());
}
#[test]
fn owner_capacity_and_fresh_policy_are_required_and_retired_completion_stays_unconfirmed() {
    for patch in [
        json!({"access":"write","user":"OTHER"}),
        json!({"list_limits":{}}),
        json!({"list_limits":{"over_column_maximum":true}}),
    ] {
        let (mut s, _cleanup) = text_shell();
        let loaded = s
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .loaded
            .as_mut()
            .unwrap();
        for (k, v) in patch.as_object().unwrap() {
            loaded.document.raw[k] = v.clone();
        }
        loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
        assert!(!text_create::can_open(&s));
    }
    let (mut s, _cleanup) = text_shell();
    let (g, i) = begin_text(&mut s);
    let mut m = metadata(&s, None);
    m.allow = Some("none".into());
    assert!(admitted(&s, &g, &i, &m).is_none());
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).is_some());
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    let check = checking(&s);
    let fresh = addition(&s, true);
    let before = metadata(&s, None).document;
    s.core.account_epoch += 1;
    assert!(!checked(&mut s, &check, Ok(fresh)));
    assert_eq!(metadata(&s, None).document, before);
    assert!(prepare(&mut s, g.serial).is_none());
}

mod checkbox;
mod number;
