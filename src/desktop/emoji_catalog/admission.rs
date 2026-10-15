use super::*;
pub(super) enum Prepared {
    Ready(Option<Arc<Loaded>>),
    Build {
        owner: Owner,
        revision: u64,
        locale: Option<String>,
        custom: snapshot::Snapshot,
        ticket: u64,
        context: Option<Context>,
        refresh: bool,
        ready: Option<Arc<Loaded>>,
    },
}
pub(super) fn prepare(
    shell: &mut ShellState,
    capture: impl FnOnce(
        &HashMap<String, super_platinum_core::slack::models::Emoji>,
    ) -> Result<snapshot::Snapshot, String>,
) -> Prepared {
    let diagnostic_remote = retirement::enabled()
        && retirement::remote_allowed(
            shell.core.account_epoch,
            shell
                .core
                .active_team
                .as_ref()
                .and_then(|t| shell.core.workspaces.get(t))
                .map_or(0, |w| w.custom_emoji_revision),
        );
    prepare_local(
        shell,
        capture,
        !crate::fixture::is_fixture() || diagnostic_remote,
    )
}
fn prepare_local(
    shell: &mut ShellState,
    capture: impl FnOnce(
        &HashMap<String, super_platinum_core::slack::models::Emoji>,
    ) -> Result<snapshot::Snapshot, String>,
    refresh_allowed: bool,
) -> Prepared {
    let owner = match Owner::capture(shell) {
        Some(owner) => owner,
        None => return Prepared::Ready(None),
    };
    let workspace = match shell.core.workspaces.get(&owner.team) {
        Some(workspace) => workspace,
        None => return Prepared::Ready(None),
    };
    let revision = workspace.custom_emoji_revision;
    let locale = workspace.locale.clone();
    let ready = current(shell);
    let busy = shell
        .emoji_catalog
        .building
        .get(&owner.team)
        .is_some_and(|b| b.owner.same(&owner) && b.revision == revision && b.locale == locale);
    let refresh = refresh_allowed
        && shell.emoji_catalog.attempted.get(&owner.team).is_none_or(
            |(previous, attempted_revision, at)| {
                !previous.same(&owner)
                    || *attempted_revision != revision
                    || at.elapsed() >= Duration::from_secs(300)
            },
        );
    if busy || ready.is_some() && !refresh {
        return Prepared::Ready(ready);
    }
    // Only catalog identity/value fields are captured here. This remains
    // an O(N) serial snapshot; native large-catalog latency needs measurement.
    let capture_at = probe::begin();
    let custom = match capture(&workspace.custom_emoji) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            probe::finish(
                capture_at,
                "capture-failed",
                || serde_json::json!({"revision":revision,"error":error}),
            );
            return Prepared::Ready(ready);
        }
    };
    probe::finish(
        capture_at,
        "capture",
        || serde_json::json!({"ticket":shell.emoji_catalog.serial.wrapping_add(1),"revision":revision,"locale":locale,"account":owner.account,"epoch":owner.epoch,"team":owner.team,"count":workspace.custom_emoji.len()}),
    );
    let context = Context::capture(shell);
    shell.emoji_catalog.serial = shell.emoji_catalog.serial.wrapping_add(1);
    let ticket = shell.emoji_catalog.serial;
    shell.emoji_catalog.building.insert(
        owner.team.clone(),
        Building {
            owner: owner.clone(),
            revision,
            locale: locale.clone(),
            ticket,
        },
    );
    if refresh {
        shell.emoji_catalog.attempted.insert(
            owner.team.clone(),
            (owner.clone(), revision, Instant::now()),
        );
    }
    Prepared::Build {
        owner,
        revision,
        locale,
        custom,
        ticket,
        context,
        refresh,
        ready,
    }
}
#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
