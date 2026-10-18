//! Controlled static refinement states. No worker/callback or native claim.
use super::*;
use super_platinum_core::slack_lists::{BoardLaneOrderDraft, BoardLanePlacement};
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-board-refinement"
            | "slack-list-board-refinement-narrow"
            | "slack-list-board-refinement-new"
            | "slack-list-board-refinement-new-narrow"
    ));
    super::board_lane_order::install(shell, "slack-list-board-local-order");
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Board view refinements");
    // New-view review requires a positively observed synthetic capacity.
    if name.contains("-new") {
        f.document["list_limits"] = json!({"over_view_maximum":false});
    }
    let loaded = super::loaded(f);
    let view = shell.slack_lists.view.as_mut().unwrap();
    view.title = loaded.document.title.clone();
    view.loaded = Some(loaded);
    super::super::rebuild_rows(shell);
    let l = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
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
    shell.slack_lists.view.as_mut().unwrap().layout.board_order = Some(provider);
    super::super::rebuild_rows(shell);
    let action = super::super::refinement::capture(shell).unwrap();
    let doc = &shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .document;
    let draft = action
        .draft(shell)
        .unwrap()
        .with_completed(doc, true)
        .unwrap();
    assert!(super::super::refinement::apply(shell, &action, draft));
    if name.contains("-new") {
        let action = super::super::rename::refinement_save::capture(shell, true).unwrap();
        let v = shell.slack_lists.view.as_ref().unwrap();
        let (visit, selection) = (v.generation, v.layout.selection_generation);
        assert!(
            super::super::rename::refinement_save::open_shell(shell, action, visit, selection)
                .is_some()
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_named_static_states_keep_full_raw_population_and_are_not_worker_receipts() {
        for name in [
            "slack-list-board-refinement",
            "slack-list-board-refinement-narrow",
            "slack-list-board-refinement-new",
            "slack-list-board-refinement-new-narrow",
        ] {
            let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            let v = s.slack_lists.view.as_ref().unwrap();
            let l = v.loaded.as_ref().unwrap();
            assert_eq!(l.records.records.len(), 73);
            assert_eq!(
                l.records.raw["response_metadata"]["next_cursor"],
                "precision-opaque+/="
            );
            assert!(v.layout.board_order.is_some());
            assert!(
                super::super::super::refinement::changes(&s)
                    .unwrap()
                    .completed
            );
            assert_eq!(super::super::attempt_count(&s), 0);
            let p = super::super::super::rename::presentation(&s);
            if name.contains("-new") {
                let p = p.unwrap();
                assert_eq!(p.stage, super::super::super::rename::Stage::Editing);
                assert!(p.refinement_save.unwrap().new);
            } else {
                assert!(p.is_none());
            }
            assert!(s.core.transport.is_none() && s.core.session.is_none());
        }
    }
}
