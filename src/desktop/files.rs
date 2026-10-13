//! Native Files browser loading and preview actions.
use crate::state::{Overlay, ShellState, ViewerVm};
use dioxus::prelude::*;
use super_platinum_core::{
    MediaAssetKind,
    files::{FilesQuery, FilesState},
    slack::{api, models::File},
};

pub fn visible_state(shell: &ShellState) -> Option<&FilesState> {
    visible_for(shell, None)
}

pub fn visible_for<'a>(shell: &'a ShellState, channel: Option<&str>) -> Option<&'a FilesState> {
    let files = if channel.is_some() {
        &shell.core.channel_files
    } else {
        &shell.core.files
    };
    (files.matches_scope(shell.core.account_epoch, shell.core.active_team.as_deref()?)
        && files.query.channel.as_deref() == channel)
        .then_some(files)
}

fn store(shell: &mut ShellState, channel: bool) -> &mut FilesState {
    if channel {
        &mut shell.core.channel_files
    } else {
        &mut shell.core.files
    }
}

pub async fn load(mut state: Signal<ShellState>, query: FilesQuery, page: u32) {
    let Some((transport, client, sessions)) = crate::bootstrap::credentials(&state) else {
        return;
    };
    let Some(team) = state.peek().core.active_team.clone() else {
        return;
    };
    let Some(session) = sessions.into_iter().find(|s| s.team_id == team) else {
        return;
    };
    let channel = query.channel.is_some();
    let (epoch, generation) = {
        let mut shell = state.write();
        let epoch = shell.core.account_epoch;
        let files = store(&mut shell, channel);
        files.ensure_scope(epoch, &team);
        (epoch, files.begin(query.clone(), page))
    };
    let result = api::fetch_files(&transport, &client, &session, &query, page).await;
    {
        let mut shell = state.write();
        if shell.core.account_epoch != epoch
            || shell.core.active_team.as_ref() != Some(&team)
            || !store(&mut shell, channel).matches_scope(epoch, &team)
            || store(&mut shell, channel).generation != generation
            || !shell
                .core
                .transport
                .as_ref()
                .is_some_and(|current| std::sync::Arc::ptr_eq(current, &transport))
        {
            return;
        }
        match result {
            Ok(page) => {
                let deleted = shell
                    .core
                    .workspaces
                    .get(&team)
                    .map(|w| w.deleted_files.clone())
                    .unwrap_or_default();
                store(&mut shell, channel).deleted.extend(deleted);
                store(&mut shell, channel).apply(generation, page);
            }
            Err(error) => {
                store(&mut shell, channel).loading = false;
                store(&mut shell, channel).error =
                    Some("Files could not load. Try changing your search or refreshing.".into());
                shell.report_failure(
                    &error,
                    "Files will be available when you reconnect.",
                    || "Could not load files. Please try again.".into(),
                );
            }
        }
    }
    let missing = {
        let shell = state.peek();
        let Some(workspace) = shell.core.workspaces.get(&team) else {
            return;
        };
        visible_for(&shell, query.channel.as_deref())
            .into_iter()
            .flat_map(|files| &files.items)
            .filter_map(super_platinum_core::state::file_uploader_id)
            .filter(|id| !workspace.users.contains_key(*id))
            .map(str::to_owned)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    if !missing.is_empty() {
        if let Ok(users) = api::fetch_users_info(&transport, &client, &session, missing).await {
            let mut shell = state.write();
            if shell.core.account_epoch != epoch
                || shell.core.active_team.as_ref() != Some(&team)
                || !shell
                    .core
                    .transport
                    .as_ref()
                    .is_some_and(|current| std::sync::Arc::ptr_eq(current, &transport))
            {
                return;
            }
            if let Some(workspace) = shell.core.workspaces.get_mut(&team) {
                for user in users {
                    workspace.apply_user_update(user);
                }
            }
        }
    }
}

pub fn open(mut state: Signal<ShellState>, file: File) {
    let deleted = super_platinum_core::file_actions::deleted(&file) || {
        let shell = state.peek();
        shell
            .core
            .active_team
            .as_ref()
            .and_then(|team| shell.core.workspaces.get(team))
            .is_some_and(|w| {
                file.id
                    .as_ref()
                    .is_some_and(|id| w.deleted_files.contains(id))
            })
    };
    if deleted {
        state.write().show_toast("This file has been deleted.");
        return;
    }
    if matches!(file.filetype.as_deref(), Some("quip" | "canvas")) {
        crate::canvas::open(state, file);
        return;
    }
    if super_platinum_core::slack_lists::is_list(&file) {
        crate::slack_lists::open(state, file);
        return;
    }
    let Some(url) = file.full_url() else {
        state
            .write()
            .show_toast("This file is not available to preview.");
        return;
    };
    let mut shell = state.write();
    let mime = super_platinum_core::file_preview::mime_for_file(&file).to_owned();
    let id = shell
        .media
        .register_deferred(MediaAssetKind::Attachment, url, mime.clone());
    shell.media.request(&id);
    shell.viewer = Some(ViewerVm {
        file: super_platinum_core::file_actions::identity(&file),
        id,
        name: super_platinum_core::state::file_title(&file),
        download_name: super_platinum_core::state::file_download_name(&file),
        mime,
    });
    shell.overlay = Some(Overlay::Viewer);
}
