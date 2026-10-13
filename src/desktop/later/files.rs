//! Scoped file metadata loading shared by Later previews and native opening.
use super::{Context, SavedKey, ShellState};
use dioxus::prelude::*;
use std::sync::Arc;
use super_platinum_core::{later::FileHydrationFailure, slack::models::File};

#[derive(Clone)]
struct Visit {
    account: Option<String>,
    epoch: u64,
    team: String,
    generation: u64,
    transport: Option<Arc<super_platinum_core::slack::Transport>>,
}
impl Visit {
    fn capture(shell: &ShellState) -> Option<Self> {
        let team = shell.core.active_team.clone()?;
        shell
            .core
            .later
            .matches_scope(shell.core.account_epoch, &team)
            .then(|| Self {
                account: shell.core.active_account.clone(),
                epoch: shell.core.account_epoch,
                team,
                generation: shell.core.later.generation,
                transport: shell.core.transport.clone(),
            })
    }
    fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_account == self.account
            && shell.core.account_epoch == self.epoch
            && shell.core.active_team.as_ref() == Some(&self.team)
            && shell.core.later.matches_scope(self.epoch, &self.team)
            && shell.core.later.generation == self.generation
            && match (&self.transport, &shell.core.transport) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}
fn deleted(shell: &ShellState, team: &str, id: &str) -> bool {
    shell
        .core
        .workspaces
        .get(team)
        .is_some_and(|w| w.deleted_files.contains(id))
        || matches!(
            shell.core.later.file_errors.get(id),
            Some(FileHydrationFailure::Deleted)
        )
}
fn apply(
    shell: &mut ShellState,
    visit: &Visit,
    id: &str,
    revision: u64,
    result: Result<File, ()>,
) -> bool {
    if !visit.current(shell) {
        return false;
    }
    if deleted(shell, &visit.team, id) {
        return shell.core.later.file_unavailable_since(id, revision, true);
    }
    match result {
        Ok(file) if file.id.as_deref() == Some(id) => {
            if super_platinum_core::file_actions::deleted(&file) {
                shell.core.later.file_unavailable_since(id, revision, true)
            } else {
                shell.core.later.hydrate_file_since(id, file, revision)
            }
        }
        _ => shell.core.later.file_unavailable_since(id, revision, false),
    }
}
async fn fetch(ctx: Option<&Context>, id: &str) -> Result<File, ()> {
    if let Some(ctx) = ctx {
        super_platinum_core::slack::api::fetch_file(&ctx.transport, &ctx.client, &ctx.session, id)
            .await
            .map_err(|_| ())
    } else {
        super::file_fixture::metadata(id).ok_or(())
    }
}
pub(super) async fn hydrate(mut state: Signal<ShellState>, ctx: Option<&Context>, generation: u64) {
    let Some(visit) = Visit::capture(&state.peek()) else {
        return;
    };
    if visit.generation != generation
        || (ctx.is_none() && !super::file_fixture::enabled(&state.peek()))
    {
        return;
    }
    let ids = {
        let shell = state.peek();
        shell
            .core
            .later
            .visible
            .iter()
            .filter(|k| k.item_type == "file")
            .filter(|k| !shell.core.later.files.contains_key(&k.item_id))
            .map(|k| k.item_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
    };
    for id in ids {
        if !visit.current(&state.peek()) {
            return;
        }
        let revision = state.write().core.later.begin_hydration();
        let result = if deleted(&state.peek(), &visit.team, &id) {
            Err(())
        } else {
            fetch(ctx, &id).await
        };
        apply(&mut state.write(), &visit, &id, revision, result);
    }
}
pub(super) fn open(mut state: Signal<ShellState>, key: SavedKey) {
    if key.item_type != "file" || state.peek().main_view != super::MainView::Later {
        return;
    }
    let Some(visit) = Visit::capture(&state.peek()) else {
        return;
    };
    let ctx = Context::get(&state);
    if ctx.is_none() && !super::file_fixture::enabled(&state.peek()) {
        state
            .write()
            .show_toast("Connect to Slack to open this saved file.");
        return;
    }
    let open = super::navigation::OpenVisit::begin(&mut state.write());
    dioxus::core::spawn_forever(async move {
        let current = |shell: &ShellState| visit.current(shell) && open.current(shell);
        if !current(&state.peek()) {
            return;
        }
        if deleted(&state.peek(), &visit.team, &key.item_id) {
            state
                .write()
                .show_toast("This saved file was deleted. You can remove it from Later.");
            return;
        }
        let cached = state.peek().core.later.files.get(&key.item_id).cloned();
        if cached.is_none() {
            let revision = state.write().core.later.begin_hydration();
            let result = fetch(ctx.as_ref(), &key.item_id).await;
            if !current(&state.peek()) {
                return;
            }
            apply(&mut state.write(), &visit, &key.item_id, revision, result);
        }
        if !current(&state.peek()) {
            return;
        }
        let file = state.peek().core.later.files.get(&key.item_id).cloned();
        if deleted(&state.peek(), &visit.team, &key.item_id) {
            return;
        }
        if let Some(file) = file {
            crate::files::open(state, file);
        } else {
            state.write().show_toast("This saved file is unavailable. Try again when connected, or remove it from Later.");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_file_response_cannot_change_new_account_list_or_deleted_file() {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell
            .core
            .later
            .ensure_scope(shell.core.account_epoch, "T1");
        let visit = Visit::capture(&shell).unwrap();
        let rev = shell.core.later.begin_hydration();
        let file = File {
            id: Some("F1".into()),
            title: Some("old".into()),
            ..Default::default()
        };
        shell.core.active_account = Some("other".into());
        assert!(!apply(&mut shell, &visit, "F1", rev, Ok(file.clone())));
        shell.core.active_account = visit.account.clone();
        shell.core.later.generation += 1;
        assert!(!apply(&mut shell, &visit, "F1", rev, Ok(file.clone())));
        let visit = Visit::capture(&shell).unwrap();
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .deleted_files
            .insert("F1".into());
        apply(&mut shell, &visit, "F1", rev, Ok(file));
        assert!(matches!(
            shell.core.later.file_errors.get("F1"),
            Some(FileHydrationFailure::Deleted)
        ));
    }
}
