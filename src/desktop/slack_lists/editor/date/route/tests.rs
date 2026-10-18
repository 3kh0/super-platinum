use super::*;

fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::seed(&mut shell, "todo_due_date", true);
    fixture::personal_route(&mut shell);
    shell
}
fn identity(shell: &ShellState) -> (String, u64, u64) {
    let view = shell.slack_lists.view.as_ref().unwrap();
    (
        view.rows[0].instance_id.clone(),
        view.generation,
        view.layout.generation,
    )
}
#[test]
fn personal_primary_route_opens_only_the_exact_record_without_a_cell_write() {
    let mut shell = shell();
    let before = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    let (instance, visit, layout) = identity(&shell);
    assert!(reminder_for(&shell, "R_DATE", "C_DATE"));
    assert!(!reminder_for(&shell, "R_DATE", "C_NAME"));
    assert!(!reminder_for(&shell, "OTHER", "C_DATE"));
    let target = cell_target(&mut shell, &instance, "C_DATE", visit, layout).unwrap();
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert_eq!(
        record_detail::delete_target(view, target).unwrap().1,
        "R_DATE"
    );
    assert!(view.editor.is_none() && shell.slack_lists.operations.is_empty());
    assert_eq!(view.loaded.as_ref().unwrap().records, before.records);
    assert_eq!(
        view.loaded.as_ref().unwrap().document.raw,
        before.document.raw
    );
    assert_eq!(detail_target(&shell, target, "C_DATE"), Some(target));
    assert!(detail_target(&shell, target + 1, "C_DATE").is_none());
    assert!(cell_target(&mut shell, &instance, "C_DATE", visit, layout).is_none());
}
#[test]
fn stale_owner_view_policy_and_disabled_reminders_never_open_a_detail() {
    for change in 0..7 {
        let mut shell = shell();
        let (instance, visit, layout) = identity(&shell);
        match change {
            0 => shell.core.account_epoch = shell.core.account_epoch.wrapping_add(1),
            1 => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap()
                    .list_experiments = None
            }
            2 => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap()
                    .list_experiments
                    .as_mut()
                    .unwrap()
                    .todos = false
            }
            3 => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap()
                    .list_experiments
                    .as_mut()
                    .unwrap()
                    .lists_in_later = false
            }
            4 => shell.slack_lists.view.as_mut().unwrap().stale = true,
            5 => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap()
                    .personal_task_lists = None
            }
            _ => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .loaded
                    .as_mut()
                    .unwrap()
                    .list_experiments
                    .as_mut()
                    .unwrap()
                    .todos_rollback = true
            }
        }
        assert!(cell_target(&mut shell, &instance, "C_DATE", visit, layout).is_none());
        assert!(shell.slack_lists.view.as_ref().unwrap().detail.is_none());
        assert!(shell.slack_lists.operations.is_empty());
    }
    let mut shell = shell();
    let (instance, visit, layout) = identity(&shell);
    assert!(cell_target(&mut shell, &instance, "C_DATE", visit + 1, layout).is_none());
    assert!(cell_target(&mut shell, &instance, "C_DATE", visit, layout + 1).is_none());
}
#[test]
fn calendar_draft_is_never_canceled_or_replaced_by_a_reminder_route() {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::seed(&mut shell, "todo_due_date", true);
    let (instance, visit, layout) = identity(&shell);
    let generation = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation;
    let loaded = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    loaded.list_experiments.as_mut().unwrap().todos = true;
    loaded.list_experiments.as_mut().unwrap().lists_in_later = true;
    assert!(cell_target(&mut shell, &instance, "C_DATE", visit, layout).is_none());
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert_eq!(view.editor.as_ref().unwrap().generation, generation);
    assert!(view.detail.is_none());
}
