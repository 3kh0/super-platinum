//! Own-user Vote gestures use the existing one-attempt cell worker.
use super::*;
use crate::media::MediaRegistry;
use super_platinum_core::{
    slack_lists::{Column, MAX_VOTERS, Record, VoteIntent, VoteOptions},
    state::Workspace,
};

pub(crate) fn toggle(
    mut state: Signal<ShellState>,
    instance: String,
    column: String,
    visit: u64,
    layout_generation: u64,
    intent: VoteIntent,
) {
    let result = begin_table(
        &mut state.write(),
        &instance,
        &column,
        visit,
        layout_generation,
        intent,
    );
    match result {
        Ok(attempt) => network::dispatch(state, attempt),
        Err(error) => {
            if checkbox_visit_current(&state.peek(), visit) {
                if let Some(view) = state.write().slack_lists.view.as_mut() {
                    view.error = Some(error.into());
                }
            }
        }
    }
}
pub(super) fn begin_table(
    shell: &mut ShellState,
    instance: &str,
    column: &str,
    visit: u64,
    layout_generation: u64,
    intent: VoteIntent,
) -> Result<EditAttempt, &'static str> {
    if !checkbox_visit_current(shell, visit)
        || shell
            .slack_lists
            .view
            .as_ref()
            .is_none_or(|v| v.layout.kind != ProjectionLayout::Table)
    {
        return Err("This Vote control is no longer current.");
    }
    let row = layout::instance_record(shell, instance, column, visit, layout_generation)
        .ok_or("This Vote control is no longer visible.")?;
    begin(shell, &row, column, EditAuthority::Layout, intent)
}
pub(super) fn begin(
    shell: &mut ShellState,
    row: &str,
    column: &str,
    authority: EditAuthority,
    intent: VoteIntent,
) -> Result<EditAttempt, &'static str> {
    poll(shell);
    let scope = shell
        .slack_lists
        .scope
        .clone()
        .filter(|s| s.current(shell))
        .ok_or("Open the list again.")?;
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Open the list again.")?;
    if view.stale || view.loading || view.unavailable || view.retry_at > now() {
        return Err("Refresh this list before changing your vote.");
    }
    if rename::blocks_mutation(shell)
        || create::blocks_mutation(shell)
        || delete::blocks_mutation(shell)
        || reminder::blocks_mutation(shell)
        || completion::blocks_mutation(shell)
        || editor::assignment::blocks_mutation(shell)
        || shell.slack_lists.operations.contains_key(&view.id)
        || view.editor.is_some()
    {
        return Err("Finish the current edit before changing your vote.");
    }
    cell_receipts::capacity(shell)?;
    if shell.overlay != Some(Overlay::SlackList)
        || !authority.admits(view, row, column)
        || matches!(authority, EditAuthority::BoardMove(_))
        || (authority == EditAuthority::Layout && view.layout.kind != ProjectionLayout::Table)
    {
        return Err("This Vote control is no longer visible.");
    }
    let loaded = view.loaded.as_ref().ok_or("Wait for the list to load.")?;
    let user = scope.stable_owner().2;
    let edit = CellEdit::capture_vote(
        &loaded.document,
        &loaded.records,
        row,
        column,
        loaded.allow_lists.as_deref(),
        loaded.personal_task_lists.as_ref(),
        &user,
        intent,
    )
    .map_err(|_| "Your vote or access changed. Refresh before trying again.")?;
    let attempt = EditAttempt {
        date_policy: None,
        selection_generation: view.layout.selection_generation,
        authority,
        scope,
        serial: next_id(),
        generation: view.generation,
        revision: view.revision,
        edit,
    };
    let id = view.id.clone();
    layout::protect_selection(shell.slack_lists.view.as_mut().unwrap());
    shell.slack_lists.operations.insert(
        id,
        Mutation {
            attempt: attempt.clone(),
            phase: Phase::Preparing,
        },
    );
    shell.slack_lists.view.as_mut().unwrap().error = None;
    Ok(attempt)
}

/// Bounded read-only projection. Distinct IDs determine count; malformed values
/// stay visibly unsupported and cannot acquire mutation authority.
pub(super) fn project(
    workspace: Option<&Workspace>,
    media: &MediaRegistry,
    column: &Column,
    record: &Record,
) -> Option<(String, CellContent)> {
    if column.kind != "vote" {
        return None;
    }
    let fallback = || Some(("Unsupported Vote value".into(), CellContent::Plain));
    let Ok(options) = VoteOptions::from_column(column) else {
        return fallback();
    };
    let values = match record.fields.get(&column.key) {
        None => &[][..],
        Some(field) => match field.get("user").and_then(Value::as_array) {
            Some(v) if v.len() <= MAX_VOTERS => v.as_slice(),
            _ => return fallback(),
        },
    };
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for value in values {
        let Some(id) = value.as_str().filter(|v| {
            !v.is_empty()
                && v.len() <= 1024
                && !v
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace() || c == '|')
        }) else {
            return fallback();
        };
        if seen.insert(id) {
            ids.push(id);
        }
    }
    let voted = workspace.is_some_and(|w| ids.contains(&w.self_user_id.as_str()));
    let count = ids.len();
    let mut names = ids
        .iter()
        .take(20)
        .map(|id| {
            if workspace.is_some_and(|w| w.self_user_id == *id) {
                "You".to_owned()
            } else {
                workspace
                    .map(|w| w.display_name(id))
                    .unwrap_or_else(|| (*id).to_owned())
                    .chars()
                    .take(160)
                    .collect()
            }
        })
        .collect::<Vec<String>>();
    if count > names.len() {
        names.push(format!("and {} more", count - names.len()));
    }
    let tooltip = if count == 0 {
        "No votes yet".into()
    } else {
        names.join(", ")
    };
    Some((
        count.to_string(),
        CellContent::Vote {
            count,
            voted,
            tooltip,
            glyph: super::rating::glyph(workspace, media, options.emoji(), options.team_id()),
        },
    ))
}

#[cfg(test)]
mod tests;
