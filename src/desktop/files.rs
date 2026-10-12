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
    if matches!(
        file.filetype.as_deref(),
        Some("quip" | "canvas" | "list" | "lists")
    ) {
        if let Some(url) = file.extra.get("permalink").and_then(|v| v.as_str()) {
            if crate::media::open_external(url).is_err() {
                state
                    .write()
                    .show_toast("This file link could not be opened.");
            }
        } else {
            state
                .write()
                .show_toast("This file does not have an available link.");
        }
        return;
    }
    let Some(url) = file.full_url() else {
        state
            .write()
            .show_toast("This file is not available to preview.");
        return;
    };
    let mut shell = state.write();
    let mime = file
        .mimetype
        .clone()
        .unwrap_or_else(|| "application/octet-stream".into());
    let id = shell
        .media
        .register_deferred(MediaAssetKind::Attachment, url, mime.clone());
    shell.media.request(&id);
    shell.viewer = Some(ViewerVm {
        id,
        name: super_platinum_core::state::file_title(&file),
        mime,
    });
    shell.overlay = Some(Overlay::Viewer);
}
