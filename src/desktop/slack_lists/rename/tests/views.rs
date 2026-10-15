use super::*;
struct Cleanup(Key);
impl Drop for Cleanup {
    fn drop(&mut self) {
        with_registry(|r| {
            r.entries.remove(&self.0);
        });
    }
}
fn view_shell() -> (ShellState, Cleanup) {
    let mut s = shell();
    let view = s.slack_lists.view.as_mut().unwrap();
    let l = view.loaded.as_mut().unwrap();
    let mut raw = l.document.raw.clone();
    raw["list_metadata"]["views"] = json!([
 {"id":"V_CUSTOM","name":"Custom","type":"table","columns":[{"key":"name","id":"name","visible":true}],"grouping":{},"filters":[],"sorts":[],"opaque":{"keep":1}},
 {"id":"V_OTHER","name":"Other","type":"board","grouping":{"group_by":"done","group_by_column_id":"done"},"columns":[],"filters":[],"sorts":[]}]);
    raw["list_metadata"]["default_view"] = json!("V_CUSTOM");
    l.document = Document::parse(raw).unwrap();
    view.layout = Default::default();
    rebuild_rows(&mut s);
    assert_eq!(selected_view(&s), Some("V_CUSTOM"));
    let cleanup = Cleanup(Key::current(&s).unwrap());
    (s, cleanup)
}
fn start(s: &mut ShellState) -> (Guard, Intent) {
    let visit = generation(s).unwrap();
    assert!(open_target(s, Target::View("V_CUSTOM".into()), visit));
    let e = entry(s).unwrap();
    let key = Key::current(s).unwrap();
    with_registry(|r| r.entries.get_mut(&key).unwrap().name = "New shared name".into());
    prepare(s, e.serial).unwrap()
}
fn fresh(s: &ShellState) -> network::Metadata {
    let mut m = metadata(s, None);
    m.document.raw["list_metadata"]["views"][0]["name"] = json!("New shared name");
    m.document = Document::parse(m.document.raw).unwrap();
    m
}
#[test]
fn shared_registry_serializes_column_and_view_in_both_directions() {
    for view_first in [true, false] {
        let (mut s, _cleanup) = view_shell();
        let visit = generation(&s).unwrap();
        if view_first {
            assert!(open_target(&mut s, Target::View("V_CUSTOM".into()), visit));
            assert!(!open_shell(&mut s, "name", visit));
        } else {
            assert!(open_shell(&mut s, "name", visit));
            assert!(!open_target(&mut s, Target::View("V_CUSTOM".into()), visit));
        }
        assert!(blocks_mutation(&s));
    }
}
#[test]
fn editing_selection_aba_refuses_without_losing_name() {
    let (mut s, _cleanup) = view_shell();
    let visit = generation(&s).unwrap();
    assert!(open_target(&mut s, Target::View("V_CUSTOM".into()), visit));
    let e = entry(&s).unwrap();
    with_registry(|r| {
        r.entries.get_mut(&Key::current(&s).unwrap()).unwrap().name = "Keep me".into()
    });
    assert!(presentation(&s).unwrap().can_confirm);
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(!presentation(&s).unwrap().can_confirm);
    assert!(prepare(&mut s, e.serial).is_none());
    assert_eq!(entry(&s).unwrap().name, "Keep me");
    assert!(entry(&s).unwrap().job.is_none());
}
#[test]
fn selected_view_changes_after_preflight_block_dispatch() {
    let (mut s, _cleanup) = view_shell();
    let (g, i) = start(&mut s);
    let m = metadata(&s, None);
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(admitted(&s, &g, &i, &m).is_none());
    assert_eq!(entry(&s).unwrap().stage, Stage::Preparing);
}
#[test]
fn current_full_readback_keeps_selected_id_and_all_partial_population() {
    let (mut s, _cleanup) = view_shell();
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
    let (g, i) = start(&mut s);
    assert!(admitted(&s, &g, &i, &metadata(&s, None)).unwrap().is_view());
    completed(&mut s, &g, Ok(json!({"ok":true,"list":{"views":[]}})));
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
    assert!(prepare(&mut s, g.serial).is_none());
    let check = checking(&s);
    let m = fresh(&s);
    assert!(checked(&mut s, &check, Ok(m)));
    assert_eq!(entry(&s).unwrap().stage, Stage::Observed);
    assert_eq!(selected_view(&s), Some("V_CUSTOM"));
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
fn late_check_cannot_adopt_into_another_selection() {
    let (mut s, _cleanup) = view_shell();
    let (g, i) = start(&mut s);
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    completed(&mut s, &g, Ok(json!({"ok":true})));
    let check = checking(&s);
    let m = fresh(&s);
    let before = metadata(&s, None).document;
    s.slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .selection_generation = next_id();
    assert!(!checked(&mut s, &check, Ok(m)));
    assert_eq!(metadata(&s, None).document, before);
    assert_eq!(entry(&s).unwrap().stage, Stage::Uncertain);
}
#[test]
fn late_view_rate_limit_blocks_field_after_receipt_and_account_return() {
    let (mut s, _cleanup) = view_shell();
    let (g, i) = start(&mut s);
    admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
    s.core.account_epoch += 1;
    completed(
        &mut s,
        &g,
        Err(ListColumnRenameFailure::Unconfirmed(Error::HttpStatus {
            status: 200,
            retry_after_secs: Some(120),
        })),
    );
    assert!(registry::blocked(&g.key.owner));
    with_registry(|r| {
        r.entries.remove(&g.key);
    });
    let loaded = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    assert!(crate::slack_lists::open_shell(
        &mut s,
        serde_json::from_value(loaded.document.raw.clone()).unwrap()
    ));
    crate::slack_lists::tests::read(&mut s, loaded);
    assert!(owner::available(&s));
    assert!(!blocks_mutation(&s));
    assert_eq!(selected_view(&s), Some("V_CUSTOM"));
    assert!(!can_open(&s, "name"));
    assert!(!can_view(&s));
}
#[test]
fn sibling_or_type_changes_produce_current_review_not_false_observation() {
    for field in ["sibling", "type", "deleted"] {
        let (mut s, _cleanup) = view_shell();
        let (g, i) = start(&mut s);
        admitted(&s, &g, &i, &metadata(&s, None)).unwrap();
        completed(&mut s, &g, Ok(json!({"ok":false,"error":"no_access"})));
        let check = checking(&s);
        let mut m = fresh(&s);
        match field {
            "sibling" => m.document.raw["list_metadata"]["views"][1]["name"] = json!("Changed"),
            "type" => m.document.raw["list_metadata"]["views"][0]["type"] = json!("board"),
            _ => {
                m.document.raw["list_metadata"]["views"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
        }
        m.document = Document::parse(m.document.raw).unwrap();
        assert!(checked(&mut s, &check, Ok(m)));
        assert_eq!(entry(&s).unwrap().stage, Stage::Conflict);
        assert!(entry(&s).unwrap().review.unwrap().current(&s));
        assert!(prepare(&mut s, g.serial).is_none());
    }
}
