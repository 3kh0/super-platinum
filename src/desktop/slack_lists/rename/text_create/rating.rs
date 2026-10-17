//! Rating draft changes use the existing schema owner and choice epoch.
use super::*;
use super_platinum_core::slack_lists::RatingOptions;

pub(crate) fn quantity(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    value: String,
) {
    if quantity_shell(&state.peek(), serial, visit, selection, rendered, &value) {
        state.write();
    }
}
pub(crate) fn quantity_shell(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    value: &str,
) -> bool {
    if rendered.kind != ColumnCreateKind::Rating {
        return false;
    }
    let max = match value {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        "4" => 4,
        "5" => 5,
        _ => return false,
    };
    with_edit(shell, serial, visit, selection, rendered, |entry| {
        if entry.rating_picker.is_some() || entry.rating_draft.max() == max {
            return false;
        }
        let Ok(next) = RatingOptions::capture(
            entry.rating_draft.emoji(),
            max,
            entry.rating_draft.team_id(),
        ) else {
            return false;
        };
        entry.rating_draft = next;
        entry.field_choice.revision = next_id();
        true
    })
}
pub(crate) fn open(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
) {
    if open_shell(&state.peek(), serial, visit, selection, rendered).is_some() {
        state.write();
    }
}
pub(crate) fn open_shell(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
) -> Option<u64> {
    if !matches!(
        rendered.kind,
        ColumnCreateKind::Rating | ColumnCreateKind::Vote
    ) {
        return None;
    }
    let mut token = None;
    with_edit(shell, serial, visit, selection, rendered, |entry| {
        if entry.rating_picker.is_some() {
            return false;
        }
        let n = next_id();
        entry.rating_picker = Some(n);
        token = Some(n);
        true
    });
    token
}
pub(crate) fn close(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    token: u64,
) {
    finish_picker(state, serial, visit, selection, rendered, token, None);
}

pub(crate) fn close_shell(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    token: u64,
) -> bool {
    // Dismissal is not mutation admission. A changed population, stale view or
    // changed saved-view selection must not trap this exact current picker open.
    // Key::current still binds the live account/team/scope/List; visit, serial,
    // choice and consumed token exclude retained callbacks and replacement UI.
    let _ = selection;
    if generation(shell) != Some(visit)
        || !matches!(
            rendered.kind,
            ColumnCreateKind::Rating | ColumnCreateKind::Vote
        )
    {
        return false;
    }
    let Some(key) = Key::current(shell) else {
        return false;
    };
    with_registry(|r| {
        let Some(e) = r.entries.get_mut(&key).filter(|e| {
            e.serial == serial
                && e.stage == Stage::Editing
                && e.target.is_text_creation()
                && e.field_choice == rendered
                && e.rating_picker == Some(token)
                && e.job.is_none()
                && e.intent.is_none()
        }) else {
            return false;
        };
        e.rating_picker = None;
        true
    })
}

pub(crate) fn choose(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    token: u64,
    picked: crate::emoji_catalog::Selection,
) {
    finish_picker(
        state,
        serial,
        visit,
        selection,
        rendered,
        token,
        Some(picked),
    );
}
fn finish_picker(
    mut state: Signal<ShellState>,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    token: u64,
    picked: Option<crate::emoji_catalog::Selection>,
) {
    // Capture actual DOM custody before our own removal. Snapshot failure never
    // blocks exact-token dismissal; it merely withholds focus restoration.
    let mut eval = dioxus::document::eval(&format!(
        "{}\nrestoreRatingEmoji({serial},{visit},{},{token});",
        include_str!("rating_restore.js"),
        rendered.revision
    ));
    // Bounded work survives the child picker scope disappearing. It cannot
    // acquire request authority and all mutations recheck the retained token.
    dioxus::core::spawn_forever(async move {
        let captured = matches!(
            tokio::time::timeout(std::time::Duration::from_millis(500), eval.recv::<bool>()).await,
            Ok(Ok(true))
        );
        let changed = {
            let shell = state.peek();
            match picked {
                None => close_shell(&shell, serial, visit, selection, rendered, token),
                Some(picked) => {
                    let current = crate::emoji_catalog::current(&shell);
                    if !picked.loaded.current(&shell)
                        || !current
                            .as_ref()
                            .is_some_and(|c| Arc::ptr_eq(c, &picked.loaded))
                        || !picked.loaded.picker.iter().any(|c| c.name == picked.name)
                    {
                        false
                    } else if let Some(team) = shell.core.active_team.as_deref() {
                        apply_emoji(
                            &shell,
                            serial,
                            visit,
                            selection,
                            rendered,
                            token,
                            &format!(":{}:", picked.name),
                            team,
                        )
                    } else {
                        false
                    }
                }
            }
        };
        let revision = if changed {
            state.write();
            let shell = state.peek();
            if super::super::selection(&shell) == Some(selection) {
                choice(&shell).map(|c| c.revision)
            } else {
                None
            }
        } else {
            None
        };
        let _ = eval.send(if captured { revision } else { None });
    });
}

fn apply_emoji(
    shell: &ShellState,
    serial: u64,
    visit: u64,
    selection: u64,
    rendered: Choice,
    token: u64,
    emoji: &str,
    team: &str,
) -> bool {
    if !matches!(
        rendered.kind,
        ColumnCreateKind::Rating | ColumnCreateKind::Vote
    ) || shell.core.active_team.as_deref() != Some(team)
    {
        return false;
    }
    with_edit(shell, serial, visit, selection, rendered, |entry| {
        if entry.rating_picker != Some(token) {
            return false;
        }
        let Ok(next) = RatingOptions::capture(emoji, entry.rating_draft.max(), Some(team)) else {
            return false;
        };
        entry.rating_draft = next;
        entry.rating_picker = None;
        entry.field_choice.revision = next_id();
        true
    })
}
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn close_fixture(shell: &ShellState) -> bool {
    let Some(e) = entry(shell).filter(|e| {
        e.target == Target::NewTextField("V_FILTER".into())
            && e.stage == Stage::Editing
            && e.field_choice.kind == ColumnCreateKind::Rating
            && e.job.is_none()
            && e.intent.is_none()
    }) else {
        return false;
    };
    close_entry(shell, e.serial)
}

#[cfg(test)]
mod vote_tests;
#[cfg(test)]
pub(crate) fn close_vote_fixture(shell: &ShellState) -> bool {
    let Some(e) = entry(shell).filter(|e| {
        e.target == Target::NewTextField("V_FILTER".into())
            && e.stage == Stage::Editing
            && e.field_choice.kind == ColumnCreateKind::Vote
            && e.job.is_none()
            && e.intent.is_none()
    }) else {
        return false;
    };
    close_entry(shell, e.serial)
}
