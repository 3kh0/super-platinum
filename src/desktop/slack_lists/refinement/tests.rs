use super::*;
use super_platinum_core::slack_lists::{BoardLaneOrderDraft, BoardLanePlacement, ViewLayout};
fn shell() -> ShellState {
    ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-board-local-order",
    )
}
fn select(s: &mut ShellState, id: &str) {
    let v = s.slack_lists.view.as_ref().unwrap();
    let (visit, stamp) = (v.generation, v.layout.generation);
    assert!(layout::select_shell(s, Some(id.into()), visit, stamp));
}
#[test]
fn local_visibility_and_layout_publish_only_read_projection_and_survive_switch() {
    let mut s = shell();
    let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let count = fixture::attempt_count(&s);
    let action = capture(&s).unwrap();
    let draft = action
        .draft(&s)
        .unwrap()
        .with_visibility(&before.document, "C_NUMBER", false)
        .unwrap()
        .with_layout(&before.document, ViewLayout::Table)
        .unwrap();
    assert!(apply(&mut s, &action, draft));
    let v = s.slack_lists.view.as_ref().unwrap();
    assert_eq!(v.layout.kind, saved_views::ProjectionLayout::Table);
    assert!(!v.layout.columns.iter().any(|c| c.id == "C_NUMBER"));
    assert!(changes(&s).unwrap().visibility && changes(&s).unwrap().layout);
    assert_eq!(v.loaded.as_ref().unwrap().document, before.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(v.loaded.as_ref().unwrap().records.records.len(), 73);
    assert_eq!(fixture::attempt_count(&s), count);
    assert!(!action.current(&s));
    select(&mut s, "V_PRECISION");
    assert!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .columns
            .iter()
            .any(|c| c.id == "C_NUMBER")
    );
    select(&mut s, "V_LANE");
    assert_eq!(
        s.slack_lists.view.as_ref().unwrap().layout.kind,
        saved_views::ProjectionLayout::Table
    );
    assert!(
        !s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .columns
            .iter()
            .any(|c| c.id == "C_NUMBER")
    );
}
#[test]
fn reset_clears_all_view_slots_and_preserves_provider_and_raw_page() {
    let mut s = shell();
    let l = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
    let provider = BoardLaneOrderDraft::moved(
        &l.document,
        "V_LANE",
        None,
        "OPT_DONE",
        "OPT_PROGRESS",
        BoardLanePlacement::Before,
        l.allow_lists.as_deref(),
        l.personal_task_lists.as_ref(),
    )
    .unwrap();
    s.slack_lists.view.as_mut().unwrap().layout.board_order = Some(provider.clone());
    rebuild_rows(&mut s);
    // A provider alone does not expose Save.
    assert!(!changes(&s).unwrap().any());
    for id in ["V_LANE", "V_PRECISION"] {
        select(&mut s, id);
        let a = capture(&s).unwrap();
        let d = a
            .draft(&s)
            .unwrap()
            .with_visibility(&l.document, "C_NUMBER", false)
            .unwrap();
        assert!(apply(&mut s, &a, d));
    }
    let a = capture(&s).unwrap();
    assert!(reset(&mut s, &a));
    let v = s.slack_lists.view.as_ref().unwrap();
    assert!(v.layout.refinements.is_empty());
    assert_eq!(v.layout.board_order, Some(provider));
    assert_eq!(v.loaded.as_ref().unwrap().document, l.document);
    assert_eq!(v.loaded.as_ref().unwrap().records, l.records);
    assert_eq!(
        v.loaded.as_ref().unwrap().records.raw["response_metadata"]["next_cursor"],
        "precision-opaque+/="
    );
    select(&mut s, "V_LANE");
    assert!(!changes(&s).unwrap().any());
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .lanes
            .iter()
            .map(|l| l.key.as_str())
            .collect::<Vec<_>>(),
        ["OPT_DONE", "OPT_PROGRESS"]
    );
}
#[test]
fn stale_account_find_page_or_replacement_draft_cannot_apply_old_action() {
    for variant in 0..4 {
        let mut s = shell();
        let a = capture(&s).unwrap();
        let doc = s
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document
            .clone();
        let d = a
            .draft(&s)
            .unwrap()
            .with_visibility(&doc, "C_NUMBER", false)
            .unwrap();
        match variant {
            0 => s.core.account_epoch = s.core.account_epoch.wrapping_add(1),
            1 => s.slack_lists.view.as_mut().unwrap().layout.find.input = "changed".into(),
            2 => {
                let l = s
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap();
                let mut raw = l.records.raw.clone();
                raw["response_metadata"]["next_cursor"] = Value::String("different".into());
                l.records = RecordsPage::parse(raw).unwrap();
            }
            _ => {
                s.slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .refinements
                    .replace(d.clone());
            }
        }
        assert!(!apply(&mut s, &a, d));
        assert!(!reset(&mut s, &a));
    }
}

#[test]
fn rendered_toolbar_clock_cannot_rebind_to_a_fresh_scope_or_account() {
    let mut s = shell();
    let stamp = toolbar::stamp(&s).unwrap();
    let original = s
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document
        .clone();
    s.core.account_epoch = s.core.account_epoch.wrapping_add(1);
    assert!(!toolbar::choose(
        &mut s,
        stamp,
        controls::Choice::Completed(true)
    ));
    assert_eq!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .document,
        original
    );
    assert!(
        s.slack_lists
            .view
            .as_ref()
            .unwrap()
            .layout
            .refinements
            .is_empty()
    );
}
