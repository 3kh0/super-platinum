//! Recover only a reached member using the shared absent/known acquisition.
//! No Profile opening/query receipt is reused; the original List source, read
//! serial, policy and already admitted labels guard every asynchronous stage.
use super::*;
use crate::profile_edit::sections::rich::search::{TitleMemberSource, acquire_title_members};

pub(super) fn member(
    shell: &ShellState,
    context: &Context,
    id: &str,
) -> Result<(Subset, String), Error> {
    if !context.current(shell) {
        return Err(unavailable());
    }
    let w = shell
        .core
        .workspaces
        .get(&context.session.team_id)
        .ok_or_else(unavailable)?;
    let subset = w.directory_members.subset(
        &context.account,
        &context.session,
        context.epoch,
        GENERATION,
        &BTreeSet::from([id.to_owned()]),
        &w.users,
    )?;
    let row = subset
        .rows()
        .iter()
        .find(|row| row.source["id"] == id)
        .ok_or_else(unavailable)?;
    let label = crate::profile_edit::sections::rich::opening::member_label(&row.source)?;
    Ok((subset, label))
}

pub(super) async fn member_after_acquisition(
    state: Signal<ShellState>,
    facts: &Facts,
    policy: &own_edit_policy::Observation,
    id: &str,
) -> Result<(Subset, String), Error> {
    let current = |shell: &ShellState| facts.current(shell) && policy.current(shell);
    let team = &facts.context.session.team_id;
    let required = BTreeSet::from([id.to_owned()]);
    let generation = format!(
        "{GENERATION}-{}-{}",
        facts.source.owner.serial, facts.serial
    );
    let source = {
        let shell = state.peek();
        if !current(&shell) {
            return Err(unavailable());
        }
        TitleMemberSource::capture(&shell, team, &generation, &required)?
    };
    let observed = acquire_title_members(state, team, &generation, current, source, &required)
        .await?
        .ok_or_else(unavailable)?;
    let shell = state.peek();
    if !current(&shell) || !observed.current(&shell) {
        return Err(unavailable());
    }
    // Re-open the actual source row. A successful read is not a label, and
    // an unknown/deleted/malformed member still prevents plain initialization.
    member(&shell, &facts.context, id)
}
