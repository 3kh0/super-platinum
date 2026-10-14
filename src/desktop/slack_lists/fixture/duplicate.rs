//! Valid recorded fractional positions for disconnected duplication exercises.
use super::*;

pub(crate) fn add_duplicate_views(shell: &mut ShellState) -> Option<creation::CreationSnapshot> {
    let snapshot = add_creation_views(shell)?;
    let fixture = shell.slack_lists.fixture.as_mut()?;
    for (index, record) in fixture.records["records"]
        .as_array_mut()?
        .iter_mut()
        .enumerate()
    {
        record["position"] = json!(format!("{:010}", (index + 1) * 100));
    }
    refresh(shell);
    Some(snapshot)
}
