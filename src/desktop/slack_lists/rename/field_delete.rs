//! Schema deletion uses the shared one-shot worker and explicit permanent confirmation.
use super::*;
use super_platinum_core::slack_lists::ColumnDelete;

#[derive(Clone)]
pub(crate) struct Presentation {
    pub column: String,
    pub choices: Vec<(String, String)>,
}
fn choices(document: &Document) -> Vec<(String, String)> {
    document
        .columns
        .iter()
        .filter(|c| ColumnDelete::supports(document, &c.id))
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect()
}
pub(super) fn permitted(shell: &ShellState) -> bool {
    let Some(user) = text_create::user(shell) else {
        return false;
    };
    shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .is_some_and(|l| {
            ColumnDelete::schema_policy_allows(
                &l.document,
                user,
                l.list_experiments.map(|e| e.schema_owner_only),
            )
        })
}
pub(crate) fn can_open(shell: &ShellState) -> bool {
    permitted(shell)
        && shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| {
                choices(&l.document)
                    .iter()
                    .any(|(id, _)| can_target(shell, &Target::DeleteColumn(id.clone())))
            })
}
pub(crate) fn open(mut state: Signal<ShellState>, visit: u64, selection: u64) {
    open_shell(&mut state.write(), visit, selection);
}
pub(crate) fn open_shell(shell: &mut ShellState, visit: u64, selected: u64) -> bool {
    if selection(shell) != Some(selected) || !can_open(shell) {
        return false;
    }
    let Some(id) = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .and_then(|l| choices(&l.document).first().map(|c| c.0.clone()))
    else {
        return false;
    };
    open_target(shell, Target::DeleteColumn(id), visit)
}
pub(crate) fn choose(mut state: Signal<ShellState>, serial: u64, visit: u64, column: String) {
    if choose_shell(&state.peek(), serial, visit, &column) {
        state.write();
    }
}
pub(super) fn choose_shell(shell: &ShellState, serial: u64, visit: u64, column: &str) -> bool {
    if generation(shell) != Some(visit) || !owner::available(shell) || !permitted(shell) {
        return false;
    }
    let Some(key) = Key::current(shell) else {
        return false;
    };
    if registry::blocked(&key.owner) {
        return false;
    }
    with_registry(|r| {
        let Some(e) = r.entries.get_mut(&key).filter(|e| {
            e.serial == serial
                && e.stage == Stage::Editing
                && e.target.is_deletion()
                && selection_current(shell, e)
                && text_create::population_current(shell, e)
        }) else {
            return false;
        };
        if !shell
            .slack_lists
            .view
            .as_ref()
            .and_then(|v| v.loaded.as_ref())
            .is_some_and(|l| l.document == e.original)
            || e.target.id() == Some(column)
            || !ColumnDelete::supports(&e.original, column)
        {
            return false;
        }
        e.target = Target::DeleteColumn(column.into());
        e.before = e.original.column(column).unwrap().name.clone();
        // Retire every callback for the previous field, including Confirm and Cancel.
        e.serial = next_id();
        true
    })
}
pub(super) fn presentation(e: &registry::Entry) -> Option<Presentation> {
    let Target::DeleteColumn(column) = &e.target else {
        return None;
    };
    Some(Presentation {
        column: column.clone(),
        choices: choices(&e.original),
    })
}

pub(super) fn capture(
    shell: &ShellState,
    e: &registry::Entry,
    loaded: &Loaded,
) -> Result<Intent, super_platinum_core::slack_lists::ListError> {
    use super_platinum_core::slack_lists::ListError;
    let Target::DeleteColumn(column) = &e.target else {
        return Err(ListError("The field target changed."));
    };
    if e.input != Input::DeleteColumn
        || e.original_records.is_none()
        || !text_create::population_current(shell, e)
    {
        return Err(ListError(
            "The field or loaded items changed. Review a new deletion.",
        ));
    }
    ColumnDelete::capture(
        &loaded.document,
        column,
        text_create::user(shell).ok_or(ListError("The authenticated user changed."))?,
        loaded.list_experiments.map(|e| e.schema_owner_only),
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
    )
    .map(Intent::DeleteColumn)
}
pub(super) fn admit(
    shell: &ShellState,
    intent: &ColumnDelete,
    fresh: &network::Metadata,
) -> Option<Permit> {
    super_platinum_core::slack_lists::ColumnDeleteAttempt::new(intent.clone())
        .admit(
            &fresh.document,
            text_create::user(shell)?,
            fresh.experiments.map(|e| e.schema_owner_only),
            fresh.allow.as_deref(),
            Some(&fresh.personal),
        )
        .ok()
        .map(Permit::DeleteColumn)
}

#[cfg(test)]
pub(crate) fn test_shell() -> ShellState {
    let mut shell = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-select-field-create",
    );
    assert!(text_create::select::close_fixture(&shell));
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .list_experiments = Some(Default::default());
    let visit = generation(&shell).unwrap();
    let selected = selection(&shell).unwrap();
    assert!(open_shell(&mut shell, visit, selected));
    shell
}
