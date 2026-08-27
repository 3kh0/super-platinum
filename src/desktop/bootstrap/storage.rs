use dioxus::prelude::{ReadableExt, Signal, WritableExt};

use super_platinum_core::MediaCacheKind;

use crate::state::ShellState;

use super::common::persist_workspace;
use super::persist_settings;

pub async fn refresh_storage(mut state: Signal<ShellState>) {
    if state.read().storage.fixture {
        return;
    }
    state.write().storage.scanning = true;
    let store = state.read().media.store();
    let account = state.read().core.active_account.clone();
    let result = tokio::task::spawn_blocking(move || {
        let pictures = store
            .as_ref()
            .map(|store| store.usage())
            .unwrap_or_default();
        let workspace = account
            .as_deref()
            .map(super_platinum_core::cache::Cache::usage_bytes)
            .unwrap_or(0);
        (pictures, workspace)
    })
    .await;
    let mut shell = state.write();
    shell.storage.scanning = false;
    match result {
        Ok((pictures, workspace)) => {
            shell.storage.usage.avatars = pictures.avatars;
            shell.storage.usage.emoji = pictures.emoji;
            shell.storage.usage.icons = pictures.icons;
            shell.storage.usage.other = pictures.other;
            shell.storage.usage.workspace = workspace;
        }
        Err(error) => {
            eprintln!("super-platinum: storage scan failed: {error}");
        }
    }
}

pub async fn clear_picture_cache(mut state: Signal<ShellState>) {
    if state.read().storage.fixture {
        return;
    }
    let kinds: Vec<MediaCacheKind> = MediaCacheKind::ALL
        .into_iter()
        .filter(|kind| state.read().storage.selected[kind.index()])
        .collect();
    if kinds.is_empty() {
        return;
    }
    let Some(store) = state.read().media.store() else {
        state.write().show_toast("Picture cache is unavailable.");
        return;
    };
    let purge_kinds = kinds.clone();
    let removed = tokio::task::spawn_blocking(move || store.purge(&purge_kinds))
        .await
        .unwrap_or(0);
    {
        let mut shell = state.write();
        for kind in kinds {
            shell.media.evict_kind(kind);
        }
        if shell.media.take_dirty() {
            shell.media_epoch = shell.media_epoch.wrapping_add(1);
        }
        shell.show_toast(format!(
            "Cleared {}",
            super_platinum_core::state::format_file_size(removed)
        ));
    }
    refresh_storage(state).await;
}

pub async fn clear_workspace_cache(mut state: Signal<ShellState>) {
    if state.read().storage.fixture {
        return;
    }
    let account = state.read().core.active_account.clone();
    let team = state.read().core.active_team.clone();
    state.write().trim_cached_history();
    if let Some(account) = account {
        let result = tokio::task::spawn_blocking(move || {
            super_platinum_core::cache::Cache::remove_default(&account)
        })
        .await;
        if let Ok(Err(error)) = result {
            eprintln!("super-platinum: could not remove workspace cache: {error}");
        }
    }
    if let Some(team) = team {
        persist_workspace(&state, &team);
    }
    state.write().show_toast("Cleared cached history.");
    refresh_storage(state).await;
}

pub async fn apply_cache_limit(state: Signal<ShellState>) {
    persist_settings(state).await;
    let media = state.read().media.clone();
    media.prune_now().await;
    refresh_storage(state).await;
}
