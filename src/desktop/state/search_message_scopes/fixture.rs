//! Disabled presentation data only: no session, owner or canonical receipt.
use super::*;
#[derive(Clone, Copy)]
pub(crate) enum Fixture {
    Selected,
    Cleared,
}
fn edit(s: &ShellState) -> Option<ScopeEdit> {
    s.search_message_scope_fixture?;
    ScopeEdit::capture(
        &s.search_query,
        vec![
            ScopeChoice::channel("C12345678", "ship")?,
            ScopeChoice::channel("C87654321", "design")?,
            ScopeChoice::author("U12345678", "Maya Chen")?,
            ScopeChoice::author("W87654321", "Jules Rowan")?,
        ],
    )
    .ok()
}
pub(super) fn rows(s: &ShellState, kind: Scope) -> Vec<Row> {
    let Some(edit) = edit(s) else {
        return Vec::new();
    };
    edit.choices(kind)
        .map(|c| Row {
            id: c.id().into(),
            label: c.label().into(),
            username: String::new(),
            selected: edit.selected(kind, c.id()),
            current: false,
        })
        .collect()
}
pub(super) fn count(s: &ShellState, kind: Scope) -> usize {
    edit(s).map(|d| d.selected_count(kind)).unwrap_or(0)
}
pub(super) fn install(s: &mut ShellState, name: &str) {
    s.overlay = Some(Overlay::Search);
    s.set_search_kind(SearchKind::Messages);
    s.set_search_query(if name.contains("clear") {
        "release -in:archive -from:me".into()
    } else {
        "release in:<#C12345678|ship> in:<#C87654321|design> from:<@U12345678> from:<@W87654321>"
            .into()
    });
    s.search_message_scope_fixture = Some(if name.contains("clear") {
        Fixture::Cleared
    } else {
        Fixture::Selected
    });
    s.search_filters = Some(SearchFilters::default());
}
