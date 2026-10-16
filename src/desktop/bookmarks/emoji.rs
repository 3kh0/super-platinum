//! Stored bookmark decoration only; editing and wire metadata are unchanged.
use crate::{
    icons::{Icon, icon},
    state::ShellState,
};
use dioxus::prelude::*;
use super_platinum_core::{
    MediaAssetId,
    bookmarks::Bookmark,
    state::{EmojiSource, custom_emoji_source, emoji_glyph, is_standard_emoji},
};

#[derive(Debug, PartialEq, Eq)]
enum Decoration {
    Folder,
    Bookmark,
    Glyph(String),
    Image(MediaAssetId),
}

fn custom_id(shell: &ShellState, team: &str, name: &str, url: &str) -> MediaAssetId {
    // Prefer a scoped disk slot for new sources. Existing same-URL sources
    // retain their original slot, so projection must also reject provisional bytes.
    let account = shell.core.active_account.as_deref().unwrap_or("");
    let key = cache_key(account, team, name);
    shell.media.register_emoji(&key, url)
}

fn cache_key(account: &str, team: &str, name: &str) -> String {
    format!("bookmark/{account}/{team}/{name}")
}

fn project(shell: &ShellState, bookmark: &Bookmark) -> Decoration {
    if bookmark.folder() {
        return Decoration::Folder;
    }
    let Some(name) = bookmark
        .emoji
        .as_deref()
        .map(str::trim)
        .map(|v| v.trim_matches(':'))
        .filter(|v| !v.is_empty())
    else {
        return Decoration::Bookmark;
    };
    let Some(team) = shell.core.active_team.as_deref() else {
        return Decoration::Bookmark;
    };
    let Some(workspace) = shell.core.workspaces.get(team) else {
        return Decoration::Bookmark;
    };
    match custom_emoji_source(&workspace.custom_emoji, name) {
        Some(EmojiSource::Unicode(glyph)) => Decoration::Glyph(glyph.into()),
        Some(EmojiSource::Url(url)) => {
            let id = custom_id(shell, team, name, url);
            // is_ready also reactivates an evicted visible source. ready_bytes
            // rejects old-URL provisional cache bytes, including those hydrated
            // through an earlier unscoped same-URL registration.
            if shell.media.is_ready(&id)
                && shell.media.ready_bytes(&id).is_some()
                && !shell.media.image_state(&id).1
            {
                Decoration::Image(id)
            } else {
                Decoration::Bookmark
            }
        }
        None if is_standard_emoji(name) => Decoration::Glyph(emoji_glyph(name)),
        None => Decoration::Bookmark,
    }
}

pub(super) fn render(
    state: Signal<ShellState>,
    shell: &ShellState,
    bookmark: &Bookmark,
) -> Element {
    match project(shell, bookmark) {
        Decoration::Folder => icon(Icon::ChevronRight, "icon"),
        Decoration::Bookmark => icon(Icon::Bookmark, "icon"),
        Decoration::Glyph(glyph) => {
            rsx! {span {class:"bookmark-emoji", "aria-hidden":"true", "{glyph}"}}
        }
        Decoration::Image(id) => {
            let media_epoch = shell.media_epoch;
            rsx! {span {class:"bookmark-emoji", "aria-hidden":"true",
                crate::view::media_image::MediaImage {
                    state, id, media_epoch, compact:true, class:"bookmark-emoji-image",
                    alt:"", fallback:rsx!{ {icon(Icon::Bookmark,"icon")} }
                }
            }}
        }
    }
}

mod fixture;
pub(crate) use fixture::apply as fixture;

#[cfg(test)]
mod tests;
