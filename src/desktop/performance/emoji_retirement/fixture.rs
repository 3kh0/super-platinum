use crate::state::ShellState;
use super_platinum_core::slack::models::Emoji;
#[path = "../emoji_lifecycle/corpus.rs"]
mod corpus;

pub(super) fn install(shell: &mut ShellState, ordinal: u64) {
    assert!(ordinal <= 3 && shell.core.session.is_none() && shell.core.transport.is_none());
    shell.core.cache = None;
    shell.core.active_account = Some(format!("emoji-retirement-{ordinal}"));
    shell.core.account_epoch = ordinal;
    shell.overlay = None;
    shell.toast = None;
    shell.close_thread();
    shell.close_profile();
    shell.core.threads.clear();
    shell.pending_delivery.clear();
    shell.core.composer = Default::default();
    shell.checkpoint_composers();
    let workspace = shell
        .core
        .workspaces
        .get_mut("T1")
        .expect("fixture workspace");
    workspace.emoji_preferences.replace(&Default::default());
    workspace.custom_emoji = corpus::corpus(64_000, 0);
    workspace.custom_emoji_revision = 1;
    let inventory = corpus::inventory(&workspace.custom_emoji);
    assert_eq!(inventory["fixture_fnv1a"], "9e4a16cfa4295f4d");
}
pub(super) fn owner(shell: &ShellState, ordinal: u64) -> bool {
    ordinal <= 3
        && shell.core.account_epoch == ordinal
        && shell.core.active_account.as_deref() == Some(&format!("emoji-retirement-{ordinal}"))
        && shell.core.active_team.as_deref() == Some("T1")
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.cache.is_none()
        && shell
            .core
            .workspaces
            .get("T1")
            .is_some_and(|w| w.emoji_preferences.loaded())
}
fn patch(rows: &mut std::collections::HashMap<String, Emoji>, revision: u64) {
    let zero = rows.get_mut(&corpus::name(0)).expect("first row");
    let base = zero.value.split("&retirement_revision=").next().unwrap();
    zero.value = format!("{base}&retirement_revision={revision:02}");
    rows.get_mut(&corpus::name(3)).expect("alias row").value =
        format!("alias:{}", corpus::name(((revision - 2) % 3) as usize));
}
pub(super) fn mutate(shell: &mut ShellState, ordinal: u64, revision: u64) -> Result<(), String> {
    if !owner(shell, ordinal) || !(2..=6).contains(&revision) {
        return Err("Mutation owner/revision refused".into());
    }
    let workspace = shell.core.workspaces.get_mut("T1").unwrap();
    if workspace.custom_emoji_revision + 1 != revision {
        return Err("Mutation previous revision mismatch".into());
    }
    patch(&mut workspace.custom_emoji, revision);
    workspace.custom_emoji_revision = revision;
    Ok(())
}
pub(super) fn remote_response() -> Vec<Emoji> {
    let mut rows = corpus::corpus(64_000, 0);
    patch(&mut rows, 5);
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_patch_replaces_suffix_and_preserves_other_fields() {
        let mut rows = corpus::corpus(4, 0);
        let before = rows.clone();
        patch(&mut rows, 2);
        patch(&mut rows, 3);
        assert!(
            rows[&corpus::name(0)]
                .value
                .ends_with("&retirement_revision=03")
        );
        assert_eq!(
            rows[&corpus::name(0)]
                .value
                .matches("&retirement_revision=")
                .count(),
            1
        );
        assert_eq!(
            rows[&corpus::name(3)].value,
            format!("alias:{}", corpus::name(1))
        );
        for (name, row) in &rows {
            assert_eq!(row.updated, before[name].updated);
            assert_eq!(row.extra, before[name].extra);
            if name != &corpus::name(0) && name != &corpus::name(3) {
                assert_eq!(row.value, before[name].value);
            }
        }
    }
}
