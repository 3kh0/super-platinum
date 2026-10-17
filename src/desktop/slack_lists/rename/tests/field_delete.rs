use super::super::field_delete as deletion;
use super::*;

struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn deletion_shell() -> (ShellState, Cleanup) {
    let mut s = shell();
    let mut m = metadata(&s, None);
    m.document.raw["user"] = json!(super::super::text_create::user(&s).unwrap());
    m.document.raw["list_metadata"]["schema"][0]["is_primary_column"] = json!(true);
    m.document.raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"HIDDEN","key":"hidden","name":"Hidden field","type":"checkbox"}),
            json!({"id":"ABSENT","key":"absent","name":"Absent field","type":"checkbox"}),
            json!({"id":"TASK","key":"task","name":"Task field","type":"todo_completed"}),
        ]);
    m.document.raw["list_metadata"]["views"] = json!([
        {"id":"V_DELETE","name":"Table","type":"table","columns":[{"key":"name","visible":true},{"key":"done","visible":true},{"key":"hidden","visible":false}],"sorts":[],"filters":[],"opaque":{"keep":1}},
        {"id":"V_OTHER","name":"Board","type":"board","columns":[{"key":"hidden","visible":true}],"grouping":{"group_by":"hidden"},"sorts":[{"column_key":"hidden","asc":true}],"filters":[{"column_key":"hidden","operator":"equals","value":true}]}
    ]);
    m.document.raw["list_metadata"]["default_view"] = json!("V_DELETE");
    let v = s.slack_lists.view.as_mut().unwrap();
    v.loaded.as_mut().unwrap().document = Document::parse(m.document.raw).unwrap();
    v.loaded.as_mut().unwrap().list_experiments = Some(Default::default());
    v.layout = Default::default();
    rebuild_rows(&mut s);
    let cleanup = Cleanup(Key::current(&s).unwrap());
    (s, cleanup)
}
fn open_delete(s: &mut ShellState) -> u64 {
    let visit = generation(s).unwrap();
    let selected = selection(s).unwrap();
    assert!(deletion::open_shell(s, visit, selected));
    entry(s).unwrap().serial
}
fn choose(s: &ShellState, column: &str) -> bool {
    deletion::choose_shell(s, entry(s).unwrap().serial, generation(s).unwrap(), column)
}
fn deleted(s: &ShellState, key: &str) -> network::Metadata {
    let mut m = metadata(s, None);
    let meta = &mut m.document.raw["list_metadata"];
    meta["schema"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["key"] != key);
    for view in meta["views"].as_array_mut().unwrap() {
        view["columns"]
            .as_array_mut()
            .unwrap()
            .retain(|c| c["key"] != key);
    }
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
fn send(s: &mut ShellState) -> Guard {
    open_delete(s);
    assert!(choose(s, "HIDDEN"));
    let serial = entry(s).unwrap().serial;
    let (g, i) = prepare(s, serial).unwrap();
    assert!(
        admitted(s, &g, &i, &metadata(s, None))
            .unwrap()
            .is_deletion()
    );
    g
}
#[test]
fn chooser_covers_hidden_and_absent_fields_and_retires_stale_confirmation() {
    let (mut s, _cleanup) = deletion_shell();
    let before = metadata(&s, None).document;
    let serial = open_delete(&mut s);
    let p = presentation(&s).unwrap().deletion.unwrap();
    assert_eq!(
        p.choices.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        ["done", "HIDDEN", "ABSENT"]
    );
    assert!(!choose(&s, "name"));
    assert!(!choose(&s, "TASK"));
    assert!(choose(&s, "ABSENT"));
    assert_ne!(entry(&s).unwrap().serial, serial);
    assert!(prepare(&mut s, serial).is_none());
    assert!(!deletion::choose_shell(
        &s,
        serial,
        generation(&s).unwrap(),
        "HIDDEN"
    ));
    assert!(!close_entry(&s, serial));
    assert_eq!(entry(&s).unwrap().stage, Stage::Editing);
    assert!(entry(&s).unwrap().intent.is_none());
    assert!(close_entry(&s, entry(&s).unwrap().serial));
    assert_eq!(metadata(&s, None).document, before);
}
#[test]
fn changes_to_page_or_selection_block_choice_and_confirmation_without_changing_target() {
    for page in [false, true] {
        let (mut s, _cleanup) = deletion_shell();
        let serial = open_delete(&mut s);
        let original = entry(&s).unwrap();
        if page {
            let l = s
                .slack_lists
                .view
                .as_mut()
                .unwrap()
                .loaded
                .as_mut()
                .unwrap();
            let mut raw = l.records.raw.clone();
            raw["response_metadata"]["next_cursor"] = json!("different");
            l.records = RecordsPage::parse(raw).unwrap();
        } else {
            s.slack_lists
                .view
                .as_mut()
                .unwrap()
                .layout
                .selection_generation = next_id();
        }
        assert!(!choose(&s, "HIDDEN"));
        assert!(prepare(&mut s, serial).is_none());
        assert_eq!(entry(&s).unwrap().target, original.target);
    }
}
#[test]
fn deletion_admission_rechecks_schema_page_revision_and_account_owner() {
    for change in 0..5 {
        let (mut s, _cleanup) = deletion_shell();
        let serial = open_delete(&mut s);
        let visit = generation(&s).unwrap();
        let (g, i) = prepare(&mut s, serial).unwrap();
        let mut m = metadata(&s, None);
        match change {
            0 => {
                m.document.raw["list_metadata"]["opaque"] = json!("changed");
                m.document = Document::parse(m.document.raw).unwrap();
            }
            1 => s.slack_lists.view.as_mut().unwrap().revision = next_id(),
            2 => s.core.account_epoch += 1,
            3 => s.slack_lists.view.as_mut().unwrap().read = next_id(),
            _ => {
                let l = s
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                let mut raw = l.records.raw.clone();
                raw["records"][0]["updated_timestamp"] = json!("changed");
                l.records = RecordsPage::parse(raw).unwrap();
            }
        }
        assert!(admitted(&s, &g, &i, &m).is_none());
        assert!(!deletion::choose_shell(&s, serial, visit, "HIDDEN"));
    }
}
#[test]
fn positive_ack_never_projects_or_replays_and_exact_read_preserves_raw_records_and_view_references()
{
    let (mut s, _cleanup) = deletion_shell();
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
    let g = send(&mut s);
    completed(&mut s, &g, Ok(json!({"ok":true})));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert_eq!(metadata(&s, None).document, before);
    assert!(prepare(&mut s, g.serial).is_none());
    let fresh = deleted(&s, "hidden");
    let check = checking(&s);
    assert!(checked(&mut s, &check, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    let l = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    assert_eq!(l.records, records);
    assert!(!l.records.complete());
    for k in ["grouping", "sorts", "filters"] {
        assert_eq!(
            l.document.raw["list_metadata"]["views"][1][k],
            before.raw["list_metadata"]["views"][1][k]
        );
    }
    assert!(prepare(&mut s, g.serial).is_none());
}
#[test]
fn conflict_requires_current_read_and_retired_page_read_cannot_project() {
    let (mut s, _cleanup) = deletion_shell();
    let g = send(&mut s);
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
            "lost".into(),
        ))),
    );
    let check = checking(&s);
    let original = metadata(&s, None);
    assert!(checked(&mut s, &check, Ok(original)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
    let fresh = deleted(&s, "hidden");
    let check = checking(&s);
    s.slack_lists.view.as_mut().unwrap().generation = next_id();
    assert!(!checked(&mut s, &check, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(metadata(&s, None).document.column("HIDDEN").is_some());
    let fresh = deleted(&s, "hidden");
    let check = checking(&s);
    assert!(checked(&mut s, &check, Ok(fresh)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
}

#[test]
fn exact_schema_experiment_controls_collaborator_open_and_fresh_admission() {
    for policy in [None, Some(false), Some(true)] {
        for owner in [false, true] {
            let (mut s, _cleanup) = deletion_shell();
            let user = super::super::text_create::user(&s).unwrap().to_owned();
            let l = s
                .slack_lists
                .view
                .as_mut()
                .unwrap()
                .loaded
                .as_mut()
                .unwrap();
            l.document.raw["user"] = json!(if owner { user.as_str() } else { "OTHER_OWNER" });
            l.document = Document::parse(l.document.raw.clone()).unwrap();
            l.list_experiments =
                policy.map(
                    |schema_owner_only| super_platinum_core::slack::api::ListExperiments {
                        schema_owner_only,
                        ..Default::default()
                    },
                );
            assert_eq!(
                deletion::can_open(&s),
                policy == Some(false) || (policy == Some(true) && owner)
            );
            if deletion::can_open(&s) {
                let serial = open_delete(&mut s);
                let (guard, intent) = prepare(&mut s, serial).unwrap();
                assert!(admitted(&s, &guard, &intent, &metadata(&s, None)).is_some());
            }
        }
    }
    let (mut s, _cleanup) = deletion_shell();
    let l = s
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    l.document.raw["user"] = json!("OTHER_OWNER");
    l.document = Document::parse(l.document.raw.clone()).unwrap();
    let serial = open_delete(&mut s);
    let (guard, intent) = prepare(&mut s, serial).unwrap();
    let mut fresh = metadata(&s, None);
    fresh.experiments.as_mut().unwrap().schema_owner_only = true;
    assert!(admitted(&s, &guard, &intent, &fresh).is_none());
    fresh.experiments = None;
    assert!(admitted(&s, &guard, &intent, &fresh).is_none());
}
