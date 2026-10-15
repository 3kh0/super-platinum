use super::*;
pub(crate) struct Presentation {
    pub serial: u64,
    pub stage: Stage,
    pub pending: bool,
    pub message: &'static str,
    pub can_check: bool,
    pub can_close: bool,
    pub can_open: bool,
}
pub(crate) fn presentation(shell: &ShellState) -> Option<Presentation> {
    let key = Key::current(shell)?;
    // Borrow the retained receipt: rendering never clones captured populations.
    with(|r| {
        let e = r.entries.get(&key)?;
        let current = e.review.as_ref().is_some_and(|p| p.current(shell));
        Some(Presentation {
            serial: e.serial,
            stage: e.stage,
            pending: e.in_flight,
            message: e.message,
            can_check: !e.in_flight
                && !e.stage.busy()
                && e.stage != Stage::NotRequested
                && available(shell, true)
                && shell.slack_lists.view.as_ref().is_some_and(|v| {
                    record_detail::subitem_origin(v, e.intent.parent_id()).is_some()
                }),
            can_close: e.stage == Stage::Preparing
                || (!e.in_flight && e.stage == Stage::NotRequested)
                || (!e.in_flight
                    && !e.stage.busy()
                    && (current || e.retirement.as_ref().is_some_and(|p| p.current(shell)))),
            can_open: e.stage == Stage::Available && current,
        })
    })
}
#[derive(Clone)]
pub(crate) struct Child {
    pub id: String,
    pub title: String,
    pub available: bool,
}
pub(in crate::slack_lists) fn project_children(v: &View, parent: &str) -> Arc<Vec<Child>> {
    let Some(l) = v.loaded.as_ref() else {
        return Arc::new(vec![]);
    };
    Arc::new(
        l.records
            .ordered(None)
            .into_iter()
            .filter(|r| r.parent_id() == Some(parent))
            .take(50)
            .map(|r| Child {
                id: r.id.clone(),
                title: v
                    .all_rows
                    .iter()
                    .find(|row| row.id == r.id)
                    .map(|r| r.label.clone())
                    .unwrap_or_else(|| "Unavailable subitem".into()),
                available: !r.archived()
                    && r.raw
                        .get("is_deleted")
                        .is_none_or(|v| v.is_null() || v == &Value::Bool(false)),
            })
            .collect(),
    )
}
pub(crate) fn children(shell: &ShellState, parent: &str) -> Arc<Vec<Child>> {
    shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| record_detail::subitem_children(v, parent))
        .unwrap_or_else(|| Arc::new(vec![]))
}
pub(in crate::slack_lists) fn project_reason(view: &View, parent: &str) -> Option<&'static str> {
    let l = view.loaded.as_ref()?;
    SubitemCreationIntent::eligible(
        &l.document,
        &l.records,
        parent,
        l.allow_lists.as_deref(),
        l.personal_task_lists.as_ref(),
        l.list_experiments.is_some_and(|f| f.sub_tasks),
    )
    .err()
    .map(|e| e.0)
}
pub(crate) fn reason(shell: &ShellState, detail: u64) -> Option<&'static str> {
    if blocks(shell) || !available(shell, false) {
        return Some("Finish the current change or refresh the parent first.");
    }
    let v = shell.slack_lists.view.as_ref()?;
    if record_detail::subitem_target(v, detail).is_none() {
        return Some("Only an active root item can have subitems.");
    }
    record_detail::subitem_reason(v)
}
