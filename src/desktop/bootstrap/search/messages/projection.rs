//! Keep the original flat search VM and an optional inert supplied-body copy.
use super::super::message_pages::{Bodies, Supplied, clean_nodes};
use crate::{media::MediaRegistry, state::SearchResultVm};
use super_platinum_core::{slack::models::SearchItem, state::Workspace};

pub(super) fn rows(
    workspace: Option<&Workspace>,
    media: &MediaRegistry,
    items: Vec<SearchItem>,
    rich: bool,
) -> (Vec<SearchResultVm>, Bodies) {
    let mut rows = Vec::new();
    let mut bodies = Bodies::new();
    for item in items {
        let channel_id = item
            .channel
            .as_ref()
            .map(|c| c.id.clone())
            .unwrap_or_default();
        let (channel_name, direct_conversation) = item
            .channel
            .as_ref()
            .map(|channel| {
                workspace
                    .map(|w| super::super::conversation::label(w, channel))
                    .unwrap_or_else(|| {
                        (
                            super_platinum_core::state::channel_name(channel),
                            channel.is_im || channel.is_mpim,
                        )
                    })
            })
            .unwrap_or_else(|| ("conversation".into(), false));
        for message in item.messages {
            let author = workspace
                .map(|w| super_platinum_core::state::message_author_name(w, &message))
                .unwrap_or_else(|| "Slack".into());
            let vm = workspace.map(|w| crate::message_vm::message_vm(w, &message, media));
            let text = vm
                .as_ref()
                .map(super::super::preview::text)
                .unwrap_or_else(|| super_platinum_core::state::message_text(&message))
                .replace(['\u{e000}', '\u{e001}'], "");
            let hit = SearchResultVm {
                direct_conversation,
                thread_ts: message
                    .thread_ts
                    .clone()
                    .filter(|root| Some(root) != message.ts.as_ref()),
                channel_id: channel_id.clone(),
                channel_name: channel_name.clone(),
                ts: message.ts.clone().unwrap_or_default(),
                author,
                text,
            };
            if rich && !hit.channel_id.is_empty() && !hit.ts.is_empty() {
                let coherent = workspace.is_some_and(|w| {
                    item.team.as_deref().is_none_or(|t| t == w.team_id)
                        && message.team.as_deref().is_none_or(|t| t == w.team_id)
                        && message.channel.as_deref().is_none_or(|c| c == channel_id)
                });
                let supplied = if coherent {
                    let vm = vm.unwrap();
                    Supplied {
                        coherent: true,
                        ordinary_root: super::super::message_pages::ordinary_root(&message),
                        reply_count: message.reply_count,
                        nodes: clean_nodes(crate::message_vm::preview::nodes(vm.body)),
                        reactions: workspace
                            .map(|w| {
                                crate::message_vm::reaction_vms_for_reading(w, &message, media)
                            })
                            .unwrap_or_default(),
                    }
                } else {
                    Supplied::default()
                };
                // Preserve first-row deduplication and attachment-only fallback.
                bodies
                    .entry((hit.channel_id.clone(), hit.ts.clone()))
                    .or_insert(supplied);
            }
            rows.push(hit);
        }
    }
    (rows, bodies)
}
