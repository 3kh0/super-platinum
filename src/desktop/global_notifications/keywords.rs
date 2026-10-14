//! Explicit keyword draft ownership. No debounce or unmount writer.
use super::*;
pub(crate) fn edit_keywords(mut state: Signal<ShellState>, visit: u64, text: String) {
    edit_shell(&mut state.write(), visit, text);
}
pub(super) fn edit_shell(s: &mut ShellState, visit: u64, text: String) {
    if presentation(s, visit).is_none_or(|p| !p.can_change) {
        return;
    }
    let owner = s
        .global_notifications
        .view
        .as_ref()
        .unwrap()
        .token
        .owner
        .clone();
    let Some(d) = s
        .global_notifications
        .entries
        .get_mut(&owner)
        .and_then(|e| e.keywords.as_mut())
    else {
        return;
    };
    d.dirty = api::notification_keywords_display(&d.baseline) != text;
    d.text = text;
    d.generation = next_id();
}
pub(crate) fn save_keywords(state: Signal<ShellState>, visit: u64) {
    let text = {
        let s = state.read();
        let Some(p) = presentation(&s, visit)
            .filter(|p| p.can_change && p.keyword_dirty && p.keyword_error.is_none())
        else {
            return;
        };
        p.keywords
    };
    // The core intent owns the source normalization; do not normalize twice.
    change(state, visit, Action::Keywords(text));
}
pub(crate) fn discard_keywords(mut state: Signal<ShellState>, visit: u64) {
    let mut s = state.write();
    if presentation(&s, visit).is_none_or(|p| !p.can_change) {
        return;
    }
    let v = s.global_notifications.view.as_ref().unwrap();
    let Some(settings) = v.snapshot.as_ref().and_then(|p| p.global_settings().ok()) else {
        return;
    };
    let owner = v.token.owner.clone();
    s.global_notifications
        .entries
        .get_mut(&owner)
        .unwrap()
        .keywords = Some(KeywordDraft {
        text: api::notification_keywords_display(&settings.keywords),
        baseline: settings.keywords,
        generation: next_id(),
        seed: next_id(),
        dirty: false,
    });
}
