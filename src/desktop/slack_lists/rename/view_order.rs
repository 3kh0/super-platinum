//! One immutable adjacent custom-view move in the shared schema receipt.
use super::*;
use super_platinum_core::slack_lists::ViewReorder;
pub(crate) use super_platinum_core::slack_lists::ViewReorderDirection as Direction;

#[derive(Clone)]
pub(crate) struct Presentation {
    pub lanes: bool,
    pub direction: &'static str,
    pub name: String,
    pub before: Vec<(String, String)>,
    pub after: Vec<(String, String)>,
    pub current: Option<Vec<(String, String)>>,
}
pub(crate) fn can_open(shell: &ShellState, direction: Direction) -> bool {
    selected_view(shell).is_some_and(|id| can_target(shell, &Target::Reorder(id.into(), direction)))
}
pub(crate) fn open(
    mut state: Signal<ShellState>,
    visit: u64,
    selection: u64,
    view: String,
    direction: Direction,
) {
    open_shell(&mut state.write(), visit, selection, &view, direction);
}
pub(crate) fn open_shell(
    shell: &mut ShellState,
    visit: u64,
    selected: u64,
    view: &str,
    direction: Direction,
) -> bool {
    if selection(shell) != Some(selected)
        || selected_view(shell) != Some(view)
        || !can_open(shell, direction)
    {
        return false;
    }
    open_target(shell, Target::Reorder(view.into(), direction), visit)
}
fn named(doc: &Document, ids: &[String]) -> Option<Vec<(String, String)>> {
    ids.iter()
        .map(|id| {
            let raw = doc.views.iter().find(|v| v["id"] == id.as_str())?;
            Some((id.clone(), raw["name"].as_str()?.to_owned()))
        })
        .collect()
}
pub(super) fn presentation(shell: &ShellState, e: &registry::Entry) -> Option<Presentation> {
    if e.target.is_lane_order() {
        return lane_order::presentation(shell, e);
    }
    let Target::Reorder(id, direction) = &e.target else {
        return None;
    };
    let before = ViewReorder::current_order(&e.original).ok()?;
    let mut after = before.clone();
    let at = before.iter().position(|v| v == id)?;
    let other = match direction {
        Direction::Up => at.checked_sub(1)?,
        Direction::Down => at.checked_add(1).filter(|i| *i < before.len())?,
    };
    after.swap(at, other);
    let current = shell
        .slack_lists
        .view
        .as_ref()?
        .loaded
        .as_ref()
        .and_then(|l| named(&l.document, &ViewReorder::current_order(&l.document).ok()?));
    Some(Presentation {
        lanes: false,
        direction: match direction {
            Direction::Up => "up",
            Direction::Down => "down",
        },
        name: e.before.clone(),
        before: named(&e.original, &before)?,
        after: named(&e.original, &after)?,
        current,
    })
}
