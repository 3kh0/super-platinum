//! Native Files browser loading and preview actions.
use crate::state::{Overlay, ShellState, ViewerVm};
use dioxus::prelude::*;
use super_platinum_core::{MediaAssetKind, files::FilesState, slack::models::File};

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

mod read;
pub(crate) use read::{Source, load, load_picker, load_reconnect};

pub fn open(mut state: Signal<ShellState>, file: File) {
    if crate::canvas_create::departure::request(
        &mut state.write(),
        crate::canvas_create::departure::Destination::File(Box::new(file.clone())),
    ) {
        return;
    }
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
    let Some(url) = file
        .full_url()
        .or_else(|| super_platinum_core::state::file_download_url(&file))
    else {
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
    shell.media.register_file_download(&id, &file);
    shell.media.request(&id);
    shell.viewer = Some(ViewerVm {
        file: super_platinum_core::file_actions::identity(&file),
        id,
        name: super_platinum_core::state::file_title(&file),
        download_name: super_platinum_core::state::file_download_name(&file),
        mime,
    });
    shell.overlay = Some(Overlay::Viewer);
    crate::file_preview::refresh::opened(&mut shell);
}
